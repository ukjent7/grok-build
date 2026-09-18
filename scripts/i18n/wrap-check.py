#!/usr/bin/env python3
"""i18n wrap baseline + drift check for the grok-build fork.

Why this exists
---------------
The zh-CN localization wraps UI string literals in place, e.g.

    crate::locale::ctx().tr_static("nav")

The wrapped surface is whatever `wraps.jsonl` lists -- read it for the current
scale rather than trusting a number here, which would rot on the next wrap.
When upstream rewrites a line we wrapped, a `git merge` conflicts and the usual
way to clear it fast is to take the upstream side -- which silently drops the
wrap. That loss is invisible at runtime (the fallback just renders English
again), so it is never noticed until a user reports a half-English UI.

What it does
------------
  baseline  Scan the tree, rewrite scripts/i18n/wraps.jsonl from what is there.
  check     Compare the tree against the baseline; non-zero exit on drift.

Six failure classes, all fatal on `check`:
  1. missing      a wrap the baseline has and the tree no longer does (dropped
                  by a merge resolution).
  2. added        a wrap the tree has and the baseline does not. Intentional
                  additions must be baselined so the next upstream sync can
                  still tell a dropped wrap from an untouched one.
  3. thinned      an anchor the baseline recorded at N wrap sites that the tree
                  now wraps at fewer. One key can cover several sites (same
                  English string, several call sites in one file), so membership
                  alone cannot see all but one of them being unwrapped.
  4. untranslated a wrap whose id has no zh-CN entry. It renders English, so the
                  interface goes half-Chinese with nothing else to notice it.
                  English-keyed `tr` wraps check against `en-to-zh.json` instead.
  5. bare_braces  a bare `{}` in the English anchor. `named_*`/`format_named`/
                  `tr_format` substitute named `{placeholder}` only, so `{}` would
                  reach the screen verbatim. Rejected by design (see
                  `english_fallback_survives_an_unknown_id_verbatim`).
  6. placeholder_mismatch  an `en-to-zh.json` value uses a `{name}` the English
                  key lacks. `tr_format` substitutes by name, so the extra
                  placeholder would reach the screen verbatim. Convention is
                  single-name: rename, never dual-pass (see `tr_format` docs).

English-keyed (`tr`) anchors are stored decoded to their runtime value: source
`"a\nb"` and `"a<newline>b"` are the same lookup. `rust_unescape` below must
stay in sync with the Rust string escapes the tree actually uses; unknown
escapes fall back to raw so the gate never crashes on new syntax.

Classes 1-2 are reported pairwise as `rewritten` when the same (file, id, kind)
appears on both sides -- that is the shape of "upstream rewrote the line".

Each baseline record also carries `locs`: for every occurrence of the anchor in
the file, the enclosing function, the anchor's index among same-value string
literals in that function (or -1 for a const anchor), and whether the anchor was
a literal or a const. `check` does read it -- the "thinned" class compares
`len(locs)` per key, so a baseline written without `locs` raises KeyError instead
of passing quietly -- and it is what lets `sync-report.py` re-apply a dropped wrap
to the exact literal after upstream overwrites the line, instead of guessing from
the English text alone (which mis-fires on test assertions and log lines that read
the same).

Usage (from anywhere; paths are resolved from this file's location)
-------------------------------------------------------------------
  python3 scripts/i18n/wrap-check.py baseline   # after adding/removing wraps
  python3 scripts/i18n/wrap-check.py check      # after every upstream sync
  python3 scripts/i18n/wrap-check.py diff --diff-base <rev>
      # list UI copy added since <rev> that nobody wrapped. Advisory: this never
      # affects the exit code, and it should stay that way until the scanner can
      # tell a real leak apart. On the current tree all 19 hits are artifacts --
      # const initializers whose value `scan` already records, match-arm needles,
      # a `format_named` template, and a `#[path]`-included test file. Promote it
      # to a gate only once those are filtered out and the list is empty.
      # `check --diff-base <rev>` does the same scan after its baseline work.

Only reports; never edits source. Exit codes: 0 clean, 1 drift, 2 usage error.
`sync-report.py` builds on this file's scanner for the same tree.
"""

import json
import re
import subprocess
import sys
from pathlib import Path

# Imported by sync-report.py; a .pyc next to the sources is noise in a tree
# whose only Python is these two scripts.
sys.dont_write_bytecode = True

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
TR_KINDS = {"tr", "tr_static", "tr_format"}

# Direct wrap forms. `english` is the fallback that stays at the call site, so it
# doubles as the anchor for re-application. The `fixed(...)` form used to be matched
# here too: `FixedCopy` is gone (error_display.rs holds English-keyed copy now) and
# `scan` reads only the worktree, so no history needs the alternation -- it matched
# nothing and made every consumer carry a case that cannot occur.
# English-keyed forms (`tr`, `tr_static`, `tr_format`) are the
# preferred tier going forward: no invented id, upstream rewording falls back
# to English without a baseline entry to update. Fragments and identifiers
# stay in English by design and are not wrapped at all.
# Id segments allow uppercase: action-table ids derive from `ActionId` variants
# (`shortcuts.action.OpenPrevLink.label`), and a lowercase-only pattern silently
# skipped all 186 of them. The first segment allows `_` too (`startup_failure.*`,
# `plugin_cli.*`); without it a whole screen stayed invisible to the gate.
ID = r"[a-zA-Z][a-zA-Z0-9_]*(?:\.[a-zA-Z0-9_]+)+"
CALL = re.compile(
    r'(?P<kind>named_static_text|named_text|format_named)\(\s*'
    r'"(?P<id>' + ID + r')"\s*,\s*'
    r'"(?P<english>(?:[^"\\]|\\.)*)"'
)
# Same forms, but the anchor is a same-file `const` (`named_text("plan.empty",
# EMPTY_PLAN_SCROLLBACK)`). Resolved against the file's const table below; an
# unresolvable const (imported from elsewhere) is recorded anchor-less under a
# `kind+const` kind so disappearance still fails the check.
CALL_CONST = re.compile(
    r'(?P<kind>named_static_text|named_text|format_named)\(\s*'
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
# English-keyed forms: `.tr("literal")`, `.tr_static("literal")`,
# `.tr_format("literal with {name}", ...)`.
# No id; the English text is the key into `en-to-zh.json`.
TR_CALL = re.compile(
    r'\.(?P<kind>tr_static|tr_format|tr)\(\s*'
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


def lex_spans(src):
    """`(kind, start, end, value)` for every string literal and comment in `src`.

    `kind` is "str", "rawstr", "char" or "comment". `start`/`end` bound the whole
    token, quotes included. `value` is the decoded body for "str", the raw body
    for "rawstr", and None otherwise.

    A hand-rolled lexer beats a regex here: Rust raw strings (`r#"..."#`) ignore
    `\\`, and block comments nest. Either shortcut would mis-place every offset
    after the first such construct.
    """
    spans = []
    i, n = 0, len(src)
    while i < n:
        two = src[i:i + 2]
        if two == "//":
            end = src.find("\n", i)
            end = n if end < 0 else end
            spans.append(("comment", i, end, None))
            i = end
        elif two == "/*":
            depth, j = 1, i + 2
            while j < n and depth:
                if src[j:j + 2] == "/*":
                    depth += 1
                    j += 2
                elif src[j:j + 2] == "*/":
                    depth -= 1
                    j += 2
                else:
                    j += 1
            spans.append(("comment", i, j, None))
            i = j
        elif src[i] == "r" and src[i + 1:i + 2] in ('#', '"'):
            # Raw string. A raw *identifier* (`r#match`) also starts this way but
            # has no quote after the hashes, so it falls through as plain code.
            j = i + 1
            hashes = 0
            while j < n and src[j] == "#":
                hashes += 1
                j += 1
            if j < n and src[j] == '"':
                close = '"' + "#" * hashes
                found = src.find(close, j + 1)
                end = n if found < 0 else found + len(close)
                body_end = n if found < 0 else found
                spans.append(("rawstr", i, end, src[j + 1:body_end]))
                i = end
            else:
                i += 1
        elif src[i] == '"':
            j, closed = i + 1, False
            while j < n:
                if src[j] == "\\":
                    j += 2
                    continue
                if src[j] == '"':
                    closed = True
                    j += 1
                    break
                j += 1
            end = min(j, n)
            spans.append(
                ("str", i, end, rust_unescape(src[i + 1:end - 1 if closed else end]))
            )
            i = end
        elif src[i] == "'":
            # `'a'` / `'\n'` is a char literal; `'a` is a lifetime. Both hold no
            # braces, so masking the literal and merely skipping the tick for a
            # lifetime is enough either way.
            m = re.match(r"'(?:\\.|[^'\\\n])'", src[i:])
            if m:
                spans.append(("char", i, i + m.end(), None))
                i += m.end()
            else:
                i += 1
        else:
            i += 1
    return spans


def mask_code(src):
    """`src` with every string/char literal and comment blanked, offsets kept.

    Brace counting must not see a `{` that lives inside a string or a comment.
    Every masked character becomes a space except newlines, so offsets stay
    valid in the original.
    """
    out = list(src)
    for _, start, end, _ in lex_spans(src):
        for k in range(start, end):
            if out[k] != "\n":
                out[k] = " "
    return "".join(out)


FN_DECL = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)")


def fn_spans(src):
    """`(body_start, body_end, name)` for every `fn` that has a body.

    `body_start` is the offset of the opening `{`, `body_end` of the matching
    `}`. Trait declarations (`fn f(&self);`) have no body and are skipped: the
    first `;` or `{` after the parameter list decides which this is.
    """
    code = mask_code(src)
    spans = []
    for m in FN_DECL.finditer(code):
        name = m.group(1)
        i = code.find("(", m.end())
        if i < 0:
            continue
        depth, j = 0, i
        while j < len(code):
            if code[j] == "(":
                depth += 1
            elif code[j] == ")":
                depth -= 1
                if depth == 0:
                    j += 1
                    break
            j += 1
        # Walk the return type / where-clause to the body brace. A `;` at
        # bracket depth 0 means a declaration; `-> [u8; 4]` keeps the `;` inside
        # `[]`, so bracket depth has to be tracked to not stop early. `<`/`>`
        # are counted too and clamped at 0, which absorbs the `>` of `->`.
        depth, k = 0, j
        while k < len(code):
            c = code[k]
            if c in "([<":
                depth += 1
            elif c in ")]>":
                depth = max(0, depth - 1)
            elif c == "{" and depth == 0:
                break
            elif c == ";" and depth == 0:
                k = -1
                break
            k += 1
        if k < 0 or k >= len(code) or code[k] != "{":
            continue
        depth, e = 0, k
        while e < len(code):
            if code[e] == "{":
                depth += 1
            elif code[e] == "}":
                depth -= 1
                if depth == 0:
                    break
            e += 1
        # An unbalanced body (an unclosed block comment can swallow the closing
        # brace) would otherwise yield a span running to EOF and silently claim
        # every later offset. Skip it: a missing anchor beats a wrong one.
        if e >= len(code):
            continue
        spans.append((k, e, name))
    return spans


def enclosing_fn(spans, offset):
    """Name of the innermost function body containing `offset`, else "".

    "" means top-level (a `const`, a `static`, an `impl`-level item): those have
    no function anchor and are reported locator-less rather than mis-attributed.
    """
    best = None
    for start, end, name in spans:
        if start <= offset <= end and (best is None or start > best[0]):
            best = (start, end, name)
    return best[2] if best else ""


def wrapped_offsets(src):
    """Offset map marking every string literal that an existing wrap holds."""
    taken = bytearray(len(src))
    for pattern in (CALL, TR_CALL):
        for m in pattern.finditer(src):
            for k in range(*m.span("english")):
                taken[k] = 1
    return taken


def _join_same_length(src):
    """`src` with line continuations blanked out, offsets preserved.

    Const matching needs the join because a literal may be split across lines, but
    the spans are applied to the original text, so the replacement keeps the length.
    """
    return CONTINUED_STRING.sub(lambda m: " " * len(m.group(0)), src)


def taken_offsets(src):
    """Offsets holding a literal that must never be re-wrapped.

    Three groups on top of plain wrap call sites: comment/char/raw-literal tokens (a
    raw string does not honour escapes, so re-escaping one would corrupt it), and
    const initializers (wrapping a const translates every use of it, not just the UI
    call site). `cmd_diff` and `sync-report.py` both call this one function, so the
    two "new unwrapped copy" reports cannot disagree about what counts as taken.
    """
    taken = wrapped_offsets(src)
    for kind, start, end, _ in lex_spans(src):
        if kind != "str":
            for k in range(start, end):
                taken[k] = 1
    for m in CONST_DEF.finditer(_join_same_length(src)):
        for k in range(*m.span(2)):
            taken[k] = 1
    return taken


# Any `{...}` group, format spec included (`{:>ord_width$}`, `{0}`, `{name:?}`).
BRACED = re.compile(r"\{[^{}]*\}")


def looks_like_copy(text):
    """Heuristic for "this is screen copy", not an identifier or a log format.

    Tuned conservative on purpose: a gate that cries wolf gets switched off, so
    anything ambiguous stays unflagged and is caught by review instead.
    """
    if len(text) < 8 or text.startswith("--") or "://" in text:
        return False
    if " " not in text:
        return False
    if not re.search(r"[A-Za-z]", BRACED.sub("", text)):
        return False  # `{:>ord_width$} `: pure format machinery, never copy
    if re.fullmatch(r"[a-z0-9_.:/\\{}\- ]+", text):
        return False
    if re.search(r"\{[^{}]*:[^{}]*\}", text):
        return False  # `{x:?}`, `{x:>8}`: format specs, never copy. `{name}` and `{}` still are.
    if re.match(r"(?i)^(expected|assert|got|want|actual)\b", text):
        return False
    return sum(c.isalpha() for c in text) / max(len(text), 1) > 0.5


def git(*args):
    result = subprocess.run(
        ["git", *args], cwd=str(REPO), capture_output=True, text=True,
        encoding="utf-8", errors="replace")
    if result.returncode != 0:
        sys.exit("git %s failed: %s" % (" ".join(args), result.stderr.strip()))
    return result.stdout


OPEN_CALL = re.compile(r"([A-Za-z_][A-Za-z0-9_]*)(!?)\s*\($")


def enclosing_open_paren(src, offset):
    """Index of the `(` of the innermost call whose arguments hold `offset`."""
    depth, i = 0, offset - 1
    while i >= 0:
        if src[i] == ")":
            depth += 1
        elif src[i] == "(":
            if depth == 0:
                return i
            depth -= 1
        i -= 1
    return -1


def enclosing_call(src, offset):
    """Name of that call, `!` kept if it is a macro.

    The `!` is what lets the caller tell `error!` (a log macro) from a function
    that merely happens to be called `error`. Without it, every
    `tracing::error!("...")` in the tree reads as a render site.
    """
    i = enclosing_open_paren(src, offset)
    if i < 0:
        return ""
    m = OPEN_CALL.search(src[:i + 1])
    return (m.group(1) + m.group(2)) if m else ""


# `session.field.last_turn`, `settings.auto_approve.label`: a dotted, lowercase
# path is how this tree spells a catalog id.
DOTTED_ID = re.compile(r"^[a-z][a-z0-9_]*(?:\.[a-z0-9_]+)+$")


def localized_by_id(src, offset):
    """Is `offset`'s literal the English of an `("<dotted.id>", "<english>")` pair?

    Helpers with that shape localize internally, so the call site carries no
    `tr`/`named_*` for the scanner to see and the English reads as bare copy.
    Structural rather than a name list because the helpers are re-declared
    locally -- `let field = |id, english, value| (ctx.named_text(id, english),
    value)` -- so no set of names covers them. It is also *safer* than a name
    list: `usage_modal.rs` has an unrelated `field(label, value, compact)` whose
    first argument is a bare label, and the dotted test rejects it.
    """
    open_paren = enclosing_open_paren(src, offset)
    if open_paren < 0:
        return False
    code = mask_code(src)
    args = []
    for kind, s, _e, value in lex_spans(src):
        if s <= open_paren:
            continue
        nest = (code.count("(", open_paren + 1, s)
                - code.count(")", open_paren + 1, s))
        if nest < 0:
            break  # the call's own `)` closed
        if nest or kind not in ("str", "rawstr"):
            continue
        args.append((s, value))
        if len(args) == 2:
            break
    return (len(args) == 2
            and args[0][0] != offset  # the English is not the id itself
            and DOTTED_ID.match(args[0][1]) is not None
            and not DOTTED_ID.match(args[1][1]))


# `#[cfg(test)] mod tests { .. }`: assertions and fixtures, never rendered.
# The literal in `assert!(has("Last turn", ..))` reads exactly like a column
# header, and that shape is the most common false positive this scan has.
CFG_TEST = re.compile(r"#\[cfg\(test\)\]")


def test_spans(src):
    """Ranges covered by a `#[cfg(test)]` block."""
    code = mask_code(src)
    spans = []
    for m in CFG_TEST.finditer(code):
        start = code.find("{", m.end())
        if start < 0 or ";" in code[m.end():start]:
            continue  # `#[cfg(test)] use ..;` carries no block
        depth, i = 0, start
        while i < len(code):
            if code[i] == "{":
                depth += 1
            elif code[i] == "}":
                depth -= 1
                if depth == 0:
                    spans.append((m.start(), i + 1))
                    break
            i += 1
    return spans


# Helpers whose contract is "the English argument is a lookup key, localized
# inside", where the id is *not* the first argument (`localized_by_id` covers
# those). Each one ends in `ctx().setting_*` or `ctx().named_static_text`.
LOCALIZING_HELPERS = (
    "save_setting_bool_toast",
    "save_setting_choice_toast",
    "save_setting_value_toast",
    "setting_already_default_toast",
    "setting_cleared_toast",
    "banner_static_text",
)

# `std`/`core` methods whose string argument is a needle or a pattern, never
# something a user reads. Without this, `line.contains("Screen mode")` reads as
# untranslated copy.
NON_COPY_METHODS = (
    "contains", "starts_with", "ends_with", "strip_prefix", "strip_suffix",
    "trim_start_matches", "trim_end_matches", "match_indices", "split",
    "rsplit", "splitn", "split_once", "find", "rfind", "replace",
    "is_match", "expect", "unwrap_or", "unwrap_or_else", "unwrap_or_default",
)

# Macros (matched with the `!`) whose string argument never reaches the screen:
#   * tracing/log levels and stdio: a log line or a backtrace frame;
#   * assertion/panic: a test log;
#   * anyhow error plumbing: a CLI/diagnostic message, English by convention
#     (structured fields, diagnostics, logs, CLI and headless all stay English);
#   * `json!`: a wire payload -- the string is a protocol field, not a label.
# `format!`/`write!`/`writeln!` are deliberately absent: a formatted string is
# exactly where screen copy lives (`format!("{} of {}", ..)`).
NON_COPY_MACROS = (
    "error!", "warn!", "info!", "debug!", "trace!",
    "println!", "print!", "eprintln!", "eprint!", "dbg!",
    "assert!", "assert_eq!", "assert_ne!",
    "debug_assert!", "debug_assert_eq!", "debug_assert_ne!",
    "panic!", "unreachable!", "todo!", "unimplemented!",
    "anyhow!", "bail!", "ensure!", "format_err!",
    "json!",
)

# Build scripts emit code, they never render it: `pub const RELEASE: ...` is a
# string written into a generated .rs, not a label a user reads.
NON_UI_FILES = ("build.rs",)

HUNK = re.compile(r"^@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@")


def is_unlocalized_copy(src, start, end, value, taken, tests, known):
    """The whole filter chain: is this literal unlocalized screen copy?

    Shared by `diff` (per push) and sync-report's new-copy list (per sync), and
    both now hand it the same `taken` from `taken_offsets` plus the same `known`
    catalog set, so neither can call a literal clean while the other calls it
    unlocalized. What still differs is scope -- which files each one walks, and
    what each means by "new" -- so a shorter sync-report list is not a filter that
    drifted; check the file set before assuming a disagreement is a bug.
    """
    if value in known or not looks_like_copy(value):
        return False
    if any(taken[k] for k in range(start, end)):
        return False
    if any(s <= start < e for s, e in tests):
        return False
    caller = enclosing_call(src, start)
    if caller in LOCALIZING_HELPERS or caller in NON_COPY_METHODS:
        return False
    if caller in NON_COPY_MACROS:
        return False
    return not localized_by_id(src, start)


def added_lines(base):
    """`{rel path: new-file line numbers}` that this change introduces.

    Keyed on lines, not on literal text: `/model <name> [effort]` appears both
    in the `usage:` metadata and in the error path, and the change touches only
    the second. Matching by text reported both.

    One `git diff` for the whole change, not one per file: a monorepo sync
    touches 300-1900 files, i.e. ~1000 process spawns for the same answer.
    """
    out = {}
    rel = None
    for line in git("diff", "--no-color", "-U0", base).splitlines():
        if line.startswith("+++ "):
            path = line[4:].split("\t")[0].strip().strip('"')
            rel = None if path == "/dev/null" else path[2:] if path.startswith("b/") else path
            if rel is not None:
                out.setdefault(rel, set())
        else:
            m = HUNK.match(line)
            if m is None or rel is None:
                continue
            start = int(m.group(1))
            count = int(m.group(2)) if m.group(2) is not None else 1
            if count:  # `+0,0` is a pure deletion: nothing added on the new side
                out[rel].update(range(start, start + count))
    return out


def cmd_diff(base):
    """List UI copy added since `base` that nobody wrapped.

    The baseline `check` proves the wraps we already had survived; this is the
    other direction. It works on the *current* file -- so a literal rustfmt
    pushed onto its own line still reads as wrapped -- and flags a literal only
    when its own line is one the change added.

    Report only, with no switch to make it fail a build. Deciding whether a bare
    literal is user copy is a heuristic, and across the whole range since the last
    upstream sync every hit was something the scanner cannot see past rather than a
    leak: a const initializer whose value `scan` already records, a match-arm
    needle, a `format_named` template, and a test file pulled in through `#[path]`.
    A gate that is wrong every time it fires gets switched off, which is how the
    previous `--fail-on-diff` flag ended up with no caller; this stays a review aid
    until those four cases are filtered out and the list runs clean.
    """
    known = set(load_en_to_zh_keys()) | load_zh_catalog_ids()
    hits = []
    for rel, fresh in sorted(added_lines(base).items()):
        if not fresh or not rel.endswith(".rs"):
            continue
        if is_excluded(rel) or rel.endswith(NON_UI_FILES):
            continue
        path = REPO / rel
        if not path.exists():
            continue
        src = path.read_text(encoding="utf-8", errors="ignore")
        taken = taken_offsets(src)
        tests = test_spans(src)
        for kind, start, end, value in lex_spans(src):
            if kind != "str":
                continue
            line_no = src.count("\n", 0, start)
            if line_no + 1 not in fresh:
                continue
            if is_unlocalized_copy(src, start, end, value, taken, tests, known):
                hits.append((rel, line_no + 1, value))

    print("\n=== added since %s: bare string literals that read as UI copy ===" % base)
    if not hits:
        print("  none")
        return 0
    for rel, line_no, value in hits[:MAX_SHOWN]:
        print('  %s:%d  "%s"' % (rel, line_no, value[:70]))
    if len(hits) > MAX_SHOWN:
        print("  ... %d more" % (len(hits) - MAX_SHOWN))
    print("  wrap it, or leave it English on purpose and say so at the call site.")
    print("  (advisory: this never changes the exit code -- see the `diff` note above)")
    return 0


def scan():
    """Every wrap currently in the tree, keyed by (file, id, kind, english).

    The anchor is part of the key: a handful of ids are called twice with
    different anchors (singular/plural, or a different placeholder set), and
    keying on (file, id) alone silently dropped one of them. Anchors passed
    as consts resolve to the const value (same file first, then crate-wide
    when the name is unambiguous); an unresolvable const degrades to a
    `kind+const` entry that still tracks disappearance.

    Each record also gets `locs`, the function anchor described in the module
    docstring. It is the only field `sync-report.py` needs beyond the key.
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
        # Offsets are carried alongside each match so the wrap can be anchored
        # to its enclosing function; the record itself stays keyed on the four
        # stable fields, so `check` semantics do not change. `lit` is the offset
        # of the opening quote of the anchor literal, or -1 for a const anchor,
        # which has no literal at the call site.
        matches = [
            (m.group("kind"), m.group("id"), m.group("english"), m.start(),
             m.start("english") - 1, "lit")
            for m in CALL.finditer(src)
        ]
        # English-keyed: id is the decoded English text itself, matching the
        # runtime lookup key in `en-to-zh.json`.
        matches += [
            (
                m.group("kind"),
                rust_unescape(m.group("english")),
                rust_unescape(m.group("english")),
                m.start(),
                m.start("english") - 1,
                "lit",
            )
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
                matches.append((kind, id_, consts[name], m.start(), -1, "const"))
            elif name in global_consts:
                matches.append((kind, id_, global_consts[name], m.start(), -1, "const"))
            else:
                matches.append(
                    (kind + CONST_KIND_SUFFIX, id_, name, m.start(), -1, "const"))
        for m in TR_CALL_CONST.finditer(src):
            # Disjoint from TR_CALL above by the same `"`-vs-uppercase split.
            kind, name = m.group("kind"), m.group("const")
            if name in consts:
                value = rust_unescape(consts[name])
                matches.append((kind, value, value, m.start(), -1, "const"))
            elif name in global_consts:
                value = rust_unescape(global_consts[name])
                matches.append((kind, value, value, m.start(), -1, "const"))
            else:
                matches.append(
                    (kind + CONST_KIND_SUFFIX, name, name, m.start(), -1, "const"))
        spans = fn_spans(src)
        # Every string literal in source order, numbered within its
        # (function, value) group. `occ` is that number. It has to count *all*
        # same-value literals, wrapped or not: the restore side re-derives the
        # same list and a bounds check against it is what stops a re-apply from
        # landing on a lookalike -- `prefix == "Creating "` reads the same as the
        # label but is comparison logic, and translating it silently breaks the
        # branch.
        seq = {}
        lit_occ = {}
        for lexed_kind, start, _end, value in lex_spans(src):
            if lexed_kind != "str":
                continue
            group = (enclosing_fn(spans, start), value)
            occ = seq.get(group, 0)
            seq[group] = occ + 1
            lit_occ[start] = (group[0], occ)
        for kind, id_, english, pos, lit, form in sorted(matches, key=lambda t: t[3]):
            key = (rel, id_, kind, english)
            # No unicode_escape round-trip: the anchor has to compare
            # byte-for-byte with what the source says. Decoding it used to
            # mangle non-ASCII anchors into mojibake in the baseline.
            rec = found.get(key)
            if rec is None:
                rec = found[key] = {
                    "file": rel,
                    "id": id_,
                    "kind": kind,
                    "english": english,
                    "locs": [],
                }
            if lit in lit_occ:
                fname, occ = lit_occ[lit]
            else:
                # Const anchor: no literal to number, but the function is still
                # worth recording for whoever reads the report.
                fname, occ = enclosing_fn(spans, pos), -1
            rec["locs"].append([fname, occ, form])
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

    # A key can carry several wrap sites: the same English anchor wrapped in more
    # than one place in one file (`locs` lists them). Key membership alone would
    # let every site but one be unwrapped and still print OK, so compare counts.
    # Key element 1 is None for English-keyed tr wraps, so order by file alone.
    thinned = sorted(
        (
            (k, len(old[k]["locs"]), len(new[k]["locs"]))
            for k in old
            if k in new and len(new[k]["locs"]) < len(old[k]["locs"])
        ),
        key=lambda row: row[0][0],
    )

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
        f" | thinned {len(thinned)}"
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

    if thinned:
        print("\n=== anchors that lost wrap sites (key still present, copy now renders English) ===")
        for k, was, now in thinned[:MAX_SHOWN]:
            print(f'  {k[0]}: {k[2]} {k[1] if k[1] is not None else k[3]}  {was} -> {now} sites')
        if len(thinned) > MAX_SHOWN:
            print(f"  ... {len(thinned) - MAX_SHOWN} more")

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

    failed = bool(missing or added or thinned or orphans or bare_braces or placeholder_mismatch)
    if not failed:
        print("\nOK: wraps intact")
    return 1 if failed else 0


if __name__ == "__main__":
    argv = sys.argv[1:]
    cmd = argv[0] if argv else "check"

    def diff_base_arg():
        """`--diff-base <rev>`, or None when not asked for."""
        if "--diff-base" not in argv:
            return None
        i = argv.index("--diff-base")
        if i + 1 >= len(argv):
            sys.exit("--diff-base needs a revision")
        return argv[i + 1]

    if cmd == "baseline":
        cmd_baseline()
    elif cmd == "check":
        status = cmd_check()
        base = diff_base_arg()
        if base:
            cmd_diff(base)
        sys.exit(status)
    elif cmd == "diff":
        # Added-copy scan on its own, so CI can report it as its own step
        # instead of paying for a second full-tree baseline scan. Advisory:
        # its exit status is always 0.
        base = diff_base_arg()
        if not base:
            sys.exit("diff needs --diff-base <rev>")
        cmd_diff(base)
        sys.exit(0)
    else:
        sys.exit(__doc__)
