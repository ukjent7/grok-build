//! Pure renderer over [`DiskUsageReport`].
//! `xai_grok_config::grok_home()`, whose first call creates the home, must stay out of this module.

use std::borrow::Cow;
use std::io::Write;
use std::path::Path;

use unicode_width::UnicodeWidthStr;
use xai_fast_worktree::{WorktreeKind, WorktreeStatus};

use super::{DiskUsageReport, Registration, RegistryState, WorktreeUsage};
use crate::util::{format_age, format_bytes, pad_to_width, truncate_to_width};

const SIZE_WIDTH: usize = 10;
const AGE_WIDTH: usize = 10;
const TYPE_HEADER: &str = "TYPE";
const LABEL_HEADER: &str = "LABEL";
const LABEL_WIDTH_MAX: usize = 24;

pub fn print_report(
    report: &DiskUsageReport,
    now: i64,
    out: &mut impl Write,
) -> std::io::Result<()> {
    let ctx = crate::locale::ctx();
    let home_label = home_prefix_label(&report.grok_home);
    writeln!(
        out,
        "{}",
        ctx.format_named("du.title", "Disk usage for {home}", &[("home", &home_label)])
    )?;
    for entry in &report.top_level_dirs {
        writeln!(
            out,
            "  {:>SIZE_WIDTH$}  {}",
            size_cell(entry.bytes),
            entry.name
        )?;
    }
    if report.root_files_bytes > 0 {
        writeln!(
            out,
            "  {:>SIZE_WIDTH$}  {}",
            format_bytes(report.root_files_bytes),
            ctx.named_text("du.top_level_files", "(top-level files)")
        )?;
    }
    writeln!(
        out,
        "  {:>SIZE_WIDTH$}  {}",
        format_bytes(report.total_bytes),
        ctx.named_text("du.total", "total")
    )?;
    if report.skips.unreadable_dirs > 0 {
        let (id, template) = if report.skips.unreadable_dirs == 1 {
            (
                "du.skip.unreadable_one",
                "{count} directory could not be read; what is under it may be missing from the total. RUST_LOG=debug names it.",
            )
        } else {
            (
                "du.skip.unreadable",
                "{count} directories could not be read; what is under them may be missing from the total. RUST_LOG=debug names them.",
            )
        };
        writeln!(
            out,
            "  {}",
            ctx.format_named(
                id,
                template,
                &[("count", &report.skips.unreadable_dirs.to_string())],
            )
        )?;
    }
    if report.skips.unstatable_entries > 0 {
        let (id, template) = if report.skips.unstatable_entries == 1 {
            (
                "du.skip.unstatable_one",
                "{count} entry could not be read and is not counted.",
            )
        } else {
            (
                "du.skip.unstatable",
                "{count} entries could not be read and are not counted.",
            )
        };
        writeln!(
            out,
            "  {}",
            ctx.format_named(
                id,
                template,
                &[("count", &report.skips.unstatable_entries.to_string())],
            )
        )?;
    }
    if report.skips.other_filesystem_dirs > 0 {
        let (id, template) = if report.skips.other_filesystem_dirs == 1 {
            (
                "du.skip.other_filesystem_one",
                "{count} directory is on another filesystem and is not counted, here or in any row.",
            )
        } else {
            (
                "du.skip.other_filesystem",
                "{count} directories are on another filesystem and are not counted, here or in any row.",
            )
        };
        writeln!(
            out,
            "  {}",
            ctx.format_named(
                id,
                template,
                &[("count", &report.skips.other_filesystem_dirs.to_string())],
            )
        )?;
    }
    if report.unfollowed_dir_symlinks > 0 {
        let (id, template) = if report.unfollowed_dir_symlinks == 1 {
            (
                "du.skip.unfollowed_symlink_one",
                "{count} top-level symlink to a directory is not followed, so its contents are missing from the total.",
            )
        } else {
            (
                "du.skip.unfollowed_symlink",
                "{count} top-level symlinks to directories are not followed, so their contents are missing from the total.",
            )
        };
        writeln!(
            out,
            "  {}",
            ctx.format_named(
                id,
                template,
                &[("count", &report.unfollowed_dir_symlinks.to_string())],
            )
        )?;
    }
    // The proven statement replaces the general note rather than joining it.
    if report.total_exceeds_volume_used() {
        writeln!(
            out,
            "  {}",
            ctx.named_text(
                "du.volume.shared_blocks",
                "Total exceeds the used space on this volume, so shared blocks are counted once per path."
            )
        )?;
    } else if cfg!(unix) && !report.worktrees.is_empty() {
        writeln!(
            out,
            "  {}",
            ctx.named_text(
                "du.volume.clone_shared_storage",
                "Worktree clones share storage with their source, so the total can exceed real disk use."
            )
        )?;
    }

    writeln!(out)?;
    writeln!(out, "{}", ctx.named_text("du.worktrees.title", "Worktrees"))?;
    if report.worktrees_outside_managed_roots > 0 {
        let (id, template) = if report.worktrees_outside_managed_roots == 1 {
            (
                "du.worktrees.outside_managed_one",
                "{count} worktree outside the managed worktree dirs is not shown here.",
            )
        } else {
            (
                "du.worktrees.outside_managed",
                "{count} worktrees outside the managed worktree dirs are not shown here.",
            )
        };
        writeln!(
            out,
            "  {}",
            ctx.format_named(
                id,
                template,
                &[("count", &report.worktrees_outside_managed_roots.to_string())],
            )
        )?;
    }
    match report.registry {
        RegistryState::Read => {}
        RegistryState::Absent => {
            if !report.worktrees.is_empty() {
                writeln!(
                    out,
                    "  {}",
                    ctx.named_text(
                        "du.registry.absent",
                        "Worktree registry not found; rows may show as untracked."
                    )
                )?;
            }
        }
        RegistryState::Busy => {
            writeln!(
                out,
                "  {}",
                ctx.named_text(
                    "du.registry.busy",
                    "  Worktree registry is in use by another process; rows show as untracked. Retry in a moment."
                )
            )?;
        }
        RegistryState::Unopenable => {
            writeln!(
                out,
                "  {}",
                ctx.format_named(
                    "du.registry.unopenable",
                    "  Worktree registry at {path} could not be opened; rows show as untracked. Check its permissions.",
                    &[(
                        "path",
                        &abbreviate(&report.registry_path, &report.grok_home, &home_label)
                    )],
                )
            )?;
        }
        RegistryState::Corrupt => {
            writeln!(
                out,
                "  {}",
                ctx.format_named(
                    "du.registry.corrupt",
                    "  Worktree registry is damaged; rows show as untracked. Remove {path} and run `grok worktree db rebuild` to recreate it.",
                    &[(
                        "path",
                        &abbreviate(&report.registry_path, &report.grok_home, &home_label)
                    )],
                )
            )?;
        }
    }
    if report.worktrees.is_empty() {
        writeln!(
            out,
            "  {}",
            ctx.named_text("du.worktrees.empty", "No worktrees found.")
        )?;
    } else {
        let type_header = ctx.named_text("du.columns.type", "TYPE");
        let label_header = ctx.named_text("du.columns.label", "LABEL");
        let kind_cells: Vec<Cow<'_, str>> = report.worktrees.iter().map(kind_cell).collect();
        let kind_width = kind_cells
            .iter()
            .map(|k| UnicodeWidthStr::width(k.as_ref()))
            .fold(UnicodeWidthStr::width(type_header.as_ref()), usize::max);
        let label_width = report
            .worktrees
            .iter()
            .map(|w| UnicodeWidthStr::width(w.label()))
            .fold(UnicodeWidthStr::width(label_header.as_ref()), usize::max)
            .min(LABEL_WIDTH_MAX);
        writeln!(
            out,
            "  {:>SIZE_WIDTH$}  {} {:<AGE_WIDTH$} {} {}",
            ctx.named_text("du.columns.size", "SIZE"),
            pad_to_width(type_header.as_ref(), kind_width),
            ctx.named_text("du.columns.age", "AGE"),
            pad_to_width(label_header.as_ref(), label_width),
            ctx.named_text("du.columns.path", "PATH"),
        )?;
        for (wt, kind) in report.worktrees.iter().zip(&kind_cells) {
            let age = wt
                .age_stamp()
                .map_or_else(|| "-".to_owned(), |ts| format_age(ts, now));
            let label = truncate_to_width(wt.label(), label_width);
            writeln!(
                out,
                "  {:>SIZE_WIDTH$}  {} {:<AGE_WIDTH$} {} {}",
                size_cell(wt.bytes),
                pad_to_width(kind, kind_width),
                age,
                pad_to_width(&label, label_width),
                abbreviate(&wt.path, &report.grok_home, &home_label),
            )?;
        }
    }

    // gc's age pass needs `--max-age` and walks registry records, so neither half of the hint holds for both row kinds
    if report.worktrees_dominate() && !report.worktrees.is_empty() {
        writeln!(out)?;
        if report.worktrees.iter().any(WorktreeUsage::is_tracked) {
            writeln!(
                out,
                "{}",
                ctx.named_text(
                    "du.hint.reclaim_tracked",
                    "To reclaim space, run `grok worktree gc --max-age 7d --dry-run`, then the same command without `--dry-run`. Without `--max-age`, gc expires nothing, and it keeps a worktree whose work it cannot find elsewhere, naming each one."
                )
            )?;
        }
        if !report.worktrees.iter().all(WorktreeUsage::is_tracked) {
            writeln!(
                out,
                "{}",
                ctx.named_text(
                    "du.hint.untracked",
                    "Untracked rows are not in the registry, so gc never visits them. Remove one with `grok worktree rm --dry-run <path>`, then without `--dry-run`."
                )
            )?;
        }
    }
    Ok(())
}

pub fn print_missing_home(grok_home: &str, out: &mut impl Write) -> std::io::Result<()> {
    writeln!(
        out,
        "{}",
        crate::locale::ctx().format_named(
            "du.empty_home",
            "Nothing on disk yet at {home}.",
            &[("home", &home_prefix_label(grok_home))],
        )
    )
}

/// A dash where nothing was measured, which is not zero bytes.
fn size_cell(bytes: Option<u64>) -> String {
    bytes.map_or_else(|| "-".to_owned(), format_bytes)
}

fn count_noun(n: u64, one: &'static str, many: &'static str) -> &'static str {
    if n == 1 { one } else { many }
}

fn count_verb(n: u64) -> &'static str {
    if n == 1 { "is" } else { "are" }
}

fn kind_cell(wt: &WorktreeUsage) -> Cow<'static, str> {
    let ctx = crate::locale::ctx();
    match &wt.registration {
        Registration::Untracked => Cow::Owned(ctx.format_named(
            "du.kind.untracked",
            "untracked ({kind})",
            &[("kind", kind_text(wt.kind))],
        )),
        Registration::Tracked(rec) => match rec.status {
            WorktreeStatus::Dead => Cow::Owned(ctx.format_named(
                "du.kind.dead",
                "{kind} (dead)",
                &[("kind", kind_text(wt.kind))],
            )),
            WorktreeStatus::Alive => Cow::Borrowed(kind_text(wt.kind)),
        },
    }
}

fn kind_text(kind: WorktreeKind) -> &'static str {
    let ctx = crate::locale::ctx();
    match kind {
        WorktreeKind::Session => ctx.named_static_text("du.kind.session", "session"),
        WorktreeKind::Ab => ctx.named_static_text("du.kind.ab", "ab"),
        WorktreeKind::Pool => ctx.named_static_text("du.kind.pool", "pool"),
        WorktreeKind::Fork => ctx.named_static_text("du.kind.fork", "fork"),
        WorktreeKind::Manual => ctx.named_static_text("du.kind.manual", "manual"),
        WorktreeKind::Subagent => ctx.named_static_text("du.kind.subagent", "subagent"),
    }
}

fn abbreviate(path: &str, home: &str, label: &str) -> String {
    match Path::new(path).strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => label.to_owned(),
        Ok(rest) => format!("{label}/{}", rest.display()),
        Err(_) => path.to_owned(),
    }
}

fn home_prefix_label(grok_home: &str) -> String {
    crate::util::display_grok_home_prefix_for(Path::new(grok_home))
}
