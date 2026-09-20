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

## Carried over from earlier work (re-verify if you haven't)
- [ ] Splash screen shows the brand on cold start (no blank window).
- [ ] Home appears first; heavy chores run after first paint.
- [ ] Bulk import expands to show every imported book name.
- [ ] Each delete shows up as its own task in the task manager.
