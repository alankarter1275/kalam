# Phase 2 test plan

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

**Note:** the undo *button* lives in the top-left dock, and that dock hides
itself when you're not near it — so the button may not be visible. Backspace
always works. Turning a page normally should make the offer go away.

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

## Part E — PDFs

### T11 · A text PDF is readable (2.11)

**Do:** open a **text** PDF (one you can select text in).

**Look for:** readable reflowed text — not grey stripes — and a note in the
toolbar saying *"Reflowed text — this page has no image to show."* Try the
font size and theme controls, and the **Text Reflow** button.

**Report if:** you still see grey bars; the text is garbled, out of order, or
missing chunks (tell me which — two-column layouts and tables are the known
weak spots); or the note is missing.

### T12 · A scanned PDF still works (2.11)

**Do:** open a **scanned** PDF. Turn smart crop on and off.

**Look for:** exactly what you had before — the page image, correctly
cropped. Scans should be untouched by all of this.

**Report if:** a scan now shows text instead of the image, or the crop broke.

### T13 · The numbers

With `KALAM_TIMING=1` on, send me the `[timing]` lines from:

- opening one **text** PDF → `pdf_open`, then `pdf_page_reflow` per page turn
- opening one **scanned** PDF → `pdf_open`, then `pdf_page_image`
- opening an EPUB → `book_open`

Even three or four lines is enough. This is the measurement behind the
"reflow is the cheap path" claim — I reasoned it out from the code, I have
not measured it, and there are no PDFs in my sandbox.

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
