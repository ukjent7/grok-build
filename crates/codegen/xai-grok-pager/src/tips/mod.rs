//! Ephemeral tips: one hint line at a time, rendered in the banner rect above the prompt input and cleared after a TTL.
//!
//! Unlike the toast, an ephemeral tip survives typing: only TTL expiry, prompt-box submission, or an explicit clear removes it.
//! A tip that carries a seen-count key stops appearing once `AppView::tip_seen_counts` says it has shown often enough this run.
//! That map is in-memory only, so the counts reset every run.
//!
//! ## Localized copy
//!
//! A tip that embeds a key chord or a path is spliced from several catalog-backed
//! fragments (`tip.ephemeral.*.lead` / `.middle` / `.tail`) around the styled dynamic
//! token, because each piece carries a different style. The fragment *order* therefore
//! lives in code and the catalogs only supply words: translating a fragment cannot
//! move it. zh-CN's and en-US's word order around these tokens happen to agree, but a
//! locale that needs the token in a different position must give that tip its own
//! assembly (or one template plus a styled-span splitter) rather than a translation.
//! `scripts/i18n/wrap-check.py` proves every fragment id has a translation; it cannot
//! prove the fragments compose into a grammatical sentence.

pub mod clear_detector;
pub mod clipboard_focus;
pub mod ephemeral;
pub mod export_copy;
pub mod plan_nudge;
pub mod render;
pub mod send_now;
pub mod small_screen;
pub mod ssh_wrap;
pub mod word_select;

pub use ephemeral::{DEFAULT_TIP_TICKS, EphemeralTip, EphemeralTipState, tip_row_renderable};
