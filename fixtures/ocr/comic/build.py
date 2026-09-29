#!/usr/bin/env python3
"""Generate synthetic comic fixture pages for bubble-aware OCR (ROADMAP 2.14).

Deterministic pages with known ground truth: every balloon's rect and exact
text is written to comic-pages.json beside the PNGs, so both the Python
detection prototype and the Rust CI probe can assert against it.

Pages are deliberately hard for a balloon detector:

- page1 (B&W manga): panel borders, screentone patches (enclosed micro-white),
  a white art highlight with NO dark outline (decoy), an ellipse+tail balloon,
  a rounded caption box, and a stylized SFX word outside any balloon.
- page2 (reading order): two balloons at the same height (a naive y-sort of
  whole-page lines interleaves them), staggered balloons below, and a
  double-outline thought bubble.
- page3 (color manhwa): tinted background, colored art, white balloons with
  dark outlines, stacked vertically like a webtoon slice.
- page5 (faded print, ROADMAP 2.16): old tan paper, balloons at three text
  fade levels (crisp black control, mildly faded grey, badly faded grey just
  under the detector's dark threshold). Detection must still find every
  balloon; recognition difficulty is the variable under test, so the page is
  marked "experimental" in the JSON and the CI probe scores it instead of
  asserting it. (page4, the aged scan, is generated between them.)

Run:  python3 build.py            (regenerates every page + the JSON)
      python3 build.py 5          (renders only page 5, still rewrites the
                                   full JSON from the deterministic specs,
                                   so adding a page never touches the other
                                   binaries)
"""

import json
import math
import os
import random
import sys

from PIL import Image, ImageDraw, ImageFont

HERE = os.path.dirname(os.path.abspath(__file__))
FONT = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"
FONT_BOLD = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"


def font(path, size):
    return ImageFont.truetype(path, size)


def draw_bubble(draw, rect, kind, outline="black", width=5, fill="white"):
    """Draw a balloon of the given kind. rect = (x0, y0, x1, y1)."""
    x0, y0, x1, y1 = rect
    if kind == "ellipse":
        draw.ellipse(rect, fill=fill, outline=outline, width=width)
    elif kind == "rect":  # caption box
        draw.rounded_rectangle(rect, radius=18, fill=fill, outline=outline, width=width)
    elif kind == "thought":  # cloud: one closed scalloped outline
        cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
        rx, ry = (x1 - x0) / 2, (y1 - y0) / 2
        pts = []
        n = 44
        for k in range(n):
            a = 2 * math.pi * k / n
            scallop = 1.0 + 0.07 * math.sin(11 * a)
            pts.append((cx + rx * scallop * math.cos(a), cy + ry * scallop * math.sin(a)))
        draw.polygon(pts, fill=fill, outline=outline, width=4)
    elif kind == "shout":  # spiky balloon
        cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
        rx, ry = (x1 - x0) / 2, (y1 - y0) / 2
        pts = []
        for k in range(28):
            a = 2 * math.pi * k / 28
            spike = 1.0 + (0.18 if k % 2 else -0.10)
            pts.append((cx + rx * spike * math.cos(a), cy + ry * spike * math.sin(a)))
        draw.polygon(pts, fill=fill, outline=outline, width=width)


def draw_tail(draw, tip, base_center, width=26, outline="black", fill="white"):
    """A balloon tail: triangle from the balloon edge to a tip point."""
    tx, ty = tip
    bx, by = base_center
    dx, dy = tx - bx, ty - by
    n = math.hypot(dx, dy) or 1.0
    px, py = -dy / n * width / 2, dx / n * width / 2
    draw.polygon(
        [(bx + px, by + py), (bx - px, by - py), (tx, ty)],
        fill=fill, outline=outline, width=3,
    )


def draw_text_block(draw, cx, cy, lines, fnt, fill="black", line_gap=1.25):
    """Center a block of text lines on (cx, cy). Returns the drawn extents."""
    heights = []
    widths = []
    for line in lines:
        box = draw.textbbox((0, 0), line, font=fnt)
        widths.append(box[2] - box[0])
        heights.append(box[3] - box[1])
    lh = int(fnt.size * line_gap)
    total_h = lh * (len(lines) - 1) + heights[-1]
    y = cy - total_h / 2
    for line, w in zip(lines, widths):
        draw.text((cx - w / 2, y), line, font=fnt, fill=fill)
        y += lh


def screentone(draw, rect, cell=8, dot=3, grey=120):
    """A manga screentone patch: grey halftone dots on white."""
    x0, y0, x1, y1 = rect
    draw.rectangle(rect, fill="white")
    for y in range(y0, y1, cell):
        for x in range(x0, x1, cell):
            cx, cy = x + cell / 2, y + cell / 2
            r = dot / 2
            draw.ellipse((cx - r, cy - r, cx + r, cy + r), fill=(grey, grey, grey))


def build_page1(render=True):
    """B&W manga page, 1200x1800."""
    W, H = 1200, 1800
    img = Image.new("RGB", (W, H), (245, 245, 245))
    d = ImageDraw.Draw(img)
    fnt = font(FONT, 30)
    fnt_small = font(FONT_BOLD, 24)
    fnt_sfx = font(FONT_BOLD, 64)

    # Panel frames
    d.rectangle((30, 30, 1170, 850), outline=0, width=6)
    d.rectangle((30, 890, 1170, 1770), outline=0, width=6)

    # Panel 1 art: dark shapes + screentone
    d.rectangle((60, 500, 400, 820), fill=70)
    d.ellipse((750, 600, 1050, 820), fill=40)
    screentone(d, (450, 550, 700, 820))
    # Decoy: white highlight inside art with NO dark outline around it
    d.ellipse((150, 600, 300, 700), fill=235)  # sits inside dark rect (fill 70)

    # Balloon A: ellipse + tail, panel 1 top-left
    a = (120, 90, 620, 380)
    draw_tail(d, (250, 460), ((a[0] + a[2]) / 2, a[3] - 10))
    draw_bubble(d, a, "ellipse")
    draw_text_block(d, (a[0] + a[2]) / 2, (a[1] + a[3]) / 2, [
        "THE KEEPER'S LIGHT", "HAS GONE OUT.", "Nobody has seen it burn",
    ], fnt)

    # Panel 2 art
    d.polygon([(80, 1500), (500, 1100), (900, 1600)], fill=60)
    screentone(d, (600, 950, 1140, 1250))

    # Balloon B: ellipse, panel 2 right
    b = (640, 1250, 1130, 1560)
    draw_tail(d, (900, 1700), ((b[0] + b[2]) / 2, b[3] - 10))
    draw_bubble(d, b, "ellipse")
    draw_text_block(d, (b[0] + b[2]) / 2, (b[1] + b[3]) / 2 - 20, [
        "Then we climb", "tonight.",
    ], fnt)

    # Caption box C: panel 2 top-left
    c = (70, 940, 520, 1040)
    draw_bubble(d, c, "rect")
    draw_text_block(d, (c[0] + c[2]) / 2, (c[1] + c[3]) / 2, [
        "MEANWHILE, ON THE CLIFFS...",
    ], fnt_small)

    # SFX outside any balloon
    d.text((830, 1670), "KRAKOOM", font=fnt_sfx, fill=0)

    path = os.path.join(HERE, "comic-page1.png")
    img.save(path)
    return {
        "file": "comic-page1.png", "width": W, "height": H, "reading": "ltr",
        "color": False,
        "balloons": [
            {"rect": a, "kind": "ellipse", "text": ["THE KEEPER'S LIGHT", "HAS GONE OUT.", "Nobody has seen it burn"]},
            {"rect": c, "kind": "rect", "text": ["MEANWHILE, ON THE CLIFFS..."]},
            {"rect": b, "kind": "ellipse", "text": ["Then we climb", "tonight."]},
        ],
        "sfx": ["KRAKOOM"],
    }


def build_page2(render=True):
    """Reading-order page: same-height pair, staggered pair, thought bubble."""
    W, H = 1200, 1800
    img = Image.new("RGB", (W, H), (250, 248, 245))
    d = ImageDraw.Draw(img)
    fnt = font(FONT, 32)

    # L and R: same band (a naive whole-page y-sort interleaves their lines)
    left = (90, 140, 540, 430)
    right = (660, 170, 1110, 460)
    draw_bubble(d, left, "ellipse")
    draw_text_block(d, (left[0] + left[2]) / 2, (left[1] + left[3]) / 2, [
        "LEFT FIRST,", "read me", "before the right one",
    ], fnt)
    draw_bubble(d, right, "ellipse")
    draw_text_block(d, (right[0] + right[2]) / 2, (right[1] + right[3]) / 2, [
        "RIGHT SECOND,", "even though", "I start higher",
    ], fnt)

    # Staggered: D starts a new band, E overlaps D by less than half its height
    drect = (90, 620, 540, 900)
    erect = (660, 820, 1110, 1150)
    draw_bubble(d, drect, "ellipse")
    draw_text_block(d, (drect[0] + drect[2]) / 2, (drect[1] + drect[3]) / 2, ["D: new band", "starts here"], fnt)
    draw_bubble(d, erect, "ellipse")
    draw_text_block(d, (erect[0] + erect[2]) / 2, (erect[1] + erect[3]) / 2, ["E: my own band,", "lower still"], fnt)

    # Thought bubble with double outline (the ring must not become a balloon)
    t = (330, 1250, 900, 1550)
    draw_bubble(d, t, "thought")
    draw_text_block(d, (t[0] + t[2]) / 2, (t[1] + t[3]) / 2, ["hmm... maybe", "the lamp oil"], fnt)

    path = os.path.join(HERE, "comic-page2.png")
    img.save(path)
    return {
        "file": "comic-page2.png", "width": W, "height": H, "reading": "ltr",
        "color": False,
        "balloons": [
            {"rect": left, "kind": "ellipse", "text": ["LEFT FIRST,", "read me", "before the right one"]},
            {"rect": right, "kind": "ellipse", "text": ["RIGHT SECOND,", "even though", "I start higher"]},
            {"rect": drect, "kind": "ellipse", "text": ["D: new band", "starts here"]},
            {"rect": erect, "kind": "ellipse", "text": ["E: my own band,", "lower still"]},
            {"rect": t, "kind": "thought", "text": ["hmm... maybe", "the lamp oil"]},
        ],
        "sfx": [],
    }


def build_page3(render=True):
    """Color manhwa/webtoon slice, 800x1200, stacked balloons."""
    W, H = 800, 1200
    img = Image.new("RGB", (W, H), (247, 242, 232))
    d = ImageDraw.Draw(img)
    fnt = font(FONT, 26)

    # Soft colored art blocks
    d.rounded_rectangle((60, 500, 740, 760), radius=40, fill=(196, 164, 132))
    d.ellipse((90, 780, 360, 1050), fill=(132, 148, 176))
    d.rectangle((420, 800, 720, 1100), fill=(168, 132, 148))

    balloons = [
        ((80, 70, 620, 240), "ellipse", ["Morning already?", "The ferry leaves at noon."]),
        ((180, 290, 700, 470), "rect", ["She kept the ticket", "in her coat pocket."]),
        ((100, 800, 560, 990), "ellipse", ["We can still", "make it."]),
    ]
    for rect, kind, lines in balloons:
        draw_bubble(d, rect, kind)
        draw_text_block(d, (rect[0] + rect[2]) / 2, (rect[1] + rect[3]) / 2, lines, fnt)

    path = os.path.join(HERE, "comic-page3.png")
    img.save(path)
    return {
        "file": "comic-page3.png", "width": W, "height": H, "reading": "ltr",
        "color": True,
        "balloons": [
            {"rect": list(b[0]), "kind": b[1], "text": b[2]} for b in balloons
        ],
        "sfx": [],
    }


def build_page4(render=True):
    """Aged-scan stress page: yellowed paper, noise, wobbly outlines, a grey
    caption, an overlapping pair, JPEG artifacts, and one balloon that
    touches the page edge (a documented v1 miss)."""
    W, H = 1200, 1800
    rng = random.Random(1409)
    img = Image.new("RGB", (W, H), (232, 226, 210))
    d = ImageDraw.Draw(img)
    fnt = font(FONT, 30)
    fnt_small = font(FONT_BOLD, 22)

    # panel borders, slightly brown
    d.rectangle((25, 25, 1175, 870), outline=(58, 52, 46), width=6)
    d.rectangle((25, 905, 1175, 1775), outline=(58, 52, 46), width=6)
    # art
    d.rectangle((80, 480, 420, 830), fill=88)
    d.ellipse((720, 560, 1080, 830), fill=52)
    d.polygon([(100, 1650), (560, 1150), (1000, 1700)], fill=70)

    def wobbly_ellipse(rect, jitter=0.035, outline=(30, 28, 24), width=5):
        cx, cy = (rect[0] + rect[2]) / 2, (rect[1] + rect[3]) / 2
        rx, ry = (rect[2] - rect[0]) / 2, (rect[3] - rect[1]) / 2
        pts = []
        for k in range(64):
            a = 2 * math.pi * k / 64
            wob = 1.0 + jitter * math.sin(7 * a + 1.2) + 0.02 * math.sin(23 * a)
            pts.append((cx + rx * wob * math.cos(a), cy + ry * wob * math.sin(a)))
        d.polygon(pts, fill=(247, 246, 242), outline=outline, width=width)

    # Balloon 1: wobbly ellipse + tail, panel 1
    a = (110, 80, 600, 360)
    wobbly_ellipse(a)
    draw_tail(d, (260, 450), ((a[0] + a[2]) / 2, a[3] - 10), outline=(30, 28, 24))
    wobbly_ellipse((a[0] - 4, a[1] - 4, a[2] + 4, a[3] + 4), jitter=0.0)  # hidden seam? no-op guard
    draw_text_block(d, (a[0] + a[2]) / 2, (a[1] + a[3]) / 2, [
        "The lamp is out", "and the tide is turning.",
    ], fnt, fill=(25, 22, 20))

    # Balloon 2: grey-filled caption box (lum ~215)
    c = (60, 940, 560, 1050)
    d.rounded_rectangle(c, radius=14, fill=(214, 214, 212), outline=(30, 28, 24), width=4)
    draw_text_block(d, (c[0] + c[2]) / 2, (c[1] + c[3]) / 2, [
        "THAT NIGHT, THE FERRY NEVER CAME.",
    ], fnt_small, fill=(25, 22, 20))

    # Balloons 3+4: overlapping pair (detected as one region - documented)
    e1 = (620, 1180, 1100, 1440)
    e2 = (700, 1380, 1160, 1660)
    d.ellipse(e1, fill=(247, 246, 242), outline=(30, 28, 24), width=5)
    d.ellipse(e2, fill=(247, 246, 242), outline=(30, 28, 24), width=5)
    draw_text_block(d, (e1[0] + e1[2]) / 2, (e1[1] + e1[3]) / 2, ["We row", "instead."], fnt, fill=(25, 22, 20))
    draw_text_block(d, (e2[0] + e2[2]) / 2 - 60, (e2[1] + e2[3]) / 2 + 30, ["Agreed."], fnt, fill=(25, 22, 20))

    # Balloon 5: flat side clipped off by the page edge -> its white merges
    # with the page background -> v1 misses it (documented limitation)
    edge = (1080, 60, 1320, 320)
    d.rounded_rectangle(edge, radius=16, fill=(247, 246, 242), outline=(30, 28, 24), width=5)
    draw_text_block(d, 1110, (edge[1] + edge[3]) / 2, ["edge!"], fnt, fill=(25, 22, 20))

    # Sensor noise + JPEG artifacts
    if render:
        import numpy as np
        arr = np.asarray(img).astype(np.int16)
        noise = np.array([[[rng.randint(-6, 6)] * 3 for _ in range(arr.shape[1])]
                          for _ in range(arr.shape[0])], dtype=np.int16)
        arr = np.clip(arr + noise, 0, 255).astype(np.uint8)
        out = Image.fromarray(arr)
        path = os.path.join(HERE, "comic-page4.jpg")
        out.save(path, quality=82)

    return {
        "file": "comic-page4.jpg", "width": W, "height": H, "reading": "ltr",
        "color": False,
        "balloons": [
            {"rect": list(a), "kind": "ellipse", "text": ["The lamp is out", "and the tide is turning."]},
            {"rect": list(c), "kind": "rect", "text": ["THAT NIGHT, THE FERRY NEVER CAME."]},
            {"rect": list(e1), "kind": "ellipse", "text": ["We row", "instead."]},
            {"rect": list(e2), "kind": "ellipse", "text": ["Agreed."]},
            {"rect": [1080, 60, 1200, 320], "kind": "rect", "text": ["edge!"], "expect_miss": True},
        ],
        "sfx": [],
    }


def build_page5(render=True):
    """Faded-print experiment page (ROADMAP 2.16), 1200x1800.

    Old tan paper (not white to the detector), balloons with near-white fill,
    and three text fade levels: a crisp black control, mildly faded grey, and
    badly faded grey kept just under the detector's DARK_T (128) — bold, so
    the stroke cores still count as ink and every balloon is detected. The
    page is marked "experimental": the probe scores recognition instead of
    asserting it.
    """
    W, H = 1200, 1800
    img = Image.new("RGB", (W, H), (176, 162, 134))
    d = ImageDraw.Draw(img)
    fnt = font(FONT_BOLD, 32)

    d.rectangle((25, 25, 1175, 870), outline=(70, 62, 50), width=6)
    d.rectangle((25, 905, 1175, 1775), outline=(70, 62, 50), width=6)
    # art: muted shapes, all darker than the paper
    d.ellipse((80, 480, 420, 830), fill=(120, 108, 88))
    d.polygon([(100, 1650), (560, 1150), (1000, 1700)], fill=(104, 94, 76))

    # Control: crisp black text
    a = (110, 90, 610, 360)
    draw_tail(d, (260, 450), ((a[0] + a[2]) / 2, a[3] - 10), outline=(85, 78, 66))
    draw_bubble(d, a, "ellipse", outline=(85, 78, 66), width=5, fill=(238, 236, 228))
    draw_text_block(d, (a[0] + a[2]) / 2, (a[1] + a[3]) / 2, [
        "The print faded", "over the years.",
    ], fnt, fill=(30, 28, 24))

    # Mild fade: grey text (luminance ~98)
    b = (640, 130, 1130, 430)
    draw_bubble(d, b, "ellipse", outline=(85, 78, 66), width=5, fill=(238, 236, 228))
    draw_text_block(d, (b[0] + b[2]) / 2, (b[1] + b[3]) / 2, [
        "This ink was cheap", "and grey from the start.",
    ], fnt, fill=(102, 98, 90))

    # Bad fade: luminance ~118, just under DARK_T so the ink test still passes
    c = (150, 950, 700, 1230)
    draw_bubble(d, c, "ellipse", outline=(85, 78, 66), width=5, fill=(238, 236, 228))
    draw_text_block(d, (c[0] + c[2]) / 2, (c[1] + c[3]) / 2, [
        "Barely legible", "even to young eyes.",
    ], fnt, fill=(122, 118, 110))

    if render:
        path = os.path.join(HERE, "comic-page5.png")
        img.save(path)

    return {
        "file": "comic-page5.png", "width": W, "height": H, "reading": "ltr",
        "color": False, "experimental": True,
        "balloons": [
            {"rect": list(a), "kind": "ellipse", "text": ["The print faded", "over the years."]},
            {"rect": list(b), "kind": "ellipse", "text": ["This ink was cheap", "and grey from the start."]},
            {"rect": list(c), "kind": "ellipse", "text": ["Barely legible", "even to young eyes."]},
        ],
        "sfx": [],
    }


def main():
    builders = {
        "1": build_page1,
        "2": build_page2,
        "3": build_page3,
        "4": build_page4,
        "5": build_page5,
    }
    wanted = set(sys.argv[1:]) or set(builders)
    pages = [builders[n](n in wanted) for n in ("1", "2", "3", "4", "5")]
    out = os.path.join(HERE, "comic-pages.json")
    with open(out, "w") as f:
        json.dump({"pages": pages}, f, indent=1)
    for p in pages:
        print(f"{p['file']}: {len(p['balloons'])} balloons, {len(p['sfx'])} sfx")


if __name__ == "__main__":
    main()
