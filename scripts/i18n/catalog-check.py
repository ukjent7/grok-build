#!/usr/bin/env python3
"""Gate the id-keyed catalogs against entries no code path can reach.

Why this exists
---------------
`zh-CN-metadata.json` is an id-keyed catalog: `named_text("<id>", english)`
resolves `<id>` against it, falling back to the English literal when the id is
absent. That fallback is what makes a dead entry invisible -- nothing at
runtime ever complains that 80% of the catalog has no call site, and no test
can, because the ids never appear in a failing lookup.

They accumulated during the migration to English-keyed `tr()` (commit
`9b77299b` onward): a surface that used to be id-keyed got re-wrapped as
`tr("literal")`, and the metadata block it left behind was never pruned. The
result was 2882 unreachable entries out of the 3388 both id-keyed catalogs
held (85%), 198 KB of dead weight that also diluted `wrap-check.py`'s orphan
check, since that check is "`wrap id` not in catalog" and the catalog had grown
to cover almost anything. Those are the counts `d5ec9899` pruned
(`zh-CN-metadata.json` 3320 -> 489, `zh-CN.json` 68 -> 17); re-measure them with
`git show d5ec9899 --stat` rather than trusting this paragraph.

What it checks
--------------
An id is considered reachable when any of these holds:

  A. Its exact quoted literal appears in `crates/codegen/**.rs`. Covers plain
     call sites, consts, and helper arguments (`banner_static_text("id", ..)`).
  B. It is the id of a non-`tr` wrap in `wraps.jsonl`. That baseline is
     produced by `wrap-check.py`'s own scanner and CI proves every entry still
     exists in the tree, so it is authoritative -- and it is the only source
     that reliably catches CamelCase ids such as
     `shortcuts.action.Collapse.label`.
  C. It starts with a prefix that a `format!`/`concat!` template proves is
     built at runtime (e.g. `slash.command.{name}.description` covers every
     `slash.command.*` id). Prefixes are derived from the tree rather than
     hardcoded, and kept only when the catalog actually holds a key under them.
     WEAKNESS, stated so nobody over-trusts a green run: C clears a whole
     namespace on a single anchor, so deleting almost any id it covers still
     passes. For the two namespaces that hold most of that weight it is
     replaced by C-prime.
  C'. `settings.setting.*` and `tutorial.topic.*` -- 213 of the catalog ids,
     and the two namespaces C used to clear wholesale -- are checked against an
     enumeration instead: the setting keys registered in `settings/defs.rs`,
     and the topic count in `tutorial_docs.rs`. A renamed setting key or a
     deleted topic now reports its catalog ids as unreachable.
     Still open inside these namespaces: the `<canonical>` segment of
     `settings.setting.<key>.choice.<canonical>.label`. Enum choice literals
     could be listed, but `SettingKind::DynamicEnum` choices (models, voices)
     are built from runtime catalogs at picker-open time, so any enumeration of
     them would red a correct change. The key and leaf segments are checked.

Anything else is unreachable and fails the gate.

The same script also checks `en-to-zh.json` in the direction `wrap-check.py`
cannot:

  D. Every English-keyed entry must appear, escapes decoded, somewhere in the
     sources. `wrap-check.py` proves each wrap has an entry, never the reverse,
     and an entry for a string no call site spells translates nothing -- the miss
     falls back to English, which is what the string already was.

Usage
-----
    python3 scripts/i18n/catalog-check.py            # fail on unreachable ids
    python3 scripts/i18n/catalog-check.py --list     # print them and exit 0

Adding a genuinely dynamic prefix: it is picked up automatically from the
template, as long as the template is a string literal in a `format!`/`concat!`
call. A prefix assembled entirely from constants needs a manual entry in
EXTRA_PREFIXES.

Adding a setting or a tutorial topic needs nothing here: the enumeration reads
`settings/defs.rs` and `tutorial_docs.rs`. Adding a `settings.setting.*` id whose
key is not registered -- or a topic id past the last topic -- does fail, and the
fix is the code, not the gate.
"""

import argparse
import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]  # scripts/i18n/ -> repo root
ROOTS = ["crates/codegen"]
# The locale crate is scanned too, unlike `wrap-check.py`: that tool excludes it
# because it counts *wraps* and the crate is only a definition site, but for
# reachability it is the opposite -- `LocaleContext::setting_label` builds the
# `settings.setting.*` ids, and its tests look ids up by literal. Excluding it
# here drops the only proof that those prefixes exist.
# Skip a stray per-crate `target/` (someone ran cargo inside a crate directory).
# Matched by path component: every relpath under ROOTS begins `crates/codegen/`,
# so the `startswith` test this replaced could never fire against a "target"
# prefix and the filter was decoration.
EXCLUDED_DIR_NAMES = {"target"}
CATALOGS = [
    "crates/codegen/xai-grok-locale/locales/zh-CN-metadata.json",
    "crates/codegen/xai-grok-locale/locales/zh-CN.json",
]
BASELINE = HERE / "wraps.jsonl"
TR_KINDS = {"tr", "tr_static", "tr_format"}
# The English-keyed catalog is the dominant one and had no gate in either
# direction: `wrap-check.py` proves every *wrap* has an entry here, never that
# every entry has a wrap, so a translation of a string the UI stopped rendering
# just keeps taking up space.
EN_TO_ZH = "crates/codegen/xai-grok-locale/locales/en-to-zh.json"

# Prefixes whose templates are not string literals (assembled from consts).
EXTRA_PREFIXES: list[str] = []

# Namespaces rule C would clear on one anchor, narrowed to an enumeration.
ENUMERATED_NAMESPACES = ("settings.setting.", "tutorial.topic.")
SETTINGS_DEFS = "crates/codegen/xai-grok-pager/src/settings/defs.rs"
TUTORIAL_DOCS = "crates/codegen/xai-grok-pager/src/tutorial_docs.rs"
# A registry entry's stable key, spelled either inline or as a `&str` const
# defined in the same file (`key: MAX_THOUGHTS_WIDTH_KEY`).
SETTING_KEY = re.compile(r'key:\s*(?:"([^"]+)"|([A-Za-z_][A-Za-z0-9_]*))')
CONST_STR = re.compile(r'const\s+([A-Za-z_][A-Za-z0-9_]*)\s*:\s*&str\s*=\s*"([^"]+)"')
# One `topic!(..)` call in the `TUTORIAL_TOPICS` list.
TOPIC = re.compile(r'^\s*topic!\(', re.M)


def read(rel: str, what: str) -> str:
    path = REPO / rel
    if not path.exists():
        # Same reason `wrap_ids` exits rather than returning an empty set: an
        # enumeration that silently found nothing checks nothing, and the gate
        # would read as a pass. A moved file is a rename, not a bug to ignore.
        sys.exit(f"missing {what}: {rel}")
    return path.read_text(encoding="utf-8")


def enumerated_prefixes() -> list[str]:
    """Longest-prefix anchors that replace rule C for ENUMERATED_NAMESPACES.

    `settings.setting.<key>.` expands to three shapes -- `.label`,
    `.description`, `.choice.` -- so a misspelt leaf (`...lable`) is caught too.
    The remap in `setting_choice_catalog_key` needs no mirror here: every target
    it names (`theme`, `default_model`) is itself a registered key. A future
    remap to a non-registry key would report that key's choice ids unreachable,
    which is loud and points straight at the enumeration.
    """
    defs = read(SETTINGS_DEFS, "settings registry")
    consts = dict(CONST_STR.findall(defs))
    keys = {inline or consts.get(symbol, symbol)
            for inline, symbol in SETTING_KEY.findall(defs)}
    if not keys:
        sys.exit(f"no setting keys found in {SETTINGS_DEFS}")

    out = set()
    for key in keys:
        out.add(f"{ENUMERATED_NAMESPACES[0]}{key}.label")
        out.add(f"{ENUMERATED_NAMESPACES[0]}{key}.description")
        out.add(f"{ENUMERATED_NAMESPACES[0]}{key}.choice.")

    # `tutorial.rs` spells the ids `index + 1` (the open topic) and `index + 2`
    # (the next-topic hint, only reached when `TUTORIAL_TOPICS.get(index + 1)`
    # is Some), so 1..=topic count is the whole reachable range.
    topics = len(TOPIC.findall(read(TUTORIAL_DOCS, "tutorial topics")))
    if not topics:
        sys.exit(f"no topic!() entries found in {TUTORIAL_DOCS}")
    out.update(f"{ENUMERATED_NAMESPACES[1]}{i}.title" for i in range(1, topics + 1))
    out.update(f"{ENUMERATED_NAMESPACES[1]}{i}.blurb" for i in range(1, topics + 1))
    return sorted(out)


# An id-shaped literal: dot-separated segments, mixed case, digits, dashes
# (`settings.setting.default_model.choice.grok-4.5.label`).
LITERAL_ID = re.compile(r'"([A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z0-9_\-]+){1,8})"')
# `format!("a.b.{x}.c", ..)` / `concat!(..)` templates.
TEMPLATE = re.compile(r'(?:format!|concat!)\s*\(\s*"([^"]*)"')


def source_blob() -> str:
    parts = []
    for root in ROOTS:
        base = REPO / root
        if not base.exists():
            sys.exit(f"missing source root: {root}")
        for path in sorted(base.rglob("*.rs")):
            if EXCLUDED_DIR_NAMES & set(path.parts):
                continue
            parts.append(path.read_text(encoding="utf-8"))
    return "\n".join(parts)


def load_catalogs() -> dict[str, str]:
    merged: dict[str, str] = {}
    for rel in CATALOGS:
        path = REPO / rel
        if not path.exists():
            sys.exit(f"missing catalog: {rel}")
        with open(path, encoding="utf-8") as fh:
            merged.update(json.load(fh))
    return merged


_RUST_ESCAPES = {"n": "\n", "r": "\r", "t": "\t", "0": "\0"}
_U_BRACE = re.compile(r"\\u\{([0-9a-fA-F]+)\}")
_U_SIMPLE = re.compile(r"\\(.)")
_U_CONTINUED = re.compile(r"\\\n[ \t]*")


def decode_rust_escapes(src: str) -> str:
    r"""Rewrite Rust source into the character space the catalog is written in.

    A multi-line English anchor is stored as `\n`, an ellipsis as `\u{2026}`, and
    rustfmt may split a long literal with a trailing `\`. The catalog holds the real
    characters, so comparing raw source text reports every such key as missing --
    decode first, or the orphan check cries wolf and gets switched off.
    """
    src = _U_CONTINUED.sub("", src)
    src = _U_BRACE.sub(lambda m: chr(int(m.group(1), 16)), src)
    return _U_SIMPLE.sub(lambda m: _RUST_ESCAPES.get(m.group(1), m.group(1)), src)


def english_orphans(blob: str) -> list[str]:
    """`en-to-zh.json` keys that no call site can ever spell.

    `tr` looks the English literal up verbatim, so a key absent from the sources
    translates nothing. It stays invisible the same way a dead id does: the lookup
    misses and falls back to English, which is what the string already was.
    """
    with open(REPO / EN_TO_ZH, encoding="utf-8") as fh:
        keys = json.load(fh)
    decoded = decode_rust_escapes(blob)
    return sorted(key for key in keys if key not in decoded)


def wrap_ids() -> set[str]:
    """Ids of non-`tr` wraps: CI proves each one still exists in the tree."""
    if not BASELINE.exists():
        # Returning an empty set here would let the gate pass while checking
        # nothing, which is the failure mode this whole script exists to prevent.
        sys.exit(f"missing baseline: {BASELINE}")
    out = set()
    with open(BASELINE, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            wrap = json.loads(line)
            if wrap.get("kind") not in TR_KINDS and wrap.get("id"):
                out.add(wrap["id"])
    return out


def dynamic_prefixes(blob: str, catalog: dict[str, str]) -> list[str]:
    """Prefixes proven by a runtime template, kept only if the catalog uses them.

    Templates are read off every `format!`/`concat!` literal, so the filter has
    to be strict: a single segment is far too common in ordinary code
    (`auth.{field}`, `model.{name}` are config keys, not catalog ids) and would
    silently mark whole blocks reachable. Catalog id prefixes are always at
    least two segments (`settings.setting.`, `slash.command.`), so require that,
    plus a trailing dot and nothing path- or sentence-shaped.

    A template that only proves an ENUMERATED_NAMESPACES ancestor is dropped:
    those namespaces are checked per-id instead, and keeping the blanket anchor
    here would make the enumeration dead code.
    """
    found = set(EXTRA_PREFIXES)
    for template in TEMPLATE.findall(blob):
        if "{" not in template:
            continue
        prefix = template[: template.find("{")]
        if not prefix.endswith("."):
            continue
        if prefix.count(".") < 2:
            continue
        if any(ch in prefix for ch in " /=\n\t"):
            continue
        if any(ns.startswith(prefix) for ns in ENUMERATED_NAMESPACES):
            continue
        if any(key.startswith(prefix) for key in catalog):
            found.add(prefix)
    return sorted(found)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--list", action="store_true",
                        help="print unreachable ids and exit 0")
    args = parser.parse_args()

    catalog = load_catalogs()
    blob = source_blob()
    literal = set(LITERAL_ID.findall(blob))
    wrapped = wrap_ids()
    prefixes = dynamic_prefixes(blob, catalog)
    enumerated = enumerated_prefixes()

    unreachable = sorted(
        key for key in catalog
        if not (
            key in literal
            or key in wrapped
            or any(key.startswith(p) for p in enumerated)
            or any(key.startswith(p) for p in prefixes)
        )
    )

    orphans = english_orphans(blob)

    print(f"catalog {len(catalog)} ids | literal {len(literal)}"
          f" | wrapped {len(wrapped)} | enumerated {len(enumerated)}"
          f" | dynamic prefixes {len(prefixes)}"
          f" | en-to-zh orphans {len(orphans)}")
    if not unreachable and not orphans:
        print("OK: every catalog id is reachable")
        return 0

    if unreachable:
        print(f"\n=== {len(unreachable)} catalog ids no code path can reach ===")
        print("These never resolve: the English fallback at the call site renders")
        print("instead. Either wire the id up or drop the entry.")
        print("Under settings.setting./tutorial.topic. the id is checked against the")
        print("registry and the topic list, so a rename there also lands here.")
        for key in unreachable[:40]:
            print(f"  {key}")
        if len(unreachable) > 40:
            print(f"  ... {len(unreachable) - 40} more")

    if orphans:
        print(f"\n=== {len(orphans)} en-to-zh.json keys no call site can spell ===")
        print("`tr` matches the English literal verbatim, so these translate")
        print("nothing. Drop them, or wire up the string they were meant for.")
        for key in orphans[:40]:
            print(f"  {key!r}")
        if len(orphans) > 40:
            print(f"  ... {len(orphans) - 40} more")
    return 0 if args.list else 1


if __name__ == "__main__":
    sys.exit(main())
