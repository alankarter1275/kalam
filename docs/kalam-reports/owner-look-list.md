=== KALAM REPORT ===
engine: 59157cd (bundle on my branch as cac1134)
step: your action — rebuild against the cursor change, run CI, hand over the look list
status: DONE — built and green against 59157cd; no further engine item outstanding

Rebuilt and green. Cargo.lock's seven branch-dep entries move to 59157cd
(its parent in the chain being 90dd094, whose band and handles this branch
already carries); clippy clean, 343 passed / 0 failed / 2 ignored, the
reader opens the book in 3.9 ms, and app stderr is unchanged — the only
line is the GTK a11y-bus note that every run has. No cursor-theme noise
from the motion controller in the headless pass, which is the only thing
the change could have added there.

Checked your claim rather than taking it: `git diff 90dd094 59157cd` is
`crates/kalam-reader/src/view.rs` (43 lines: the `Inner.cursor` cell,
`set_cursor` on change, "grabbing" at the grab press, "grab" on release
when the handle came along, the motion controller's hit test, `None` on
leave and in `close()`) plus the two docs. No API surface moved, so the
only Kalam change is the lock.

That closes the engine's list. What is left on this branch is the owner's
own look, and item 4's merge, which waits for it. The look list follows.

================ THE OWNER'S LOOK LIST (branch state: 291f08c) ==========

1. Does a tap on a word turn the page?
   Open a book, tap a word in the middle of the text. It should turn to
   the next page. (Tap-to-look-up was removed; this is the check for it.)

2. Does the dictionary card match the mockup?
   Select a word, tap "Look up". Expected, from
   `docs/files/kalam_dictionary_popup_v3.html`: 380 px wide (narrower if
   the window is), a 26 px serif headword, an 11 px monospace
   pronunciation, a violet part-of-speech pill, 13 px definitions, 12 px
   chips, radius-10 idiom cards, three round buttons — save, find
   (disabled), copy.

3. Does the selection look like the old reader?
   Drag-select two lines. Expected: a band hugging the text (2 px of air
   above and below, soft corners) that blends into the page, a thin bar
   with a small teardrop grip at each end.
   Then drag one handle: only that end should move, the other stays, and
   the bar above the selection should follow when you let go.

4. Does the pointer turn into a small open hand over a handle, and a
   closed one while dragging it?

5. Does the action bar over the selection match the old one?
   A highlighter icon that opens the five colours when pressed, then a
   quote mark, an "Aa", and a copy mark, separated by thin lines. D opens
   the dictionary for the selection; Escape drops the selection first and
   only closes the book when nothing is selected.

Anything on this list that looks wrong and is *text on the page* — fonts,
spacing, colours of the book's text, page turns, scrolling, the band, the
handles, the cursor — goes back to you as a KALAM REPORT with the exact
symptom. Anything in the chrome (the card, the action bar, the D and
Escape keys) is mine to fix here.
=== END ===

=== LOOK LIST (the owner's plain-language copy) ===

Please check five things, in this order, and tell me the ones that are
wrong — a screenshot where it is something you can see:

A. Tap a word in the middle of a page. It should turn the page.
B. Select a word and press "Look up". The card should look like your
   mockup: wider (380 px), bigger serif word, small monospace
   pronunciation, violet part-of-speech tag, violet-tinted button row.
C. Drag over two lines of text. The highlight should hug the words with a
   little air above and below, and there should be a thin bar with a small
   teardrop grip at each end. Drag one grip: only that end moves, and the
   bar above the selection follows when you let go.
D. The pointer should become a small open hand over a grip, and a closed
   hand while you drag it.
E. The bar over the selection: a highlighter that opens the five colours,
   then a quote mark, an "Aa" and a copy mark, with thin separators. "D"
   opens the dictionary for the selection; Escape drops the selection
   first, and closes the book only when nothing is selected.
