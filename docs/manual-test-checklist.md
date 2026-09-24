# Phase 2 test plan

## Round 4 — settings reorganized and continuous arrow scroll speed

- **Arrow scroll speed (continuous mode):**
  - Added an **Arrow scroll speed** stepper in the left sidebar Settings panel
    under **Navigation & Scrolling** (persisted in `reader.arrow_step`).
  - Scales both single-tap arrow step and the 20 ms hold glide speed
    proportionally (default 45 = 15 px/tick, ~750 px/s).
  - Dynamically active in continuous scroll mode, and automatically greyed
    out in paged mode (where vertical arrows step chapters instead).
  **Re-test:** continuous mode — increase/decrease Arrow scroll speed in Settings;
  verify ↑/↓ single-tap and hold-glide speed changes to your preference. Switch
  to paged mode: verify the stepper greys out.
- **Left sidebar settings reorganized & properly sorted:**
  - Consolidated the previous 7 fragmented sections into 5 clean, coherent
    groups in the **Reading** pane:
    1. **Theme**: Sepia, Light, Dark, Ink palette dots.
    2. **Typography**: Typeface dropdown, Fonts folder + hint, Font size stepper,
       Line height stepper, Justify toggle, Publisher styles toggle.
    3. **Layout**: Column width stepper, Continuous scroll toggle, Two pages
       side by side toggle (greyed in continuous scroll).
    4. **Navigation & Scrolling**: Wheel scroll speed stepper, Arrow scroll speed
       stepper (greyed in paged mode), Hide pointer while reading toggle + hint.
    5. **Dictionary**: Sense hint toggle, Lookup history toggle.
  **Re-test:** open Settings (left sidebar), check all 5 sections and the UI
  flow.

---

## Round 3 — typeface list, fourth attempt (the correct one), and arrow keys

- **T1 Typeface picker (again):** the real root cause, finally. The panel
  holding the Typeface dropdown lives in the **left** sidebar — all three
  earlier fixes guarded the **right** sidebar's close timer, which was
  never the one that fired. The left sidebar now holds exactly like the
  right one while a dropdown list is open. **Re-test:** open Typeface,
  move onto the list, pick Default or any face. If it still closes
  mid-pick, say so immediately.
- **Arrow keys — new behaviour, your spec:**
  - **Scrolled mode:** Up/Down scroll — tap for a step, *hold* to glide
    smoothly until you let go. Left/Right step chapters.
  - **Paged mode:** Left/Right turn pages as before; Up/Down now step
    chapters.
  - **Chapter stepping:** forward always lands at the next chapter's
    start. Backward from the middle of a chapter first rewinds to *that*
    chapter's start; only the next press crosses into the previous one.
  - **Opposite arrow = undo:** right after a chapter step, pressing the
    opposite arrow takes you back to where you were (and vice versa).
    Scrolling, Home/End or any other key in between drops that notion, so
    later opposite presses just step chapters again.
  **Re-test:** scrolled mode — hold Down a few seconds (smooth glide?),
  Left from mid-chapter (chapter start?), Right immediately after (back
  where you were?). Same pass in paged mode with Up/Down.

Both built and CI-green (all tests) before this reached you. Report as
usual: "T1" for the dropdown, "arrows" for the key cases.

---

## Round 2 — what changed from your first report

- **T1 Typeface picker:** fixed. Moving the pointer onto a dropdown list
  fired the settings sidebar's leave-timer and killed list and sidebar
  together. The sidebar now holds while a popup owns the session and
  re-judges when focus comes home. **Re-test:** open Typeface, pick Default
  or any face — the list should stay up until you choose.
- **T2 Hyphenation:** fixed for real this time. The toggle only ever
  *turned hyphenation off at the publisher's request*; it never asked for
  it, so ordinary books showed nothing in either position. "On" now claims
  hyphenation on paragraphs and the engine's dictionary hyphenator runs.
  **Re-test:** narrow window, narrow column — long words should break with
  a visible hyphen at line ends. Heads up: English words only (the bundled
  dictionary is en-US), and broken words appear where the line needs them,
  not at every opportunity.
- **T3 Two pages:** your reading was right and so was the build — with the
  setting **on** and a wide window, a turn *should* move two pages (1–2 →
  3–4). The mismatched case was the setting **off** moving two anyway. What
  changed: in **continuous scroll** the toggle is now greyed out, because
  a spread can never apply there. You asked for exactly that — done.
- **T6 Jump back:** the on-screen button is gone for good — keyboard only.
  Backspace is now wired the same way Ctrl+F always was (window-global,
  with a typing guard), which is the route your run proved missing.
  **Re-test:** TOC jump → Backspace, with and without clicking into the
  page first.
- **T7 Pointer:** now also hides while scrolling; only mouse *movement*
  brings it back. You asked for exactly that — done.
- **T8 Scroll speed:** root cause found — a touchpad reports smooth pixel
  deltas and took them 1:1, ignoring the slider entirely. Both wheel and
  pad now honour it, with the default pad feel unchanged. **Re-test:**
  slider low vs high, two-finger scroll.
- **T9 Shortcuts:** keyboard bindings moved to their own **Shortcuts** tab
  next to Reading and UI. You asked for exactly that — done (re-test the
  keys there; behaviour is otherwise identical).
- **T10 Fonts folder:** on your setup (no desktop file manager) the launch
  fails silently; now the folder's path is copied to your clipboard with a
  toast saying so, and you can open it in yazi. **Re-test.**
- **T11/T12/T13 PDF:** root cause found — the importer never accepted
  `.pdf` at all, even though the reader it feeds has existed for months.
  Import now works; the file name is the title and page one becomes the
  cover. **Re-test:** import your text PDF and a scanned one, then the
  timing lines.

Everything built and CI-verified since the dictionary work, in one session.
Each test has an ID — report by ID ("T7 failed: …") and I'll know exactly
where to look.

---

## Before you start

**Build and run**

    cargo run --release

**Run it with logging and timings on** — worth doing for the whole session,
it costs nothing and it's what turns "felt slow" into a number:

    RUST_LOG=info KALAM_TIMING=1 cargo run --release

On startup it prints one line saying where the log went. The log file is
`~/.local/share/kalam/kalam.log`, and `[timing]` lines appear in the
terminal you launched from.

**What you'll need to hand**

- An EPUB **with footnotes** (a non-fiction or classics edition usually has
  them). If you don't have one, skip T6 and T7 and tell me.
- An EPUB with a table of contents.
- One **text** PDF (a novel or an academic paper — text you can select).
- One **scanned** PDF (pictures of pages).
- Any `.ttf` or `.otf` font file you don't mind copying.
- A word worth looking up in the dictionary.

**How to report**

For each test: the ID, then one of —

- **ok** — it did what's written
- **wrong** — what you saw instead, in your words
- **crash / froze** — please paste the last ~20 lines of `kalam.log`

A screenshot beats a description wherever something *looks* off. And if a
test can't be run at all (no footnote book, no scanned PDF), say "skipped,
no file" rather than leaving it blank.

---

## Part A — the page itself

### T1 · Typeface picker (2.3)

**Do:** open a book → Settings → Reading → **Typeface**. Pick a few faces.
Then pick **Default**. Close the app, reopen the book.

**Look for:** the body text changes as you pick; "Default" brings back the
bundled face; your last choice is still there after restart.

**Report if:** the list is empty, a face is picked but nothing changes, or
the choice is forgotten after restart.

### T2 · Text layout switches (2.4)

**Do:** Settings → Reading → **Text**. Try all three: **Justify**,
**Hyphenation**, **Publisher styles**. For hyphenation, make the window
narrow (or drop Column width) so lines break often.

**Look for:**
- Justify on → both edges of the text are flush.
- Hyphenation off → the little dashes at line ends disappear.
- Publisher styles off → the page loses the book's own fonts and spacing and
  looks like plain Kalam text.
- All three apply instantly, and survive restart.

**Report if:** nothing changes, the text becomes unreadable, or a switch
resets itself.

### T3 · Two pages side by side (2.7)

**Do:** make the window wide — you should get facing pages, as always. Now
Settings → Reading → Layout → turn **"Two pages side by side"** off. Turn a
page. Restart. Then turn the setting back on and narrow the window.

**Look for:** with it off, one page at full width at any window size, and a
page turn moves **one** page. With it on and a narrow window, the spread
gives way to a single page.

**Report if:** with it off you still see two pages, or a page turn jumps two.

---

## Part B — notes and getting back

### T4 · Footnotes in place (2.5)

*What's a footnote?* A tiny aside at the bottom of a page or the end of a
chapter that the main text points at with a small raised number or `[1]`,
so the author can comment or cite without interrupting the sentence.
Non-fiction and classic novels digested by Standard Ebooks are full of
them; most modern fiction has none. If your book has none, skip this test
and say so.

**Do:** find a footnote marker in the text (a `[1]` or similar) and tap it.
Then tap **Go to the note**. Then tap outside the card.

**Look for:** the note appears **in a card, right where you are** — your page
does not move. "Go to the note" jumps to it properly. Tapping outside closes
the card.

**Report if:** the card is empty or shows the wrong note; the page jumps
anyway; the card won't close.

### T5 · Chapter links still navigate (2.5, the regression check)

**Do:** tap a link that goes to **another chapter**, or a table-of-contents
entry inside the page text.

**Look for:** it just navigates. **No card.**

**Report if:** a card pops up instead of navigating — this is the one I most
want checked, because it's the behaviour the footnote work could have
broken.

### T6 · Jump back (2.6)

**Do:** jump somewhere (a TOC entry, a search result, a link in the page).
Then press **Backspace**. Try it again after a second jump.

**Look for:** you land back exactly where you were. Jump twice and back
twice should retrace both.

**Note:** there is no on-screen button for this — Backspace only, and it
should work wherever the pointer and focus happen to be. Turning a page
normally should quietly expire the jump (Backspace then does nothing).

**Report if:** Backspace does nothing, or takes you somewhere other than
where you were.

---

## Part C — pointer, wheel, keys

### T7 · The pointer hides (2.9)

**Do:** leave the mouse still over the page for about two seconds. Then move
it. Then Settings → Reading → Pointer → turn **Hide the mouse pointer** off
and try again.

**Look for:** it vanishes after ~2s and comes back the instant you move.
With the switch off, it never vanishes. Survives restart.

**Report if:** it flickers, never comes back, or hides while you're clicking.

### T8 · Scroll speed (2.9)

**Do:** turn on **continuous scroll** (Settings → Reading → Layout). Change
**Scroll speed** a few notches each way and scroll with the wheel.

**Look for:** noticeably more or less travel per notch. Survives restart.

**Note:** in ordinary paged mode the wheel does nothing — that's how it has
always been, not a bug.

**Report if:** the setting changes nothing, or scrolling becomes jumpy.

### T9 · Editable keys (2.9)

**Do:** Settings → Reading → **Keyboard**. Click a key button, then press
the key you want. Try Escape mid-capture. Then give one action a key another
action already has. Finally hit **Reset to defaults**.

**Look for:**
- The button shows the new key and it works immediately.
- Escape cancels without changing anything.
- The action that lost its key shows **"None"** — it stops working, it does
  not keep the key.
- Reset puts everything back.
- Survives restart.

**Note:** `m` no longer adds a bookmark; `b` does. That's deliberate.

**Report if:** a capture never finishes, two actions fire on one key, or
Reset doesn't restore.

---

## Part D — your own fonts

### T10 · Fonts folder (2.8)

**Do:** Settings → Reading → Type → **Fonts folder → Open**. Copy a `.ttf`
or `.otf` into the folder that opens. **Close the book and reopen it.** Then
look in the Typeface picker.

**Look for:** the folder opens in your file manager; after reopening the
book, the new face is in the picker and renders correctly.

**Note:** fonts are read when a book opens, not per page turn — reopening is
required, and the settings panel says so.

**Report if:** the folder doesn't open, the face never appears, or it appears
but renders as boxes.

---

## Part E — PDFs (Rebuilt with MuPDF & Unified Chrome)

### T11 · True MuPDF PDF Rasterization & Visual Fidelity
**Do:** open a text PDF (e.g. *Tell Me Why #66*, an academic paper, or book) and a scanned PDF.
**Look for:** crisp, high-fidelity native page rendering (sharp vector fonts, diagrams, tables, figures, artwork) rendered via MuPDF. No synthetic ink lines or grey bars.
**Report if:** page is blank, distorted, text characters are missing or overlapping, or rasterization fails.

### T12 · Unified Reader Chrome & Edge Hover Autohiding
**Do:** 
- Hover near the top edge (< 50px from top) or the back button.
- Hover near the bottom edge (< 60px from bottom) or the bottom dock.
- Hover within 20px of the left edge of the screen (Zen Browser style).
- Click the middle of the reading viewport.
- Move mouse back into the reading area or scroll.
- Test in both PDF and EPUB readers.
**Look for:**
- Top edge hover reveals floating back button pill ("Library") without title text (prevents overlaps). Autohides smoothly after 2.5s when leaving with a clean SlideDown / SlideUp transition.
- Bottom edge hover reveals floating bottom pill with navigation, zoom controls, mode toggle, and smart crop toggle. Autohides consistently after 2.5s when leaving with a clean SlideUp / SlideDown transition.
- Clicking the middle of the reading viewport toggles controls on/off cleanly without getting stuck.
- Left edge hover smoothly slides open the left TOC outlines sidebar (`SlideRight`). Moving cursor away from sidebar closes it with 350ms debounce.
- Scrolling the document immediately hides unhovered controls.
- EPUB reader also uses smooth SlideDown and SlideUp transitions for its top back chip and bottom pill.
**Report if:** controls fail to reveal on edge hover, fail to autohide, bottom bar gets stuck, or transitions are missing.

### T13 · Table of Contents & Settings Left Sidebar (PDF & EPUB)
**Do:** hover the left edge or press `T`.
**Look for:**
- Fixed 340px width sidebar (`width: 340px`, `size_request: (340, -1)`) that never blows out to fill the screen even on books with long titles or chapters.
- Book title wraps at 24 chars with ellipsis (`End`).
- 2-tab switcher at the bottom of the left sidebar: **TOC** (`view-list-bullet-symbolic`) and **Settings** (`preferences-system-symbolic`).
- In **TOC** tab: document outlines listed cleanly; clicking any entry jumps to that page.
- In **Settings** tab:
  - View Mode switcher: **Continuous**, **Single**, and **Two-Page**.
  - Dual Page Spread option: "First page as single cover" switch (active in Two-Page mode; greys out in Continuous/Single modes).
  - Display option: "Smart Crop (Zathura style)" switch with explanatory hint.
  - Magnification / Zoom controls: `-`, percentage display, `+`, and `100%` reset.
- Pressing `Esc` or clicking the dim backdrop immediately closes the sidebar.
**Report if:** sidebar expands wider than 340px, tabs do not switch content, or settings do not update the document.

### T14 · Paged Mode, Continuous Vertical Scroll, & Two-Page Spread Mode (PDF)
**Do:**
- Open any PDF document in Kalam.
- Verify that the first page displays immediately upon open with NO blank dashed placeholder.
- In Settings tab (or press `M` to cycle modes), switch between **Continuous**, **Single (Paged)**, and **Two-Page**.
- In **Two-Page** mode:
  - Verify facing pages are displayed side-by-side with centered alignment.
  - With "First page as single cover" ON: page 1 shows centered alone; navigating Next shows pages 2-3 together, 4-5 together, etc.
  - With "First page as single cover" OFF: pages 1-2 show together, 3-4 together, etc.
  - Verify bottom pill displays spread notation (e.g., `2-3/120 (3%)`).
- Verify top font ascenders are NOT clipped in either PDF or EPUB readers (EPUB band blit bleed allowance + PDF top margins).
- In Continuous mode, use `Up` / `Down` arrows or mouse wheel to scroll vertically.
**Look for:**
- Page appears immediately upon opening without needing to scroll first.
- In Two-Page mode: two pages side-by-side with clear spine separation, or single centered cover when cover-alone is enabled.
- No top line ascender clipping (d, h, k, l, t, accents, capital letters have full breathing room).
- Bottom pill is streamlined: contains Prev/Next page navigation, page info indicator, and zoom controls only.
**Report if:** continuous mode is not default, two-page mode shows misalignment, top font is cut off, or spread navigation skips pages.

### T15 · Touchpad Pinch-to-Zoom & Smart Crop Persistence
**Do:**
- On a trackpad, use two fingers to pinch in/out. Alternatively, hold `Ctrl` and scroll.
- Toggle Smart Crop (`C` or via Settings tab). Verify that fonts and page numbers are never clipped (background luminance detection + 5.5% / 44px breathing margin).
- Verify all fonts across diverse PDFs render cleanly with full glyph coverage (MuPDF `system-fonts` enabled).
- Close and reopen that document: verify Smart Crop state was saved for that specific book.
- Open a different PDF: verify Smart Crop is OFF by default.
**Look for:**
- Fluid touchpad pinch-to-zoom and Ctrl+Scroll zoom.
- Smart Crop removes margins without cutting off any text, accents, headers, or numbers.
- Smart Crop state is document-specific and survives app restart.
**Report if:** pinch-to-zoom doesn't react, fonts are cut off, or persistence fails.

---

## Part F — dictionary and annotations (earlier work, unverified on device)

### T14 · The dictionary card

**Do:** select a word and look it up.

**Look for:** dark rounded card; the word in serif; pronunciation in mono;
a purple part-of-speech pill; numbered definitions with italic examples;
blue synonym chips and red antonym chips; idiom cards; a fade at the bottom.

**Report if:** it looks nothing like that, or sections are missing.

### T15 · Chips, more senses, saving

**Do:** tap a synonym chip. Tap **"Show N more"**. Tap the bookmark/save
button and look at it again.

**Look for:** the chip re-looks-up that word; more definitions expand; the
save button shows its saved state.

### T16 · Pronunciation audio

**Do:** tap the 🔊 button.

**Look for:** the word is spoken. Needs `espeak-ng` or `spd-say` installed —
if neither is, it should tell you which to install rather than doing nothing.

### T17 · The bottom pill floats

**Do:** look at the reader's bottom navigation pill (← 10 / 140 title →).

**Look for:** a visible gap between it and the bottom edge — it floats, it
doesn't hug the edge.

### T18 · Export highlights

**Do:** make a few highlights and notes → Highlights panel → **Export**.

**Look for:** a success toast with a count and a path; `~/Highlights.md`
exists and lists highlights, quotes and notes with their chapter and your
note text.

### T19 · Review page

**Do:** Library → **Review**. Save a few words first if you haven't.

**Look for:** a word shows; *Show definition* reveals it; **Again / Good /
Easy** move to the next card; "Again" brings a word back in ~10 minutes while
Good and Easy push it out by days. With nothing due: *"All caught up —
nothing due right now."* With no saved words at all: a hint to save some.

---

## Part G — carried over (re-check only if you haven't)

### T20 · Cold start

The splash shows the brand — no blank window. Home appears first; the heavy
chores happen after the first paint.

### T21 · Bulk import and delete

A bulk import expands to show **every** imported book name. Each delete
shows up as its own task in the task manager.

---

## At the end

Send me whatever you have, in this shape:

    T1 ok
    T2 ok, but justify looks odd in the indented quotes
    T5 wrong — tapping a chapter link opened a card
    T13 [timing] pdf_open  412.7 ms
        [timing] pdf_page_reflow  3.1 ms
        [timing] pdf_page_image  188.4 ms

Plus `~/.local/share/kalam/kalam.log` if anything crashed or froze.
