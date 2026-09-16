=== KALAM REPORT ===
engine: d325d2b (bundle on my branch as 4c19bbb)
step: your list, item 3 — the three selection items in the owner's list
status: DONE for the action box (Kalam's) / SPLIT for the other two (yours)

The owner's item 3 points at three things from the previous reader: "the
action box that appears when we select text", "how the selection highlight
worked", and "the draggable handles". One of the three is the host's; the
other two are drawn on the page, so they are yours and this is the report
you asked for. I have no separate symptom sentence from the owner beyond
those three pointers, so where the visible difference is a judgement I say
what the code does and leave the call to you.

---- 1. the action box (Kalam's, done) --------------------------------

The GTK chip that replaced the JS one was a different object: five
swatches always out, then three text buttons reading Quote / Dictionary /
Copy. The old chip was a pill of icon buttons — a highlighter that *opens*
the swatch row, a quote, an "Aa", a copy — with hairlines between them.

Rebuilt to that, in commit "reader: the selection chip is the old action
box again":

* 32 px round buttons, 4 px pill padding, 2 px gaps, 20 px swatches behind
  a collapsed row, 1x20 px separators at 18 per cent of the border colour,
  radius-999 pill, the old shadow (`0 10px 28px` at 26 per cent).
* the four icons are the JS chip's own 24-unit SVG paths, painted by a
  17 px GtkDrawingArea in the button's CSS colour: the highlighter's three
  strokes, the filled quote pair, the two copy sheets (including the stub
  the SVG path started with, `M16 8 V6`, whose right edge hides behind the
  front sheet), and "Aa" drawn as strokes rather than set as text so the
  icon does not depend on fontconfig.
* the tooltips are the old ones, including "Dictionary (D)".
* the swatch row starts hidden, as `.kalam-chip-colors` did.

Two behaviours came with the box: `D` opens the dictionary for a standing
selection (the old button's promise), and `Escape` drops the selection and
its chip before it closes the book, which is the order the old shell used.
The chip is also dropped on a page turn now — the tap that turns the page
has already dropped the selection in the widget, so the chip must not
survive pointing at a rectangle on the previous page.

OUTSTANDING: the owner's look. Drag-select a sentence and screenshot the
bar. The three actions and the five colours are unchanged; this is the
shape, the icons and the spacing.

---- 2. the selection highlight (yours) -------------------------------

Old reader, for reference: the band was drawn as one div per line, in a
layer *under* the chapter content with the selected content stacked above
it, so the tint never sat over the glyphs. Its height was the line's
reference glyph height **plus 2 px above and below**, it had a 2 px corner
radius, its colour was the theme's selection background, and it carried
`mix-blend-mode: multiply` on Light and Sepia and `screen` on Dark and Ink
("so the highlight does not tint the glyphs", per the comment in the old
stylesheet).

Now: `crates/chapbook-paint/src/display.rs:220 push_selection_rect` fills,
per visually contiguous span, `fragment.rect` — `origin.y` and `size.h`,
the *whole line fragment box* — with one flat translucent fill,
`settings.palette().selection` (`crates/kalam-reader/src/prefs.rs:93`,
which is Kalam's `::selection` rgba with the alpha rounded to a byte),
square-cornered, no blend, painted before the line's glyphs.

So the three visible differences from the old band are: the fill's height
is the line box rather than the glyph height + 2 px, the corners are
square rather than 2 px, and there is no per-theme blend mode. If "see how
the selection highlight worked" means it reads heavier or blockier than
the old one, that is where it comes from. Numbers if you want them: band
padding 2 px, radius 2 px, multiply on light/sepia, screen on dark/ink.

Question: should those three be engine constants, or prefs the host sets?
Kalam's theme module no longer has `selection_style()` (it went with the
WebView), so today there is nothing on our side to hand you; say the word
and I will add whatever shape you want to `KalamPrefs`.

---- 3. the draggable handles (yours) ---------------------------------

Old reader: `.kalam-selection-handle-start` and `-end`, a 2 px wide
vertical bar in the theme's handle colour spanning the selection's height,
with a 24 px-wide invisible hit area reaching 12 px past either end of it
and a rotated teardrop grip at the outer end; `cursor: grab`; dragging
either one moved *that end* of the selection, with the bands and the chip
following live.

Now there are no handles anywhere in the crates, and no gesture to adjust a
standing selection either: a press calls `selection_begin`
(`crates/kalam-reader/src/view.rs:1388`/`:1391`), which per
`crates/chapbook-reader/src/text_surface.rs:87` *replaces* the selection
with a fresh anchor at the press point, and the drag extends from that
press point. Nothing tells a press on an end of the selection from a press
in its middle.

Kalam cannot add this on its own even if you would rather it did:
`SelectedText` (`view.rs:125`) carries the whole selection's text and a
single union `rect`, so there is no geometry to place a handle at either
end and no callback that would fire on a handle drag.

Ask: either you draw the handles (preferred — they have to be placed and
hit-tested in the same coordinates as the text), or `SelectedText` grows
the two edge rects and the widget gains a "drag this end" entry point, and
Kalam draws and drags them. Either is fine by me; the second needs the two
rects to be in widget coordinates like `rect` is.

---- for the record --------------------------------------------------

`view.rs:1441`, the tap path, calls `session.selection_clear()` and never
`notify_selection()`, so the host hears that a selection ended on the next
press (`view.rs:1397`) rather than at the moment it ends. No symptom I can
construct — the chip is dismissed on that next press — but a selection's
life has two ends and only one of them is reported; one line beside the
clear would close it.

tried: read the old reader's stylesheet and JS out of the base commit
(`#kalam-chip`, `.kalam-selection-band`, `.kalam-selection-handle`, and
`theme.selection_style()`), then read your painter and gesture code to
compare — `push_selection_rect`, the `Selection` struct, `frame.rs`'s
selection push, `prefs.rs`'s tint, `selection_begin`/`selection_drag`/
`selection_clear`, the press/drag/tap handlers and `SelectedText`. The D
and Escape bindings were checked against the old `keydown` handler before
adding them, and the cairo fill/stroke Result (5 sites) was the first
compiler pass' only complaint.
=== END ===
