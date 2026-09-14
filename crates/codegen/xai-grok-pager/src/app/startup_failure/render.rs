use super::{ConnectAttempt, Context, EarlierAttempt, Reason, StartupFailure};
use crate::app::connect_timeout::CONNECT_UI_TIMEOUT_TRY_COMMAND;
use std::fmt::Write as _;
use std::time::Duration;
use xai_grok_telemetry::startup::{AgentKind, PhaseSnapshot, StartupPhase, format_duration};
const WRAP_WIDTH: usize = 76;
pub(super) fn render(failure: &StartupFailure) -> String {
    let locale = crate::locale::ctx();
    let context = &failure.context;
    let mut rows = vec![
        (
            locale.tr("Mode").into_owned(),
            attempted_agents(context),
        ),
        (
            locale
                .tr("Version")
                .into_owned(),
            context.version.clone(),
        ),
    ];
    let mut report = match &failure.reason {
        Reason::TimedOut { waited, timings } => {
            let advice = advice_for(timings, context.attempt);
            rows.push((
                locale
                    .tr("Steps")
                    .into_owned(),
                format_steps(timings),
            ));
            if let Some(command) = advice.next_step.command() {
                rows.push((
                    locale
                        .tr("Try")
                        .into_owned(),
                    command.to_owned(),
                ));
            }
            let explanation = fill_indented(&advice.explanation(), "  ", "  ");
            let seconds = whole_seconds(*waited);
            locale.tr_format("Couldn't start Grok: startup timed out after {seconds}.\n\n{explanation}",
                &[("seconds", &seconds), ("explanation", &explanation)],
            )
        }
        Reason::Cancelled => {
            let agent = agent_name(context.target).to_string();
            locale.tr_format("Startup cancelled while connecting to the {agent}.",
                &[("agent", &agent)],
            )
        }
    };
    rows.push((
        locale
            .tr("Log")
            .into_owned(),
        context.log_path.display().to_string(),
    ));
    let _ = write!(report, "\n\n{}", label_rows(&rows));
    report
}
struct Advice {
    doing: Option<&'static str>,
    earlier: Option<EarlierAttempt>,
    next_step: NextStep,
}
/// A wedged leader is only ever the earlier attempt: the fallback that renders this message never enters `LeaderConnect` itself.
fn advice_for(timings: &PhaseSnapshot, attempt: ConnectAttempt) -> Advice {
    let step = timings.longest_step().map(step_advice);
    let earlier = attempt.earlier();
    Advice {
        doing: step.map(|(doing, _)| doing),
        earlier: earlier.filter(|earlier| earlier.shaped_the_wait()),
        next_step: if earlier.is_some_and(|earlier| earlier.wedged_leader()) {
            NextStep::RestartSharedLeader
        } else {
            step.map_or(NextStep::Retry, |(_, next_step)| next_step)
        },
    }
}
impl Advice {
    fn explanation(&self) -> String {
        let locale = crate::locale::ctx();
        let mut explanation = match self.doing {
            Some(doing) => locale.tr_format("The longest step was {doing}.",
                &[("doing", doing)],
            ),
            None => locale
                .tr("No startup step had begun.")
                .into_owned(),
        };
        if let Some(earlier) = self.earlier {
            let target = agent_name(earlier.target).to_string();
            let _ = write!(
                explanation,
                " {}",
                locale.tr_format("Grok spent the first {seconds} on the {target}.",
                    &[
                        ("seconds", &whole_seconds(earlier.wait)),
                        ("target", &target)
                    ],
                )
            );
        }
        let _ = write!(explanation, " {}", self.next_step.text());
        if matches!(
            self.next_step,
            NextStep::Retry | NextStep::CheckNetworkThenRetry
        ) {
            let _ = write!(
                explanation,
                " {}",
                locale.named_text(
                    "startup_failure.slow_machine_hint",
                    "On a slow machine or network filesystem, a larger startup budget can help. \
                     Set it with the command below."
                )
            );
        }
        explanation
    }
}
fn format_steps(timings: &PhaseSnapshot) -> String {
    let completed = timings
        .completed
        .iter()
        .map(|&(phase, elapsed)| (phase, elapsed, ""));
    let open = timings.open.map(|(phase, elapsed)| (phase, elapsed, "+"));
    let steps: Vec<String> = completed
        .chain(open)
        .map(|(phase, elapsed, still_running)| {
            format!(
                "{}={}{still_running}",
                phase.label(),
                format_duration(elapsed)
            )
        })
        .collect();
    if steps.is_empty() {
        return "none".to_owned();
    }
    steps.join(", ")
}
/// Values hang under their label, so a wrapped one never reads as a new field.
fn label_rows(rows: &[(String, String)]) -> String {
    let column_width = rows
        .iter()
        .map(|(label, _)| label.len() + ":".len())
        .max()
        .unwrap_or(0);
    rows.iter()
        .map(|(label, value)| {
            let label = format!("  {:<column_width$} ", format!("{label}:"));
            let hanging = " ".repeat(label.len());
            fill_indented(value, &label, &hanging)
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn fill_indented(text: &str, initial_indent: &str, subsequent_indent: &str) -> String {
    textwrap::fill(
        text,
        textwrap::Options::new(WRAP_WIDTH)
            .initial_indent(initial_indent)
            .subsequent_indent(subsequent_indent)
            .break_words(false)
            .wrap_algorithm(textwrap::WrapAlgorithm::FirstFit),
    )
}
#[derive(Clone, Copy)]
enum NextStep {
    Retry,
    CheckNetworkThenRetry,
    RestartSharedLeader,
}
impl NextStep {
    fn text(self) -> &'static str {
        let locale = crate::locale::ctx();
        match self {
            Self::Retry => locale.tr_static("Start Grok again.",
            ),
            Self::CheckNetworkThenRetry => locale.tr_static("Check your network connection, then start Grok again.",
            ),
            Self::RestartSharedLeader => locale.named_static_text(
                "startup_failure.next_step.restart_leader",
                "Stop it with the command below, which also stops any other Grok \
                 session using it, then start Grok again.",
            ),
        }
    }
    /// Kept out of the prose so wrapping can never split it.
    fn command(self) -> Option<&'static str> {
        match self {
            Self::Retry | Self::CheckNetworkThenRetry => Some(CONNECT_UI_TIMEOUT_TRY_COMMAND),
            Self::RestartSharedLeader => Some("grok leader kill"),
        }
    }
}
/// Reads as the object of "The longest step was".
fn step_advice(phase: StartupPhase) -> (&'static str, NextStep) {
    use NextStep::{CheckNetworkThenRetry as Network, RestartSharedLeader, Retry};
    let locale = crate::locale::ctx();
    match phase {
        StartupPhase::ConfigLoad => (
            locale.tr_static("reading your local configuration",
            ),
            Retry,
        ),
        StartupPhase::ManagedPolicy => (
            locale.tr_static("checking your organization's managed policy",
            ),
            Network,
        ),
        StartupPhase::Bootstrap => (
            locale.tr_static("loading your account settings",
            ),
            Network,
        ),
        StartupPhase::ModelCatalog => (
            locale.tr_static("reading the list of available models",
            ),
            Retry,
        ),
        StartupPhase::WorkerSpawn => (
            locale.tr_static("starting the local agent",
            ),
            Retry,
        ),
        StartupPhase::LeaderConnect => (
            locale.tr_static("connecting to the shared leader",
            ),
            RestartSharedLeader,
        ),
        StartupPhase::AcpInitialize => (
            locale.tr_static("waiting for the agent to respond",
            ),
            Retry,
        ),
        StartupPhase::EagerAuth => (
            locale.tr_static("refreshing your sign-in",
            ),
            Network,
        ),
        StartupPhase::AppInit => (
            locale.tr_static("preparing the interface",
            ),
            Retry,
        ),
        StartupPhase::SessionCreate => (
            locale.tr_static("creating the session",
            ),
            Retry,
        ),
    }
}
fn attempted_agents(context: &Context) -> String {
    let target = agent_name(context.target);
    match context.attempt {
        ConnectAttempt::First => target.to_owned(),
        ConnectAttempt::AfterFallback(earlier) => crate::locale::ctx().tr_format("{first}, then {target}",
            &[("first", agent_name(earlier.target)), ("target", target)],
        ),
    }
}
fn agent_name(agent: AgentKind) -> &'static str {
    let locale = crate::locale::ctx();
    match agent {
        AgentKind::Embedded => {
            locale.tr_static("local agent")
        }
        AgentKind::Leader => {
            locale.tr_static("shared leader")
        }
    }
}
/// Rounded: a truncated total can print smaller than the steps it sums.
pub(super) fn whole_seconds(wait: Duration) -> String {
    format!("{}s", (wait.as_millis() + 500) / 1000)
}
