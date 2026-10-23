#!/usr/bin/env python3
"""Route user-facing egui strings through youpdf_shared::t().

Companion to scripts/i18n_extract.py: that one finds the unwrapped literals,
this one wraps them. Review the diff — the patterns are text-rendering egui
calls only, but a wrong label here is a wrong string in the UI.

    python3 scripts/i18n_wrap.py

Dynamic text is left alone: it needs format!() around translated fragments,
which no regex should invent.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent / "crates" / "youpdf-app" / "src"

# (regex over the call, index of the string literal inside it)
PATTERNS = [
    (re.compile(r'\.button\(\s*"(?:[^"\\]|\\.)*"'), 0),
    (re.compile(r'\.checkbox\([^;]*?,\s*"(?:[^"\\]|\\.)*"'), 1),
    (re.compile(r'\.radio_value\([^;]*?,\s*"(?:[^"\\]|\\.)*"'), 1),
    (re.compile(r'\.toggle_value\([^;]*?,\s*"(?:[^"\\]|\\.)*"'), 1),
    (re.compile(r'\.selectable_label\([^;]*?,\s*"(?:[^"\\]|\\.)*"'), 1),
    (re.compile(r'\.on_hover_text\(\s*"(?:[^"\\]|\\.)*"'), 0),
    (re.compile(r'\.on_hover_text_at_pointer\(\s*"(?:[^"\\]|\\.)*"'), 0),
    (re.compile(r'\.help\(\s*"(?:[^"\\]|\\.)*"'), 0),
    (re.compile(r'\.heading\(\s*"(?:[^"\\]|\\.)*"'), 0),
    (re.compile(r'\.strong\(\s*"(?:[^"\\]|\\.)*"'), 0),
    (re.compile(r'\.weak\(\s*"(?:[^"\\]|\\.)*"'), 0),
    (re.compile(r'\.label\(\s*"(?:[^"\\]|\\.)*"'), 0),
    (re.compile(r'RichText::new\(\s*"(?:[^"\\]|\\.)*"'), 0),
    (re.compile(r'Window::new\(\s*"(?:[^"\\]|\\.)*"'), 0),
]
QUOTED = re.compile(r'"((?:[^"\\]|\\.)*)"\s*$')
ALREADY = re.compile(r"\bt\(\s*$|\bt\(\s*\n")


def wrap(line):
    """Rewrite one source line, returning it with every literal argument wrapped."""
    out, span = [], 0
    for pattern, _ in PATTERNS:
        for hit in pattern.finditer(line):
            text = QUOTED.search(hit.group(0))
            if not text:
                continue
            # The literal sits at the end of the match: replace exactly those bytes.
            start = hit.end() - (text.end() - text.start())
            before = line[max(span, 0):start]
            if ALREADY.search(before):
                continue
            out.append(line[span:start])
            out.append(f't("{text.group(1)}")')
            span = start + (text.end() - text.start())
    out.append(line[span:])
    return "".join(out)


def ensure_import(path, source):
    """Add TU to the crate's youpdf_shared import when the file needs it and lacks it."""
    if re.search(r"^use youpdf_shared::\{[^}]*\bt\b", source, re.M):
        return source
    match = re.search(r"^use youpdf_shared::([A-Za-z_][A-Za-z0-9_]*);$", source, re.M)
    if match:
        name = match.group(1)
        return source.replace(match.group(0), f"use youpdf_shared::{{t, {name}}};", 1)
    if re.search(r"^use youpdf_shared::\{", source, re.M):
        return re.sub(
            r"^(use youpdf_shared::\{)([^}]*)\}",
            lambda m: f"{m.group(1)}t, {m.group(2)}}}",
            source,
            count=1,
            flags=re.M,
        )
    match = re.search(r"^use (super|crate)::\*;$", source, re.M)
    if match:
        # app/*.rs reach `t` through app.rs's glob import; ai.rs and inspector.rs do not.
        if "t(" in source and "use youpdf_shared" not in source:
            return source[: match.end()] + "\nuse youpdf_shared::t;" + source[match.end():]
    return source


def main():
    changed = []
    for path in sorted(ROOT.rglob("*.rs")):
        source = path.read_text(encoding="utf-8")
        newlines = [wrap(line) for line in source.split("\n")]
        lines_changed = sum(
            1 for a, b in zip(source.split("\n"), newlines) if a != b
        )
        if not lines_changed:
            continue
        changed.append((path.relative_to(ROOT), lines_changed))
        path.write_text(ensure_import(path, "\n".join(newlines)), encoding="utf-8")
    for path, count in changed:
        print(f"  {count:4d}  {path}")
    print(f"{sum(c for _, c in changed)} call sites wrapped")
    return 0


if __name__ == "__main__":
    sys.exit(main())
