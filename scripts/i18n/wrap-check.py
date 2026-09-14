#!/usr/bin/env python3
"""i18n wrap baseline + drift check for the grok-build fork.

Why this exists
---------------
The zh-CN localization wraps UI string literals in place, e.g.

    ctx.named_static_text("shortcuts.action.SelectNext.label", "nav")

That touches ~148 upstream files, and upstream syncs every 1-3 days
(310-1896 files per sync, 24-137 of them files we wrapped). When upstream
rewrites a line we wrapped, a `git merge` conflicts and the usual way to
clear it fast is to take the upstream side -- which silently drops the wrap.
That loss is invisible at runtime (the fallback just renders English again),
so it is never noticed until a user reports a half-English UI.

What it does
------------
  baseline  Scan the tree, rewrite scripts/i18n/wraps.jsonl from what is there.
  check     Compare the tree against the baseline; non-zero exit on drift.

Five failure classes, all fatal on `check`:
  1. missing      a wrap the baseline has and the tree no longer does (dropped
                  by a merge resolution).
  2. added        a wrap the tree has and the baseline does not. Intentional
                  additions must be baselined so the next upstream sync can
                  still tell a dropped wrap from an untouched one.
  3. untranslated a wrap whose id has no zh-CN entry. It renders English, so the
                  interface goes half-Chinese with nothing else to notice it.
                  English-keyed `tr` wraps check against `en-to-zh.json` instead.
  4. bare_braces  a bare `{}` in the English anchor. `named_*`/`format_named`/
                  `tr_format` substitute named `{placeholder}` only, so `{}` would
                  reach the screen verbatim. Rejected by design (see
                  `english_fallback_survives_an_unknown_id_verbatim`).
  5. placeholder_mismatch  an `en-to-zh.json` value uses a `{name}` the English
                  key lacks. `tr_format` substitutes by name, so the extra
                  placeholder would reach the screen verbatim. Convention is
                  single-name: rename, never dual-pass (see `tr_format` docs).

English-keyed (`tr`) anchors are stored decoded to their runtime value: source
`"a\nb"` and `"a<newline>b"` are the same lookup. `rust_unescape` below must
stay in sync with the Rust string escapes the tree actually uses; unknown
escapes fall back to raw so the gate never crashes on new syntax.

Classes 1-2 are reported pairwise as `rewritten` when the same (file, id, kind)
appears on both sides -- that is the shape of "upstream rewrote the line".

Usage (from anywhere; paths are resolved from this file's location)
-------------------------------------------------------------------
  python3 scripts/i18n/wrap-check.py baseline   # after adding/removing wraps
  python3 scripts/i18n/wrap-check.py check      # after every upstream sync

Only reports; never edits source. Exit codes: 0 clean, 1 drift, 2 usage error.
"""

import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]  # scripts/i18n/ -> repo root
BASELINE = HERE / "wraps.jsonl"
ROOTS = ["crates/codegen"]
# The locale crate is the definition site, not a consumer: it owns the catalogs and
# its only wrap is a test probe for the English fallback.
EXCLUDED_DIRS = ["crates/codegen/xai-grok-locale"]
ZH_CATALOGS = [
    "crates/codegen/xai-grok-locale/locales/zh-CN-metadata.json",
    "crates/codegen/xai-grok-locale/locales/zh-CN.json",
]
EN_TO_ZH_CATALOG = "crates/codegen/xai-grok-locale/locales/en-to-zh.json"
TR_KINDS = {"tr", "tr_static", "tr_format", "tr_ctx"}

# Direct wrap forms. `english` is the fallback that stays at the call site, so it
# doubles as the anchor for re-application. `fixed` is
# `app/error_display.rs`'s one-line constructor for `FixedCopy { id, english }`.
# English-keyed forms (`tr`, `tr_static`, `tr_format`, `tr_ctx`) are the
# preferred tier going forward: no invented id, upstream rewording falls back
# to English without a baseline entry to update. Fragments and identifiers
# stay in English by design and are not wrapped at all.
# Id segments allow uppercase: action-table ids derive from `ActionId` variants
# (`shortcuts.action.OpenPrevLink.label`), and a lowercase-only pattern silently
# skipped all 186 of them. The first segment allows `_` too (`startup_failure.*`,
# `plugin_cli.*`); without it a whole screen stayed invisible to the gate.
ID = r"[a-zA-Z][a-zA-Z0-9_]*(?:\.[a-zA-Z0-9_]+)+"
CALL = re.compile(
    r'(?P<kind>named_static_text|named_text|format_named|fixed)\(\s*'
    r'"(?P<id>' + ID + r')"\s*,\s*'
    r'"(?P<english>(?:[^"\\]|\\.)*)"'
)
# Same forms, but the anchor is a same-file `const` (`named_text("plan.empty",
# EMPTY_PLAN_SCROLLBACK)`). Resolved against the file's const table below; an
# unresolvable const (imported from elsewhere) is recorded anchor-less under a
# `kind+const` kind so disappearance still fails the check.
CALL_CONST = re.compile(
    r'(?P<kind>named_static_text|named_text|format_named|fixed)\(\s*'
    r'"(?P<id>' + ID + r')"\s*,\s*'
    r'(?P<const>[A-Z][A-Z0-9_]*)'
)
CONST_DEF = re.compile(
    r'^\s*(?:pub(?:\([^)]*\))?\s+)?const\s+([A-Z][A-Z0-9_]*)\s*(?::\s*[^=;]+)?=\s*"((?:[^"\\]|\\.)*)"',
    re.MULTILINE,
)
# A trailing `\` splices the next line into a string literal
# (`const X: &str = "\` + newline + `  continued";`). Join before const
# extraction; a non-string const (e.g. `[&str; N]`) never matches CONST_DEF
# and stays on the `kind+const` fallback by design.
CONTINUED_STRING = re.compile(r'\\\r?\n\s*')
CONST_KIND_SUFFIX = "+const"
# Indirect form: the `"<english>" => "<id>"` lookup tables in dashboard/slash/
# settings render code, whose arms feed a `named_*` call with the id they pick.
# The left-hand side is the canonical label and is what has to be re-read when
# upstream rewords a state/hint/section name.
TABLE = re.compile(
    r'"(?P<english>(?:[^"\\]|\\.){1,90})"\s*=>\s*"(?P<id>' + ID + r')"'
)
TABLE_KIND = "table"
# English-keyed forms: `.tr("literal")`, `.tr_static("literal")`,
# `.tr_format("literal with {name}", ...)`, `.tr_ctx("literal", "context")`.
# No id; the English text is the key into `en-to-zh.json`.
TR_CALL = re.compile(
    r'\.(?P<kind>tr_static|tr_format|tr|tr_ctx)\(\s*'
    r'"(?P<english>(?:[^"\\]|\\.)*)"'
)
# Same forms with a same-file `const` anchor (`tr_static(MODAL_TITLE)`).
# Resolved and decoded exactly like CALL_CONST below; unresolvable names
# degrade to a `kind+const` entry so disappearance still fails the check.
TR_CALL_CONST = re.compile(
    r'\.(?P<kind>tr_static|tr_format|tr)\(\s*'
    r'(?P<const>[A-Z][A-Z0-9_]*)'
)

# Matches one `{name}` placeholder; shared by the parity check.
PLACEHOLDER = re.compile(r"\{([A-Za-z0-9_]+)\}")


def rust_unescape(raw: str) -> str:
    """Decode a Rust string literal body to its runtime value.

    Covers the escapes the tree uses (`\\n \\r \\t \\\\ \\" \\' \\0 \\xNN
    \\u{XXXX}`); anything else falls back to raw so the gate never crashes
    on syntax it does not know.
    """

    def replace(match: re.Match) -> str:
        escape = match.group(1)
        simple = {
            "n": "\n",
            "r": "\r",
            "t": "\t",
            "\\": "\\",
            '"': '"',
            "'": "'",
            "0": "\0",
        }
        if escape in simple:
            return simple[escape]
        if escape.startswith("x"):
            try:
                return chr(int(escape[1:], 16))
            except ValueError:
                return match.group(0)
        if escape.startswith("u{") and escape.endswith("}"):
            try:
                return chr(int(escape[2:-1], 16))
            except ValueError:
                return match.group(0)
        return match.group(0)

    return re.sub(r"\\(u\{[0-9a-fA-F]+\}|x[0-9a-fA-F]{2}|.)", replace, raw, flags=re.DOTALL)

MAX_SHOWN = 12


def is_excluded(rel: str) -> bool:
    return any(rel.startswith(d + "/") for d in EXCLUDED_DIRS)


def read_sources():
    """All scanned (rel, src) pairs, locale crate excluded."""
    out = []
    for root in ROOTS:
        base = REPO / root
        if not base.exists():
            continue
        for path in base.rglob("*.rs"):
            rel = path.relative_to(REPO).as_posix()
            if is_excluded(rel):
                continue
            try:
                out.append((rel, path.read_text(encoding="utf-8", errors="ignore")))
            except OSError:
                continue
    return out


def scan():
    """Every wrap currently in the tree, keyed by (file, id, kind, english).

    The anchor is part of the key: a handful of ids are called twice with
    different anchors (singular/plural, or a different placeholder set), and
    keying on (file, id) alone silently dropped one of them. Anchors passed
    as consts resolve to the const value (same file first, then crate-wide
    when the name is unambiguous); an unresolvable const degrades to a
    `kind+const` entry that still tracks disappearance.
    """
    sources = read_sources()
    # Crate-wide const table for anchors imported from another file
    # (e.g. MODAL_TITLE). A name with conflicting values stays unresolved.
    global_consts = {}
    conflicted = set()
    for _, src in sources:
        for name, value in CONST_DEF.findall(CONTINUED_STRING.sub("", src)):
            if name in conflicted:
                continue
            if name in global_consts and global_consts[name] != value:
                del global_consts[name]
                conflicted.add(name)
            else:
                global_consts[name] = value
    found = {}
    for rel, src in sources:
            matches = [(m.group("kind"), m.group("id"), m.group("english")) for m in CALL.finditer(src)]
            matches += [(TABLE_KIND, m.group("id"), m.group("english")) for m in TABLE.finditer(src)]
            # English-keyed: id is the decoded English text itself, matching the
            # runtime lookup key in `en-to-zh.json`.
            matches += [
                (m.group("kind"), rust_unescape(m.group("english")), rust_unescape(m.group("english")))
                for m in TR_CALL.finditer(src)
            ]
            consts = dict(CONST_DEF.findall(CONTINUED_STRING.sub("", src)))
            for m in CALL_CONST.finditer(src):
                # Disjoint from CALL above by construction (opening `"` vs
                # uppercase initial), so no double counting. Lowercase variables
                # (`named_text(id, &dynamic)`) deliberately do not match: a
                # runtime value is not a stable anchor.
                kind, id_, name = m.group("kind"), m.group("id"), m.group("const")
                if name in consts:
                    matches.append((kind, id_, consts[name]))
                elif name in global_consts:
                    matches.append((kind, id_, global_consts[name]))
                else:
                    matches.append((kind + CONST_KIND_SUFFIX, id_, name))
            for m in TR_CALL_CONST.finditer(src):
                # Disjoint from TR_CALL above by the same `"`-vs-uppercase split.
                kind, name = m.group("kind"), m.group("const")
                if name in consts:
                    value = rust_unescape(consts[name])
                    matches.append((kind, value, value))
                elif name in global_consts:
                    value = rust_unescape(global_consts[name])
                    matches.append((kind, value, value))
                else:
                    matches.append((kind + CONST_KIND_SUFFIX, name, name))
            for kind, id_, english in matches:
                # No unicode_escape round-trip: the anchor has to compare byte-for-byte
                # with what the source says. Decoding it used to mangle non-ASCII
                # anchors into mojibake in the baseline.
                found[(rel, id_, kind, english)] = {
                    "file": rel,
                    "id": id_,
                    "kind": kind,
                    "english": english,
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
            old[(w["file"], w["id"], w["kind"], w["english"])] = w
    return old


def load_zh_catalog_ids():
    ids = set()
    for rel in ZH_CATALOGS:
        path = REPO / rel
        if not path.exists():
            sys.exit(f"missing catalog: {rel}")
        with open(path, encoding="utf-8") as fh:
            ids.update(json.load(fh))
    return ids


def load_en_to_zh_keys():
    path = REPO / EN_TO_ZH_CATALOG
    if not path.exists():
        return {}
    with open(path, encoding="utf-8") as fh:
        data = json.load(fh)
        return data if isinstance(data, dict) else {}


def cmd_baseline():
    wraps = scan()
    with open(BASELINE, "w", encoding="utf-8", newline="\n") as fh:
        for w in sorted(wraps.values(), key=lambda x: (x["file"], x["id"], x["kind"], x["english"])):
            fh.write(json.dumps(w, ensure_ascii=False) + "\n")
    files = {w["file"] for w in wraps.values()}
    print(f"baseline: {len(wraps)} wraps over {len(files)} files -> {BASELINE}")


def cmd_check():
    old = load_baseline()
    new = scan()
    catalog = load_zh_catalog_ids()
    en_to_zh = load_en_to_zh_keys()

    missing = [w for k, w in old.items() if k not in new]
    added = [w for k, w in new.items() if k not in old]

    # Same (file, id, kind) on both sides = upstream reworded the anchor, which is the
    # one case a reviewer has to re-read the translation for.
    missing_sites = {(w["file"], w["id"], w["kind"]) for w in missing}
    added_sites = {(w["file"], w["id"], w["kind"]) for w in added}
    rewritten = [
        (o, w)
        for o in missing
        for w in added
        if (o["file"], o["id"], o["kind"]) == (w["file"], w["id"], w["kind"])
    ]
    rewritten_sites = {(o["file"], o["id"], o["kind"]) for o, _ in rewritten}

    orphans = [
        w
        for w in new.values()
        if (w["english"] not in en_to_zh if w["kind"] in TR_KINDS else w["id"] not in catalog)
    ]
    bare_braces = [w for w in new.values() if "{}" in w["english"]]

    # A translated value must not introduce `{name}` the English key lacks;
    # `tr_format` substitutes by name, so the extra placeholder would render
    # verbatim. Dropping a name (e.g. the `{s}` plural suffix, absent in
    # Chinese) is safe: unknown arguments are ignored.
    placeholder_mismatch = sorted(
        {
            w["english"]
            for w in new.values()
            if w["kind"] in TR_KINDS
            and w["english"] in en_to_zh
            and not set(PLACEHOLDER.findall(en_to_zh[w["english"]])) <= set(
                PLACEHOLDER.findall(w["english"])
            )
        }
    )

    print(f"baseline {len(old)} wraps | now {len(new)}")
    print(
        f"missing {len(missing)} | added {len(added)} | rewritten {len(rewritten)}"
        f" | untranslated {len(orphans)} | bare-brace {len(bare_braces)}"
        f" | placeholder-mismatch {len(placeholder_mismatch)}"
    )

    if rewritten:
        print("\n=== upstream rewrote the English fallback (re-read the translation) ===")
        for o, n in rewritten[:MAX_SHOWN]:
            print(f'  {n["file"]}: {n["id"]}  [{n["kind"]}]')
            print(f'    - "{o["english"][:70]}"')
            print(f'    + "{n["english"][:70]}"')
        if len(rewritten) > MAX_SHOWN:
            print(f"  ... {len(rewritten) - MAX_SHOWN} more")

    dropped = [w for w in missing if (w["file"], w["id"], w["kind"]) not in rewritten_sites]
    if dropped:
        print("\n=== missing wraps (usually an upstream-side merge resolution) ===")
        by_file = {}
        for w in dropped:
            by_file.setdefault(w["file"], []).append(w)
        for f in sorted(by_file):
            print(f"\n  {f}  ({len(by_file[f])})")
            for w in by_file[f][:8]:
                print(f'    {w["kind"]:18s} {w["id"]}  <- "{w["english"][:60]}"')
            if len(by_file[f]) > 8:
                print(f'    ... {len(by_file[f]) - 8} more')

    fresh = [w for w in added if (w["file"], w["id"], w["kind"]) not in rewritten_sites]
    if fresh:
        print("\n=== new wraps not in the baseline (rerun `baseline` once reviewed) ===")
        for w in fresh[:MAX_SHOWN]:
            print(f'  {w["file"]}: {w["kind"]} {w["id"]}  <- "{w["english"][:60]}"')
        if len(fresh) > MAX_SHOWN:
            print(f"  ... {len(fresh) - MAX_SHOWN} more")

    if orphans:
        print("\n=== wraps with no zh-CN translation (these render English) ===")
        for w in orphans[:MAX_SHOWN]:
            print(f'  {w["file"]}: {w["kind"]} {w["id"]}  <- "{w["english"][:60]}"')
        if len(orphans) > MAX_SHOWN:
            print(f"  ... {len(orphans) - MAX_SHOWN} more")

    if bare_braces:
        print("\n=== bare `{}` in an English anchor (never substituted: use {name}) ===")
        for w in bare_braces[:MAX_SHOWN]:
            print(f'  {w["file"]}: {w["id"]}  <- "{w["english"][:70]}"')

    if placeholder_mismatch:
        print("\n=== en-to-zh.json values using a {name} the English key lacks ===")
        for key in placeholder_mismatch[:MAX_SHOWN]:
            print(f'  "{key[:70]}" -> "{en_to_zh[key][:70]}"')

    failed = bool(missing or added or orphans or bare_braces or placeholder_mismatch)
    if not failed:
        print("\nOK: wraps intact")
    return 1 if failed else 0


if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else "check"
    if cmd == "baseline":
        cmd_baseline()
    elif cmd == "check":
        sys.exit(cmd_check())
    else:
        sys.exit(__doc__)
