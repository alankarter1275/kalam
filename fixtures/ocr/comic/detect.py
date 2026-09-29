#!/usr/bin/env python3
"""Prototype of the balloon detector for bubble-aware comic OCR (ROADMAP 2.14).

This mirrors, step for step, what the Rust implementation in src/ will do, so
thresholds tuned here transfer directly:

  1. luminance -> white mask (lum >= WHITE_T)
  2. connected components (8-conn) on the mask
  3. drop border-connected components (page background white)
  4. geometry filters: area range, bbox fill ratio, aspect ratio
  5. outline test: fraction of boundary neighbors that are dark
  6. text test: dark-pixel fraction inside the component (glyphs)
  7. reading order: banded top-to-bottom; within a band LTR (or RTL)

Run:  python3 detect.py     (from this directory)
"""

import json
import math
import os
import sys
import time

import numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))

# --- tunables (to transfer verbatim to Rust) -------------------------------
WHITE_T = 200          # pixel is "white" at this luminance or above
DARK_T = 128           # pixel is "dark" (outline / glyph) at or below this
MIN_AREA_FRAC = 0.002  # smallest balloon = 0.2% of the page
MAX_AREA_FRAC = 0.50   # largest = half the page
MIN_FILL = 0.35        # component area / bbox area
MIN_ASPECT = 0.20      # bbox w/h
MAX_ASPECT = 8.0
MIN_PERIM_DARK = 0.55  # fraction of boundary neighbors that must be dark
MIN_INK = 0.003        # dark-pixel fraction inside (glyphs live here)
MAX_INK = 0.60
BAND_OVERLAP = 0.50    # vertical overlap (of the shorter box) => same band
# ---------------------------------------------------------------------------


def luminance(img):
    a = np.asarray(img.convert("RGB"), dtype=np.uint32)
    return ((a[:, :, 0] * 299 + a[:, :, 1] * 587 + a[:, :, 2] * 114) // 1000).astype(np.uint8)


def components(mask):
    """8-connected components. Returns list of (labels set is implicit):
    (pixel_count, bbox, member_rows, member_cols). Pure-BFS like the Rust."""
    h, w = mask.shape
    labels = np.full((h, w), -1, dtype=np.int32)
    comps = []
    ys, xs = np.nonzero(mask)
    for start in range(len(ys)):
        y0, x0 = ys[start], xs[start]
        if labels[y0, x0] != -1:
            continue
        cid = len(comps)
        stack = [(y0, x0)]
        labels[y0, x0] = cid
        count = 0
        minx = miny = 1 << 30
        maxx = maxy = -1
        while stack:
            y, x = stack.pop()
            count += 1
            if x < minx: minx = x
            if x > maxx: maxx = x
            if y < miny: miny = y
            if y > maxy: maxy = y
            for dy in (-1, 0, 1):
                for dx in (-1, 0, 1):
                    ny, nx = y + dy, x + dx
                    if 0 <= ny < h and 0 <= nx < w and mask[ny, nx] and labels[ny, nx] == -1:
                        labels[ny, nx] = cid
                        stack.append((ny, nx))
        comps.append((count, (minx, miny, maxx + 1, maxy + 1)))
    return labels, comps


def detect(lum):
    h, w = lum.shape
    mask = lum >= WHITE_T
    total = h * w

    labels, comps = components(mask)

    # neighbor deltas: 4-neighbors for boundary walking
    results = []
    for cid, (count, bbox) in enumerate(comps):
        x0, y0, x1, y1 = bbox
        # border-connected => page background (or a tail off the page edge)
        if x0 == 0 or y0 == 0 or x1 == w or y1 == h:
            continue
        if count < MIN_AREA_FRAC * total or count > MAX_AREA_FRAC * total:
            continue
        bw, bh = x1 - x0, y1 - y0
        fill = count / (bw * bh)
        if fill < MIN_FILL:
            continue
        aspect = bw / bh
        if aspect < MIN_ASPECT or aspect > MAX_ASPECT:
            continue

        # Ink = dark pixels enclosed by this component (the glyphs). The
        # component itself is all white; its text lives in HOLES. Flood the
        # non-member pixels of the bbox from the bbox border; the pixels the
        # flood cannot reach are the holes.
        sub_lab = labels[y0:y1, x0:x1]
        sub_lum = lum[y0:y1, x0:x1]
        member = sub_lab == cid
        nonmember = ~member
        reach = np.zeros_like(nonmember)
        stack = []
        bh_, bw_ = nonmember.shape
        for xx in range(bw_):
            for yy in (0, bh_ - 1):
                if nonmember[yy, xx] and not reach[yy, xx]:
                    reach[yy, xx] = True; stack.append((yy, xx))
        for yy in range(bh_):
            for xx in (0, bw_ - 1):
                if nonmember[yy, xx] and not reach[yy, xx]:
                    reach[yy, xx] = True; stack.append((yy, xx))
        while stack:
            y, x = stack.pop()
            for dy, dx in ((-1, 0), (1, 0), (0, -1), (0, 1)):
                ny, nx = y + dy, x + dx
                if 0 <= ny < bh_ and 0 <= nx < bw_ and nonmember[ny, nx] and not reach[ny, nx]:
                    reach[ny, nx] = True; stack.append((ny, nx))
        holes = nonmember & ~reach
        ink = int(np.count_nonzero(holes & (sub_lum <= DARK_T))) / count
        if ink < MIN_INK or ink > MAX_INK:
            continue

        # Perimeter darkness: for member pixels, count non-member 4-neighbors,
        # and how many of those positions are dark. A position counts as dark
        # when the pixel itself is dark OR a 4-neighbor of it is dark - JPEG
        # ringing paints a 1px light halo right against a black outline, and
        # without this tolerance a quality-82 scan loses a third of its
        # outline signal.
        m = member
        non = ~m
        dark = sub_lum <= DARK_T
        dark_ring = dark.copy()
        for dy, dx in ((-1, 0), (1, 0), (0, -1), (0, 1)):
            shifted = np.zeros_like(dark)
            ys_ = slice(max(0, -dy), m.shape[0] - max(0, dy))
            ys2 = slice(max(0, dy), m.shape[0] - max(0, -dy))
            xs_ = slice(max(0, -dx), m.shape[1] - max(0, dx))
            xs2 = slice(max(0, dx), m.shape[1] - max(0, -dx))
            shifted[ys2, xs2] = dark[ys_, xs_]
            dark_ring |= shifted
        neigh = np.zeros_like(m, dtype=np.int32)
        dark_here = np.zeros_like(m, dtype=np.int32)
        for dy, dx in ((-1, 0), (1, 0), (0, -1), (0, 1)):
            shifted_non = np.zeros_like(non)
            shifted_dark = np.zeros_like(non)
            ys_ = slice(max(0, -dy), m.shape[0] - max(0, dy))
            ys2 = slice(max(0, dy), m.shape[0] - max(0, -dy))
            xs_ = slice(max(0, -dx), m.shape[1] - max(0, dx))
            xs2 = slice(max(0, dx), m.shape[1] - max(0, -dx))
            shifted_non[ys2, xs2] = non[ys_, xs_]
            shifted_dark[ys2, xs2] = dark_ring[ys_, xs_]
            neigh += (m & shifted_non).astype(np.int32)
            dark_here += (m & shifted_non & shifted_dark).astype(np.int32)
        total_border = int(neigh.sum())
        if total_border == 0:
            continue
        perim_dark = int(dark_here.sum()) / total_border
        if perim_dark < MIN_PERIM_DARK:
            continue

        results.append({"rect": (x0, y0, x1, y1), "area": count,
                        "fill": fill, "ink": ink, "perim": perim_dark})

    # Containment pruning: a balloon never contains another balloon. A
    # candidate whose bbox fully contains a smaller candidate's bbox AND its
    # center is a panel or other background region - drop it.
    def contains(big, small):
        bx0, by0, bx1, by1 = big["rect"]
        sx0, sy0, sx1, sy1 = small["rect"]
        cx, cy = (sx0 + sx1) / 2, (sy0 + sy1) / 2
        return (bx0 <= sx0 and sx1 <= bx1 and by0 <= sy0 and sy1 <= by1
                and bx0 <= cx <= bx1 and by0 <= cy <= by1)

    pruned = []
    for big in results:
        if not any(small is not big and contains(big, small) for small in results):
            pruned.append(big)
    return pruned


def reading_order(balloons, rtl=False):
    """Banded sort: bands by y-overlap; within a band by x (asc/desc)."""
    remaining = sorted(balloons, key=lambda b: b["rect"][1])
    ordered = []
    while remaining:
        band = [remaining[0]]
        ref = band[0]["rect"]
        for cand in remaining[1:]:
            r = cand["rect"]
            overlap = min(ref[3], r[3]) - max(ref[1], r[1])
            shorter = min(ref[3] - ref[1], r[3] - r[1])
            if shorter > 0 and overlap / shorter >= BAND_OVERLAP:
                band.append(cand)
        for b in band:
            remaining.remove(b)
        band.sort(key=lambda b: b["rect"][0], reverse=rtl)
        ordered.extend(band)
    return ordered


def iou(a, b):
    ax0, ay0, ax1, ay1 = a
    bx0, by0, bx1, by1 = b
    ix = max(0, min(ax1, bx1) - max(ax0, bx0))
    iy = max(0, min(ay1, by1) - max(ay0, by0))
    inter = ix * iy
    union = (ax1 - ax0) * (ay1 - ay0) + (bx1 - bx0) * (by1 - by0) - inter
    return inter / union if union else 0.0


def center_in(rect, other):
    cx = (rect[0] + rect[2]) / 2
    cy = (rect[1] + rect[3]) / 2
    return other[0] <= cx <= other[2] and other[1] <= cy <= other[3]


def main():
    gt = json.load(open(os.path.join(HERE, "comic-pages.json")))
    ok = True
    for page in gt["pages"]:
        img = Image.open(os.path.join(HERE, page["file"]))
        lum = luminance(img)
        t0 = time.perf_counter()
        found = detect(lum)
        dt = (time.perf_counter() - t0) * 1000

        gts = [tuple(b["rect"]) for b in page["balloons"]]
        expect_miss = [i for i, b in enumerate(page["balloons"]) if b.get("expect_miss")]
        matched = [None] * len(gts)
        for f in found:
            for i, g in enumerate(gts):
                if matched[i] is None and center_in(g, f["rect"]) and center_in(f["rect"], g):
                    matched[i] = f
        missed = [gts[i] for i, m in enumerate(matched)
                  if m is None and i not in expect_miss]
        extra = [f for i, f in enumerate(found) if f not in matched]

        print(f"--- {page['file']}  ({lum.shape[1]}x{lum.shape[0]}, detect {dt:.0f} ms)")
        print(f"    ground truth {len(gts)}, found {len(found)}, matched {sum(m is not None for m in matched)}, extra {len(extra)}")
        for i, (g, m) in enumerate(zip(gts, matched)):
            if m is None:
                if i in expect_miss:
                    continue
                print(f"    MISS  gt{i} {g}")
                ok = False
            else:
                v = iou(g, m["rect"])
                print(f"    ok    gt{i} iou={v:.2f} fill={m['fill']:.2f} ink={m['ink']:.3f} perim={m['perim']:.2f}")
                if v < 0.55:
                    print(f"          WARNING low iou (tail inflates the box)")
        for f in extra:
            print(f"    EXTRA {f['rect']} fill={f['fill']:.2f} ink={f['ink']:.3f} perim={f['perim']:.2f}")
            ok = False
        for i in expect_miss:
            if matched[i] is not None:
                print(f"    NOTE gt{i} was expected to be missed but WAS found (update the ground truth)")
            else:
                print(f"    expected-miss gt{i} (documented v1 limitation): confirmed")

        order = reading_order(found, rtl=False)
        got_order = []
        for f in order:
            for i, m in enumerate(matched):
                if m is f:
                    got_order.append(i)
        print(f"    reading order (gt indices): {got_order}")
        expected = [i for i in range(len(gts)) if i not in expect_miss]
        if got_order != expected:
            print(f"    ORDER MISMATCH, expected {expected}")
            ok = False

        if page["file"] == "comic-page2.png":
            rtl = [i for i, m in enumerate(
                reading_order(found, rtl=True)) for i2, m2 in
                enumerate(matched) if m2 is m and i2 == i]
            # rebuild properly: map each ordered balloon to its gt index
            rtl_idx = []
            for f in reading_order(found, rtl=True):
                for i, m in enumerate(matched):
                    if m is f:
                        rtl_idx.append(i)
            print(f"    rtl reading order (gt indices): {rtl_idx}")
            if rtl_idx != [1, 0, 2, 3, 4]:
                print("    RTL ORDER MISMATCH, expected [1, 0, 2, 3, 4]")
                ok = False
    print("RESULT:", "PASS" if ok else "FAIL")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
