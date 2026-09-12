#!/usr/bin/env python3
"""i18n wrap baseline + drift check for the grok-build fork.

Why this exists
---------------
The zh-CN localization wraps UI string literals in place, e.g.

    ctx.named_static_text("shortcuts.action.SelectNext.label", "nav")

That touches ~162 upstream files, and upstream syncs every 1-3 days
(310-1896 files per sync, 24-137 of them files we wrapped). When upstream
rewrites a line we wrapped, a `git merge` conflicts and the usual way to
clear it fast is to take the upstream side -- which silently drops the wrap.
That loss is invisible at runtime (the fallback just renders English again),
so it is never noticed until a user reports a half-English UI.

What it does
------------
  baseline  Scan the tree, rewrite scripts/i18n/wraps.jsonl from what is there.
  check     Compare the tree against the baseline; non-zero exit on drift.

Usage (from anywhere; paths are resolved from this file's location)
-------------------------------------------------------------------
  python3 scripts/i18n/wrap-check.py baseline   # after adding/removing wraps
  python3 scripts/i18n/wrap-check.py check      # after every upstream sync

Only reports; never edits source. Exit codes: 0 clean, 1 drift (missing wraps
or upstream-changed English), 2 usage error.
"""

import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]  # scripts/i18n/ -> repo root
BASELINE = HERE / "wraps.jsonl"
ROOTS = ["crates/codegen"]

# The three wrap forms used by xai-grok-locale. `english` is the fallback that
# stays at the call site, so it doubles as the anchor for re-application.
CALL = re.compile(
    r'(?P<kind>named_static_text|named_text|format_named)\(\s*'
    r'"(?P<id>[a-z][a-z0-9]*(?:\.[a-z0-9_]+)+)"\s*,\s*'
    r'"(?P<english>(?:[^"\\]|\\.)*)"'
)


def scan():
    """Every wrap call currently in the tree, keyed by (file, id)."""
    found = {}
    for root in ROOTS:
        base = REPO / root
        if not base.exists():
            continue
        for path in base.rglob("*.rs"):
            try:
                src = path.read_text(encoding="utf-8", errors="ignore")
            except OSError:
                continue
            rel = path.relative_to(REPO).as_posix()
            for m in CALL.finditer(src):
                key = (rel, m.group("id"))
                found[key] = {
                    "file": rel,
                    "id": m.group("id"),
                    "kind": m.group("kind"),
                    "english": m.group("english").encode().decode("unicode_escape", errors="ignore"),
                }
    return found


def load_baseline():
    if not BASELINE.exists():
        sys.exit("no baseline; run `baseline` first")
    old = {}
    with open(BASELINE, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            w = json.loads(line)
            old[(w["file"], w["id"])] = w
    return old


def cmd_baseline():
    wraps = scan()
    with open(BASELINE, "w", encoding="utf-8", newline="\n") as fh:
        for w in sorted(wraps.values(), key=lambda x: (x["file"], x["id"])):
            fh.write(json.dumps(w, ensure_ascii=False) + "\n")
    files = {w["file"] for w in wraps.values()}
    print(f"baseline: {len(wraps)} wraps over {len(files)} files -> {BASELINE}")


def cmd_check():
    old = load_baseline()
    new = scan()

    missing = [w for k, w in old.items() if k not in new]
    added = [w for k, w in new.items() if k not in old]
    changed = [
        (old[k], w)
        for k, w in new.items()
        if k in old and old[k]["english"] != w["english"]
    ]

    print(f"baseline {len(old)} wraps | now {len(new)}")
    print(f"missing {len(missing)} | added {len(added)} | english changed {len(changed)}")

    if missing:
        print("\n=== missing wraps (usually an upstream-side merge resolution) ===")
        by_file = {}
        for w in missing:
            by_file.setdefault(w["file"], []).append(w)
        for f in sorted(by_file):
            print(f"\n  {f}  ({len(by_file[f])})")
            for w in by_file[f][:8]:
                print(f'    {w["kind"]:18s} {w["id"]}  <- "{w["english"][:60]}"')
            if len(by_file[f]) > 8:
                print(f'    ... {len(by_file[f]) - 8} more')

    if changed:
        print("\n=== upstream rewrote the English fallback (translation needs a re-read) ===")
        for o, n in changed[:20]:
            print(f'  {n["file"]}: {n["id"]}')
            print(f'    - "{o["english"][:70]}"')
            print(f'    + "{n["english"][:70]}"')

    if added:
        print("\n=== new wraps not in the baseline (rerun `baseline` once reviewed) ===")
        for w in added[:10]:
            print(f'  {w["file"]}: {w["id"]}')

    if not missing and not changed:
        print("\nOK: wraps intact")
    return 1 if missing or changed else 0


if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else "check"
    if cmd == "baseline":
        cmd_baseline()
    elif cmd == "check":
        sys.exit(cmd_check())
    else:
        sys.exit(__doc__)
