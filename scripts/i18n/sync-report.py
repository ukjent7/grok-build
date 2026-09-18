#!/usr/bin/env python3
"""i18n sync report for the grok-build fork: what to do after `check` reports drift.

Why this exists
---------------
`wrap-check.py` answers one question: did any wrap disappear? That is enough to
fail a build, but not enough to act on a sync. A reviewer still has to work out
*where* the dropped wrap was before re-applying it by hand.

This script turns that into a list, in three sections:

  A drift    The baseline has the wrap, the tree does not. Each entry carries
             the enclosing function recorded at baseline time, so the wrap goes
             back at the right call site. `--apply` writes those.
  B reworded Same (file, id, kind) with different English: upstream reworded a
             line we had wrapped. The wrap survived but the translation no
             longer matches, so a human has to re-read it.
  C new copy With `--since <rev>`, English literals that appear in the tree but
             not at <rev>, that no catalog covers, restricted to files that
             already carry wraps. Unfiltered this is ~500 lines of logs and test
             assertions; the filter is what makes it a reviewable list.

Section C covers the one gap the gate structurally cannot see. The gate proves
the wraps we already have survived; it can never prove that copy upstream newly
added got translated. That gap is how a UI silently goes half-English.

Why the anchor matters
----------------------
Re-applying a dropped wrap by matching English text alone is not safe: the same
string also appears in test assertions and log lines. Measured on this tree,
text-only matching re-wrapped 528 literals to recover 141 lost wraps -- a 73%
false-positive rate.

The baseline `locs` field narrows it to one function and one literal position
inside that function, and section A writes only when that exact literal is still
there and still unwrapped. Measured against a simulated upstream sync, that
takes the false-positive rate to 0 and re-applies 93-99% of what a partial
overwrite drops. Anything the anchor cannot pin down is reported for a human
instead of guessed at.

A baseline record is keyed on (file, id, kind, english) and deduplicated, so one
record can stand for several call sites; `locs` lists them all.

Restores use the canonical `crate::locale::ctx().<kind>(...)` receiver rather
than whatever form the original call site used. The two are equivalent at
runtime, and the canonical form keeps re-applied wraps uniform.

Usage (from anywhere; paths are resolved from this file's location)
-------------------------------------------------------------------
  python3 scripts/i18n/sync-report.py                    # sections A + B
  python3 scripts/i18n/sync-report.py --since <rev>       # + section C
  python3 scripts/i18n/sync-report.py --apply             # write the A restores

Exit codes: 0 nothing actionable, 1 something to review, 2 usage error.
"""

import importlib.util
import json
import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
MAX_SHOWN = 12


def _load_wrap_check():
    """`wrap-check.py` has a hyphen in its name, so it is not importable by name.

    Sharing its lexer and scanner is the point: a second copy of the raw-string
    and nested-comment rules would drift from the gate's.
    """
    sys.dont_write_bytecode = True  # no __pycache__ in a tree we never import from
    spec = importlib.util.spec_from_file_location("wrap_check", HERE / "wrap-check.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


wc = _load_wrap_check()

# Kinds a literal can be re-wrapped as. A `+const` anchor or a lookup-table arm
# leaves no literal at the call site once dropped, so neither can be re-applied
# mechanically; both are reported for a human instead.
#
# `tr_format` and `format_named` are excluded on purpose, even though their call
# sites do carry a literal: both take an arguments slice after the English
# anchor (`tr_format("...", &[("name", value)]);` `format_named` adds the id in
# front of it), and the baseline record does not preserve those arguments, so a
# re-applied call would not compile. They land in the manual list with their own
# reason.
RESTORABLE = {"tr", "tr_static", "named_text", "named_static_text"}
MANUAL_KINDS = {"tr_format", "format_named"}


def git(*args):
    result = subprocess.run(
        ["git", *args], cwd=str(REPO), capture_output=True, text=True,
        encoding="utf-8", errors="replace")
    if result.returncode != 0:
        sys.exit("git %s failed: %s" % (" ".join(args), result.stderr.strip()))
    return result.stdout


def read_source(path, errors="ignore"):
    """`(text with LF endings, whether the file is CRLF on disk)`.

    The fork's working tree is CRLF (`core.autocrlf=true`) while the repository
    stores LF, and text-mode reads normalise the difference away. Writing has to
    put the convention back: `--apply` touches a handful of lines, but an LF
    write replaces *every* line ending in the file on disk. Git hides that under
    `core.autocrlf=true`, editors and `diff` do not.

    `errors` is `"strict"` on the `--apply` path: the text read here is the text
    written back, so a malformed byte must raise there instead of being
    silently dropped from the file on disk. The dry run stays lenient.
    """
    with open(path, encoding="utf-8", errors=errors, newline="") as fh:
        raw = fh.read()
    return raw.replace("\r\n", "\n"), "\r\n" in raw


def write_source(path, text, crlf):
    with open(path, "w", encoding="utf-8", newline="\r\n" if crlf else "\n") as fh:
        fh.write(text)


def rust_escape(value):
    """Re-escape a decoded string for insertion into a Rust literal.

    Lossy in the harmless direction: `\\x41` decodes to `A` and comes back as
    `A`, which is the same string at runtime.
    """
    out = []
    for ch in value:
        if ch == "\\":
            out.append("\\\\")
        elif ch == '"':
            out.append('\\"')
        elif ch == "\n":
            out.append("\\n")
        elif ch == "\r":
            out.append("\\r")
        elif ch == "\t":
            out.append("\\t")
        elif ch == "\0":
            out.append("\\0")
        else:
            out.append(ch)
    return "".join(out)


def wrap_call(kind, id_, english):
    """Source text that wraps `english` as `kind`."""
    text = rust_escape(english)
    if kind in wc.TR_KINDS:
        return 'crate::locale::ctx().%s("%s")' % (kind, text)
    return 'crate::locale::ctx().%s("%s", "%s")' % (kind, id_, text)


def taken_offsets(src):
    """The one definition, in `wrap-check.py`, shared with its `diff` report."""
    return wc.taken_offsets(src)


def literal_groups(src):
    """`(function, english) -> [(start, end)]`, in source order.

    `occ` indexes into this list, and the baseline built it the same way, so the
    index survives upstream rewriting the surrounding lines. It counts *all*
    same-value literals, wrapped or not, which is what lets a bounds check
    reject a re-apply that would otherwise land on a lookalike.
    """
    spans = wc.fn_spans(src)
    groups = {}
    for kind, start, end, value in wc.lex_spans(src):
        if kind != "str":
            continue
        groups.setdefault((wc.enclosing_fn(spans, start), value), []).append(
            (start, end))
    return groups


def plan_restore(rel, src, missing):
    """`(replacements, restored, manual)` for one file.

    `replacements` are `(start, end, text)` edits to apply right-to-left.
    """
    taken = taken_offsets(src)
    groups = literal_groups(src)
    replacements = []
    manual = []
    for rec in missing:
        kind = rec["kind"]
        if kind in MANUAL_KINDS:
            manual.append(
                (rec, "a %s wrap needs its arguments; the baseline does not record them"
                 % kind))
            continue
        if kind not in RESTORABLE:
            manual.append((rec, "anchor is a %s, not a call-site literal" % kind))
            continue
        sites = []
        refused = None
        for fname, occ, form in rec.get("locs", []):
            if form != "lit":
                continue
            where = "%s#%d" % (fname or "<top-level>", occ)
            group = groups.get((fname, rec["english"]), [])
            if occ < 0 or occ >= len(group):
                refused = "no literal at %s (the function was reshaped)" % where
                break
            start, end = group[occ]
            if any(taken[k] for k in range(start, end)):
                refused = "%s is already wrapped" % where
                break
            sites.append((start, end))
        if refused:
            manual.append((rec, refused))
            continue
        if not sites:
            manual.append((rec, "no call-site literal recorded in the baseline"))
            continue
        for start, end in sites:
            replacements.append(
                (start, end, wrap_call(kind, rec["id"], rec["english"])))
    return replacements, len(replacements), manual


def known_keys():
    keys = set(wc.load_en_to_zh_keys())
    keys |= wc.load_zh_catalog_ids()
    return keys


def new_untranslated(since, baseline_files, known):
    """Section C: literals new since `since` that no catalog covers.

    "New" is by literal text, not by line: this list is a translation work
    queue, so a literal whose line merely moved is not new work. Filtering goes
    through `wc.is_unlocalized_copy`, the same chain `wrap-check.py diff` uses.
    """
    changed = [f for f in git("diff", "--name-only", since).splitlines()
               if f.endswith(".rs")]
    hits = []
    for rel in changed:
        if rel not in baseline_files:
            continue
        old = set()
        for kind, _s, _e, value in wc.lex_spans(git("show", "%s:%s" % (since, rel))):
            if kind == "str":
                old.add(value)
        path = REPO / rel
        if not path.exists():
            continue
        src, _ = read_source(path)
        taken = taken_offsets(src)
        tests = wc.test_spans(src)
        for kind, start, end, value in wc.lex_spans(src):
            if kind != "str" or value in old:
                continue
            if wc.is_unlocalized_copy(src, start, end, value, taken, tests, known):
                hits.append((rel, src.count("\n", 0, start) + 1, value))
    return sorted(set(hits))


def section_a(missing, apply):
    """Restore locatable drift; return (restored, manual, touched files).

    `missing` is computed once in `main` (it also feeds section B) and passed
    in, so the two sections cannot disagree about what counts as lost.
    """
    if not missing:
        return 0, [], []
    by_file = {}
    for rec in missing:
        by_file.setdefault(rec["file"], []).append(rec)

    restored = 0
    manual = []
    touched = []
    for rel in sorted(by_file):
        path = REPO / rel
        if not path.exists():
            manual.extend((rec, "file is gone") for rec in by_file[rel])
            continue
        src, crlf = read_source(path, errors="strict" if apply else "ignore")
        replacements, count, refused = plan_restore(rel, src, by_file[rel])
        manual.extend(refused)
        if not replacements:
            continue
        for start, end, text in sorted(replacements, reverse=True):
            src = src[:start] + text + src[end:]
        restored += count
        touched.append(rel)
        if apply:
            write_source(path, src, crlf)
    return restored, manual, touched


def main(argv):
    apply = "--apply" in argv
    since = None
    if "--since" in argv:
        i = argv.index("--since")
        if i + 1 >= len(argv):
            sys.exit("--since needs a revision")
        since = argv[i + 1]

    baseline = wc.load_baseline()
    current = wc.scan()

    missing = [w for k, w in baseline.items() if k not in current]
    restored, manual, touched = section_a(missing, apply)

    print("baseline %d wraps | now %d" % (len(baseline), len(current)))
    actionable = False

    print("\n=== A drift: the baseline has the wrap, the tree does not ===")
    if not (restored or manual):
        print("  none")
    else:
        actionable = True
        print("  %d lost, %d re-applied%s, %d need a human"
              % (len(missing), restored, "" if apply else " (dry run, use --apply)",
                 len(manual)))
        if touched:
            print("  files %s%s" % ("written: " if apply else "would change: ",
                                    ", ".join(touched)))
        wc.print_hits(
            manual,
            lambda m: '    %s  [%s] %s  <- "%s"\n      %s'
            % (m[0]["file"], m[0]["kind"],
               " ".join("fn %s#%d" % (f or "<top-level>", o)
                        for f, o, _ in m[0].get("locs", [])),
               m[0]["english"][:60], m[1]),
            limit=MAX_SHOWN, indent="    ")

    missing_sites = {(w["file"], w["id"], w["kind"]) for w in missing}
    added = [w for k, w in current.items() if k not in baseline]
    rewritten = sorted({
        (w["file"], w["id"], w["kind"])
        for w in added if (w["file"], w["id"], w["kind"]) in missing_sites
    })
    print("\n=== B reworded: upstream changed the English we had wrapped ===")
    if not rewritten:
        print("  none")
    else:
        actionable = True
        wc.print_hits(
            rewritten,
            lambda t: '  %s  [%s] %s\n    re-read the translation: "%s"'
            % (t[0], t[1], t[2], t[2][:70]),
            limit=MAX_SHOWN)

    print("\n=== C new copy: literals new since %s with no catalog entry ==="
          % (since or "<not checked>"))
    if since:
        hits = new_untranslated(since, {w["file"] for w in baseline.values()},
                                known_keys())
        if not hits:
            print("  none")
        else:
            actionable = True
            wc.print_hits(hits, lambda h: '  %s:%d  "%s"' % (h[0], h[1], h[2][:70]),
                          limit=MAX_SHOWN)
    else:
        print("  pass --since <rev> to check (e.g. the last upstream sync)")

    if apply and touched:
        print("\nwrote %d file(s); now run `wrap-check.py check` to confirm"
              % len(touched))
    return 1 if actionable else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
