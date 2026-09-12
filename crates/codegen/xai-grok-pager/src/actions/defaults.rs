//! Default action definitions for the MVP.
//!
//! All key bindings are defined here, not scattered across event handlers.

use crate::key;
use crate::terminal::{TerminalName, terminal_context};

use super::{ActionDef, ActionId, Category, When};

/// True when `Ctrl+.` is not a reliable primary key for the shortcuts cheatsheet. Both keys stay registered either
/// way; this only chooses which the UI advertises.
pub fn ctrl_dot_unreliable() -> bool {
    terminal_context().ctrl_dot_unreliable() || cfg!(target_os = "windows") || crate::host::is_wsl()
}

/// Choose the one agent-screen action that owns Ctrl+G for this mode.
fn mode_ctrl_g_action(screen_mode: crate::app::ScreenMode) -> ActionDef {
    if screen_mode.is_minimal() {
        ActionDef {
            id: ActionId::EditPromptExternal,
            label: crate::locale::ctx().named_static_text("shortcuts.action.EditPromptExternal.label", "edit prompt"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.EditPromptExternal.description", "Edit prompt in external editor"),
            default_key: key!('g', CONTROL),
            alt_keys: vec![],
            category: Category::Input,
            context: When::AgentScreen,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.EditPromptExternal.long_help",
                    "Opens the current prompt draft in $VISUAL or $EDITOR, falling back to vi when neither is set.\nSaving and closing the editor returns the updated text to the composer; it does not send the prompt.\nAvailable in minimal mode for ordinary attachment-free drafts.",
                )
            ),
        }
    } else {
        ActionDef {
            id: ActionId::ToggleTasks,
            label: crate::locale::ctx().named_static_text("shortcuts.action.ToggleTasks.label", "tasks"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.ToggleTasks.description", "Toggle tasks pane"),
            default_key: key!('g', CONTROL),
            alt_keys: vec![],
            category: Category::Panels,
            context: When::AgentScreen,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.ToggleTasks.long_help",
                    "Shows or hides the tasks pane, which lists background tasks and their status.\nUse it to monitor or return to work you sent to the background with Ctrl+B.\nA side pane; toggle off to reclaim width.",
                )
            ),
        }
    }
}

/// Build the default action definitions for a screen mode.
///
/// `mouse_reporting_toggle_enabled` gates the opt-in `ToggleMouseCapture` shortcut (see below); pass `false` for the standard set.
pub(super) fn default_actions(
    screen_mode: crate::app::ScreenMode,
    mouse_reporting_toggle_enabled: bool,
) -> Vec<ActionDef> {
    let ctx = terminal_context();
    // xterm.js embeds have no KKP and the host often steals Ctrl+I
    // Share one family flag for quit / half-page / interject so VS Code-family embeds match VS Code
    let in_vscode_family = ctx.brand.is_vscode_family();
    let in_vscode = in_vscode_family;
    let in_apple_terminal = ctx.brand == TerminalName::AppleTerminal;
    // Shared by ToggleQueue (Ctrl+4 primary) and OpenDashboard (omit Ctrl+4 alt).
    let local_mac_vscode = in_vscode_family && !ctx.is_ssh && cfg!(target_os = "macos");
    let ctrl_dot_unreliable = ctrl_dot_unreliable();
    let send_to_background_help = if screen_mode.is_minimal() {
        crate::locale::ctx().named_static_text(
            "shortcuts.action.SendToBackground.long_help",
            "Detaches the running foreground Execute so it keeps working in the background while you read, queue prompts, or start something else.\nTrack background work with /tasks.\nOnly meaningful while a foreground Execute is actually running.",
        )
    } else {
        crate::locale::ctx().named_static_text(
            "shortcuts.action.SendToBackground.long_help",
            "Detaches the running foreground Execute so it keeps working in the background while you read, queue prompts, or start something else.\nTrack and resume it from the tasks pane (Ctrl+G).\nOnly meaningful while a foreground Execute is actually running.",
        )
    };

    let mut actions = vec![
        // ── Navigation (scrollback) ─────────────────────────────────
        ActionDef {
            id: ActionId::SelectNext,
            label: crate::locale::ctx().named_static_text("shortcuts.action.SelectNext.label", "nav"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.SelectNext.description", "Select next entry"),
            default_key: key!('j'),
            alt_keys: vec![key!(Down)],
            category: Category::ConversationNav,
            context: When::ScrollbackFocused,
            hint_priority: Some(0),
            hint_key_display: Some("j/k"),
            requires_confirmation: false,
            long_help: None,
        },
        ActionDef {
            id: ActionId::SelectPrev,
            label: crate::locale::ctx().named_static_text("shortcuts.action.SelectPrev.label", "nav"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.SelectPrev.description", "Select previous entry"),
            default_key: key!('k'),
            alt_keys: vec![key!(Up)],
            category: Category::ConversationNav,
            context: When::ScrollbackFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: None,
        },
        ActionDef {
            id: ActionId::NextTurn,
            label: crate::locale::ctx().named_static_text("shortcuts.action.NextTurn.label", "turn"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.NextTurn.description", "Next turn"),
            default_key: key!('L'),
            alt_keys: vec![key!(Right, SHIFT)],
            category: Category::ConversationNav,
            context: When::ScrollbackFocused,
            hint_priority: Some(1),
            hint_key_display: None,
            requires_confirmation: false,
            long_help: None,
        },
        ActionDef {
            id: ActionId::PrevTurn,
            label: crate::locale::ctx().named_static_text("shortcuts.action.PrevTurn.label", "turn"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.PrevTurn.description", "Previous turn"),
            default_key: key!('H'),
            alt_keys: vec![key!(Left, SHIFT)],
            category: Category::ConversationNav,
            context: When::ScrollbackFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: None,
        },
        ActionDef {
            id: ActionId::NextResponse,
            label: crate::locale::ctx().named_static_text("shortcuts.action.NextResponse.label", "turn"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.NextResponse.description", "Jump to next turn at viewport top"),
            default_key: key!('J'),
            alt_keys: vec![],
            category: Category::ConversationNav,
            context: When::ScrollbackFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: None,
        },
        ActionDef {
            id: ActionId::PrevResponse,
            label: crate::locale::ctx().named_static_text("shortcuts.action.PrevResponse.label", "turn"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.PrevResponse.description", "Jump to previous turn at viewport top"),
            default_key: key!('K'),
            alt_keys: vec![],
            category: Category::ConversationNav,
            context: When::ScrollbackFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: None,
        },
        ActionDef {
            id: ActionId::GotoTop,
            label: crate::locale::ctx().named_static_text("shortcuts.action.GotoTop.label", "top/btm"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.GotoTop.description", "Go to top"),
            default_key: key!('g'),
            alt_keys: vec![],
            category: Category::ConversationNav,
            context: When::ScrollbackFocused,
            hint_priority: Some(4),
            hint_key_display: None,
            requires_confirmation: false,
            long_help: None,
        },
        ActionDef {
            id: ActionId::GotoBottom,
            label: crate::locale::ctx().named_static_text("shortcuts.action.GotoBottom.label", "bottom"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.GotoBottom.description", "Go to bottom"),
            default_key: key!('G'),
            alt_keys: vec![],
            category: Category::ConversationNav,
            context: When::ScrollbackFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: None,
        },
        ActionDef {
            id: ActionId::ScrollUp,
            label: crate::locale::ctx().named_static_text("shortcuts.action.ScrollUp.label", "scroll up"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.ScrollUp.description", "Scroll up one line"),
            default_key: key!('k', CONTROL),
            alt_keys: vec![],
            category: Category::ConversationNav,
            context: When::ScrollbackFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: None,
        },
        ActionDef {
            id: ActionId::ScrollDown,
            label: crate::locale::ctx().named_static_text("shortcuts.action.ScrollDown.label", "scroll down"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.ScrollDown.description", "Scroll down one line"),
            default_key: key!('j', CONTROL),
            alt_keys: vec![],
            category: Category::ConversationNav,
            context: When::ScrollbackFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: None,
        },
        ActionDef {
            id: ActionId::HalfPageUp,
            label: crate::locale::ctx().named_static_text("shortcuts.action.HalfPageUp.label", "half page up"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.HalfPageUp.description", "Scroll up half page"),
            default_key: key!('u', CONTROL),
            alt_keys: vec![],
            category: Category::ConversationNav,
            context: When::ScrollbackFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: None,
        },
        ActionDef {
            id: ActionId::HalfPageDown,
            label: crate::locale::ctx().named_static_text("shortcuts.action.HalfPageDown.label", "half page down"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.HalfPageDown.description", "Scroll down half page"),
            default_key: if in_vscode {
                key!('D')
            } else {
                key!('d', CONTROL)
            },
            alt_keys: vec![],
            category: Category::ConversationNav,
            context: When::ScrollbackFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: None,
        },
        ActionDef {
            id: ActionId::PageUp,
            label: crate::locale::ctx().named_static_text("shortcuts.action.PageUp.label", "page up"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.PageUp.description", "Scroll up one page"),
            default_key: key!(PageUp),
            alt_keys: vec![],
            category: Category::ConversationNav,
            context: When::ScrollbackFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: None,
        },
        ActionDef {
            id: ActionId::PageDown,
            label: crate::locale::ctx().named_static_text("shortcuts.action.PageDown.label", "page down"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.PageDown.description", "Scroll down one page"),
            default_key: key!(PageDown),
            alt_keys: vec![],
            category: Category::ConversationNav,
            context: When::ScrollbackFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: None,
        },
        // ── View (scrollback) ───────────────────────────────────────
        ActionDef {
            id: ActionId::Collapse,
            label: crate::locale::ctx().named_static_text("shortcuts.action.Collapse.label", "fold"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.Collapse.description", "Collapse selected entry"),
            default_key: key!('h'),
            alt_keys: vec![key!(Left)],
            category: Category::ConversationAction,
            context: When::ScrollbackFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: None,
        },
        ActionDef {
            id: ActionId::Expand,
            label: crate::locale::ctx().named_static_text("shortcuts.action.Expand.label", "fold"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.Expand.description", "Expand selected entry"),
            default_key: key!('l'),
            alt_keys: vec![key!(Right)],
            category: Category::ConversationAction,
            context: When::ScrollbackFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: None,
        },
        ActionDef {
            id: ActionId::ToggleFold,
            label: crate::locale::ctx().named_static_text("shortcuts.action.ToggleFold.label", "fold"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.ToggleFold.description", "Expand / collapse"),
            default_key: key!('e'),
            alt_keys: vec![],
            category: Category::ConversationAction,
            context: When::ScrollbackFocused,
            hint_priority: Some(3),
            hint_key_display: None,
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.ToggleFold.long_help",
                    "Folds or unfolds the selected scrollback entry to hide or show its full body.\nHandy for skimming long tool output or reasoning.\nRelated: E folds/unfolds every entry, Ctrl+E toggles all thinking blocks.",
                )
            ),
        },
        ActionDef {
            id: ActionId::ToggleExpandAll,
            label: crate::locale::ctx().named_static_text("shortcuts.action.ToggleExpandAll.label", "all"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.ToggleExpandAll.description", "Expand all / collapse all"),
            default_key: key!('E'),
            alt_keys: vec![],
            category: Category::ConversationAction,
            context: When::ScrollbackFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.ToggleExpandAll.long_help",
                    "Folds or unfolds every scrollback entry at once, unlike e which toggles only the selected row.\nCollapse a long transcript to scan headers, then expand it all back.\nThinking blocks have their own toggle, Ctrl+E.",
                )
            ),
        },
        ActionDef {
            id: ActionId::ExpandAllThinking,
            label: crate::locale::ctx().named_static_text("shortcuts.action.ExpandAllThinking.label", "expand/collapse thinking"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.ExpandAllThinking.description", "Toggle all thinking blocks"),
            default_key: key!('e', CONTROL),
            alt_keys: vec![],
            category: Category::ConversationAction,
            context: When::ScrollbackFocused,
            hint_priority: Some(3),
            hint_key_display: None,
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.ExpandAllThinking.long_help",
                    "Shows or hides the agent's reasoning (thinking) blocks across the whole transcript in one keypress.\nReveal how the agent reached an answer, or hide reasoning to focus on results.\nSeparate from E, which folds every entry regardless of type.",
                )
            ),
        },
        ActionDef {
            id: ActionId::ToggleRaw,
            label: crate::locale::ctx().named_static_text("shortcuts.action.ToggleRaw.label", "raw"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.ToggleRaw.description", "Toggle raw markdown"),
            default_key: key!('r'),
            alt_keys: vec![],
            category: Category::ConversationAction,
            context: When::ScrollbackFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.ToggleRaw.long_help",
                    "Switches the selected entry between rendered markdown and its raw source text.\nUse it to copy exact markdown, inspect a link target, or see formatting the renderer hides.\nPress again to return to the rendered view.",
                )
            ),
        },
        // ── Block content ────────────────────────────────────────────
        ActionDef {
            id: ActionId::CopyBlockContent,
            label: crate::locale::ctx().named_static_text("shortcuts.action.CopyBlockContent.label", "copy"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.CopyBlockContent.description", "Copy content"),
            default_key: key!('y'),
            alt_keys: vec![],
            category: Category::ConversationAction,
            context: When::ScrollbackFocused,
            hint_priority: None, // shown dynamically when block supports copy
            hint_key_display: None,
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.CopyBlockContent.long_help",
                    "Copies the selected block's body to the clipboard: message text, full tool output, or a code block's contents.\nOffered only on blocks that support copy.\nFor just the command or file path, use Y instead.",
                )
            ),
        },
        ActionDef {
            id: ActionId::CopyBlockMeta,
            label: crate::locale::ctx().named_static_text("shortcuts.action.CopyBlockMeta.label", "copy cmd"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.CopyBlockMeta.description", "Copy command / path"),
            default_key: key!('Y'),
            alt_keys: vec![],
            category: Category::ConversationAction,
            context: When::ScrollbackFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.CopyBlockMeta.long_help",
                    "Copies only the block's identifier: a tool call's command line or a file block's path, not the body.\nHandy to re-run a command or paste a path elsewhere.\nUse lowercase y to copy the full content instead.",
                )
            ),
        },
        ActionDef {
            id: ActionId::OpenBlockViewer,
            label: crate::locale::ctx().named_static_text("shortcuts.action.OpenBlockViewer.label", "view"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.OpenBlockViewer.description", "Open in viewer"),
            default_key: key!(Enter),
            alt_keys: vec![key!('f', CONTROL)],
            category: Category::ConversationAction,
            context: When::ScrollbackFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.OpenBlockViewer.long_help",
                    "Opens the selected block in a focused, scrollable full-screen viewer.\nBest for long tool output, large files, or code you want to read away from the surrounding transcript.\nEsc returns to the conversation.",
                )
            ),
        },
        // ── Link navigation ─────────────────────────────────────────
        ActionDef {
            id: ActionId::OpenNextLink,
            label: crate::locale::ctx().named_static_text("shortcuts.action.OpenNextLink.label", "link"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.OpenNextLink.description", "Next link"),
            default_key: key!('o'),
            alt_keys: vec![],
            category: Category::ConversationNav,
            context: When::ScrollbackFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: None,
        },
        ActionDef {
            id: ActionId::OpenPrevLink,
            label: crate::locale::ctx().named_static_text("shortcuts.action.OpenPrevLink.label", "link"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.OpenPrevLink.description", "Previous link"),
            default_key: key!('O'),
            alt_keys: vec![],
            category: Category::ConversationNav,
            context: When::ScrollbackFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: None,
        },
        // ── Scrollback (contextual, block-type-dependent) ────────────
        ActionDef {
            id: ActionId::Rewind,
            label: crate::locale::ctx().named_static_text("shortcuts.action.Rewind.label", "rewind"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.Rewind.description", "Rewind to selected turn"),
            default_key: key!(Null),
            alt_keys: vec![],
            category: Category::ConversationAction,
            context: When::ScrollbackFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.Rewind.long_help",
                    "Rewinds the conversation to an earlier turn, discarding later turns. File changes made after that turn are left as-is.\nPick a turn from the list; a running turn is offered for cancel first. When Confirm before rewind is on (default), each pick asks Yes / Yes, and don't ask again / No. Picking \"Yes, and don't ask again\" turns the setting off in /settings.\nDestructive: later turns are dropped.\nAlso reachable idle with an empty prompt via Esc Esc (within 800ms), same as `/rewind`.",
                )
            ),
        },
        ActionDef {
            id: ActionId::KillBgTask,
            label: crate::locale::ctx().named_static_text("shortcuts.action.KillBgTask.label", "kill"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.KillBgTask.description", "Kill background task"),
            default_key: key!('x'),
            alt_keys: vec![],
            category: Category::ConversationAction,
            context: When::ScrollbackFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.KillBgTask.long_help",
                    "Terminates the background task owned by the selected task block (e.g. a long shell command sent to the background).\nReach for it to stop a runaway or no-longer-needed process.\nApplies only to a live task; finished ones are unaffected.",
                )
            ),
        },
        // ── Essentials ────────────────────────────────────────────────
        ActionDef {
            id: ActionId::SendPrompt,
            label: crate::locale::ctx().named_static_text("shortcuts.action.SendPrompt.label", "send"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.SendPrompt.description", "Send"),
            default_key: key!(Enter),
            alt_keys: vec![],
            category: Category::GettingStarted,
            context: When::PromptFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: None,
        },
        ActionDef {
            id: ActionId::FocusPrompt,
            label: crate::locale::ctx().named_static_text("shortcuts.action.FocusPrompt.label", "prompt"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.FocusPrompt.description", "Focus prompt"),
            default_key: key!(Tab),
            alt_keys: vec![key!('i'), key!(' ')],
            category: Category::GettingStarted,
            context: When::ScrollbackFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: None,
        },
        ActionDef {
            id: ActionId::FocusScrollback,
            label: crate::locale::ctx().named_static_text("shortcuts.action.FocusScrollback.label", "scrollback"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.FocusScrollback.description", "Focus scrollback"),
            default_key: key!(Tab),
            alt_keys: vec![],
            category: Category::GettingStarted,
            context: When::PromptFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.FocusScrollback.long_help",
                    "Moves focus from the prompt to the scrollback so you can navigate the transcript.\nTab works in both simple and vim scrollback modes.\nEsc is reserved for the cancel / clear / rewind policy, not focus.",
                )
            ),
        },
        ActionDef {
            id: ActionId::CancelTurn,
            label: crate::locale::ctx().named_static_text("shortcuts.action.CancelTurn.label", "cancel"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.CancelTurn.description", "Cancel turn"),
            default_key: key!('c', CONTROL),
            alt_keys: vec![],
            category: Category::GettingStarted,
            context: When::AgentScreen,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.CancelTurn.long_help",
                    "Interrupts the agent's current turn and stops generation, keeping the session open.\nCtrl+C cancels when the prompt is empty; with a non-empty draft it clears the prompt first and leaves the turn running.\nEsc never cancels a turn; pressed mid-turn it shows a reminder to use Ctrl+C.\nIt stops the turn, not the app; use the quit shortcut to exit.",
                )
            ),
        },
        ActionDef {
            id: ActionId::CycleMode,
            label: crate::locale::ctx().named_static_text("shortcuts.action.CycleMode.label", "mode"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.CycleMode.description", "Cycle mode (Normal / Plan / Always-approve)"),
            // All Shift+Tab encodings; see `input::key::shift_tab_keys()`
            default_key: crate::input::key::shift_tab_keys()[0],
            alt_keys: crate::input::key::shift_tab_keys()[1..].to_vec(),
            category: Category::GettingStarted,
            context: When::PromptFocused,
            hint_priority: None,
            hint_key_display: Some("Shift+Tab"),
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.CycleMode.long_help",
                    "Steps the session mode: Normal -> Plan -> Always-Approve -> Normal.\nPlan keeps the agent planning first and writes no files; Always-Approve runs every tool call without asking.\nCtrl+O toggles auto-approve directly.",
                )
            ),
        },
        // ── Panes (agent-level: toggle side panes) ─────────────────
        mode_ctrl_g_action(screen_mode),
        ActionDef {
            id: ActionId::ToggleTodos,
            label: crate::locale::ctx().named_static_text("shortcuts.action.ToggleTodos.label", "todos"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.ToggleTodos.description", "Toggle todo pane"),
            default_key: key!('t', CONTROL),
            alt_keys: vec![],
            category: Category::Panels,
            context: When::AgentScreen,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.ToggleTodos.long_help",
                    "Shows or hides the todo pane: the agent's live task checklist for the current work.\nWatch what it plans to do and what's left as the turn runs.\nA side pane; toggle it off to reclaim width.",
                )
            ),
        },
        ActionDef {
            id: ActionId::ToggleQueue,
            label: crate::locale::ctx().named_static_text("shortcuts.action.ToggleQueue.label", "queue"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.ToggleQueue.description", "Toggle prompt queue"),
            // Local macOS VS Code family only: ; / ' often never arrive (saw Ctrl+4 in input-debug)
            // SSH and non-Mac keep ; with ' as alt
            // Win/Linux VS maps Ctrl+4 to focusFourthEditorGroup
            default_key: if local_mac_vscode {
                key!('4', CONTROL)
            } else {
                key!(';', CONTROL)
            },
            // Apostrophe alt for consoles that drop Ctrl on `;`
            // Local Mac VS also keeps ; / ' as alts alongside primary Ctrl+4
            alt_keys: if local_mac_vscode {
                vec![key!(';', CONTROL), key!('\'', CONTROL)]
            } else {
                vec![key!('\'', CONTROL)]
            },
            category: Category::Panels,
            context: When::AgentScreen,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.ToggleQueue.long_help",
                    "Shows or hides the prompt queue.\nThe queue lets you line up follow-up prompts while a turn is running; each is sent automatically when the agent finishes.\nLocal macOS VS Code family: Ctrl+4 primary (Ctrl+; / Ctrl+' alts). Otherwise Ctrl+; with Ctrl+' alt.",
                )
            ),
        },
        ActionDef {
            id: ActionId::OpenSessions,
            label: crate::locale::ctx().named_static_text("shortcuts.action.OpenSessions.label", "sessions"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.OpenSessions.description", "Open sessions"),
            default_key: key!('r', CONTROL),
            alt_keys: vec![],
            category: Category::Session,
            context: When::AgentScreen,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.OpenSessions.long_help",
                    "Opens the session browser to resume or switch between past conversations.\nSelect one to reattach to its full history. `/resume` does the same.\nSeparate from the Agent Dashboard (Ctrl+\\), which manages many live agents at once.",
                )
            ),
        },
        ActionDef {
            id: ActionId::OpenExtensions,
            label: crate::locale::ctx().named_static_text("shortcuts.action.OpenExtensions.label", "extensions"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.OpenExtensions.description", "Open extensions"),
            // VS Code family: Ctrl+L is interject; plugins via /plugins (no chord here).
            default_key: if in_vscode_family {
                key!(Null)
            } else {
                key!('l', CONTROL)
            },
            alt_keys: vec![],
            category: Category::Panels,
            context: When::AgentScreen,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.OpenExtensions.long_help",
                    "Opens the extensions manager for MCP servers and plugins: see what's connected and the tools they add.\nUse it to confirm an integration loaded or browse available tools.\nDistinct from settings, which holds general app options.",
                )
            ),
        },
        ActionDef {
            id: ActionId::SendToBackground,
            label: crate::locale::ctx().named_static_text("shortcuts.action.SendToBackground.label", "send to bg"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.SendToBackground.description", "Send running task to background"),
            default_key: key!('b', CONTROL),
            alt_keys: vec![],
            category: Category::Panels,
            context: When::AgentScreen,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: Some(send_to_background_help),
        },
        // ── Prompt ───────────────────────────────────────────────────
        ActionDef {
            id: ActionId::InterjectPrompt,
            // "send now" label: Enter queues a follow-up while a turn runs; this chord stops the current turn and runs the message as the next one
            label: crate::locale::ctx().named_static_text("shortcuts.action.InterjectPrompt.label", "send now"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.InterjectPrompt.description", "Send now while running (cancels the current turn)"),
            default_key: if in_apple_terminal {
                key!('o', CONTROL)
            } else if in_vscode_family {
                // Ctrl+L is a stable C0 form feed on xterm.js; the user guide's interject section explains the choice
                key!('l', CONTROL)
            } else {
                key!(Enter, CONTROL)
            },
            // Windows: Ctrl+Enter may drop Ctrl, so Ctrl+I is an alt
            // VS Code family: no alts (Ctrl+L sole chord; OpenExtensions unbound so it does not steal)
            alt_keys: if in_apple_terminal {
                vec![key!(Enter, CONTROL), key!('i', CONTROL)]
            } else if in_vscode_family {
                vec![]
            } else {
                vec![key!('i', CONTROL)]
            },
            category: Category::Input,
            context: When::PromptFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.InterjectPrompt.long_help",
                    "Sends a message to the agent mid-turn without cancelling it (interject), so you can steer or add context while it keeps working.\nPlain Enter while a turn is running queues a follow-up for later; this chord merges composer text into the current turn instead.\nWith an empty composer, bare Enter (or this chord) force-sends the top queued follow-up from the prompt: no need to focus the queue pane. On the queue pane, this chord force-sends the selected row.\nReach for it to correct course without losing the turn's progress.",
                )
            ),
        },
        ActionDef {
            id: ActionId::EnableVoiceMode,
            label: crate::locale::ctx().named_static_text("shortcuts.action.EnableVoiceMode.label", "voice mode"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.EnableVoiceMode.description", "Start voice dictation (Ctrl+Space / F8)"),
            // No key binding (`KeyCode::Null`): dispatched directly by the voice chord's hold-to-talk press in the event loop, not via the registry
            default_key: key!(Null),
            alt_keys: vec![],
            category: Category::Input,
            context: When::AgentScreen,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: None,
        },
        ActionDef {
            // Voice capture chord (the same capture as `/voice`; Esc/Enter stop). Ctrl+Space decodes on every terminal
            // (without the Kitty protocol it collapses to NUL, reported as `Char(' ')`+CONTROL).
            id: ActionId::VoiceToggle,
            label: crate::locale::ctx().named_static_text("shortcuts.action.VoiceToggle.label", "mic"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.VoiceToggle.description", "Voice dictation (Ctrl+Space / F8)"),
            default_key: key!(' ', CONTROL),
            alt_keys: vec![key!(F(8))],
            category: Category::Input,
            // `Always` so the toggle key works on the agent screen and the session-less dashboard (resolved via the global fallthrough)
            context: When::Always,
            hint_priority: Some(11),
            hint_key_display: Some("Ctrl+Space / F8"),
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.VoiceToggle.long_help",
                    "Microphone capture for dictation, bound to Ctrl+Space (or F8: handy where Ctrl+Space is taken, e.g. macOS input-source switching; use Fn+F8 on a laptop).\nBehavior follows the Voice capture setting: toggle (press to start, press again to stop) or hold-to-talk (hold to record, release to stop), where hold needs a Kitty-protocol terminal and falls back to toggle elsewhere. `/voice` toggles everywhere.\nSpeech is transcribed straight into the prompt.",
                )
            ),
        },
        // Prompt history has no key chord of its own:
        // `/history` opens the search panel; Up on an empty prompt browses.
        ActionDef {
            id: ActionId::ToggleMultiline,
            label: crate::locale::ctx().named_static_text("shortcuts.action.ToggleMultiline.label", "multiline"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.ToggleMultiline.description", "Toggle multiline"),
            default_key: key!('m', CONTROL),
            alt_keys: vec![],
            category: Category::Input,
            context: When::PromptFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.ToggleMultiline.long_help",
                    "Toggles a persistent multi-line prompt so the editor stays expanded for composing longer messages.\nInsert newlines with Shift+Enter or Alt+Enter (or a trailing backslash); bare Enter still sends.\nCtrl+M toggles multiline in the prompt; off the prompt it opens the model picker.",
                )
            ),
        },
        ActionDef {
            id: ActionId::StashPrompt,
            label: crate::locale::ctx().named_static_text("shortcuts.action.StashPrompt.label", "stash"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.StashPrompt.description", "Stash / pop prompt draft"),
            default_key: key!('s', CONTROL),
            // The escape hatch for terminals that swallow Ctrl+S as XOFF.
            alt_keys: vec![key!('s', ALT)],
            category: Category::Input,
            context: When::PromptFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.StashPrompt.long_help",
                    "Stash your current prompt as a draft.\nCtrl+S sets the draft aside and clears the composer. Ctrl+S on an empty composer restores it. The draft also restores by itself after you send your next prompt. Use Alt+S if your terminal swallows Ctrl+S.\nOne draft at a time: a new stash replaces the old one.",
                )
            ),
        },
        ActionDef {
            id: ActionId::BashMode,
            label: crate::locale::ctx().named_static_text("shortcuts.action.BashMode.label", "shell"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.BashMode.description", "Shell mode (type ! on empty prompt)"),
            default_key: key!('!'),
            alt_keys: vec![],
            category: Category::Input,
            context: When::PromptFocused,
            hint_priority: None,
            hint_key_display: Some("!"),
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.BashMode.long_help",
                    "Runs a shell command without leaving the chat: type ! at the start of an empty prompt, then the command.\nThe command output is captured into the scrollback.\nDelete the leading ! to go back to a normal prompt.",
                )
            ),
        },
        // ── Agent ────────────────────────────────────────────────────
        ActionDef {
            id: ActionId::ToggleYolo,
            label: crate::locale::ctx().named_static_text("shortcuts.action.ToggleYolo.label", "yolo"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.ToggleYolo.description", "Toggle always-approve"),
            default_key: key!('o', CONTROL),
            alt_keys: vec![],
            category: Category::Session,
            context: When::AgentScreen,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.ToggleYolo.long_help",
                    "Turns auto-approve (YOLO) on or off for this session.\nWhile on, the agent runs every tool call (edits, shell, deletes) with no per-action confirmation.\nSame state as the Shift+Tab cycle's Always-Approve; use with care.",
                )
            ),
        },
        ActionDef {
            id: ActionId::NewSession,
            label: crate::locale::ctx().named_static_text("shortcuts.action.NewSession.label", "new"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.NewSession.description", "New session"),
            default_key: key!('n', CONTROL),
            alt_keys: vec![],
            category: Category::Session,
            context: When::Always,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: true,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.NewSession.long_help",
                    "Starts a fresh session with empty scrollback and context.\nRequires confirmation: press it twice (the first press arms, the second starts)\nso you don't discard the current conversation by accident.",
                )
            ),
        },
        ActionDef {
            id: ActionId::Quit,
            label: crate::locale::ctx().named_static_text("shortcuts.action.Quit.label", "quit"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.Quit.description", "Quit"),
            default_key: if in_vscode {
                key!('d', CONTROL)
            } else {
                key!('q', CONTROL)
            },
            alt_keys: if in_vscode {
                vec![]
            } else {
                vec![key!('d', CONTROL)]
            },
            category: Category::GettingStarted,
            context: When::Always,
            hint_priority: Some(10),
            hint_key_display: None,
            requires_confirmation: true,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.Quit.long_help",
                    "Exits the app. Requires confirmation: press twice in quick succession;\na lone press is treated as a stray key and ignored.\nBound to Ctrl+Q, with Ctrl+D as an alias (Ctrl+D is primary in VS Code's terminal).",
                )
            ),
        },
        ActionDef {
            id: ActionId::CommandPalette,
            label: crate::locale::ctx().named_static_text("shortcuts.action.CommandPalette.label", "commands"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.CommandPalette.description", "Command palette"),
            default_key: key!('p', CONTROL),
            alt_keys: vec![key!('?')],
            category: Category::GettingStarted,
            context: When::AgentScreen,
            hint_priority: Some(5),
            hint_key_display: Some("?"),
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.CommandPalette.long_help",
                    "Fuzzy-search every action and slash command, then run it by name.\nUseful when you don't remember a key binding.\nAlso opens with ? while the scrollback is focused.",
                )
            ),
        },
        ActionDef {
            id: ActionId::ShortcutsHelp,
            label: crate::locale::ctx().named_static_text("shortcuts.action.ShortcutsHelp.label", "shortcuts"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.ShortcutsHelp.description", "Keyboard shortcuts"),
            default_key: if ctrl_dot_unreliable {
                key!('x', CONTROL)
            } else {
                key!('.', CONTROL)
            },
            alt_keys: vec![if ctrl_dot_unreliable {
                key!('.', CONTROL)
            } else {
                key!('x', CONTROL)
            }],
            category: Category::GettingStarted,
            context: When::AgentScreen,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.ShortcutsHelp.long_help",
                    "Opens this keyboard cheatsheet.\nBrowse with j/k, expand a row's inline help with e, or press Enter for a shortcut's full detail page.\nBound to both Ctrl+. and Ctrl+X; the bar advertises whichever your terminal sends reliably.",
                )
            ),
        },
        ActionDef {
            id: ActionId::ModelPicker,
            label: crate::locale::ctx().named_static_text("shortcuts.action.ModelPicker.label", "model"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.ModelPicker.description", "Pick model"),
            default_key: key!('m', CONTROL),
            alt_keys: vec![],
            category: Category::Session,
            context: When::AgentScreen,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.ModelPicker.long_help",
                    "Opens the model picker to switch the model for this session; the choice applies to later turns.\nBound to Ctrl+M, but while the prompt is focused that chord toggles multiline instead.\nReach it from the scrollback or the command palette.",
                )
            ),
        },
        ActionDef {
            id: ActionId::OpenSettings,
            label: crate::locale::ctx().named_static_text("shortcuts.action.OpenSettings.label", "settings"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.OpenSettings.description", "Open the settings modal"),
            default_key: key!(F(2)),
            alt_keys: vec![key!(',', CONTROL), key!(',', SUPER)],
            category: Category::GettingStarted,
            context: When::AgentScreen,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: None,
        },
    ];

    // Toggle terminal mouse reporting (mouse capture). Single binding: Ctrl+R on scrollback only, which takes that
    // chord from OpenSessions while scrollback is focused. Under Panels (not Essentials): advanced/opt-in only.
    if mouse_reporting_toggle_enabled {
        actions.push(ActionDef {
            id: ActionId::ToggleMouseCapture,
            label: crate::locale::ctx().named_static_text("shortcuts.action.ToggleMouseCapture.label", "mouse reporting"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.ToggleMouseCapture.description", "Toggle mouse reporting (native copy/paste)"),
            default_key: key!('r', CONTROL),
            alt_keys: vec![],
            category: Category::Panels,
            context: When::ScrollbackFocused,
            hint_priority: None,
            hint_key_display: Some("Ctrl+r"),
            requires_confirmation: false,
            long_help: None,
        });
    }

    // They all share the dedicated `Category::Dashboard` section so the cheatsheet groups them under a single
    // "Dashboard" header. `Ctrl+\` (OpenDashboard) is registered against `Always` (global) so it works from any view,
    // including the dashboard itself (which Esc closes).
    actions.extend([
        ActionDef {
            id: ActionId::OpenDashboard,
            label: crate::locale::ctx().named_static_text("shortcuts.action.OpenDashboard.label", "dashboard"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.OpenDashboard.description", "Open the Agent Dashboard"),
            default_key: key!('\\', CONTROL),
            // Classic C0 FS (0x1c): without KKP, Ctrl+\ arrives as Char('4')+CONTROL (e.g. Apple Terminal).
            // Omit when ToggleQueue already owns Ctrl+4
            alt_keys: if local_mac_vscode {
                vec![]
            } else {
                vec![key!('4', CONTROL)]
            },
            category: Category::Dashboard,
            context: When::Always,
            hint_priority: None,
            hint_key_display: Some("Ctrl+\\"),
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.OpenDashboard.long_help",
                    "Opens the Agent Dashboard: a list of all your running and recent agents to monitor and switch between.\nWorks from anywhere, including the welcome screen and inside a session.\nFrom there you can dispatch, attach, stop, group, and reorder agents.",
                )
            ),
        },
        // Register all in-dashboard shortcuts through the registry under `When::DashboardFocused`
        // The dispatch path in `dashboard::state::handle_key` looks these up via `registry.lookup(key, When::DashboardFocused)` so users can
        // rebind any of them through `~/.grok/config.toml`.
        ActionDef {
            id: ActionId::DashboardSelectNext,
            label: crate::locale::ctx().named_static_text("shortcuts.action.DashboardSelectNext.label", "next"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.DashboardSelectNext.description", "Select next row"),
            default_key: key!(Down),
            alt_keys: vec![key!('j')],
            category: Category::Dashboard,
            context: When::DashboardFocused,
            hint_priority: None,
            hint_key_display: Some("\u{2191}\u{2193}"),
            requires_confirmation: false,
            long_help: None,
        },
        ActionDef {
            id: ActionId::DashboardSelectPrev,
            label: crate::locale::ctx().named_static_text("shortcuts.action.DashboardSelectPrev.label", "prev"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.DashboardSelectPrev.description", "Select previous row"),
            default_key: key!(Up),
            alt_keys: vec![key!('k')],
            category: Category::Dashboard,
            context: When::DashboardFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: None,
        },
        ActionDef {
            id: ActionId::DashboardTogglePin,
            label: crate::locale::ctx().named_static_text("shortcuts.action.DashboardTogglePin.label", "pin"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.DashboardTogglePin.description", "Pin / unpin agent"),
            default_key: key!('t', CONTROL),
            alt_keys: vec![],
            category: Category::Dashboard,
            context: When::DashboardFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.DashboardTogglePin.long_help",
                    "Pins or unpins the selected agent so it stays at the top of the list regardless of sorting or grouping.\nKeep the agents you care about in view as others come and go.\nPins persist across dashboard sessions.",
                )
            ),
        },
        ActionDef {
            id: ActionId::DashboardBeginRename,
            label: crate::locale::ctx().named_static_text("shortcuts.action.DashboardBeginRename.label", "rename"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.DashboardBeginRename.description", "Rename agent"),
            default_key: key!('r', CONTROL),
            alt_keys: vec![],
            category: Category::Dashboard,
            context: When::DashboardFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: None,
        },
        ActionDef {
            id: ActionId::DashboardStop,
            label: crate::locale::ctx().named_static_text("shortcuts.action.DashboardStop.label", "delete"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.DashboardStop.description", "Stop / Delete agent"),
            default_key: key!('x', CONTROL),
            alt_keys: vec![],
            category: Category::Dashboard,
            context: When::DashboardFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.DashboardStop.long_help",
                    "On a busy top-level row, Ctrl+X cancels the running turn. Once the row is idle, press Ctrl+X again within 2s to permanently delete the session.\nOn a subagent row, Ctrl+X kills the subagent.",
                )
            ),
        },
        ActionDef {
            id: ActionId::DashboardCycleMode,
            label: crate::locale::ctx().named_static_text("shortcuts.action.DashboardCycleMode.label", "mode"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.DashboardCycleMode.description", "Cycle dispatch mode"),
            // All Shift+Tab encodings; see `input::key::shift_tab_keys()`
            // Registry `matches` is exact-modifier, so the SHIFT-bearing forms must be alts
            default_key: crate::input::key::shift_tab_keys()[0],
            alt_keys: crate::input::key::shift_tab_keys()[1..].to_vec(),
            category: Category::Dashboard,
            context: When::DashboardFocused,
            hint_priority: None,
            hint_key_display: Some("Shift+Tab"),
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.DashboardCycleMode.long_help",
                    "Cycles the dispatch mode for agents you launch from the dashboard: Normal, Plan, then Always-Approve.\nPlan has new agents plan before changing files; Always-Approve runs their tools without prompting.\nMirrors the in-session Shift+Tab cycle, applied to new dispatches.",
                )
            ),
        },
        ActionDef {
            id: ActionId::DashboardToggleGrouping,
            label: crate::locale::ctx().named_static_text("shortcuts.action.DashboardToggleGrouping.label", "group"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.DashboardToggleGrouping.description", "Toggle row grouping"),
            // `Ctrl+G` ("group")
            // `Ctrl+S` was reassigned to the peek / dispatch "send + open" chord so `Shift+Enter` could be freed for newline insertion
            // (`Ctrl+G` also has a mode-specific `When::AgentScreen` action, a context that never overlaps the dashboard.)
            default_key: key!('g', CONTROL),
            alt_keys: vec![],
            category: Category::Dashboard,
            context: When::DashboardFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.DashboardToggleGrouping.long_help",
                    "Switches the dashboard between a flat list and rows grouped by state, such as working versus idle.\nGrouping surfaces the agents that need attention; the flat list keeps a stable order.\nYour choice persists across sessions.",
                )
            ),
        },
        ActionDef {
            id: ActionId::DashboardReorderUp,
            label: crate::locale::ctx().named_static_text("shortcuts.action.DashboardReorderUp.label", "reorder up"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.DashboardReorderUp.description", "Reorder agent up"),
            default_key: key!(Up, SHIFT),
            alt_keys: vec![],
            category: Category::Dashboard,
            context: When::DashboardFocused,
            hint_priority: None,
            hint_key_display: Some("Shift+\u{2191}"),
            requires_confirmation: false,
            long_help: None,
        },
        ActionDef {
            id: ActionId::DashboardReorderDown,
            label: crate::locale::ctx().named_static_text("shortcuts.action.DashboardReorderDown.label", "reorder down"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.DashboardReorderDown.description", "Reorder agent down"),
            default_key: key!(Down, SHIFT),
            alt_keys: vec![],
            category: Category::Dashboard,
            context: When::DashboardFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: None,
        },
        ActionDef {
            id: ActionId::DashboardShortcutsHelp,
            label: crate::locale::ctx().named_static_text("shortcuts.action.DashboardShortcutsHelp.label", "shortcuts"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.DashboardShortcutsHelp.description", "Show shortcuts overlay"),
            // Ctrl+. / `?` dual-bound; primary follows ctrl_dot_unreliable.
            // Ctrl+X is DashboardStop, never an alt here
            default_key: if ctrl_dot_unreliable {
                key!('?')
            } else {
                key!('.', CONTROL)
            },
            alt_keys: vec![if ctrl_dot_unreliable {
                key!('.', CONTROL)
            } else {
                key!('?')
            }],
            category: Category::Dashboard,
            context: When::DashboardFocused,
            hint_priority: None,
            hint_key_display: None,
            requires_confirmation: false,
            long_help: None,
        },
        // `DashboardExit` is registered as a discoverable action with its default key set to Esc. The Esc cascade in
        // `state::handle_key` runs before this registry lookup, so Esc always cascades. The original Esc cascade still
        // works because the cascade is keyed on `KeyCode::Esc` directly. The contract is therefore: "Esc always cascades.
        ActionDef {
            id: ActionId::DashboardExit,
            label: crate::locale::ctx().named_static_text("shortcuts.action.DashboardExit.label", "exit"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.DashboardExit.description", "Close dashboard"),
            default_key: key!(Esc),
            alt_keys: vec![],
            category: Category::Dashboard,
            context: When::DashboardFocused,
            hint_priority: None,
            hint_key_display: Some("Esc"),
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.DashboardExit.long_help",
                    "Closes the dashboard and returns to where you were.\nEsc is a cascade: it first dismisses an open peek or clears an active filter, and only exits once nothing else is pending.\nRebind this action to a different key to exit directly.",
                )
            ),
        },
        // Mirror of `ToggleYolo` (Ctrl+O) but scoped to the dashboard: flips the selected row's agent's always-approve / YOLO mode
        // Reachable from the dashboard view (and from inside the session overlay)
        ActionDef {
            id: ActionId::DashboardToggleAutoApprove,
            label: crate::locale::ctx().named_static_text("shortcuts.action.DashboardToggleAutoApprove.label", "always-approve"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.DashboardToggleAutoApprove.description", "Toggle always-approve"),
            default_key: key!('o', CONTROL),
            alt_keys: vec![],
            category: Category::Dashboard,
            context: When::DashboardFocused,
            hint_priority: None,
            hint_key_display: Some("Ctrl+O"),
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.DashboardToggleAutoApprove.long_help",
                    "Toggles auto-approve (YOLO) for the selected agent right from the dashboard, without attaching to it.\nWhile on, that agent runs every tool call with no per-action confirmation.\nThe per-session equivalent is Ctrl+O inside a session.",
                )
            ),
        },
        // Open the location picker, a floating modal to change the working directory new dashboard sessions spawn in
        // Ctrl+L ("location") is free under `DashboardFocused` (it only binds OpenExtensions under `AgentScreen`, a different context)
        ActionDef {
            id: ActionId::DashboardOpenLocationPicker,
            label: crate::locale::ctx().named_static_text("shortcuts.action.DashboardOpenLocationPicker.label", "location"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.DashboardOpenLocationPicker.description", "Change working directory for new agents"),
            default_key: key!('l', CONTROL),
            alt_keys: vec![],
            category: Category::Dashboard,
            context: When::DashboardFocused,
            hint_priority: None,
            hint_key_display: Some("Ctrl+l"),
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.DashboardOpenLocationPicker.long_help",
                    "Opens a picker to set the working directory that newly dispatched dashboard agents run in.\nLaunch agents against a different repo or folder without leaving the dashboard.\nAffects new dispatches only, not agents already running.",
                )
            ),
        },
        // Toggle worktree-dispatch mode. Free under `DashboardFocused` (Ctrl+W only binds the overlay-exit fallback under
        // `DashboardOverlay`, a different context).
        ActionDef {
            id: ActionId::DashboardToggleWorktree,
            label: crate::locale::ctx().named_static_text("shortcuts.action.DashboardToggleWorktree.label", "worktree"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.DashboardToggleWorktree.description", "Toggle worktree mode for new agents"),
            default_key: key!('w', CONTROL),
            alt_keys: vec![],
            category: Category::Dashboard,
            context: When::DashboardFocused,
            hint_priority: None,
            hint_key_display: Some("Ctrl+w"),
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.DashboardToggleWorktree.long_help",
                    "Arms the next dashboard-dispatched agent to spawn in a fresh git worktree, isolating its work on a separate checkout.\nOnly applies when the working directory is a git repo.\nAffects newly dispatched agents, not ones already running.",
                )
            ),
        },
        // Session overlay (attaching to an agent from the dashboard) bindings
        // They use `When::DashboardOverlay`: the agent-side overlay intercept (`app_view`) looks them up in that context
        // The cheatsheet uses it to dim them on the dashboard list (where they don't apply) while keeping them lit inside the overlay
        ActionDef {
            id: ActionId::DashboardOverlayExit,
            label: crate::locale::ctx().named_static_text("shortcuts.action.DashboardOverlayExit.label", "close overlay"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.DashboardOverlayExit.description", "Back to dashboard"),
            // The primary back-out shortcuts are reached through different routes. A `[✗]` click, routed via this action by
            // the mouse handler The `default_key` mirrors the primary route, Ctrl+\ (OpenDashboard, treated as overlay-exit),
            // so the cheatsheet hint is accurate (Ctrl+W is not used here.
            default_key: key!('\\', CONTROL),
            alt_keys: vec![],
            category: Category::Dashboard,
            context: When::DashboardOverlay,
            hint_priority: None,
            hint_key_display: Some("Ctrl+\\"),
            requires_confirmation: false,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.DashboardOverlayExit.long_help",
                    "Leaves the attached session overlay and returns to the dashboard list, without stopping the agent.\nAlso reachable via q on the scrollback, a neutral Esc, or the close button.\nTo stop the agent instead of just detaching, use Ctrl+X.",
                )
            ),
        },
        ActionDef {
            id: ActionId::DashboardOverlayPrev,
            label: crate::locale::ctx().named_static_text("shortcuts.action.DashboardOverlayPrev.label", "prev session"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.DashboardOverlayPrev.description", "Previous session"),
            default_key: key!('[', CONTROL),
            alt_keys: vec![],
            category: Category::Dashboard,
            context: When::DashboardOverlay,
            hint_priority: None,
            hint_key_display: Some("Ctrl+["),
            requires_confirmation: false,
            long_help: None,
        },
        ActionDef {
            id: ActionId::DashboardOverlayNext,
            label: crate::locale::ctx().named_static_text("shortcuts.action.DashboardOverlayNext.label", "next session"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.DashboardOverlayNext.description", "Next session"),
            default_key: key!(']', CONTROL),
            alt_keys: vec![],
            category: Category::Dashboard,
            context: When::DashboardOverlay,
            hint_priority: None,
            hint_key_display: Some("Ctrl+]"),
            requires_confirmation: false,
            long_help: None,
        },
        // Stop with dashboard parity inside the session overlay; the state machine is documented at `dispatch_dashboard_overlay_stop`
        // Intentionally shadows the agent view's `ShortcutsHelp` alt binding (Ctrl+X) inside the overlay; Ctrl+. still opens the cheatsheet there.
        ActionDef {
            id: ActionId::DashboardOverlayStop,
            label: crate::locale::ctx().named_static_text("shortcuts.action.DashboardOverlayStop.label", "stop"),
            description: crate::locale::ctx().named_static_text("shortcuts.action.DashboardOverlayStop.description", "Stop agent, close session (back to dashboard)"),
            default_key: key!('x', CONTROL),
            alt_keys: vec![],
            category: Category::Dashboard,
            context: When::DashboardOverlay,
            hint_priority: None,
            hint_key_display: Some("Ctrl+x"),
            requires_confirmation: true,
            long_help: Some(
                crate::locale::ctx().named_static_text(
                    "shortcuts.action.DashboardOverlayStop.long_help",
                    "Inside a session overlay, stops the attached agent and closes it, returning you to the dashboard list.\nRequires confirmation: press Ctrl+X twice.\nCtrl+. still opens the cheatsheet here; only Ctrl+X is taken over by stop.",
                )
            ),
        },
    ]);

    // Minimal has no interactive scrollback and no dashboard
    // Keep its logical prompt, agent-screen, and legitimate global actions
    // Do not register bindings whose target UI cannot exist in this process mode
    if screen_mode.is_minimal() {
        actions.retain(|def| {
            !matches!(
                def.context,
                When::ScrollbackFocused | When::DashboardFocused | When::DashboardOverlay
            ) && !matches!(def.id, ActionId::OpenDashboard | ActionId::FocusScrollback)
        });
    }

    actions
}
