use super::{DbStats, GcReport, RebuildReport};
use crate::fs_size::{Volume, physical_dir_size};
use crate::util::{format_age, format_bytes, pad_to_width, truncate_to_width, unix_now};
use std::io::Write;
use std::path::Path;
use unicode_width::UnicodeWidthStr;
use xai_fast_worktree::WorktreeRecord;
const REPO_WIDTH: usize = 6;
const BRANCH_WIDTH: usize = 20;
const AGE_WIDTH: usize = 10;
/// Truncate-then-pad to exactly `width` display columns; headers and data share it so the two stay aligned.
fn cell(s: &str, width: usize) -> String {
    pad_to_width(&truncate_to_width(s, width), width)
}
pub fn print_table(records: &[WorktreeRecord], out: &mut impl Write) -> std::io::Result<()> {
    let ctx = crate::locale::ctx();
    if records.is_empty() {
        writeln!(out, "{}", ctx.named_text("worktree.empty", "No worktrees found."))?;
        return Ok(());
    }
    let id_width = records
        .iter()
        .map(|r| UnicodeWidthStr::width(r.id.as_str()))
        .max()
        .unwrap_or(0)
        .max(16);
    let label_width = records
        .iter()
        .map(|r| r.label().map_or(0, UnicodeWidthStr::width))
        .max()
        .unwrap_or(0)
        .clamp(5, 24);
    let type_header = ctx.named_text("worktree.column.type", "TYPE");
    let type_width = records
        .iter()
        .map(|r| UnicodeWidthStr::width(kind_label(r.kind)))
        .fold(UnicodeWidthStr::width(type_header.as_ref()), usize::max);
    writeln!(
        out,
        "  {} {} {} {} {} {:<AGE_WIDTH$} {}",
        pad_to_width(ctx.named_text("worktree.column.id", "ID").as_ref(), id_width),
        cell(type_header.as_ref(), type_width),
        cell(ctx.named_text("worktree.column.repo", "REPO").as_ref(), REPO_WIDTH),
        cell(ctx.named_text("worktree.column.label", "LABEL").as_ref(), label_width),
        cell(ctx.named_text("worktree.column.branch", "BRANCH").as_ref(), BRANCH_WIDTH),
        ctx.named_text("worktree.column.age", "AGE"),
        ctx.named_text("worktree.column.path", "PATH"),
    )?;
    let now = unix_now();
    let detached_text = ctx
        .named_text("worktree.detached", "(detached)")
        .into_owned();
    for rec in records {
        let age = format_age(rec.created_at, now);
        let branch: &str = rec.git_ref.as_deref().unwrap_or(&detached_text);
        let label = rec.label().unwrap_or("");
        let path = abbreviate_home(&rec.path);
        writeln!(
            out,
            "  {} {} {} {} {} {:<AGE_WIDTH$} {}",
            pad_to_width(&rec.id, id_width),
            cell(kind_label(rec.kind), type_width),
            cell(&rec.repo_name, REPO_WIDTH),
            cell(label, label_width),
            cell(branch, BRANCH_WIDTH),
            age,
            path,
        )?;
    }
    let total = records.len();
    let by_kind: std::collections::BTreeMap<&str, usize> =
        records
            .iter()
            .fold(std::collections::BTreeMap::new(), |mut m, r| {
                *m.entry(kind_label(r.kind)).or_default() += 1;
                m
            });
    let breakdown: Vec<String> = by_kind
        .iter()
        .map(|(k, v)| {
            ctx.format_named(
                "worktree.summary.kind",
                "{count} {kind}",
                &[("count", &v.to_string()), ("kind", k)],
            )
        })
        .collect();
    writeln!(
        out,
        "  {}",
        ctx.format_named(
            "worktree.summary",
            "{count} worktrees ({breakdown})",
            &[("count", &total.to_string()), ("breakdown", &breakdown.join(", "))],
        )
    )
}
pub fn print_json(records: &[WorktreeRecord], out: &mut impl Write) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(records).unwrap_or_else(|_| "[]".to_string());
    writeln!(out, "{json}")
}
pub fn print_show(rec: &WorktreeRecord, out: &mut impl Write) -> std::io::Result<()> {
    let ctx = crate::locale::ctx();
    writeln!(
        out,
        "{}",
        ctx.format_named(
            "worktree_cli.show.path",
            "  Path:           {value}",
            &[("value", &rec.path.display().to_string())],
        )
    )?;
    writeln!(
        out,
        "{}",
        ctx.format_named(
            "worktree_cli.show.id",
            "  ID:             {value}",
            &[("value", &rec.id)],
        )
    )?;
    writeln!(
        out,
        "{}",
        ctx.format_named(
            "worktree_cli.show.type",
            "  Type:           {value}",
            &[("value", kind_label(rec.kind))],
        )
    )?;
    writeln!(
        out,
        "{}",
        ctx.format_named(
            "worktree_cli.show.source_repo",
            "  Source Repo:    {value}",
            &[("value", &rec.source_repo.display().to_string())],
        )
    )?;
    writeln!(
        out,
        "{}",
        ctx.format_named(
            "worktree_cli.show.creation_mode",
            "  Creation Mode:  {value}",
            &[("value", &rec.creation_mode.to_string())],
        )
    )?;
    if let Some(ref git_ref) = rec.git_ref {
        writeln!(
            out,
            "{}",
            ctx.format_named(
                "worktree_cli.show.git_ref",
                "  Git Ref:        {value}",
                &[("value", git_ref)],
            )
        )?;
    }
    if let Some(ref commit) = rec.head_commit {
        let short = if commit.len() > 12 {
            &commit[..12]
        } else {
            commit
        };
        writeln!(
            out,
            "{}",
            ctx.format_named(
                "worktree_cli.show.head",
                "  HEAD:           {value}",
                &[("value", short)],
            )
        )?;
    }
    writeln!(
        out,
        "{}",
        ctx.format_named(
            "worktree_cli.show.created",
            "  Created:        {value}",
            &[("value", &format_timestamp(rec.created_at))],
        )
    )?;
    if let Some(ts) = rec.last_accessed_at {
        writeln!(
            out,
            "{}",
            ctx.format_named(
                "worktree_cli.show.last_accessed",
                "  Last Accessed:  {value}",
                &[("value", &format_timestamp(ts))],
            )
        )?;
    }
    if let Some(ref sid) = rec.session_id {
        writeln!(
            out,
            "{}",
            ctx.format_named(
                "worktree_cli.show.session_id",
                "  Session ID:     {value}",
                &[("value", sid)],
            )
        )?;
    }
    if let Some(pid) = rec.creator_pid {
        writeln!(
            out,
            "{}",
            ctx.format_named(
                "worktree_cli.show.creator_pid",
                "  Creator PID:    {value}",
                &[("value", &pid.to_string())],
            )
        )?;
    }
    writeln!(
        out,
        "{}",
        ctx.format_named(
            "worktree_cli.show.status",
            "  Status:         {value}",
            &[("value", status_label(rec.status))],
        )
    )?;
    if let Some(label) = rec.label() {
        writeln!(
            out,
            "{}",
            ctx.format_named(
                "worktree_cli.show.label",
                "  Label:          {value}",
                &[("value", label)],
            )
        )?;
    }
    if rec.path.exists() {
        let size = physical_dir_size(&rec.path, Volume::of(&rec.path));
        let bytes = size.measure.bytes().unwrap_or_default();
        write!(
            out,
            "{}",
            ctx.format_named(
                "worktree_cli.show.disk_usage",
                "  Disk Usage:     {value}",
                &[("value", &format_bytes(bytes))],
            )
        )?;
        let skipped = size.issues.skipped();
        if skipped > 0 {
            write!(
                out,
                " {}",
                ctx.format_named(
                    "worktree.disk_usage.skipped",
                    "({count} entries skipped)",
                    &[("count", &skipped.to_string())],
                )
            )?;
        }
        writeln!(out)?;
    }
    Ok(())
}
pub fn print_stats(stats: &DbStats, out: &mut impl Write) -> std::io::Result<()> {
    let ctx = crate::locale::ctx();
    writeln!(
        out,
        "{}",
        ctx.named_text("worktree.stats.title", "Worktree DB Statistics")
    )?;
    writeln!(out, "======================")?;
    writeln!(
        out,
        "{}",
        ctx.format_named(
            "worktree_cli.stats.total_records",
            "  Total records:  {count}",
            &[("count", &stats.total_records.to_string())],
        )
    )?;
    writeln!(
        out,
        "{}",
        ctx.format_named(
            "worktree_cli.stats.alive",
            "  Alive:          {count}",
            &[("count", &stats.alive_count.to_string())],
        )
    )?;
    writeln!(
        out,
        "{}",
        ctx.format_named(
            "worktree_cli.stats.dead",
            "  Dead:           {count}",
            &[("count", &stats.dead_count.to_string())],
        )
    )?;
    writeln!(
        out,
        "{}",
        ctx.format_named(
            "worktree_cli.stats.db_size",
            "  DB size:        {value}",
            &[("value", &format_bytes(stats.db_file_bytes))],
        )
    )
}
pub fn print_gc(report: &GcReport, out: &mut impl Write) -> std::io::Result<()> {
    let ctx = crate::locale::ctx();
    writeln!(out, "{}", ctx.named_text("worktree.gc.title", "GC report:"))?;
    writeln!(
        out,
        "{}",
        ctx.format_named(
            "worktree_cli.gc.dead_removed",
            "  Dead records removed:      {count}",
            &[("count", &report.dead_removed.to_string())],
        )
    )?;
    writeln!(
        out,
        "{}",
        ctx.format_named(
            "worktree_cli.gc.expired_removed",
            "  Expired worktrees removed: {count}",
            &[("count", &report.expired_removed.to_string())],
        )
    )?;
    if report.no_repo_paths > 0 {
        writeln!(
            out,
            "{}",
            ctx.format_named(
                "worktree_cli.gc.no_repo_paths",
                "  Non-repository paths:      {count}",
                &[("count", &report.no_repo_paths.to_string())],
            )
        )?;
    }
    writeln!(
        out,
        "{}",
        ctx.format_named(
            "worktree_cli.gc.skipped_alive",
            "  Skipped (guarded):         {count}",
            &[("count", &report.skipped_alive.to_string())],
        )
    )?;
    if report.never_expiring > 0 {
        writeln!(
            out,
            "{}",
            ctx.format_named(
                "worktree_cli.gc.never_expiring",
                "  Kept (never expires):      {count}",
                &[("count", &report.never_expiring.to_string())],
            )
        )?;
    }
    if report.kept_unsafe > 0 {
        writeln!(
            out,
            "{}",
            ctx.format_named(
                "worktree_cli.gc.kept_unsafe",
                "  Kept (not reclaimable):    {count}",
                &[("count", &report.kept_unsafe.to_string())],
            )
        )?;
        for (reason, count) in &report.kept_reasons {
            writeln!(out, "    {reason}: {count}")?;
        }
        const MAX_KEPT_PRINTED: usize = 20;
        let printed = report.kept.len().min(MAX_KEPT_PRINTED);
        for kept in report.kept.iter().take(MAX_KEPT_PRINTED) {
            writeln!(out, "      {}  ({})", kept.path, kept.reason)?;
        }
        let rest = usize::try_from(report.kept_unsafe)
            .unwrap_or(usize::MAX)
            .saturating_sub(printed);
        if rest > 0 {
            writeln!(
                out,
                "{}",
                ctx.format_named(
                    "worktree.gc.kept_more",
                    "      and {count} more, named in the log",
                    &[("count", &rest.to_string())],
                )
            )?;
        }
    }
    if report.names_collected > 0 {
        writeln!(
            out,
            "{}",
            ctx.format_named(
                "worktree_cli.gc.names_collected",
                "  Reclaimed names dropped:   {count}",
                &[("count", &report.names_collected.to_string())],
            )
        )?;
    }
    if report.not_judged > 0 {
        writeln!(
            out,
            "{}",
            ctx.format_named(
                "worktree_cli.gc.not_judged",
                "  Not judged this pass:      {count}",
                &[("count", &report.not_judged.to_string())],
            )
        )?;
    }
    if report.unnamed > 0 {
        writeln!(
            out,
            "{}",
            ctx.format_named(
                "worktree_cli.gc.unnamed",
                "  Naming failed (kept):      {count}",
                &[("count", &report.unnamed.to_string())],
            )
        )?;
    }
    if report.remove_failed > 0 {
        writeln!(
            out,
            "{}",
            ctx.format_named(
                "worktree_cli.gc.remove_failed",
                "  Removal failures:          {count}",
                &[("count", &report.remove_failed.to_string())],
            )
        )?;
    }
    Ok(())
}
pub fn print_rebuild(report: &RebuildReport, out: &mut impl Write) -> std::io::Result<()> {
    let ctx = crate::locale::ctx();
    writeln!(
        out,
        "{}",
        ctx.named_text("worktree.rebuild.title", "Rebuild report:")
    )?;
    writeln!(
        out,
        "{}",
        ctx.format_named(
            "worktree_cli.rebuild.discovered",
            "  Discovered:      {count}",
            &[("count", &report.discovered.to_string())],
        )
    )?;
    writeln!(
        out,
        "{}",
        ctx.format_named(
            "worktree_cli.rebuild.registered",
            "  Registered:      {count}",
            &[("count", &report.registered.to_string())],
        )
    )?;
    writeln!(
        out,
        "{}",
        ctx.format_named(
            "worktree_cli.rebuild.already_tracked",
            "  Already tracked: {count}",
            &[("count", &report.already_tracked.to_string())],
        )
    )
}
fn kind_label(kind: xai_fast_worktree::WorktreeKind) -> &'static str {
    let ctx = crate::locale::ctx();
    match kind {
        xai_fast_worktree::WorktreeKind::Session => {
            ctx.named_static_text("du.kind.session", "session")
        }
        xai_fast_worktree::WorktreeKind::Ab => ctx.named_static_text("du.kind.ab", "ab"),
        xai_fast_worktree::WorktreeKind::Pool => ctx.named_static_text("du.kind.pool", "pool"),
        xai_fast_worktree::WorktreeKind::Fork => ctx.named_static_text("du.kind.fork", "fork"),
        xai_fast_worktree::WorktreeKind::Manual => {
            ctx.named_static_text("du.kind.manual", "manual")
        }
        xai_fast_worktree::WorktreeKind::Subagent => {
            ctx.named_static_text("du.kind.subagent", "subagent")
        }
    }
}
fn status_label(status: xai_fast_worktree::WorktreeStatus) -> &'static str {
    let ctx = crate::locale::ctx();
    match status {
        xai_fast_worktree::WorktreeStatus::Alive => {
            ctx.named_static_text("worktree.status.alive", "alive")
        }
        xai_fast_worktree::WorktreeStatus::Dead => {
            ctx.named_static_text("worktree.status.dead", "dead")
        }
    }
}
fn format_timestamp(ts: i64) -> String {
    let dt = chrono::DateTime::from_timestamp(ts, 0);
    match dt {
        Some(dt) => dt.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
        None => ts.to_string(),
    }
}
fn abbreviate_home(path: &Path) -> String {
    crate::util::abbreviate_path(&path.to_string_lossy()).into_owned()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn print_table_never_truncates_long_ids() {
        let long_id = "a".repeat(40);
        let mut out = Vec::new();
        print_table(&[make_record(&long_id, "lbl")], &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains(&long_id), "full ID must be present: {text}");
    }
    fn make_record(id: &str, label: &str) -> WorktreeRecord {
        crate::test_util::make_worktree_record(
            id,
            std::path::Path::new(&format!("/tmp/wt-{id}")),
            label,
        )
    }
    #[test]
    fn print_show_non_nfs_omits_nfs_block() {
        let rec = make_record("wt-copy", "c");
        let mut out = Vec::new();
        print_show(&rec, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(!text.contains("Strategy:       nfs"), "{text}");
        assert!(!text.contains("clean-artifacts"), "{text}");
    }
    #[test]
    fn print_table_pads_cjk_labels_by_display_width() {
        let records = vec![
            make_record("wt-cjk", "组件更新"),
            make_record("wt-ascii", "plain-label"),
        ];
        let mut out = Vec::new();
        print_table(&records, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("组件更新"));
        crate::test_util::assert_path_column_aligned(&text, "/tmp/wt-");
    }
}
