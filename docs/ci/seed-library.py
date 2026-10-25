#!/usr/bin/env python3
"""Build a synthetic Kalam library so CI can screenshot and stress the UI.

Kalam has no fixture library, and CI has no books. Without this the app would
start on an empty Home and every screenshot would show the same "import
something" placeholder -- useless for catching the class of bug that keeps
getting through (grey covers that never fill, a float that is the wrong width).

Writes straight to the catalog schema rather than driving the importer: the
importer is a lot of machinery to stand up just to fill a grid. The files are
real EPUBs, though -- `epub_bytes()` writes a valid zip with a container, an
OPF, a spine, both a nav and an NCX, and six chapters of text. They used to be
the bytes of "not a real epub -- screenshots only", which is enough to point a
grid at a title and a cover, but the reader route opens the file for real: with
a stub, every book landed on the open-error label and the reader screenshot
could not say anything about the reading page.

Usage:
    XDG_DATA_HOME=/tmp/kalam-ci python3 seed-library.py --books 139

The script creates a minimal schema-version-1 catalog. Opening the app runs
its migrations to the current schema, so this also exercises the migration
path used by the screenshot job. The seeded book folders use the same readable
name and short-id format as the app, so startup does not rename them while a
smoke-test route is opening a book.
"""

import argparse
import io
import json
import os
import pathlib
import random
import shutil
import sqlite3
import struct
import sys
import uuid
import unicodedata
import zipfile
import zlib
from datetime import datetime, timedelta, timezone
from xml.sax.saxutils import escape

# Deliberately varied: the point of a synthetic library is to hit the layout
# edges a tidy fixture would miss. Long titles, one-word titles, punctuation,
# and non-ASCII all changed real layout bugs in this project.
TITLES = [
    "The Silent Cartographer",
    "Dune",
    "A Very Long Title That Should Wrap Onto A Second Line And Must Not Widen The Card",
    "Ubik",
    "Le Petit Prince",
    "The Left Hand of Darkness",
    "Piranesi",
    "\u0627\u0644\u0643\u064a\u0645\u064a\u0627\u0626\u064a",  # Arabic: tests RTL + font fallback
    "I",
    "Gödel, Escher, Bach: An Eternal Golden Braid",
    "Kafka on the Shore",
    "The Wind-Up Bird Chronicle",
    "Snow Crash",
    "Neuromancer",
    "The Dispossessed",
]

AUTHORS = [
    "Ursula K. Le Guin",
    "Frank Herbert",
    "Philip K. Dick",
    "Antoine de Saint-Exup\u00e9ry",
    "Susanna Clarke",
    "Haruki Murakami",
    "Neal Stephenson",
    "William Gibson",
    "Douglas Hofstadter",
    "",  # No author at all: a real state, and it used to misalign the card.
]

SERIES = [None, "The Culture", "Earthsea", "Foundation", None, None]

DESCRIPTIONS = [
    "",  # Empty: the book float has to survive this.
    "A short one.",
    "A much longer description that exists to make the book float scroll, "
    "because the description box is a fixed height and the whole point of "
    "removing the read-more toggle was that this text should simply scroll. "
    * 3,
]

TAGS = [
    "science fiction",
    "fantasy",
    "classic",
    "to-read",
    "favourite",
    "borrowed",
    "signed first edition with a needlessly long tag name",
]


def epub_bytes(title: str, author: str, chapters: int = 6, nested: bool = False) -> bytes:
    """A small but genuinely valid EPUB, so the reader has a real book to open.

    Shape, per the spec: an uncompressed `mimetype` entry first, then
    META-INF/container.xml pointing at the OPF, an OPF with metadata/manifest/
    spine, both a nav document (EPUB 3) and an NCX (EPUB 2) so whichever parser
    the engine uses finds a table of contents, and the chapters as XHTML.

    The prose is generated from a fixed word list with a seeded PRNG, so two
    runs produce byte-identical books. It is deliberately long enough to paginate
    (six chapters, ~28 paragraphs each): a one-line chapter would render, but it
    would not exercise line breaking, hyphenation or a page turn.

    `nested=True` writes a two-level table of contents instead of a flat one:
    two parts, each holding half the chapters. Real books do this, and it is
    the shape that broke the TOC sidebar -- the list drew top-level entries
    only, so a Part -> Chapter book showed two part headings and no chapters
    at all. Both the nav and the NCX nest, so it does not matter which one the
    reader picks up.
    """
    rng = random.Random(len(title) * 31 + len(author))
    words = (
        "the of and to in a is that it was for on are as with his they at be this"
        " have from or one had by word but not what all were we when your can said"
        " there use an each which she do how their if will up other about out many"
        " then them these so some her would make like him into time has look two"
        " more write go see number no way could people my than first water been"
        " call who its now find long down day did get come made may part over new"
        " sound take only little work know place year live me back give most very"
        " after thing our just name good sentence man think say great where help"
        " through much before line right too mean old any same tell boy follow came"
        " want show also around form three small set put end does another well"
        " large must big even such because turn here why ask went men read need"
        " land different home us move try kind hand picture again change off play"
        " spell air away animal house point page letter mother answer found study"
        " still learn should world"
    ).split()

    def sentence() -> str:
        n = rng.randint(8, 22)
        return " ".join(rng.choice(words) for _ in range(n)).capitalize() + "."

    def paragraph() -> str:
        return " ".join(sentence() for _ in range(rng.randint(3, 6)))

    def xhtml(head_title: str, body: str) -> str:
        return (
            '<?xml version="1.0" encoding="utf-8"?>\n'
            '<!DOCTYPE html>\n'
            '<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="en" lang="en">\n'
            f"<head><title>{escape(head_title)}</title>"
            '<link rel="stylesheet" type="text/css" href="style.css"/></head>\n'
            f"<body>{body}</body>\n</html>\n"
        )

    uid = f"urn:uuid:{uuid.uuid5(uuid.NAMESPACE_URL, title + '|' + author)}"
    labels = [f"Chapter {n}" for n in range(1, chapters + 1)]

    def nav_item(n: int, label: str) -> str:
        return f'<li><a href="ch{n}.xhtml">{escape(label)}</a></li>'

    if nested:
        # Part One -> odd chapters, Part Two -> even ones. The parts link to
        # their first chapter, which is what a real part heading usually does.
        half = (chapters + 1) // 2
        parts = []
        for part, first in enumerate((1, 1 + half)):
            if first > chapters:
                continue
            body_items = "".join(
                nav_item(n, labels[n - 1]) for n in range(first, min(first + half, chapters + 1))
            )
            name = f"Part {'One' if part == 0 else 'Two'}"
            parts.append(
                f'<li><a href="ch{first}.xhtml">{name}</a><ol>{body_items}</ol></li>'
            )
        nav_items = "".join(parts)
    else:
        nav_items = "".join(
            nav_item(n, label) for n, label in enumerate(labels, start=1)
        )
    nav = (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        '<!DOCTYPE html>\n'
        '<html xmlns="http://www.w3.org/1999/xhtml"'
        ' xmlns:epub="http://www.idpf.org/2007/ops" xml:lang="en" lang="en">\n'
        "<head><title>Contents</title>"
        '<link rel="stylesheet" type="text/css" href="style.css"/></head>\n'
        '<body><nav epub:type="toc" id="toc"><h1>Contents</h1><ol>'
        f"{nav_items}</ol></nav></body>\n</html>\n"
    )

    if nested:
        half = (chapters + 1) // 2
        points = []
        play = 0
        for part, first in enumerate((1, 1 + half)):
            if first > chapters:
                continue
            # The part's own playOrder comes before its chapters', so it
            # has to be captured before the inner loop moves `play` on.
            play += 1
            part_play = play
            inner = []
            for n in range(first, min(first + half, chapters + 1)):
                play += 1
                inner.append(
                    f'<navPoint id="np{n}" playOrder="{play}"><navLabel>'
                    f'<text>{escape(labels[n - 1])}</text></navLabel>'
                    f'<content src="ch{n}.xhtml"/></navPoint>'
                )
            points.append(
                f'<navPoint id="part{part}" playOrder="{part_play}"><navLabel>'
                f'<text>Part {"One" if part == 0 else "Two"}</text></navLabel>'
                f'<content src="ch{first}.xhtml"/>{"".join(inner)}</navPoint>'
            )
        nav_points = "".join(points)
    else:
        nav_points = "".join(
            f'<navPoint id="np{n}" playOrder="{n}"><navLabel><text>{escape(label)}'
            f'</text></navLabel><content src="ch{n}.xhtml"/></navPoint>'
            for n, label in enumerate(labels, start=1)
        )
    ncx = (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        '<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">\n'
        f'<head><meta name="dtb:uid" content="{uid}"/></head>\n'
        f"<docTitle><text>{escape(title)}</text></docTitle>\n"
        f"<navMap>{nav_points}</navMap>\n</ncx>\n"
    )

    manifest = [
        '<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>',
        '<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>',
        '<item id="css" href="style.css" media-type="text/css"/>',
    ]
    spine = []
    for n in range(1, chapters + 1):
        manifest.append(
            f'<item id="ch{n}" href="ch{n}.xhtml" media-type="application/xhtml+xml"/>'
        )
        spine.append(f'<itemref idref="ch{n}"/>')
    opf = (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        '<package xmlns="http://www.idpf.org/2007/opf" version="3.0"'
        ' unique-identifier="bookid">\n'
        '<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">\n'
        f'<dc:identifier id="bookid">{uid}</dc:identifier>\n'
        f"<dc:title>{escape(title)}</dc:title>\n"
        f"<dc:creator>{escape(author)}</dc:creator>\n"
        "<dc:language>en</dc:language>\n"
        '<meta property="dcterms:modified">2026-01-01T00:00:00Z</meta>\n'
        "</metadata>\n"
        f"<manifest>{''.join(manifest)}</manifest>\n"
        f'<spine toc="ncx">{"".join(spine)}</spine>\n'
        "</package>\n"
    )

    css = (
        "body { font-family: serif; line-height: 1.5; margin: 1em; }\n"
        "h1 { font-size: 1.4em; }\n"
        "p { margin: 0 0 0.7em 0; text-align: justify; }\n"
    )

    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
        # First and uncompressed: parts of the ecosystem still check this.
        z.writestr("mimetype", "application/epub+zip", compress_type=zipfile.ZIP_STORED)
        z.writestr(
            "META-INF/container.xml",
            '<?xml version="1.0" encoding="utf-8"?>\n'
            '<container version="1.0"'
            ' xmlns="urn:oasis:names:tc:opendocument:xmlns:container">\n'
            '<rootfiles><rootfile full-path="OEBPS/content.opf"'
            ' media-type="application/oebps-package+xml"/></rootfiles>\n'
            "</container>\n",
        )
        z.writestr("OEBPS/content.opf", opf)
        z.writestr("OEBPS/nav.xhtml", nav)
        z.writestr("OEBPS/toc.ncx", ncx)
        z.writestr("OEBPS/style.css", css)
        for n, label in enumerate(labels, start=1):
            body = f"<h1>{escape(label)}</h1>" + "".join(
                f"<p>{paragraph()}</p>" for _ in range(28)
            )
            z.writestr(f"OEBPS/ch{n}.xhtml", xhtml(label, body))
    return buf.getvalue()


def png(width: int, height: int, rgb: tuple) -> bytes:
    """A solid-colour PNG, written by hand.

    No Pillow: adding a pip install to CI for three rectangles is not worth the
    minute it costs or the supply-chain surface. zlib and struct are stdlib.
    """
    r, g, b = rgb
    raw = b"".join(
        b"\x00" + bytes([r, g, b]) * width for _ in range(height)
    )

    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 6))
        + chunk(b"IEND", b"")
    )


def cover_colour(i: int) -> tuple:
    """Distinct per book, so a screenshot shows *which* cover went where.

    All-grey covers would hide exactly the bug we are hunting: a placeholder
    that never fills looks identical to a grey cover that did.
    """
    return (40 + (i * 37) % 200, 60 + (i * 71) % 180, 90 + (i * 113) % 160)


def _truncate_utf8(text: str, max_bytes: int) -> str:
    """Match the app's byte-bounded, character-safe folder truncation."""
    out = []
    used = 0
    for char in text:
        size = len(char.encode("utf-8"))
        if used + size > max_bytes:
            break
        out.append(char)
        used += size
    return "".join(out)


def _sanitize_folder_text(raw: str, max_bytes: int) -> str:
    """Match `src/paths.rs::sanitize_folder_text` for seeded book names."""
    out = []
    last_was_space = True
    for char in raw:
        if char.isspace() or unicodedata.category(char) == "Cc" or char == "/":
            if not last_was_space:
                out.append(" ")
                last_was_space = True
            continue
        out.append(char)
        last_was_space = False
    return _truncate_utf8("".join(out).strip(". "), max_bytes)


def book_folder_name(authors: str, title: str, book_uuid: str) -> str:
    """Build the same readable, id-suffixed folder name as `src/paths.rs`."""
    author = _sanitize_folder_text(authors, 60)
    has_author = bool(author) and author.lower() != "unknown"
    clean_title = _sanitize_folder_text(title, 64) or "Untitled"
    prefix = f"{author} - {clean_title}" if has_author else clean_title
    suffix = book_uuid[:8]
    prefix = _truncate_utf8(prefix, max(0, 120 - len(suffix.encode("utf-8")) - 1))
    return f"{prefix} {suffix}"


def _add_column_if_missing(conn, table: str, column: str, declaration: str) -> None:
    columns = {row[1] for row in conn.execute(f"PRAGMA table_info({table})")}
    if column not in columns:
        conn.execute(
            f"ALTER TABLE {table} ADD COLUMN {column} {declaration}"
        )


def comic_page_png(page: int, chapter: int) -> bytes:
    """One comic page drawn the way the design boards draw them: a
    cream sheet with bordered colour panels. Enough structure that a
    screenshot can tell a page from a placeholder and one page from
    the next — the panel colours rotate with the page. Scanlines are
    assembled by slice assignment, so no Pillow is needed."""
    width, height = 800, 1200
    base = (234, 224, 205)
    ink = (28, 25, 22)
    palette = [
        (73, 83, 101),
        (122, 92, 73),
        (73, 103, 105),
        (84, 88, 105),
        (105, 84, 96),
    ]
    shift = page + chapter
    panels = [
        (40, 40, 760, 430, palette[(shift + 0) % len(palette)]),
        (40, 470, 380, 820, palette[(shift + 1) % len(palette)]),
        (420, 470, 760, 820, palette[(shift + 2) % len(palette)]),
        (40, 860, 760, 1160, palette[(shift + 3) % len(palette)]),
    ]

    raw = bytearray()
    base_row = bytes(base) * width
    for y in range(height):
        row = bytearray(base_row)
        for x1, y1, x2, y2, colour in panels:
            if y1 <= y < y2:
                if y - y1 < 4 or y2 - y <= 4:
                    row[x1 * 3:x2 * 3] = bytes(ink) * (x2 - x1)
                else:
                    row[x1 * 3:x2 * 3] = bytes(colour) * (x2 - x1)
                    row[x1 * 3:(x1 + 4) * 3] = bytes(ink) * 4
                    row[(x2 - 4) * 3:x2 * 3] = bytes(ink) * 4
        raw.append(0)
        raw.extend(row)

    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(bytes(raw), 6))
        + chunk(b"IEND", b"")
    )


def cbz_bytes(series: str, author: str, number: int, pages: int = 12) -> bytes:
    """A genuinely readable CBZ: a ComicInfo.xml plus numbered pages."""
    buf = io.BytesIO()
    info = (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        "<ComicInfo>\n"
        f"<Series>{escape(series)}</Series>\n"
        f"<Number>{number}</Number>\n"
        f"<Writer>{escape(author)}</Writer>\n"
        "</ComicInfo>\n"
    )
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("ComicInfo.xml", info)
        for page in range(pages):
            z.writestr(f"page{page + 1:03d}.png", comic_page_png(page, number))
    return buf.getvalue()


def _pdf_escape(text: str) -> str:
    return text.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)")


def pdf_bytes(title: str, author: str, pages: int = 12) -> bytes:
    """A real, hand-built PDF for the PDF-reader screenshot route.

    MuPDF must be able to count and render the pages, search their
    vector text, walk a linked outline and read catalogue page
    labels; a byte stub says nothing about any of that. The writer
    stays dependency-free: compressed
    content streams, the standard Helvetica face, page destinations
    and a conventional cross-reference table.
    """
    if pages != 12:
        raise ValueError("the PDF fixture's outline is written for 12 pages")

    width, height = 420, 595
    objects: dict[int, bytes] = {}

    def add(object_id: int, body) -> None:
        objects[object_id] = body.encode("ascii") if isinstance(body, str) else body

    catalog_id, pages_id, font_id, outlines_id = 1, 2, 3, 4
    next_id = 5
    page_ids: list[int] = []

    section_by_page = {
        1: "Preface",
        2: "The Earth",
        3: "Structure of the Earth",
        4: "Plate tectonics",
        5: "Plate tectonics",
        6: "Volcanoes",
        7: "Volcanoes",
        8: "Oceans",
        9: "Currents",
        10: "Atmosphere",
        11: "Atmosphere",
        12: "Index",
    }
    # The catalogue labels seven roman front-matter pages, then
    # restarts the text proper at decimal 1 on physical page 8.
    # The printed furniture follows the same labels, as a real
    # book's would; stored reading positions stay physical.
    page_labels = [
        "i", "ii", "iii", "iv", "v", "vi", "vii", "1", "2", "3", "4", "5"
    ]
    body_lines = [
        "Water moves through soil, stone and living tissue,",
        "carrying minerals from high ground toward the sea.",
        "Rivers record every season in their colour and force.",
        "When rain returns, dry channels answer almost at once.",
        "The planet keeps its memory in layers of sediment.",
        "Heat below the crust drives a slow, patient motion.",
        "Mountains rise where great plates meet and compress.",
        "Oceans store the warmth that softens coastal winters.",
        "Air, water and rock exchange matter without pause.",
        "Every landscape is a temporary agreement among forces.",
    ]

    for page in range(1, pages + 1):
        page_id, content_id = next_id, next_id + 1
        next_id += 2
        page_ids.append(page_id)
        section = section_by_page[page]
        parts = [
            "0.98 0.97 0.94 rg 0 0 420 595 re f",
            "0.78 0.35 0.22 rg 0 570 420 25 re f",
        ]
        for stripe in range(6):
            shade = 0.58 + stripe * 0.06
            parts.append(
                f"{shade:.3f} {shade * 0.82:.3f} {shade * 0.68:.3f} rg "
                f"{36 + stripe * 58} 300 50 180 re f"
            )
        heading = title if page == 1 else section
        subheading = (
            f"by {author}" if page == 1 else f"Page {page_labels[page - 1]}"
        )
        parts.append(
            f"BT /F1 18 Tf 0.15 g 36 535 Td ({_pdf_escape(heading)}) Tj ET"
        )
        parts.append(
            f"BT /F1 10 Tf 0.38 g 36 515 Td ({_pdf_escape(subheading)}) Tj ET"
        )
        for offset, line in enumerate(body_lines):
            y = 260 - offset * 19
            parts.append(
                f"BT /F1 10 Tf 0.18 g 36 {y} Td ({_pdf_escape(line)}) Tj ET"
            )
        parts.append(
            f"BT /F1 9 Tf 0.42 g 205 28 Td ({page_labels[page - 1]}) Tj ET"
        )
        stream = zlib.compress("\n".join(parts).encode("ascii"))
        add(
            content_id,
            f"<< /Length {len(stream)} /Filter /FlateDecode >>\nstream\n".encode(
                "ascii"
            )
            + stream
            + b"\nendstream",
        )
        add(
            page_id,
            f"<< /Type /Page /Parent {pages_id} 0 R "
            f"/MediaBox [0 0 {width} {height}] "
            f"/Resources << /Font << /F1 {font_id} 0 R >> >> "
            f"/Contents {content_id} 0 R >>",
        )

    outline_spec = [
        ("Preface", 1, []),
        (
            "The Earth",
            2,
            [
                ("Structure of the Earth", 3, []),
                ("Plate tectonics", 4, []),
                ("Volcanoes", 6, []),
            ],
        ),
        ("Oceans", 8, [("Currents", 9, [])]),
        ("Atmosphere", 10, []),
        ("Index", 12, []),
    ]

    def build_outline(spec):
        nonlocal next_id
        object_id = next_id
        next_id += 1
        return {
            "id": object_id,
            "title": spec[0],
            "page": spec[1],
            "children": [build_outline(child) for child in spec[2]],
        }

    def descendant_count(node) -> int:
        return sum(1 + descendant_count(child) for child in node["children"])

    outline_roots = [build_outline(spec) for spec in outline_spec]

    def add_outline_items(nodes, parent_id: int) -> None:
        for index, node in enumerate(nodes):
            fields = [
                f"/Title ({_pdf_escape(node['title'])})",
                f"/Parent {parent_id} 0 R",
                f"/Dest [{page_ids[node['page'] - 1]} 0 R /XYZ 0 {height} 0]",
            ]
            if index > 0:
                fields.append(f"/Prev {nodes[index - 1]['id']} 0 R")
            if index + 1 < len(nodes):
                fields.append(f"/Next {nodes[index + 1]['id']} 0 R")
            if node["children"]:
                fields.append(f"/First {node['children'][0]['id']} 0 R")
                fields.append(f"/Last {node['children'][-1]['id']} 0 R")
                fields.append(f"/Count {descendant_count(node)}")
            add(node["id"], "<< " + " ".join(fields) + " >>")
            add_outline_items(node["children"], node["id"])

    add_outline_items(outline_roots, outlines_id)
    outline_count = sum(1 + descendant_count(node) for node in outline_roots)
    add(
        outlines_id,
        f"<< /Type /Outlines /First {outline_roots[0]['id']} 0 R "
        f"/Last {outline_roots[-1]['id']} 0 R /Count {outline_count} >>",
    )
    add(font_id, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
    kids = " ".join(f"{page_id} 0 R" for page_id in page_ids)
    add(pages_id, f"<< /Type /Pages /Count {pages} /Kids [{kids}] >>")
    add(
        catalog_id,
        f"<< /Type /Catalog /Pages {pages_id} 0 R "
        f"/Outlines {outlines_id} 0 R /PageMode /UseOutlines "
        f"/PageLabels << /Nums [0 << /S /r >> 7 << /S /D >>] >> >>",
    )

    output = bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
    offsets: dict[int, int] = {}
    for object_id in sorted(objects):
        offsets[object_id] = len(output)
        output.extend(f"{object_id} 0 obj\n".encode("ascii"))
        output.extend(objects[object_id])
        output.extend(b"\nendobj\n")
    xref_offset = len(output)
    max_id = max(objects)
    output.extend(f"xref\n0 {max_id + 1}\n".encode("ascii"))
    output.extend(b"0000000000 65535 f \n")
    for object_id in range(1, max_id + 1):
        output.extend(f"{offsets[object_id]:010d} 00000 n \n".encode("ascii"))
    output.extend(
        f"trailer\n<< /Size {max_id + 1} /Root {catalog_id} 0 R >>\n"
        f"startxref\n{xref_offset}\n%%EOF\n".encode("ascii")
    )
    return bytes(output)


def seed_pdf_fixture(
    conn, library: pathlib.Path, now: datetime, notes_empty: bool = False
) -> int:
    """Seed the book behind `pdf-read-21`. Returns covers written.

    The PDF route opens the file with MuPDF, so it needs a real
    outlined, searchable document. With `--books 20`, this is book
    21, independent of the comic fixture seeded after it. Page 4
    is both the saved place and a bookmark, and the per-book
    settings open the board's 4-5 spread at 140%. Those physical
    pages are labelled iv-v, so every PDF capture also proves the
    location text's label lead and the Contents labels.
    """
    conn.execute(
        """
        CREATE TABLE IF NOT EXISTS reading_bookmarks (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            book_id       INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
            chapter_index INTEGER NOT NULL,
            fraction      REAL    NOT NULL DEFAULT 0.0,
            label         TEXT    NOT NULL DEFAULT '',
            created_at    TEXT    NOT NULL
        )
        """
    )
    conn.execute(
        """
        CREATE TABLE IF NOT EXISTS app_prefs (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )
        """
    )
    conn.execute(
        """
        CREATE TABLE IF NOT EXISTS annotations (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            book_id       INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
            kind          TEXT    NOT NULL,
            chapter_index INTEGER NOT NULL,
            start_path    TEXT    NOT NULL,
            start_offset  INTEGER NOT NULL,
            end_path      TEXT    NOT NULL,
            end_offset    INTEGER NOT NULL,
            color         TEXT    NOT NULL DEFAULT 'yellow',
            style         TEXT    NOT NULL DEFAULT 'solid',
            text_excerpt  TEXT    NOT NULL DEFAULT '',
            note          TEXT    NOT NULL DEFAULT '',
            cfi           TEXT,
            created_at    TEXT    NOT NULL,
            updated_at    TEXT    NOT NULL
        )
        """
    )
    conn.execute(
        """
        CREATE TABLE IF NOT EXISTS saved_words (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            word          TEXT    NOT NULL,
            definition    TEXT    NOT NULL,
            dict_name     TEXT,
            book_id       INTEGER REFERENCES books(id) ON DELETE SET NULL,
            chapter_index INTEGER,
            context_text  TEXT,
            created_at    TEXT    NOT NULL
        )
        """
    )

    title, author = "The Living Earth", "Ada Meridian"
    pdf = pdf_bytes(title, author)
    book_uuid = str(uuid.uuid4())
    book_dir = library / book_folder_name(author, title, book_uuid)
    book_dir.mkdir(parents=True)
    (book_dir / "book.pdf").write_bytes(pdf)
    (book_dir / "cover.png").write_bytes(png(600, 960, cover_colour(19)))

    stamp = now.isoformat()
    cursor = conn.execute(
        "INSERT INTO books (uuid, title, sort_title, authors, series, "
        "description, format, file_name, file_hash, cover_name, added_at, "
        "progress) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
        (
            book_uuid,
            title,
            title.lower(),
            author,
            None,
            "Rivers, plates, oceans and air: the systems that keep a "
            "planet in motion.",
            "PDF",
            "book.pdf",
            "hash-pdf-000021",
            "cover.png",
            stamp,
            33,
        ),
    )
    book_id = cursor.lastrowid

    # PDF progress stores its one-based page directly: page 4
    # opens the Plate tectonics spread, bookmarked there.
    conn.execute(
        "INSERT INTO reading_progress (book_id, chapter_index, fraction, "
        "updated_at) VALUES (?,?,?,?)",
        (book_id, 4, 4 / 12, stamp),
    )
    if not notes_empty:
        conn.executemany(
            "INSERT INTO reading_bookmarks (book_id, chapter_index, fraction, "
            "label, created_at) VALUES (?,?,?,?,?)",
            [
                (book_id, 4, 4 / 12, "Where the plates meet", stamp),
                (book_id, 6, 6 / 12, "Volcanoes", stamp),
                (book_id, 9, 9 / 12, "Currents", stamp),
            ],
        )
        conn.executemany(
            "INSERT INTO annotations (book_id, kind, chapter_index, start_path, "
            "start_offset, end_path, end_offset, color, style, text_excerpt, note, "
            "cfi, created_at, updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            [
                (
                    book_id, "highlight", 4, "", 0, "", 0, "yellow", "solid",
                    "Mountains rise where great plates meet and compress.",
                    "Where the plates meet", None, stamp, stamp,
                ),
                (
                    book_id, "highlight", 5, "", 0, "", 0, "green", "solid",
                    "Heat below the crust drives a slow, patient motion.",
                    "", None, stamp, stamp,
                ),
                (
                    book_id, "highlight", 6, "", 0, "", 0, "blue", "solid",
                    "Every landscape is a temporary agreement among forces.",
                    "", None, stamp, stamp,
                ),
                (
                    book_id, "highlight", 9, "", 0, "", 0, "pink", "solid",
                    "Oceans store the warmth that softens coastal winters.",
                    "", None, stamp, stamp,
                ),
            ],
        )
        conn.executemany(
            "INSERT INTO saved_words (word, definition, dict_name, book_id, "
            "chapter_index, context_text, created_at) VALUES (?,?,?,?,?,?,?)",
            [
                (
                    "tectonic",
                    "relating to the structure and movement of the Earth’s crust",
                    None, book_id, 4, "Mountains rise where great plates meet", stamp,
                ),
                (
                    "volcano",
                    "an opening in the Earth’s crust through which lava, ash and gases erupt",
                    None, book_id, 6, "Volcanoes", stamp,
                ),
                (
                    "magma",
                    "molten rock beneath the Earth’s surface",
                    None, book_id, 6, "Heat below the crust", stamp,
                ),
                (
                    "caldera",
                    "a large crater formed when a volcano collapses after an eruption",
                    None, book_id, 7, "Volcanoes", stamp,
                ),
            ],
        )
    prefs = {
        f"book.{book_id}.pdf.scroll_mode": "page",
        f"book.{book_id}.pdf.spread_mode": "odd",
        f"book.{book_id}.pdf.zoom": "1.40",
        "reader.pdf.two_page_gap": "0",
    }
    conn.executemany(
        "INSERT OR REPLACE INTO app_prefs (key, value) VALUES (?, ?)",
        sorted(prefs.items()),
    )
    print(f"pdf fixture: book {book_id} (route pdf-read-{book_id})")
    return 1


def seed_comic_fixture(conn, library: pathlib.Path, now: datetime) -> int:
    """Add the comics reader's drawn state: one series of two real
    CBZ chapters, the first read, the second open at page 4 of 12
    with a bookmark on it — so one route (`comic-read-<second id>`)
    shows the filled ribbon, the series-wide `16 / 24` location,
    chapter states and a Pages grid with the current page ringed.

    The files live where the importer puts comics (item 2.22):
    `library/<Series>/0001.cbz` with the cover beside them, and the
    stored names carry the slash that says so. The comic series /
    chapter rows are left to the app's own startup migration, which
    is the path a real library takes. Returns covers written.
    """
    _add_column_if_missing(conn, "books", "series_index", "REAL NOT NULL DEFAULT 0")
    conn.execute(
        """
        CREATE TABLE IF NOT EXISTS reading_bookmarks (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            book_id       INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
            chapter_index INTEGER NOT NULL,
            fraction      REAL    NOT NULL DEFAULT 0.0,
            label         TEXT    NOT NULL DEFAULT '',
            created_at    TEXT    NOT NULL
        )
        """
    )

    series = "The Paper Meridian"
    author = "Ada Meridian"
    series_dir = library / series
    covers_dir = series_dir / "covers"
    covers_dir.mkdir(parents=True, exist_ok=True)

    chapters = [
        (1, "The Harbour Bell", 100),
        (2, "The Salt Road", 33),
    ]
    covers_written = 0
    second_id = None
    for number, chapter_title, progress in chapters:
        stem = f"{number:04d}"
        (series_dir / f"{stem}.cbz").write_bytes(
            cbz_bytes(series, author, number)
        )
        (covers_dir / f"{stem}.png").write_bytes(comic_page_png(0, number))
        covers_written += 1
        title = f"{series} - c{number:03d} - {chapter_title}"
        added = (now - timedelta(days=10 - number)).isoformat()
        cur = conn.execute(
            "INSERT INTO books (uuid, title, sort_title, authors, series, "
            "description, format, file_name, file_hash, cover_name, added_at, "
            "progress, series_index) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
            (
                str(uuid.uuid4()),
                title,
                title.lower(),
                author,
                series,
                "",
                # The catalog stores formats uppercase (`BookFormat::as_str`),
                # and the comic-series migration matches on that spelling.
                "CBZ",
                f"{series}/{stem}.cbz",
                f"hash-comic-{number:06d}",
                f"{series}/covers/{stem}.png",
                added,
                progress,
                float(number),
            ),
        )
        if number == 1:
            conn.execute(
                "INSERT INTO reading_progress (book_id, chapter_index, "
                "fraction, updated_at) VALUES (?,?,?,?)",
                (cur.lastrowid, 11, 1.0, added),
            )
        else:
            second_id = cur.lastrowid

    # Chapter 2 stands open at page 4 (index 3), bookmarked there.
    stamp = now.isoformat()
    conn.execute(
        "INSERT INTO reading_progress (book_id, chapter_index, fraction, "
        "updated_at) VALUES (?,?,?,?)",
        (second_id, 3, 4 / 12, stamp),
    )
    conn.execute(
        "INSERT INTO reading_bookmarks (book_id, chapter_index, fraction, "
        "label, created_at) VALUES (?,?,?,?,?)",
        (second_id, 3, 4 / 12, "Page 4", stamp),
    )
    return covers_written


def seed_series_fixture(conn, library: pathlib.Path, now: datetime, first_index: int) -> int:
    """Add one coherent series for the series-page screenshot.

    The generic loop deliberately pairs titles, authors and series at
    random offsets, which is right for grids and wrong for judging a
    series page. This fixture is the drawn "reading" state instead:
    Book 1 finished, Book 2 at 42% (the default focus and Continue
    target), Book 3 unread, and Book 4 present only in the cached
    listing — so one frame exercises the tick badge, the progress bar,
    the dashed missing cover, the years and the source line. The cache
    row is what keeps the screenshot off the network.

    The columns/table it needs postdate schema v1; adding them here is
    safe because the app's migrations add-if-missing and create-if-absent.
    Returns the number of covers written.
    """
    _add_column_if_missing(conn, "books", "published", "TEXT NOT NULL DEFAULT ''")
    _add_column_if_missing(conn, "books", "series_index", "REAL NOT NULL DEFAULT 0")
    _add_column_if_missing(conn, "books", "finished_at", "TEXT")
    conn.execute(
        """
        CREATE TABLE IF NOT EXISTS series_cache (
            series_key TEXT PRIMARY KEY,
            source     TEXT NOT NULL,
            fetched_at TEXT NOT NULL,
            works_json TEXT NOT NULL
        )
        """
    )

    series = "The Glass Meridian"
    author = "Ada Meridian"
    owned = [
        (
            "The Glass Meridian",
            1998,
            100,
            "2026-10-03T00:00:00Z",
            "The first voyage beyond the glass meridian, and the chart "
            "that brought its crew home changed.",
        ),
        (
            "The Salt Meridian",
            2001,
            42,
            None,
            "Salt, debt and a second crossing: the crew returns to the "
            "meridian with a chart nobody trusts, a captain keeping one "
            "secret too many, and a coastline that has moved since the "
            "first voyage.",
        ),
        (
            "The Iron Meridian",
            2004,
            0,
            None,
            "An iron fleet, a winter port, and the price of finishing "
            "the chart.",
        ),
    ]
    works = [
        {
            "title": title,
            "key": "",
            "cover_path": "",
            "year": year,
            "author": author,
        }
        for title, year, _progress, _finished, _description in owned
    ]
    works.append(
        {
            "title": "The Last Meridian",
            "key": "",
            "cover_path": "",
            "year": 2008,
            "author": author,
        }
    )

    covers_written = 0
    for offset, (title, year, progress, finished_at, description) in enumerate(owned):
        book_uuid = str(uuid.uuid4())
        bdir = library / book_folder_name(author, title, book_uuid)
        bdir.mkdir(parents=True, exist_ok=True)
        (bdir / "book.epub").write_bytes(epub_bytes(title, author))
        (bdir / "cover.png").write_bytes(
            png(600, 960, cover_colour(first_index + offset))
        )
        covers_written += 1
        added = (now - timedelta(days=30 - offset)).isoformat()
        cur = conn.execute(
            "INSERT INTO books (uuid, title, sort_title, authors, series, "
            "description, format, file_name, file_hash, cover_name, added_at, "
            "progress, published, series_index, finished_at) "
            "VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            (
                book_uuid,
                title,
                title.lower(),
                author,
                series,
                description,
                "epub",
                "book.epub",
                f"hash-series-{offset:06d}",
                "cover.png",
                added,
                progress,
                str(year),
                float(offset + 1),
                finished_at,
            ),
        )
        if progress == 42:
            # Chapter 4 of the fixture EPUB's six, for the focused row.
            conn.execute(
                "INSERT INTO reading_progress (book_id, chapter_index, "
                "fraction, updated_at) VALUES (?,?,?,?)",
                (cur.lastrowid, 3, 0.42, added),
            )

    # Mirrors `src/db/series.rs::series_key`: the leading article drops
    # from the series, then both halves (series, full first author)
    # keep alphanumerics only, lowercased.
    def _norm(s: str) -> str:
        return "".join(c for c in s.lower() if c.isalnum())

    series_no_article = series
    first_word, _, rest = series.partition(" ")
    if rest and first_word.lower() in ("the", "a", "an"):
        series_no_article = rest
    series_key = f"{_norm(series_no_article)}|{_norm(author)}"
    assert series_key == "glassmeridian|adameridian", series_key
    conn.execute(
        "INSERT OR REPLACE INTO series_cache (series_key, source, "
        "fetched_at, works_json) VALUES (?,?,?,?)",
        (
            series_key,
            "openlibrary",
            "2026-10-20T00:00:00Z",
            json.dumps(works),
        ),
    )
    return covers_written


def seed_author_fixture(conn, library: pathlib.Path, now: datetime, first_index: int) -> int:
    """Add the author-page fixture: one author, a cached bibliography
    with a standalone and a series the reader only partly owns, plus a
    highlight, a bookmark and a saved word so Highlights and the
    author-scope dialog have something true to show.

    Same author as the series fixture (Ada Meridian, "The Glass
    Meridian"), so one author page shows Standalone first (decision
    107) then the series, In library and Not in library both
    populated. The bibliography lives in author_profiles.works_json:
    the page never fetches on open (decision 110), so the cache row
    is the only source and the screenshot stays off the network.

    Tables it needs postdate schema v1; creating them here is safe
    because the app's migrations are create-if-absent. Returns the
    number of covers written.
    """
    _add_column_if_missing(conn, "books", "published", "TEXT NOT NULL DEFAULT ''")
    _add_column_if_missing(conn, "books", "series_index", "REAL NOT NULL DEFAULT 0")
    _add_column_if_missing(conn, "books", "finished_at", "TEXT")
    conn.executescript(
        """
        CREATE TABLE IF NOT EXISTS author_profiles (
            id                INTEGER PRIMARY KEY AUTOINCREMENT,
            canonical_name    TEXT NOT NULL,
            sort_name         TEXT NOT NULL DEFAULT '',
            normalized_name   TEXT NOT NULL UNIQUE,
            bio               TEXT NOT NULL DEFAULT '',
            birth_date        TEXT NOT NULL DEFAULT '',
            death_date        TEXT NOT NULL DEFAULT '',
            top_work          TEXT NOT NULL DEFAULT '',
            top_subjects_json TEXT NOT NULL DEFAULT '[]',
            openlibrary_key   TEXT NOT NULL DEFAULT '',
            photo_file        TEXT,
            work_count        INTEGER NOT NULL DEFAULT 0,
            works_json        TEXT NOT NULL DEFAULT '[]',
            fetched_at        TEXT NOT NULL DEFAULT '',
            source_url        TEXT NOT NULL DEFAULT ''
        );
        CREATE TABLE IF NOT EXISTS annotations (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            book_id       INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
            kind          TEXT    NOT NULL,
            chapter_index INTEGER NOT NULL,
            start_path    TEXT    NOT NULL,
            start_offset  INTEGER NOT NULL,
            end_path      TEXT    NOT NULL,
            end_offset    INTEGER NOT NULL,
            color         TEXT    NOT NULL DEFAULT 'yellow',
            style         TEXT    NOT NULL DEFAULT 'solid',
            text_excerpt  TEXT    NOT NULL DEFAULT '',
            note          TEXT    NOT NULL DEFAULT '',
            cfi           TEXT,
            created_at    TEXT    NOT NULL,
            updated_at    TEXT    NOT NULL
        );
        CREATE TABLE IF NOT EXISTS reading_bookmarks (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            book_id       INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
            chapter_index INTEGER NOT NULL,
            fraction      REAL    NOT NULL DEFAULT 0.0,
            label         TEXT    NOT NULL DEFAULT '',
            created_at    TEXT    NOT NULL
        );
        CREATE TABLE IF NOT EXISTS saved_words (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            word          TEXT    NOT NULL,
            definition    TEXT    NOT NULL,
            dict_name     TEXT,
            book_id       INTEGER REFERENCES books(id) ON DELETE SET NULL,
            chapter_index INTEGER,
            context_text  TEXT,
            created_at    TEXT    NOT NULL
        );
        """
    )

    author = "Ada Meridian"

    def norm(s: str) -> str:
        # Mirrors normalize_bits in src/author.rs: lowercase, alphanumerics only.
        return "".join(c for c in s.lower() if c.isalnum())

    assert norm(author) == "adameridian"

    # Standalones the reader owns: one finished (the highlight's home),
    # one barely begun. Either may or may not appear in the
    # bibliography below; both match it by title (decision 110).
    standalone = [
        ("Chart of Small Hours", 1992, 100, "2026-09-12T00:00:00Z"),
        ("The Wintering Harbour", 2010, 12, None),
    ]
    covers_written = 0
    highlight_book = None
    for offset, (title, year, progress, finished_at) in enumerate(standalone):
        book_uuid = str(uuid.uuid4())
        bdir = library / book_folder_name(author, title, book_uuid)
        bdir.mkdir(parents=True, exist_ok=True)
        (bdir / "book.epub").write_bytes(epub_bytes(title, author))
        (bdir / "cover.png").write_bytes(
            png(600, 960, cover_colour(first_index + offset))
        )
        covers_written += 1
        added = (now - timedelta(days=60 - offset)).isoformat()
        cur = conn.execute(
            "INSERT INTO books (uuid, title, sort_title, authors, series, "
            "description, format, file_name, file_hash, cover_name, added_at, "
            "progress, published, series_index, finished_at) "
            "VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            (
                book_uuid,
                title,
                title.lower(),
                author,
                None,
                "",
                "epub",
                "book.epub",
                f"hash-author-{offset:06d}",
                "cover.png",
                added,
                progress,
                str(year),
                0.0,
                finished_at,
            ),
        )
        if highlight_book is None:
            highlight_book = cur.lastrowid

    # (title, year, series_name, series_position) — the cached
    # bibliography. Owned: both standalones and Meridian 1–3 (from the
    # series fixture). Missing: Meridian 4, one standalone, and both
    # "Signal Books" — so the Not-in-library group holds exactly one
    # series card, standalone-first.
    works = [
        ("Chart of Small Hours", 1992, "", ""),
        ("The Wintering Harbour", 2010, "", ""),
        ("The Glass Meridian", 1998, "The Glass Meridian", "1"),
        ("The Salt Meridian", 2001, "The Glass Meridian", "2"),
        ("The Iron Meridian", 2004, "The Glass Meridian", "3"),
        ("The Last Meridian", 2008, "The Glass Meridian", "4"),
        ("A Field Guide to Leaving", 2015, "", ""),
        ("Signals in the Static", 2018, "The Signal Books", "1"),
        ("The Quiet Frequency", 2021, "The Signal Books", "2"),
    ]
    works_json = json.dumps(
        [
            {
                "title": title,
                "first_publish_year": year,
                "subjects": [],
                "cover_id": None,
                "cover_file": None,
                "work_key": "",
                "series_key": "",
                "series_name": series_name,
                "series_position": position,
                "rating_average": None,
            }
            for title, year, series_name, position in works
        ]
    )
    conn.execute(
        "INSERT OR REPLACE INTO author_profiles (canonical_name, sort_name, "
        "normalized_name, bio, birth_date, death_date, top_work, "
        "top_subjects_json, openlibrary_key, photo_file, work_count, "
        "works_json, fetched_at, source_url) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        (
            author,
            "Meridian, Ada",
            norm(author),
            "Ada Meridian (born 1962) writes seafaring fiction about maps, "
            "debts, and the harbours people leave behind. Chart of Small "
            "Hours, her first novel, began a career of quiet crossings; "
            "The Glass Meridian followed its charts to a coastline that "
            "would not sit still. She lives in Reykjavík and is said to "
            "draft every first chapter on a boat, in pencil, badly.",
            "1962-03-14",
            "",
            "The Glass Meridian",
            '["Sea voyages", "Maps", "Fiction"]',
            "/authors/OL0000000A",
            None,
            len(works),
            works_json,
            "2026-10-20T00:00:00Z",
            "https://openlibrary.org/authors/OL0000000A",
        ),
    )

    stamp = "2026-03-14T00:00:00Z"
    conn.execute(
        "INSERT INTO annotations (book_id, kind, chapter_index, start_path, "
        "start_offset, end_path, end_offset, color, style, text_excerpt, "
        "note, cfi, created_at, updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        (
            highlight_book,
            "highlight",
            3,
            "p[1]",
            0,
            "p[1]",
            0,
            "yellow",
            "solid",
            "The chart only lies to those who insist the harbour stay put.",
            "",
            None,
            stamp,
            stamp,
        ),
    )
    conn.execute(
        "INSERT INTO reading_bookmarks (book_id, chapter_index, fraction, "
        "label, created_at) VALUES (?,?,?,?,?)",
        (highlight_book, 2, 0.5, "The tolling bell", stamp),
    )
    conn.execute(
        "INSERT INTO saved_words (word, definition, dict_name, book_id, "
        "chapter_index, context_text, created_at) VALUES (?,?,?,?,?,?,?)",
        (
            "fathom",
            "A unit of depth: six feet; also, to understand fully.",
            None,
            highlight_book,
            3,
            "She could not fathom the chart's patience.",
            stamp,
        ),
    )
    return covers_written


def seed_settings_fixture(
    conn,
    library: pathlib.Path,
    root: pathlib.Path,
    now: datetime,
    *,
    empty_dictionaries: bool = False,
) -> None:
    """Add the populated states the redesigned Settings panes photograph.

    A bare seeded catalog can only show the empty/default half of most
    panes. This fixture supplies the other designed states without driving
    the UI: two shelves and a watch rule routed into them, a rejected
    Google Books key, three EPUB original backups, and a libraries
    registry holding the active library, a second folder that can be
    opened, and one whose folder is missing.

    The registry and bundled-dictionary markers are config-side state, so
    they are written under XDG_CONFIG_HOME. `screenshot.sh` gives every
    invocation a fresh config home; without that, the first run's
    dictionary markers would make every later fresh catalog look as though
    its bundled packs had already been installed and then removed.

    The dictionary rows themselves are seeded directly into the catalog.
    The first Settings captures instead cleared the installed markers and
    let the app's startup task install the bundled packs — but that task
    finishes about a second after the Settings snapshot is taken and
    nothing refreshes the pane, so the populated drag/remove frames came
    out byte-identical to the empty one. Seeding rows (with the packs'
    real entry counts) and marking them installed keeps the pane honest
    from the first frame; the empty variant just skips the rows.
    """
    stamp = now.isoformat()
    conn.executescript(
        """
        CREATE TABLE IF NOT EXISTS app_prefs (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS shelves (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT    NOT NULL UNIQUE COLLATE NOCASE,
            kind        TEXT    NOT NULL DEFAULT 'manual',
            description TEXT    NOT NULL DEFAULT '',
            rules       TEXT    NOT NULL DEFAULT '',
            position    INTEGER NOT NULL DEFAULT 0,
            created_at  TEXT    NOT NULL,
            updated_at  TEXT    NOT NULL
        );
        CREATE TABLE IF NOT EXISTS dictionaries (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT    NOT NULL UNIQUE,
            lang        TEXT,
            entry_count INTEGER NOT NULL DEFAULT 0,
            added_at    TEXT    NOT NULL,
            priority    INTEGER NOT NULL DEFAULT 100
        );
        CREATE TABLE IF NOT EXISTS dict_entries (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            dict_id    INTEGER NOT NULL REFERENCES dictionaries(id) ON DELETE CASCADE,
            word       TEXT    NOT NULL COLLATE NOCASE,
            definition TEXT    NOT NULL,
            key        TEXT
        );
        """
    )

    # The four bundled packs in their install order (src/dict.rs):
    # priority decides who speaks first, lower first. Entry counts are
    # the bundled TSVs' line counts, so the rows read like the real
    # packs. No entries are seeded — the pane photographs the list,
    # and no Settings capture looks a word up.
    conn.execute("DELETE FROM dict_entries")
    conn.execute("DELETE FROM dictionaries")
    if not empty_dictionaries:
        for name, count, priority in (
            ("English WordNet 2025", 127284, 10),
            ("English Idioms and Expressions", 1024, 20),
            ("English Synonyms (WordNet 3.0)", 110365, 30),
            ("English Antonyms (WordNet 3.0)", 6621, 40),
        ):
            conn.execute(
                "INSERT INTO dictionaries (name, lang, entry_count, added_at, priority) "
                "VALUES (?, 'en', ?, ?, ?)",
                (name, count, stamp, priority),
            )

    shelf_ids = []
    for position, name in enumerate(("New arrivals", "Comics inbox")):
        cur = conn.execute(
            "INSERT OR REPLACE INTO shelves "
            "(name, kind, description, rules, position, created_at, updated_at) "
            "VALUES (?, 'manual', '', '', ?, ?, ?)",
            (name, position, stamp, stamp),
        )
        shelf_ids.append(cur.lastrowid)

    watch_dir = root / "watch-fixture"
    watch_dir.mkdir(parents=True, exist_ok=True)
    watch_rules = [
        {
            "id": "ci-settings-watch-folder",
            "path": str(watch_dir),
            "enabled": True,
            "import_epub": True,
            "epub_shelf_id": shelf_ids[0],
            "import_pdf": True,
            "pdf_shelf_id": None,
            "import_comics": True,
            "comics_shelf_id": shelf_ids[1],
            "subfolder_shelves": True,
            "cleanup_original": False,
        }
    ]
    prefs = {
        "import.watch_folder_rules": json.dumps(watch_rules),
        "meta.googlebooks.enabled": "1",
        "meta.googlebooks.key": "AIza-fixture-key",
        "meta.googlebooks.country": "GB",
        "meta.googlebooks.key_rejected": "1",
    }
    for key, value in prefs.items():
        conn.execute(
            "INSERT INTO app_prefs (key, value) VALUES (?, ?) "
            "ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            (key, value),
        )

    # list_backups scans the whole library tree, including folders left by
    # earlier seeds in the shared CI data home. Clear those first so the
    # Book files pane reports this fixture's three backups, not however
    # many screenshot runs happened before it in the same job.
    for stale in library.glob("*/*.orig"):
        stale.unlink()
    for epub in sorted(library.glob("*/book.epub"))[:3]:
        shutil.copyfile(epub, epub.with_name("book.epub.orig"))

    config_home = os.environ.get("XDG_CONFIG_HOME")
    config_root = (
        pathlib.Path(config_home)
        if config_home
        else pathlib.Path.home() / ".config"
    )
    config_dir = config_root / "kalam"
    config_dir.mkdir(parents=True, exist_ok=True)

    work_library = root.parent / "kalam-work"
    work_library.mkdir(parents=True, exist_ok=True)
    missing_library = root.parent / "kalam-missing"
    shutil.rmtree(missing_library, ignore_errors=True)
    registry = {
        "libraries": [
            {"name": "CI library", "path": str(root)},
            {"name": "Work", "path": str(work_library)},
            {"name": "Archive", "path": str(missing_library)},
        ],
        "active": 0,
    }
    (config_dir / "libraries.json").write_text(
        json.dumps(registry, indent=2) + "\n", encoding="utf-8"
    )

    global_prefs_path = config_dir / "prefs.json"
    try:
        global_prefs = json.loads(global_prefs_path.read_text(encoding="utf-8"))
        if not isinstance(global_prefs, dict):
            global_prefs = {}
    except (FileNotFoundError, json.JSONDecodeError):
        global_prefs = {}
    # The rows above stand in for the installed packs (or, in the empty
    # variant, for packs installed and since removed), so the startup
    # installer must stay out of either photograph.
    marker = "installed"
    for key in (
        "bundled_dictionary_english_wordnet_2025",
        "bundled_dictionary_english_idioms_2024",
        "bundled_dictionary_english_synonyms_3_0",
        "bundled_dictionary_english_antonyms_3_0",
    ):
        global_prefs[key] = marker
    global_prefs_path.write_text(
        json.dumps(global_prefs, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def seed_reader_fixture(conn, now: datetime) -> None:
    """A three-word dictionary for the reader screenshots (EPUB T4).

    The full packs install at startup and finish *after* a snapshot is
    taken, so a capture cannot wait for them. This plants the four
    bundled packs as catalog rows plus just enough entries for the
    dictionary frames: "bank" carries two parts of speech, a fold of
    extra noun senses, quoted examples, synonym/antonym chips and two
    idiom cards; "river" exists so the missing frame's "riv" lookup
    has a did-you-mean to offer; "shore" gives the bank chips a real
    word to wander to. Sense blobs use the bundled WordNet shape the
    parser in src/db/dictionaries.rs reads.
    """
    import json

    stamp = now.isoformat()
    conn.executescript(
        """
        CREATE TABLE IF NOT EXISTS dictionaries (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT    NOT NULL UNIQUE,
            lang        TEXT,
            entry_count INTEGER NOT NULL DEFAULT 0,
            added_at    TEXT    NOT NULL,
            priority    INTEGER NOT NULL DEFAULT 100
        );
        CREATE TABLE IF NOT EXISTS dict_entries (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            dict_id    INTEGER NOT NULL REFERENCES dictionaries(id) ON DELETE CASCADE,
            word       TEXT    NOT NULL COLLATE NOCASE,
            definition TEXT    NOT NULL,
            key        TEXT
        );
        CREATE TABLE IF NOT EXISTS combined_words (
            key     TEXT    PRIMARY KEY COLLATE NOCASE,
            word    TEXT    NOT NULL,
            dict_id INTEGER NOT NULL REFERENCES dictionaries(id) ON DELETE CASCADE,
            senses  TEXT    NOT NULL
        );
        """
    )
    packs = {}
    for name, count, priority in (
        ("English WordNet 2025", 127284, 10),
        ("English Idioms and Expressions", 1024, 20),
        ("English Synonyms (WordNet 3.0)", 110365, 30),
        ("English Antonyms (WordNet 3.0)", 6621, 40),
    ):
        conn.execute(
            "INSERT OR IGNORE INTO dictionaries (name, lang, entry_count, added_at, priority) "
            "VALUES (?, 'en', ?, ?, ?)",
            (name, count, stamp, priority),
        )
        packs[name] = conn.execute(
            "SELECT id FROM dictionaries WHERE name = ?", (name,)
        ).fetchone()[0]
    wordnet = packs["English WordNet 2025"]

    bank = (
        'noun:; 1. the land alongside a river or lake '
        '"they sat on the bank and watched the water"; '
        "2. a financial institution that keeps money and lends it at interest "
        '"she kept her savings in the local bank"; '
        "3. a long raised mound of earth or snow; "
        "4. an amount of something kept in reserve "
        '"a bank of cloud was building in the west"; '
        "5. a row of similar things, such as lights or dials; "
        "verb:; 1. to put money into a bank; "
        '2. to tilt sideways while turning "the plane banked into the cloud"'
    )
    river = (
        "noun:; 1. a large natural stream of water flowing to the sea "
        '"the river bends below the village"; '
        "2. a large quantity of something flowing steadily"
    )
    shore = (
        "noun:; 1. the land along the edge of a sea, lake or wide river "
        '"they walked the shore at dusk"'
    )
    for word, blob in (("bank", bank), ("river", river), ("shore", shore)):
        conn.execute(
            "INSERT OR REPLACE INTO combined_words (key, word, dict_id, senses) "
            "VALUES (?, ?, ?, ?)",
            (word, word, wordnet, json.dumps([blob])),
        )

    # The auxiliary packs speak through dict_entries; chip lists hide in
    # "synonyms: a, b, c" definitions, idioms are ordinary rows whose
    # word contains the headword as a token.
    entries = (
        ("English Synonyms (WordNet 3.0)", "bank", "bank",
         "synonyms: shore, edge, brink, reserve, savings"),
        ("English Antonyms (WordNet 3.0)", "bank", "bank",
         "antonyms: spend, borrow"),
        ("English Idioms and Expressions", "bank on", "bank on",
         "to rely on something happening or proving true"),
        ("English Idioms and Expressions", "break the bank", "break the bank",
         "to cost more than one can afford"),
    )
    for pack, word, key, definition in entries:
        conn.execute(
            "INSERT INTO dict_entries (dict_id, word, definition, key) "
            "VALUES (?, ?, ?, ?)",
            (packs[pack], word, definition, key),
        )


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--books", type=int, default=139)
    ap.add_argument(
        "--no-cover-every",
        type=int,
        default=17,
        help="every Nth book gets no cover at all (a real and untidy state)",
    )
    ap.add_argument(
        "--series-fixture",
        action="store_true",
        help="add the coherent 3-of-4 series the series-page screenshot opens",
    )
    ap.add_argument(
        "--author-fixture",
        action="store_true",
        help="add the cached bibliography + marks the author-page screenshot opens",
    )
    ap.add_argument(
        "--pdf-fixture",
        action="store_true",
        help="add the real outlined PDF the PDF-reader screenshots open",
    )
    ap.add_argument(
        "--pdf-notes-empty",
        action="store_true",
        help="with --pdf-fixture, seed no highlights, bookmarks or saved words",
    )
    ap.add_argument(
        "--comic-fixture",
        action="store_true",
        help="add the two-chapter CBZ series the comics reader screenshots open",
    )
    ap.add_argument(
        "--reader-fixture",
        action="store_true",
        help="seed the tiny dictionary the reader popover screenshots look up",
    )
    ap.add_argument(
        "--settings-fixture",
        action="store_true",
        help="add populated libraries, watch, metadata and backup states for Settings",
    )
    ap.add_argument(
        "--settings-empty-dicts",
        action="store_true",
        help="with --settings-fixture, seed no dictionaries (the empty state)",
    )
    args = ap.parse_args()

    data_home = os.environ.get("XDG_DATA_HOME")
    if not data_home:
        print("seed-library: refusing to run without XDG_DATA_HOME", file=sys.stderr)
        print("  (it would write into your real ~/.local/share/kalam)", file=sys.stderr)
        return 2

    root = pathlib.Path(data_home) / "kalam"
    library = root / "library"
    library.mkdir(parents=True, exist_ok=True)

    db = root / "catalog.db"
    if db.exists():
        db.unlink()
    conn = sqlite3.connect(db)
    conn.executescript(
        """
        CREATE TABLE schema_version (version INTEGER NOT NULL);
        CREATE TABLE books (
            id            INTEGER PRIMARY KEY AUTOINCREMENT,
            uuid          TEXT    NOT NULL UNIQUE,
            title         TEXT    NOT NULL,
            sort_title    TEXT    NOT NULL,
            authors       TEXT    NOT NULL DEFAULT '',
            series        TEXT,
            description   TEXT    NOT NULL DEFAULT '',
            format        TEXT    NOT NULL,
            file_name     TEXT    NOT NULL,
            file_hash     TEXT    NOT NULL,
            cover_name    TEXT,
            added_at      TEXT    NOT NULL,
            progress      INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE tags (
            id   INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE COLLATE NOCASE
        );
        CREATE TABLE book_tags (
            book_id INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
            tag_id  INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
            PRIMARY KEY (book_id, tag_id)
        );
        CREATE INDEX idx_books_title ON books(sort_title);
        CREATE INDEX idx_books_added ON books(added_at);
        CREATE INDEX idx_books_hash  ON books(file_hash);
        CREATE TABLE reading_progress (
            book_id       INTEGER PRIMARY KEY REFERENCES books(id) ON DELETE CASCADE,
            chapter_index INTEGER NOT NULL DEFAULT 0,
            fraction      REAL    NOT NULL DEFAULT 0.0,
            updated_at    TEXT    NOT NULL
        );
        """
    )
    # The app migrates forward from whatever it finds, so it will add the
    # columns and tables this script does not create (rating, publisher, the
    # P3/P4 tables). Only the ones the grid and Home read are needed here.
    conn.execute("INSERT INTO schema_version (version) VALUES (?)", (1,))

    for name in TAGS:
        conn.execute("INSERT OR IGNORE INTO tags (name) VALUES (?)", (name,))

    now = datetime.now(timezone.utc)
    covers_written = 0

    for i in range(args.books):
        book_uuid = str(uuid.uuid4())
        title = TITLES[i % len(TITLES)]
        if args.books > len(TITLES):
            title = f"{title} #{i + 1}"
        author = AUTHORS[i % len(AUTHORS)]
        series = SERIES[i % len(SERIES)]
        description = DESCRIPTIONS[i % len(DESCRIPTIONS)]
        # Spread added_at so "recently added" is not an arbitrary tie-break.
        added = (now - timedelta(hours=i)).isoformat()

        bdir = library / book_folder_name(author, title, book_uuid)
        bdir.mkdir(parents=True, exist_ok=True)
        # Book 1 is the one the reader screenshot opens (`read-1`), so that
        # is the one that carries the nested table of contents.
        nested_toc = i == 0
        (bdir / "book.epub").write_bytes(epub_bytes(title, author, nested=nested_toc))

        cover_name = None
        if i % args.no_cover_every != 0:
            # 600x960 is roughly a real cover: big enough that decoding it is
            # honest work, which matters for the timing numbers.
            (bdir / "cover.png").write_bytes(png(600, 960, cover_colour(i)))
            cover_name = "cover.png"
            covers_written += 1

        cur = conn.execute(
            "INSERT INTO books (uuid, title, sort_title, authors, series, "
            "description, format, file_name, file_hash, cover_name, added_at, "
            "progress) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
            (
                book_uuid,
                title,
                title.lower(),
                author,
                series,
                description,
                "epub",
                "book.epub",
                f"hash-{i:06d}",
                cover_name,
                added,
                # A spread of progress values, so "continue reading" has
                # something in it and the progress bar has something to draw.
                (i * 7) % 101,
            ),
        )
        book_id = cur.lastrowid

        for t in range(i % 4):
            tag_id = (i + t) % len(TAGS) + 1
            conn.execute(
                "INSERT OR IGNORE INTO book_tags (book_id, tag_id) VALUES (?,?)",
                (book_id, tag_id),
            )

        # Partially-read books drive Home's "continue reading" strip, which was
        # empty in the first screenshot run and hid that its covers never
        # loaded at all.
        if 0 < (i * 7) % 101 < 100:
            conn.execute(
                "INSERT INTO reading_progress (book_id, chapter_index, "
                "fraction, updated_at) VALUES (?,?,?,?)",
                (book_id, i % 12, ((i * 7) % 101) / 100.0, added),
            )

    total_books = args.books
    if args.series_fixture:
        covers_written += seed_series_fixture(conn, library, now, args.books)
        total_books += 3

    if args.author_fixture:
        covers_written += seed_author_fixture(
            conn, library, now, args.books + (3 if args.series_fixture else 0)
        )
        total_books += 2

    if args.pdf_fixture:
        covers_written += seed_pdf_fixture(
            conn, library, now, notes_empty=args.pdf_notes_empty
        )
        total_books += 1

    if args.comic_fixture:
        covers_written += seed_comic_fixture(conn, library, now)
        total_books += 2

    if args.reader_fixture:
        seed_reader_fixture(conn, now)

    if args.settings_fixture or args.settings_empty_dicts:
        seed_settings_fixture(
            conn,
            library,
            root,
            now,
            empty_dictionaries=args.settings_empty_dicts,
        )

    conn.commit()
    conn.close()

    print(f"seed-library: {total_books} books, {covers_written} covers -> {root}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
