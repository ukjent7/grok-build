use crate::clipboard::{ClipboardDelivery, NativeClipboardPreflight};
use crate::diagnostics::{
    DataControlFact, DiagnosticFinding, DiagnosticReport, FindingDisposition, NewlineFact,
    ProbeStatus, RuntimeFact, VoiceFacts,
};
use crate::host::{DisplayServer, HostOs};

fn live_tui_probe_cta() -> &'static str {
    crate::locale::ctx().named_static_text(
        "doctor.live_tui_cta",
        "Some checks only run in Grok. Start Grok and run /doctor.",
    )
}

pub(super) fn format(report: &DiagnosticReport) -> String {
    let facts = &report.facts;
    let mut out = format!(
        "{}\n\n{}\n",
        crate::locale::ctx().named_text("doctor.title", "Grok Doctor"),
        crate::locale::ctx().named_text("doctor.section.environment", "Environment"),
    );

    fact(&mut out, "terminal", &facts.terminal.to_string());
    match &facts.xtversion {
        RuntimeFact::Available(value) => fact(&mut out, "terminal version", value),
        RuntimeFact::NoReply => unavailable(&mut out, "terminal version", "no reply"),
        RuntimeFact::Unavailable => unavailable(&mut out, "terminal version", "unavailable"),
    }
    fact(&mut out, "multiplexer", &facts.multiplexer.to_string());
    if let Some(byobu) = facts.byobu {
        fact(&mut out, "byobu", &byobu.to_string());
    }
    fact(&mut out, "ssh", if facts.ssh { "yes" } else { "no" });
    match &facts.color.level {
        RuntimeFact::Available(level) => {
            fact(&mut out, "color", level.as_ref());
            let themes = if facts.color.available_themes.len() == facts.color.total_themes {
                "all".to_owned()
            } else {
                format!(
                    "{}/{}: {}",
                    facts.color.available_themes.len(),
                    facts.color.total_themes,
                    facts
                        .color
                        .available_themes
                        .iter()
                        .map(|theme| theme.display_name())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            fact(&mut out, "themes", &themes);
        }
        RuntimeFact::NoReply | RuntimeFact::Unavailable => {
            unavailable(&mut out, "color", "unavailable");
            unavailable(&mut out, "themes", "unavailable");
        }
    }

    if let Some(keyboard) = &facts.keyboard {
        let rescue = if keyboard.os == HostOs::Macos {
            crate::locale::ctx()
                .named_text("doctor.keyboard.rescue_active", "OS rescue active")
                .into_owned()
        } else {
            crate::locale::ctx()
                .named_text(
                    "doctor.keyboard.rescue_unavailable",
                    "OS rescue unavailable on this platform",
                )
                .into_owned()
        };
        fact(
            &mut out,
            "keyboard",
            &format!("{} ({rescue})", keyboard.modifier_delivery.label()),
        );
    }
    if let Some(newline) = &facts.newline {
        fact(&mut out, "newline", &format_newline(newline));
    }

    let clipboard = &facts.clipboard;
    let native = match clipboard.native_preflight {
        NativeClipboardPreflight::LocalAvailable => {
            format!("local ({})", clipboard.native_tool)
        }
        NativeClipboardPreflight::RemoteOnly if clipboard.container_no_display => {
            format!("container ({})", clipboard.native_tool)
        }
        NativeClipboardPreflight::RemoteOnly => format!("remote ({})", clipboard.native_tool),
        NativeClipboardPreflight::Unavailable => "unavailable".to_owned(),
        NativeClipboardPreflight::Disabled => "off".to_owned(),
    };
    out.push_str(&format!(
        "\n{}\n",
        crate::locale::ctx().named_text("doctor.section.clipboard", "Clipboard")
    ));
    fact(&mut out, "native", &native);
    fact(
        &mut out,
        "tmux",
        if clipboard.tmux_route { "on" } else { "off" },
    );
    fact(
        &mut out,
        "osc 52",
        if clipboard.osc52_route {
            clipboard.osc52_capability.label()
        } else {
            "off"
        },
    );
    fact(
        &mut out,
        "SSH wrap",
        if clipboard.wrap_sink { "on" } else { "off" },
    );
    if clipboard.display_server == DisplayServer::Wayland {
        match clipboard.data_control {
            DataControlFact::Available => fact(&mut out, "data-control", "on"),
            DataControlFact::Missing => fact(&mut out, "data-control", "off"),
            DataControlFact::Unavailable => unavailable(&mut out, "data-control", "unavailable"),
            DataControlFact::Error => {
                let detail = report
                    .probe_notes
                    .iter()
                    .find(|note| note.probe == "wayland.data-control")
                    .and_then(|note| note.message.as_deref());
                match detail {
                    Some(message) => {
                        unavailable(&mut out, "data-control", &format!("error: {message}"))
                    }
                    None => unavailable(&mut out, "data-control", "error"),
                }
            }
            DataControlFact::NotApplicable => {}
        }
    }
    let status = match clipboard.delivery {
        ClipboardDelivery::Confirmed => "confirmed",
        ClipboardDelivery::Unverified => "unverified",
        ClipboardDelivery::Failed => "unavailable",
    };
    fact(&mut out, "status", status);

    if let Some(voice) = &facts.voice {
        out.push_str(&format!(
            "\n{}\n",
            crate::locale::ctx().named_text("doctor.section.voice", "Voice")
        ));
        match voice {
            VoiceFacts::Device { name, detail } => {
                fact(&mut out, "microphone", &format!("{name} ({detail})"));
            }
            VoiceFacts::Missing { error } => {
                fact(&mut out, "microphone", &format!("none detected ({error})"));
            }
        }
    }

    if !report.findings.is_empty() {
        out.push_str(&format!(
            "\n{}\n",
            crate::locale::ctx().named_text("doctor.section.findings", "Findings")
        ));
        for finding in &report.findings {
            format_finding(&mut out, finding);
        }
    }

    let visible_notes = report
        .probe_notes
        .iter()
        .filter(|note| !fact_already_shows_probe(note.probe));
    let mut notes = visible_notes.peekable();
    if notes.peek().is_some() {
        out.push_str(&format!(
            "\n{}\n",
            crate::locale::ctx().named_text(
                "doctor.section.checks_not_completed",
                "Checks not completed"
            )
        ));
        for note in notes {
            let message = match &note.message {
                Some(message) => format!("{}: {message}", probe_status(note.status)),
                None => probe_status(note.status).to_owned(),
            };
            row(&mut out, "?", note.probe, &message);
        }
    }

    if report
        .probe_notes
        .iter()
        .any(crate::diagnostics::probe_requires_live_tui)
    {
        out.push_str(&format!(
            "\n{}\n",
            crate::locale::ctx().named_text(
                "doctor.section.needs_running_session",
                "Needs a running session"
            )
        ));
        out.push_str(&format!("  {}\n", live_tui_probe_cta()));
    }

    let issues = report.issue_count();
    let recommendations = report.recommendation_count();
    out.push('\n');
    let issue_word = if issues == 1 { "issue" } else { "issues" };
    let recommendation_word = if recommendations == 1 {
        "recommendation"
    } else {
        "recommendations"
    };
    out.push_str(&crate::locale::ctx().format_named(
        "doctor.summary.counts",
        "{issues} {issue}, {recommendations} {recommendation}",
        &[
            ("issues", &issues.to_string()),
            ("issue", &issue_word.to_string()),
            ("recommendations", &recommendations.to_string()),
            ("recommendation", &recommendation_word.to_string()),
        ],
    ));
    out.push('\n');
    out
}

fn fact_already_shows_probe(probe: &str) -> bool {
    matches!(
        probe,
        "runtime.xtversion" | "terminal.color" | "wayland.data-control"
    )
}

fn fact(out: &mut String, label: &str, value: &str) {
    row(out, "·", label, value);
}

fn unavailable(out: &mut String, label: &str, value: &str) {
    row(out, "?", label, value);
}

fn row(out: &mut String, marker: &str, label: &str, value: &str) {
    out.push_str(&format!("  {marker} {label:<28} {value}\n"));
}

fn format_finding(out: &mut String, finding: &DiagnosticFinding) {
    let marker = match finding.disposition {
        FindingDisposition::Issue => "!",
        FindingDisposition::Recommendation => "i",
    };
    row(out, marker, &finding.id.to_string(), &finding.message);
    let locale = crate::locale::ctx();
    if let Some(automatic) = finding.automatic_remediation {
        let command = crate::diagnostics::human_fix_command(automatic.fix_id)
            .unwrap_or_else(|| automatic.command.to_owned());
        out.push_str(&format!(
            "    → {}\n",
            locale.format_named(
                "doctor.finding.automatic_setup",
                "Automatic setup: `{command}`",
                &[("command", &command)],
            )
        ));
    }
    if let Some(remediation) = &finding.remediation {
        let instruction = match (&remediation.config_path, &finding.automatic_remediation) {
            (Some(path), _) => locale.format_named(
                "doctor.finding.add_to_path",
                "Add `{fix}` to {path}",
                &[("fix", &remediation.fix), ("path", path)],
            ),
            (None, Some(_)) => locale.format_named(
                "doctor.finding.one_off",
                "One-off: `{fix}`",
                &[("fix", &remediation.fix)],
            ),
            (None, None) => locale.format_named(
                "doctor.finding.run",
                "Run: `{fix}`",
                &[("fix", &remediation.fix)],
            ),
        };
        out.push_str(&format!("    → {instruction}\n"));
    }
    if let Some(note) = &finding.note {
        out.push_str(&format!("      {note}\n"));
    }
}

fn format_newline(newline: &NewlineFact) -> String {
    let locale = crate::locale::ctx();
    let detail = match newline {
        NewlineFact::Vte {
            version: Some(version),
        } => locale.format_named(
            "doctor.newline.vte_version",
            "VTE {version}; need >= 8200 for Shift+Enter",
            &[("version", version)],
        ),
        NewlineFact::Vte { version: None } => locale
            .named_text(
                "doctor.newline.vte_legacy",
                "legacy VTE; need VTE >= 0.82 for Shift+Enter",
            )
            .into_owned(),
        NewlineFact::XtermJs { terminal } => locale.format_named(
            "doctor.newline.xterm_js",
            "{terminal}: xterm.js cannot distinguish Shift+Enter",
            &[("terminal", &terminal.to_string())],
        ),
        NewlineFact::NoKittyKeyboardProtocol => locale
            .named_text(
                "doctor.newline.no_kitty_protocol",
                "no Kitty keyboard protocol; Shift+Enter equals Enter",
            )
            .into_owned(),
    };
    locale.format_named("doctor.newline.alt_enter", "Alt+Enter ({detail})", &[("detail", &detail)])
}

fn probe_status(status: ProbeStatus) -> &'static str {
    match status {
        ProbeStatus::Unsupported => "unsupported",
        ProbeStatus::Unavailable => "unavailable",
        ProbeStatus::Error => "error",
    }
}
