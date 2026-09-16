=== KALAM REPORT ===
engine: d325d2b (bundle on my branch as 4c19bbb)
step: your numbered list — items 1 and 2 delivered, 3 blocked on a truncated relay
status: DONE (1, 2) / QUESTION (3)

Order note: your list arrived after the owner's render was already answered,
so items 1 and 2 were done in the previous commit, not this one. Nothing was
started out of order; item 3 cannot start and item 4 waits for the owner.

1. Tap-to-dictionary removed -- DONE, commit 057862a, CI green.
   `connect_word` is not called, `EngineWord` is out of `ReaderMsg`, its
   `update_with_view` arm is gone, `TappedWord` is out of the imports and
   `gdk_rect` is still used by the two handlers that remain.
   Before removing the highlight branch I checked main rather than assuming:
   the old shell's bridge messages are chapter-changed, dict-lookup,
   highlight, jump-to-chapter, link-click, progress, quote, reader-ui-hide,
   request-next/prev-chapter, save-word, search-in-book(-done), selection and
   unsave-word. `fireTapLookup` appears once, as its own definition, with no
   caller -- and nothing sends anything on a tap on a highlight span either.
   So there was no such branch on main and nothing to keep.
   Your widget change is in the build: Cargo.lock resolves kalam-reader to
   d325d2bca6ef08968447e854abad1dc46fe92fb82, and view.rs's
   `wants_words`/`_ if !wants_words => None` guard is the only crate change.
   VERIFICATION OUTSTANDING: "a tap on a word turns the page" is the owner's
   check. I tried to answer it from CI and could not -- see the harness note
   below -- so it is not a defect report, it is an open check.

2. The card rebuilt against the mockups -- DONE, same commit, CI green.
   380 px (now capped at `380.min(host.width() - 32)`, your narrow-window
   suggestion), header 20/20/16, headword 26 px serif 600 with 5 px under it,
   pronunciation 11 px mono with 7 px, POS pill `#c678dd` at 14 per cent,
   30 px round buttons at 15 px glyph, body 4/20/24, section labels 9 px at
   20/10, senses 13 px on line-height 1.6 with 3 px to the example, chips
   12 px on 4/12, idiom cards radius 10 padding 10/12, "Show N more"
   margin-top 8, chip alphas 0.15/0.14 and borders 0.30 to match the mockup
   exactly. Fonts are `"Fraunces", serif` and `monospace` -- what Kalam's own
   style.css names -- per your instruction. The three buttons stay, because
   the shipped popup this replaces has them and test_kalam_dict_preview.js
   asserts all three; the mockup draws only the save button.
   OUTSTANDING: the owner's screenshot. The card has still never been drawn
   anywhere I can see.

3. The owner's list is truncated in the relay. It reads, in full, as far as it
   arrived:

     "3. The owner's list of small things (each: what they did, what they
      saw, what they expected):
      Look at the previous projects action box that appears when we select a

      [blank]

      Anything on it that is text on the page itself ... is the engine's"

   That is one partial sentence with no "saw" or "expected", and no statement
   of what the box is or which project it belongs to. Re-send the list and I
   will do the Kalam-side items and forward the page-text ones to you with
   the exact symptom, as you ask. I have not guessed at it.

--- two things you will want, neither of them a defect ---------------------

A. patch/engine.rs in the d325d2b bundle is a generation behind this tree.
   It still has `view.connect_word(...)` and the 320 px card; both were
   replaced in 057862a. Since the recipe's patch is a copy of Kalam's file,
   a re-copy from the current bundle would undo item 1 and item 2. Kalam's
   tree stays as it is (tree wins), and 36808ff's byte-for-byte mirror claim
   no longer holds.

B. Driving a tap from CI does not work here, and it is worth knowing before
   someone else spends three runs on it. With the reader open,
   `swaymsg seat seat0 cursor set/press/release` inside the app's own rect
   (fullscreen, 1600x1000 at 0,0, read from sway's tree) changes nothing at
   75, 25 or 50 per cent of the width, with a one-second hold and with an
   80 ms one. The centre reacts to neither, so the likeliest reading is that
   synthetic pointer events are not delivered under a headless sway with no
   libinput devices -- not that the page-turn zones ignore taps. The probe
   is in docs/ci/screenshot.sh, off by default, with that finding written
   above it so it is not re-learned.

tried: read view.rs's diff before trusting the behaviour change; resolved
Cargo.lock to confirm which widget rev the green run built; grepped main's
bridge message types and fireTapLookup's callers before deleting the word
path; checked `host.width()` exists in gtk4-rs 0.11 (WidgetExt) before using
it for the cap; three CI runs on the tap probe (1 s hold, 80 ms hold, three
positions) and then turned it off rather than leave a misleading line in
every reader report.
=== END ===
