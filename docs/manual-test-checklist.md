# Manual test checklist

Things that are built and CI-verified (compile / clippy / unit tests) but still
need a real run on your machine. Work through these when a build is ready, and
tick them off. The agent will remind you about this list.

## Dictionary & annotations polish (Phase 2 plan)
- [ ] Look up a word — popup matches the v3 mockup: dark rounded card, serif
      word, mono pronunciation, purple part-of-speech pill, numbered
      definitions with italic examples, blue synonym / red antonym chips,
      idiom cards, bottom fade.
- [ ] Synonym / antonym chips are tappable and re-look-up that word.
- [ ] "Show N more" expands extra definitions.
- [ ] Bookmark (save) button works and shows the saved state.
- [ ] 🔊 audio button speaks the word (needs `espeak-ng` or `spd-say`
      installed; otherwise it should toast which one to install).

## Reader chrome
- [ ] The bottom page pill (← 10 / 140 title →) now **floats** with a gap above
      the bottom edge instead of hugging it.

## Highlights / notes export (#5 part 1)
- [ ] Reader → Highlights panel → **Export** writes `~/Highlights.md`.
- [ ] The file lists highlights, quotes and notes with chapter + note text.
- [ ] A success toast shows the count and path.

## Spaced-repetition review (#5 part 2)
- [ ] Library → **Review** quick-link opens the review page.
- [ ] With saved words: a word shows; *Show definition* reveals it;
      **Again / Good / Easy** advance to the next card.
- [ ] "Again" brings the word back soon (~10 min); Good/Easy push it out days.
- [ ] With nothing due: "All caught up — nothing due right now."
- [ ] With no saved words at all: the hint to save some words first.

## Roadmap 2.x
- [ ] **2.3 Font picker:** Reader → Settings → Reading → "Typeface" dropdown
      lists your installed fonts; picking one changes the body type live;
      "Default" restores the bundled face. The choice survives restart.
- [ ] **2.4 Text layout:** the new "Text" section has Justify / Hyphenation /
      Publisher styles. Justify gives ragged-right text flush edges; turning
      Hyphenation off removes the end-of-line dashes in narrow columns;
      Publisher styles off drops the book's own styling for plain Kalam text.
      All three apply instantly and survive restart.

- [ ] **2.5 Footnotes:** tap a footnote marker (a `[1]` in the text) — the
      note appears in a card right there, you stay on your page. "Go to the
      note" jumps to it properly. Tapping outside closes the card. A link to
      another chapter or a TOC entry should still just navigate, never open
      a card.
- [ ] **2.6 Jump back:** after jumping (a TOC entry, a search match, a link
      in the page), an undo button appears in the top-left dock next to
      Library and Bookmark; **Backspace** does the same. It takes you back to
      exactly where you were. Turning a page normally makes it disappear.
- [ ] **2.7 Two pages:** make the window wide — you get facing pages, as
      before. Settings → Reading → Layout → turn **"Two pages side by
      side"** off: one page, full width, at any window size. Turning a page
      moves one page, not two. It survives restart. Narrow the window with
      it on and the spread should still give way to one page.
- [ ] **2.8 Fonts folder:** Reader → Settings → Reading → Type → **Fonts
      folder → Open** opens `~/.local/share/kalam/fonts` in your file
      manager. Copy a `.ttf` or `.otf` in there, **close and reopen the
      book**, and the new face should be in the Typeface picker and render.
      Without any files dropped in, nothing should change or slow down.
- [ ] **2.9 Pointer (part 1 of 2):** leave the mouse still over the page for
      ~2 seconds — the pointer should vanish. Move it and it's back at once.
      Settings → Reading → Pointer → **Hide the mouse pointer** off: it should
      never vanish. Survives restart.
- [ ] **2.9 Scroll speed (part 2 of 2):** in continuous-scroll mode, change
      **Scroll speed** and the wheel should scroll noticeably more or less per
      notch. Survives restart. (Paged mode ignores the wheel, as before.)
- [ ] **2.9 Keyboard (part 3 of 3):** Settings → Reading → **Keyboard**
      lists twelve actions with their keys. Click one, press a new key — the
      button should show it and the key should work at once. Escape cancels.
      Bind a key another action already has: that other action should go to
      **"None"**, not keep working. **Reset to defaults** puts them all back.
      Survives restart. Note `m` no longer adds a bookmark — `b` does.
- [ ] **2.11 Text PDFs:** open a **text** PDF (a novel or paper, not a scan).
      It should show readable reflowed text instead of grey stripes, with a
      note saying the page has no image. Open a **scanned** PDF — it should
      still show the page image exactly as before. The **Text Reflow** button
      should still switch either way, and your choice should stick.

## Numbers, if you can (for 2.12's "measure first")
Run the app with `KALAM_TIMING=1` and send me the `[timing]` lines:

    KALAM_TIMING=1 cargo run --release

Worth having: `pdf_open` and `pdf_page_reflow` / `pdf_page_image` (open one
text PDF and one scanned PDF), plus `book_open` for an EPUB. That tells us
what the two PDF paths actually cost on your machine instead of what I
expect them to cost.

## Carried over from earlier work (re-verify if you haven't)
- [ ] Splash screen shows the brand on cold start (no blank window).
- [ ] Home appears first; heavy chores run after first paint.
- [ ] Bulk import expands to show every imported book name.
- [ ] Each delete shows up as its own task in the task manager.
