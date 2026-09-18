#![allow(
    unused_imports,
    unused_variables,
    unused_mut,
    unreachable_code,
    dead_code
)]
//! xai-grok-pager: Grok Build TUI.
//!
//! A clean-room implementation built on the v3 pager rendering engine.
#![deny(clippy::indexing_slicing)]
pub mod acp;
pub mod actions;
pub mod agent_runtime;
pub mod app;
pub mod best_effort_stderr;
pub mod client_identity;
pub mod completions_cmd;
mod config_toml_edit;
pub mod diagnostics;
pub mod disk_usage_cmd;
pub mod docs;
pub mod doctor_cmd;
pub mod export_cmd;
pub(crate) mod fs_size;
pub mod git_info;
pub mod headless;
pub mod hyperlink_route;
pub mod inline_media_ffmpeg;
pub mod input_log;
pub mod mcp_cmd;
pub mod memory_cmd;
pub mod memory_release;
pub mod memory_trace;
#[path = "minimal/api.rs"]
pub mod minimal_api;
#[path = "minimal/hook.rs"]
pub mod minimal_hook;
pub mod models;
pub mod notifications;
#[allow(unused_imports, unused_macros)]
pub mod obf;
pub mod plugin_cmd;
pub mod pty_wrap;
pub mod recent_dirs;
pub mod scrollback;
pub mod sessions_cmd;
pub mod settings;
pub mod share_cmd;
pub mod slash;
pub mod startup;
pub mod tips;
pub mod tool_usage;
pub mod tutorial_docs;
pub mod usage_cmd;
pub mod wrap_clipboard_image;
pub mod wrap_cmd;
pub(crate) mod wrap_filter;
pub(crate) mod wrap_restore;
pub use xai_grok_gboom as gboom;
pub use xai_grok_locale as locale;
pub use xai_grok_pager_render::key;
pub use xai_grok_pager_render::{
    appearance, clipboard, glyphs, host, input, link_opener, modal_window_state, prompt_images,
    render, search, syntax, terminal, theme, util,
};

/// Resolve the UI locale from user `config.toml` (`[ui].locale`) once at the
/// composition root and publish it process-wide. Values outside the `en`/`zh`
/// families fall back to the upstream English interface.
pub fn init_locale_from_config() {
    let config = xai_grok_config::load_from_disk().ok();
    let configured = config.as_ref().and_then(|value| {
        value
            .get("ui")?
            .get("locale")?
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    });
    locale::init(locale::LocaleContext::new(
        configured
            .and_then(locale::UiLocale::parse)
            .unwrap_or_default(),
    ));
}
#[cfg(test)]
pub mod test_util;
pub mod trace_cmd;
pub mod tracing;
pub mod unified_log;
pub mod views;
pub mod voice;
pub mod worktree_cmd;
