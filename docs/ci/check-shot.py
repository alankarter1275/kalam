#!/usr/bin/env python3
"""Answer 'did the book covers actually appear?' from a screenshot.

The bug that keeps getting through is a cover that never loads: the card draws
a grey placeholder and simply stays that way. A human sees it instantly. CI
sees a green build.

For a library-grid screenshot, the seeded covers are deliberately saturated
colours (see seed-library.py) and placeholders are grey, so counting strongly
coloured pixels is a rough proxy for "covers rendered". The same number is not
a useful check for reader pages, task lists or other routes.

No Pillow, no numpy: adding a pip install to CI for this is not worth the
minute or the supply-chain surface. PNG decoding here is stdlib zlib plus a
hand-rolled unfilter, which is about 60 lines and has no dependencies.

Deliberately reports numbers rather than passing or failing. A threshold
invented today would be a guess, and a wrong threshold that fails a good build
is worse than no threshold at all. Once we have seen a few real runs, a
regression check can compare against a known-good count.
"""

import struct
import sys
import zlib


def read_png(path):
    """Return (width, height, rows) with rows as bytes of RGB or RGBA."""
    data = open(path, "rb").read()
    if data[:8] != b"\x89PNG\r\n\x1a\x0a":
        raise ValueError("not a PNG")

    pos = 8
    width = height = depth = colour = None
    idat = bytearray()
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos : pos + 4])
        tag = data[pos + 4 : pos + 8]
        body = data[pos + 8 : pos + 8 + length]
        pos += 12 + length
        if tag == b"IHDR":
            width, height, depth, colour = struct.unpack(">IIBB", body[:10])
        elif tag == b"IDAT":
            idat += body
        elif tag == b"IEND":
            break

    if depth != 8 or colour not in (2, 6):
        raise ValueError(f"unsupported PNG: depth={depth} colour={colour}")

    channels = 3 if colour == 2 else 4
    raw = zlib.decompress(bytes(idat))
    stride = width * channels

    # Undo the per-scanline filter. Straight from the PNG spec; the only
    # subtlety is that filters reference the *reconstructed* previous row.
    out = []
    prev = bytearray(stride)
    p = 0
    for _ in range(height):
        ftype = raw[p]
        p += 1
        line = bytearray(raw[p : p + stride])
        p += stride
        if ftype == 1:  # Sub
            for i in range(channels, stride):
                line[i] = (line[i] + line[i - channels]) & 0xFF
        elif ftype == 2:  # Up
            for i in range(stride):
                line[i] = (line[i] + prev[i]) & 0xFF
        elif ftype == 3:  # Average
            for i in range(stride):
                left = line[i - channels] if i >= channels else 0
                line[i] = (line[i] + ((left + prev[i]) >> 1)) & 0xFF
        elif ftype == 4:  # Paeth
            for i in range(stride):
                a = line[i - channels] if i >= channels else 0
                b = prev[i]
                c = prev[i - channels] if i >= channels else 0
                pp = a + b - c
                pa, pb, pc = abs(pp - a), abs(pp - b), abs(pp - c)
                pr = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                line[i] = (line[i] + pr) & 0xFF
        out.append(bytes(line))
        prev = line
    return width, height, out, channels


def analyse(path):
    width, height, rows, channels = read_png(path)

    # Sample rather than read every pixel: a 1600x1000 shot is 1.6M pixels and
    # this is pure Python. Every 4th pixel of every 4th row is 100k samples,
    # which is plenty to distinguish "a wall of grey" from "coloured covers".
    total = 0
    coloured = 0
    non_black = 0
    for y in range(0, height, 4):
        row = rows[y]
        for x in range(0, width, 4):
            i = x * channels
            r, g, b = row[i], row[i + 1], row[i + 2]
            total += 1
            if r > 12 or g > 12 or b > 12:
                non_black += 1
            # "Colourful" = the channels disagree. Greys have r==g==b, and the
            # app's whole palette is grey, so any real spread is a cover.
            if max(r, g, b) - min(r, g, b) > 40:
                coloured += 1

    pct_colour = 100.0 * coloured / total if total else 0.0
    pct_lit = 100.0 * non_black / total if total else 0.0
    return width, height, pct_colour, pct_lit


def main(argv):
    if len(argv) < 2:
        print("usage: check-shot.py <shot.png> [...]")
        return 0

    for path in argv[1:]:
        try:
            w, h, pct_colour, pct_lit = analyse(path)
        except Exception as exc:  # noqa: BLE001 -- diagnostics must not crash CI
            print(f"  {path}: could not read ({exc})")
            continue

        name = path.rsplit("/", 1)[-1]
        note = ""
        if pct_lit < 1.0:
            note = "  <-- essentially black; the window probably never drew"
        elif pct_colour < 0.5:
            note = "  <-- low colour; interpret as a cover warning only on a library grid"
        print(
            f"  {name}: {w}x{h}  coloured={pct_colour:.1f}%  lit={pct_lit:.1f}%{note}"
        )

    print("  (numbers only -- no threshold is enforced yet; see the docstring)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
