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

Keep SCHEMA_VERSION in step with src/db.rs:53. If the app ever refuses to open
a seeded database, that mismatch is the first thing to check -- the app runs
migrations forward, so a *lower* number here is safe; a higher one is not.
"""

import argparse
import io
import os
import pathlib
import random
import sqlite3
import struct
import sys
import uuid
import zipfile
import zlib
from datetime import datetime, timedelta, timezone
from xml.sax.saxutils import escape

# Must not exceed src/db.rs SCHEMA_VERSION. Lower is fine (the app migrates up).
SCHEMA_VERSION = 14

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


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--books", type=int, default=139)
    ap.add_argument(
        "--no-cover-every",
        type=int,
        default=17,
        help="every Nth book gets no cover at all (a real and untidy state)",
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

        bdir = library / book_uuid
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

    conn.commit()
    conn.close()

    print(f"seed-library: {args.books} books, {covers_written} covers -> {root}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
