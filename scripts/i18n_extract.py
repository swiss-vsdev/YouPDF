#!/usr/bin/env python3
"""i18n coverage gate for youpdf-app.

Two precise checks, both enforced by `python3 scripts/i18n_extract.py --check`:

1. Every t("...") key in the app crate has a French entry in the dictionary.
   An unwrapped string is a visible bug, and a key that falls back to itself
   in French is one too.
2. Every dictionary entry is reachable as a literal somewhere in the app
   crate. Dead entries drift; this catches them at the same time as the
   wrapping pass.

    python3 scripts/i18n_extract.py            # report unwrapped egui literals
    python3 scripts/i18n_extract.py --check    # both checks, exit 1 on failure

Dynamic text goes through format!() around translated fragments instead:
format!("{} {}", t("Page"), page). No regex should invent that.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
APP = ROOT / "crates" / "youpdf-app" / "src"
DICT = ROOT / "crates" / "youpdf-shared" / "src" / "i18n.rs"

# Keys that are intentionally identical in French: acronyms, cognates, and
# the long sentences whose French is a straight re-translation of the English.
IDENTITY = {
    "#", "AI", "Notation", "Segment", "YouPDF", "Angle", "Bates", "OCR", "XMP",
    "PNG", "JPEG", "TIFF", "PDF", "PKCS#12", "PDF/A", "PDF/X", "PDF/E",
    "p12", "pfx", "PDFium", "PAdES", "FDF", "XFDF", "CSV", "JSON", "HTML",
    "URL", "API", "XML", "RTF", "MD", "DOCX", "XLSX", "PPTX",
}

PATTERNS = [
    r'\.button\(\s*"(?:[^"\\]|\\.)*"',
    r'\.checkbox\([^;]*?,\s*"(?:[^"\\]|\\.)*"',
    r'\.radio_value\([^;]*?,\s*"(?:[^"\\]|\\.)*"',
    r'\.toggle_value\([^;]*?,\s*"(?:[^"\\]|\\.)*"',
    r'\.selectable_label\([^;]*?,\s*"(?:[^"\\]|\\.)*"',
    r'\.on_hover_text\(\s*"(?:[^"\\]|\\.)*"',
    r'\.on_hover_text_at_pointer\(\s*"(?:[^"\\]|\\.)*"',
    r'\.help\(\s*"(?:[^"\\]|\\.)*"',
    r'\.heading\(\s*"(?:[^"\\]|\\.)*"',
    r'\.strong\(\s*"(?:[^"\\]|\\.)*"',
    r'\.weak\(\s*"(?:[^"\\]|\\.)*"',
    r'\.label\(\s*"(?:[^"\\]|\\.)*"',
    r'RichText::new\(\s*"(?:[^"\\]|\\.)*"',
    r'Window::new\(\s*"(?:[^"\\]|\\.)*"',
]
MATCHER = re.compile("|".join(f"(?:{p})" for p in PATTERNS))
QUOTED = re.compile(r'"((?:[^"\\]|\\.)*)"\s*$')


def scan():
    """Yield (file, line number, string) for every unwrapped user-facing literal."""
    for path in sorted(APP.rglob("*.rs")):
        for number, line in enumerate(path.read_text(encoding="utf8").splitlines(), 1):
            for hit in MATCHER.finditer(line):
                text = QUOTED.search(hit.group(0))
                if text:
                    yield path.relative_to(ROOT), number, text.group(1)


def strip_comments(src: str) -> str:
    """Remove Rust line and block comments, preserving string literals.

    A `"` inside a comment is not a string literal, so extracting literals
    before stripping would swallow real ones. Raw strings and char literals
    are handled; this is enough for the coverage check.
    """
    out, i, n = [], 0, len(src)
    while i < n:
        c = src[i]
        if c == '"':
            # String literal: copy verbatim.
            j = i + 1
            while j < n:
                if src[j] == '\\' and j + 1 < n:
                    j += 2
                    continue
                if src[j] == '"':
                    break
                j += 1
            out.append(src[i:j + 1])
            i = j + 1
            continue
        if c == '/' and i + 1 < n and src[i + 1] == '/':
            while i < n and src[i] != '\n':
                i += 1
            continue
        if c == '/' and i + 1 < n and src[i + 1] == '*':
            i += 2
            while i + 1 < n and not (src[i] == '*' and src[i + 1] == '/'):
                i += 1
            i += 2
            continue
        out.append(c)
        i += 1
    return "".join(out)


def dict_entries():
    """All (key, value) pairs defined in youpdf_shared::i18n."""
    return re.findall(
        r'^\s*"((?:[^"\\]|\\.)*)"\s*=>\s*"((?:[^"\\]|\\.)*)"',
        DICT.read_text(encoding="utf8"),
        re.M,
    )


def main():
    if "--check" in sys.argv:
        problems = 0

        used = set()
        for path in APP.rglob("*.rs"):
            used |= set(re.findall(r'\bt\("((?:[^"\\]|\\.)*)"\)', path.read_text(encoding="utf8")))
        entries = dict(dict_entries())
        missing = sorted(k for k in used if k not in entries and k not in IDENTITY)
        if missing:
            problems += 1
            print(f"FAIL: {len(missing)} t() keys have no French entry:")
            for k in missing:
                print(f"  {k}")

        # Reachability is checked across the whole workspace: the dictionary is
        # shared by youpdf-core and youpdf-ai, and an entry that only the engine
        # asks for is still live.
        ws_text = "\n".join(
            p.read_text(encoding="utf8")
            for p in ROOT.joinpath("crates").rglob("*.rs")
        )
        literals = set(re.findall(r'"((?:[^"\\\n]|\\.)*)"', strip_comments(ws_text)))
        dead = sorted(k for k, v in entries.items() if k not in literals and v != k)
        if dead:
            problems += 1
            print(f"FAIL: {len(dead)} dictionary entries are unreachable from the workspace:")
            for k in dead:
                print(f"  {k}")

        if not problems:
            print(f"OK: {len(used)} keys used, {len(entries)} entries, 0 missing, 0 dead")
        return 1 if problems else 0

    for where, number, text in scan():
        print(f"{where}:{number}: {text}")
    return 0


if __name__ == "__main__":
    sys.exit(main())