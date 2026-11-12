#!/usr/bin/env python3
r"""Writes the two tables the parser reads, from their sources.

    data/make.py <pulldown-cmark clone>

entities.txt: every HTML5 character reference that ends with `;`, a line each, sorted by name
(bytewise): the name, then the code points it stands for, in hex. Read from pulldown-cmark's
table, itself made from https://html.spec.whatwg.org/entities.json.

punct.bin: the code points past ASCII that CommonMark calls punctuation (Unicode's general
categories P and S), as ranges: first and last, three bytes each, big-endian, sorted. From this
Python's `unicodedata`, whose Unicode version the script prints.

commonmark.txt and github.txt: the examples of the CommonMark specification, and those of
GitHub's that are its extensions (tables, strikethrough, task lists, bare links), each as three
lines: `# number section`, the Markdown, the HTML it is to read as. The two texts are escaped:
`\\`, `\n`, `\t`, and `\u{...}` for what is not printable ASCII. Only the tests read them.
"""
import re
import sys
import unicodedata
from pathlib import Path

here = Path(__file__).parent
if len(sys.argv) != 2 or not (Path(sys.argv[1]) / "pulldown-cmark/src/entities.rs").is_file():
    sys.exit("usage: data/make.py <pulldown-cmark clone>")

source = (Path(sys.argv[1]) / "pulldown-cmark/src/entities.rs").read_text()
entities = []
for name, value in re.findall(r'\(b"(\w+)", "((?:\\u\{[0-9A-Fa-f]+\})+)"\)', source):
    points = re.findall(r"\\u\{([0-9A-Fa-f]+)\}", value)
    entities.append((name.encode(), " ".join(f"{int(point, 16):X}" for point in points)))
entities.sort()
(here / "entities.txt").write_text(
    "".join(f"{name.decode()} {points}\n" for name, points in entities)
)

ranges = []
start = None
for point in range(0x80, 0x110000):
    inside = unicodedata.category(chr(point))[0] in "PS"
    if inside and start is None:
        start = point
    if not inside and start is not None:
        ranges.append((start, point - 1))
        start = None
(here / "punct.bin").write_bytes(
    b"".join(first.to_bytes(3, "big") + last.to_bytes(3, "big") for first, last in ranges)
)


def escaped(text):
    out = []
    for ch in text:
        if ch == "\\":
            out.append("\\\\")
        elif ch == "\n":
            out.append("\\n")
        elif ch == "\t":
            out.append("\\t")
        elif " " <= ch <= "~":
            out.append(ch)
        else:
            out.append(f"\\u{{{ord(ch):X}}}")
    return "".join(out)


def examples(spec, extensions):
    """The examples of a specification: all of them, or only those marked as an extension."""
    fence = "`" * 32
    found, section, state, number = [], "", 0, 0
    markdown, html = [], []
    wanted = False
    for line in spec.read_text().splitlines(keepends=True):
        stripped = line.strip()
        if state == 0 and stripped.startswith(fence + " example"):
            state, markdown, html = 1, [], []
            # GitHub's filter of raw HTML is its renderer's, not a way of reading Markdown.
            tag = stripped[len(fence) + 8:].strip()
            wanted = bool(tag) == extensions and tag != "tagfilter"
        elif state == 2 and stripped == fence:
            state = 0
            number += 1
            if wanted:
                found.append((number, section, "".join(markdown), "".join(html)))
        elif state == 1 and stripped == ".":
            state = 2
        elif state == 1:
            markdown.append(line.replace("\u2192", "\t"))
        elif state == 2:
            html.append(line.replace("\u2192", "\t"))
        elif state == 0 and re.match(r"#+ ", line):
            section = re.sub(r"#+ ", "", stripped)
    return found


third_party = Path(sys.argv[1]) / "pulldown-cmark/third_party"
counts = []
for name, spec, extensions in [
    ("commonmark.txt", third_party / "CommonMark/spec.txt", False),
    ("github.txt", third_party / "GitHub/spec.txt", True),
]:
    found = examples(spec, extensions)
    counts.append(f"{len(found)} examples in {name}")
    (here / name).write_text(
        "".join(
            f"# {number} {section}\n{escaped(markdown)}\n{escaped(html)}\n"
            for number, section, markdown, html in found
        )
    )
print(", ".join(counts))
print(f"{len(entities)} entities, {len(ranges)} ranges of punctuation, "
      f"Unicode {unicodedata.unidata_version}")
