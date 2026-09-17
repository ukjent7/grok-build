//! Shared reasoning-effort dropdown levels for `/model` and `/effort`.

use xai_grok_shell::sampling::types::{ReasoningEffort, ReasoningEffortOption};

use crate::slash::command::ArgItem;

/// Effort levels in the built-in fallback menu (strongest first).
/// `none`/`minimal` are still accepted by `ReasoningEffort::from_str` for power users.
pub(crate) const EFFORT_LEVELS: &[ReasoningEffort] = &[
    ReasoningEffort::Xhigh,
    ReasoningEffort::High,
    ReasoningEffort::Medium,
    ReasoningEffort::Low,
];

pub(crate) fn effort_description(level: ReasoningEffort) -> &'static str {
    let ctx = crate::locale::ctx();
    match level {
        ReasoningEffort::None => ctx.tr_static("No reasoning"),
        ReasoningEffort::Minimal => ctx.tr_static("Minimal reasoning"),
        ReasoningEffort::Low => ctx.tr_static("Faster, lighter reasoning"),
        ReasoningEffort::Medium => ctx.tr_static("Balanced reasoning"),
        ReasoningEffort::High => ctx.tr_static("Heavy reasoning"),
        ReasoningEffort::Xhigh => ctx.tr_static("Extended reasoning"),
        ReasoningEffort::Max => ctx.tr_static("Maximum reasoning"),
    }
}

/// The built-in menu used when the server sends no `reasoningEfforts`.
/// Reproduces the historical rows: labels are the lowercase level (via `Display`), descriptions from `effort_description`.
/// The active row is matched by value against the session effort at render time, so `default` is left unset here.
pub(crate) fn legacy_effort_options() -> Vec<ReasoningEffortOption> {
    EFFORT_LEVELS
        .iter()
        .map(|&level| ReasoningEffortOption {
            id: level.as_ref().to_string(),
            value: level,
            label: level.to_string(),
            description: Some(effort_description(level).to_string()),
            default: false,
        })
        .collect()
}

/// Build effort rows for autocomplete from a per-model option list. `match_text` gets an `a `/`b `/…` sort prefix
/// so the matcher's alphabetical tiebreak preserves the option order.
pub(crate) fn build_effort_arg_items(
    options: &[ReasoningEffortOption],
    current_effort: Option<ReasoningEffort>,
    mark_active: bool,
    insert_text_for: impl Fn(&ReasoningEffortOption) -> String,
) -> Vec<ArgItem> {
    options
        .iter()
        .enumerate()
        .map(|(idx, option)| {
            let active = mark_active && current_effort == Some(option.value);
            let active_suffix = if active {
                crate::locale::ctx().tr_static(" (active)")
            } else {
                ""
            };
            let insert_text = insert_text_for(option);
            // Sort-key prefix: 'a' for top row, 'b' for next, etc
            // Only affects matcher tiebreak ordering, never rendered
            let sort_prefix = char::from(b'a' + idx as u8);
            ArgItem {
                display: format!("{}{active_suffix}", option.label),
                match_text: format!("{sort_prefix} {insert_text}"),
                insert_text,
                description: option.description.clone().unwrap_or_default(),
            }
        })
        .collect()
}
