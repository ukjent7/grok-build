use crate::app::ScreenMode;

/// What to tell a user who typed a command the current mode cannot run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Remedy {
    SwitchMode {
        /// Sentence fragment, parenthesized in the refusal: `"minimal is single-session"`.
        why: &'static str,
    },
    /// Imperative clause naming what to do in this mode instead.
    /// Name arrows, `Tab`, or `Ctrl+<letter>`.
    /// `Ctrl+G` is the external editor in minimal and the tasks pane everywhere else, and a bare letter resolves only under vim mode (off by default).
    UseInstead(&'static str),
    AlreadyInMode,
}

/// Which render modes a slash command functions in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeSupport {
    Both,
    FullscreenOnly(Remedy),
    MinimalOnly(Remedy),
}

impl ModeSupport {
    pub(crate) fn supports(self, mode: ScreenMode) -> bool {
        match self {
            Self::Both => true,
            Self::FullscreenOnly(_) => !mode.is_minimal(),
            Self::MinimalOnly(_) => mode.is_minimal(),
        }
    }

    pub(crate) fn refusal(self, token: &str, mode: ScreenMode) -> Option<String> {
        if self.supports(mode) {
            return None;
        }
        let (remedy, current, switch) = match self {
            Self::Both => return None,
            Self::FullscreenOnly(remedy) => (remedy, "minimal", "/fullscreen"),
            Self::MinimalOnly(remedy) => (remedy, "fullscreen", "/minimal"),
        };
        Some(match remedy {
            Remedy::SwitchMode { why } => crate::locale::ctx().format_named(
                "slash.mode.error.switch",
                "/{token} isn't available in {current} mode ({why}). \
                 Run {switch} to switch this session.",
                &[
                    ("token", token),
                    ("current", current),
                    ("why", why),
                    ("switch", switch),
                ],
            ),
            Remedy::UseInstead(instead) => crate::locale::ctx().format_named(
                "slash.mode.error.use_instead",
                "/{token} isn't available in {current} mode: {instead}.",
                &[("token", token), ("current", current), ("instead", instead)],
            ),
            Remedy::AlreadyInMode => crate::locale::ctx().format_named(
                "slash.mode.error.already_in_mode",
                "You're already in {current} mode.",
                &[("current", current)],
            ),
        })
    }
}

#[cfg(test)]
#[path = "mode_support_tests.rs"]
mod tests;
