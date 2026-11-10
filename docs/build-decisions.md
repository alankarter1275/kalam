# Build-side decisions

**This file exists because `docs/design/` is read-only here.**

The design set — `docs/design/decisions.md`, `design-system.md`,
`kalam-screens.md` and the screen specs — belongs to the design chats and is
maintained outside this repository's build work. New and corrected versions
arrive as uploads from the design side. Nothing in the build ever writes to
those files, and no decision is ever appended to `docs/design/decisions.md`.

Rulings given during build work are recorded here instead, numbered **B1**,
**B2** and so on, so they can never collide with the design set's own
numbering, which runs 1 to 207 and continues. Each entry names the design
decisions it supersedes or extends, so both sides can be read together.

**Which file wins.** For appearance — colour, type, shape, spacing, sizes —
the design set wins; that is owner ruling 1 in `docs/redesign-audit.md`. For
what the navigation rail holds, what opens what, and anything the owner ruled
on directly after that audit was written, this file is the newer word.

Entries are dated. A later entry supersedes an earlier one by saying so.

---

## B1 — the rail holds seven entries (2026-11-08)

Top group, top to bottom:

1. Home
2. My Library
3. All books
4. Authors
5. Comics
6. Online sources
7. Editor

Bottom group: the bell, then Settings.

*Supersedes* the five-entry group of design decision 179 (Home, My Library,
Shelves, Comics, Editor) and its ruling that "there is no Authors entry".
*Restores* the Authors entry of decision 117. *Adds* two entries the design
set does not have: All books and Online sources. Decision 179 anticipated
this — it records "more entries will be added later" as an owner ruling.

Nine discs in total. The rail's vertical centring has to hold with seven
entries in the top group rather than five.

**Confirmed by the owner (2026-11-08):** All books is third, between My
Library and Authors — it is the grid behind My Library, and shelves hang off
it. The owner added the entry without naming its position, then named it. The
label reads "My Library", the owner's own wording and decision 173's.

## B2 — Shelves has no rail entry (2026-11-08)

Shelves leaves the rail. It is reached two ways, both already in the design:

- from **All books**, whose title is a dropdown listing every shelf with its
  book count, plus "Manage shelves…" and "New shelf…" (design system §47,
  decisions 159 to 172);
- from **My Library**'s Shelves row (decision 173, design system §53).

The shelves pages themselves — the grid, the detail page and the two editors —
are unchanged by this. They lose a door and keep two.

## B3 — the Authors index page is built but was never designed (2026-11-08)

The owner remembered the page existing, and it does. The facts, checked
against the code:

- `src/pages/authors.rs` (617 lines) and `Route::AuthorsIndex` are real and
  reachable, fed by `LibraryService::authors`, with search and Name / Most
  books / Most read sorting;
- CI photographs it in Terracotta light and dark on every push;
- its own header comment says the page was **left undesigned** and follows the
  author page's language rather than a board of its own;
- the design set calls it not designed in four places: decision 117, decision
  179's supersession of it, the checklist line in `kalam-screens.md` §4, and
  open question 6.

**Ruling:** the rail entry stays (B1). The page as built is a stand-in. If a
design chat draws the Authors index later, the design wins and the page is
rebuilt to it — the same rule the audit already applies everywhere.

## B4 — Online sources is a placeholder entry for now (2026-11-08)

The entry appears in the rail and opens a placeholder page. It promises
nothing and fetches nothing.

The design's Part 2 boundary is untouched: online search stays disabled until
Part 2 (decision 8), and design chat 18 is what designs the real work — the
Downloads hub, the Browse client for AO3 and Royal Road, MangaDex browsing and
plugin management.

Today the app carries three separate hidden items — Downloads, Browse and
Fanfiction — each with a placeholder page and a working route. They collapse
into this one entry; the rail shows one door, and what is behind it is chat
18's to design.

## B5 — background tasks fold into the bell (2026-11-08)

The Tasks disc leaves the rail. It sits in the bottom group today, above the
bell, and shows what is running in the background.

Instead, the bell's notification panel gains a **nested box at the bottom**
holding running and recently finished tasks. One entry, two kinds of content:
what happened, above; what is happening, below.

Consequences to carry into the work:

- the task badge — a running count, or a failure — has to move onto the bell
  or into the panel, since it no longer has a disc of its own;
- design system §45 and decision 148 describe the bell and its panel and do
  not contain this box, so it needs drawing on the design side;
- design chat 17 owns progress and cancellable background work, and should be
  told the tasks list now lives inside the bell panel.

## B6 — the icon set ships with the app (2026-11-08)

Lucide, as design system §56 proposed. Checked before agreeing to ship it:

- **Licence:** ISC. Permissive; the copyright and permission notice must
  travel with the copies, so the vendored icons keep a licence file beside
  them. Some Lucide icons are derived from Feather, which the same file names.
- **Geometry:** Lucide draws on a 24 × 24 grid at stroke 2. Drawn at 22px, the
  stroke becomes 1.83 — which is the design's "icons 22 with stroke 1.8". The
  design's numbers already assume this set, so no icon has to be redrawn to
  match the board.

Nine glyphs are needed for the rail: Home, My Library, All books, Authors,
Comics, Online sources, Editor, bell and Settings. Only what the app uses is
vendored, not the whole set.

This closes open question 25 for the rail. Other screens inherit the same set.

## B7 — the rail's keys follow the rail's order (2026-11-08)

Alt plus the entry's position, top to bottom:

| Key | Entry |
|---|---|
| Alt+1 | Home |
| Alt+2 | My Library |
| Alt+3 | All books |
| Alt+4 | Authors |
| Alt+5 | Comics |
| Alt+6 | Online sources |
| Alt+7 | Editor |

The bottom group keeps the keys decision 155 gave it: Settings Ctrl+comma,
notification history Ctrl+Shift+N. Back stays Alt+Left.

This settles open question 13 — the Editor's Alt+3, which the drawn Shortcuts
list had lost. The Editor is Alt+7 now, because the order decides.

Consequences to carry into the work:

- the code has three navigation actions today (Go home, Go library, Go
  editor). Four more are needed: All books, Authors, Comics, Online sources.
- The Shortcuts pane's Navigation group (decision 155) lists five rows and
  must grow to nine.
- Saved bindings are stored per action, so a reader who rebound a key keeps
  their binding. Only the **defaults** move: "Go to Editor" changes from Alt+3
  to Alt+7, and Alt+3 becomes All books.

---

## B8 — the active rail glyph is the page colour (2026-11-08)

Design system §56 says the icon on the filled disc is the page colour, "so it
reads as cut out". §11 says anything on an `accent-fill` surface is white and
must reach 4.5 : 1. Both were measured on Terracotta and put to the owner:

| Glyph colour | Dark | Light |
|---|---|---|
| white | 4.64 : 1 | 5.12 : 1 |
| page colour (cut out) | 3.90 : 1 | 4.63 : 1 |

**Ruling: the cut-out.** The page colour, as §56 drew it. It clears the 3 : 1
an icon needs and misses the 4.5 : 1 text needs; the owner chose the board's
look over the stricter number.

This settles §56 against §11 **for the rail's active entry only**. §11 still
rules everywhere else, and §45 says explicitly that the bell while its panel is
open is `accent-fill` with a white icon — no ruling supersedes that. Two states
on the rail therefore look alike and are not the same: the active entry is cut
out, the open bell is white.

## B9 — the shelves door is a quick link on My Library, for now (2026-11-08)

B2 takes Shelves out of the rail and leaves it two doors, and when the work
started neither existed: the All books title dropdown is designed (§47,
decisions 159 to 172) but not built, and My Library's Shelves row is designed
(§53, decision 173) but not built either, because My Library is still the old
dashboard. Taking the disc out would have left shelves with no way in at all.

**Ruling:** My Library's quick-link row gains a Shelves link as a stopgap. It
is a third door, not a replacement for the other two, and it goes when the All
books page is built to its board.

## B10 — the bundled glyphs, and why they are textures (2026-11-08)

B6 agreed to ship Lucide. The thirteen glyphs bundled under `assets/icons/`,
with Lucide's ISC notice beside them:

| Entry | Glyph |
|---|---|
| Home | `house` |
| My Library | `library` |
| All books | `layout-grid` |
| Authors | `user-round` |
| Comics | `images` |
| Online sources | `globe` |
| Editor | `square-pen` |
| the bell | `bell` |
| Settings | `settings` |
| Shelves — no rail entry (B2) | `archive` |
| Downloads — hidden | `download` |
| Browse — hidden | `compass` |
| Fanfiction — hidden | `feather` |

The four without a rail entry are bundled anyway, so restoring one stays the
one-line change `NavItem::ALL`'s doc promises.

They are rasterized to textures with the colour substituted into the SVG, not
installed as symbolic theme icons. Two reasons, both measured here: the theme
rescan `icons::init` needs costs 493–505 ms and is deferred to idle while the
rail paints first, and GTK recolours a symbolic icon by its *fill*, which
erases a stroke-only drawing. The price is repainting by hand when the theme
changes, which the rail now does.

## B11 — the rail's tooltip is drawn by the shell (2026-11-08)

Decision 180 and §56 want a card, not a system tooltip: 32 high, `card` fill,
1px `border`, radius 10, the name at 13 and its shortcut at 11 in the Meta
style, to the right of the item after a short delay. GTK's tooltip is text
only, so the shell draws the card in its own root overlay.

It is an overlay card rather than a popover on purpose. A popover is its own
surface and takes a grab, which would put Alt+1 to Alt+7 (B7) behind that grab
for as long as a tip was showing.

Two consequences worth recording:

- the rail's native tooltips are gone, so anything a tooltip carried has to be
  on the card. The bell's background-work count — the badge B5 took off the
  tasks disc — is the card's third line, read when the card shows. That is the
  one place a rail card grows past 32;
- the shortcut on the card comes from the shared bindings table at show time,
  so a rebind in the Shortcuts pane is on the card the next time the pointer
  arrives.

---

## B12 — Home is rebuilt in four stages, and the old dashboard goes at the first (2026-11-08)

The Home board (`home-spec.md`, design system §60 to §66) is five pieces on one
page: the search bar and its box, the greeting, the quote, the Your-shelf
ruler, the open-book card with its pager, and the Updates timeline. A route
draws one page, so the old dashboard and the new Home cannot share the
address. **Owner's ruling:** the old dashboard goes the day stage 1 lands; the
board's own empty and loading states cover what stage 1 does not have yet.

The stages, in order:

1. the page's skeleton — two columns, greeting, quote, the open-book card and
   its pager, the state cards, and the window controls (§65), which the Home
   board draws and which this app still lacks entirely (see B16);
2. Your shelf — the ruler, one custom-drawn widget with its date maths, hover
   card and zoom (§61);
3. the search bar and its box (§57, decisions 196 to 201), including the
   inside-books switch, which the full-text index already makes real;
4. polish — motion, focus rings, the reduce-motion rules.

**Owner's ruling:** search is stage 3 of this unit, not a unit of its own. The
28px the bar occupies at the top is reserved from stage 1, so it drops in
later without moving the columns.

## B13 — the Home quote is a placeholder; its source is still to be settled (2026-11-08)

The spec's note says the quote is "a separate offline collection, not the
reader's highlights", and no such collection exists anywhere in the app.
**Owner's ruling:** keep a placeholder on the line for now, and settle the real
source in this session, later. The placeholder is drawn as the board draws the
element — Quote large with its accent rule and an attribution — so swapping the
source changes one function.

## B14 — the greeting's name waits on a Settings customization area (2026-11-08)

The greeting is the user's own name and no name is stored anywhere; first-run,
which would ask, is undesigned (decision 207). **Owner's ruling:** the name
becomes a setting in a customization area of Settings, to be discussed and
added later — and that area is meant to carry more than the name, so it is
worth designing as a small general home for dials rather than a one-row pane.
Until it exists the greeting has no name: the time of day, then the date.

## B15 — Updates is not drawn until something feeds it (2026-11-08)

§62's timeline "comes from background checks", and the spec's own rule is that
with no saved source Updates is not drawn. Online work stays off until Part 2
(B4), so nothing feeds it today and Home ships without it. Feeding it from
local events — imports, finishes, edits — is an open option the owner may pick
later; that would be a new entry here, not a quiet change.

## B16 — the window controls are built with Home (2026-11-08)

Decision 181 wants three 32 round buttons at the top right over the page, and
the Home board draws them. They were left open at the end of the rail work
(the window has no title bar, which the decision also wants, and no buttons, so
Alt+F4 is the only way out). They are shell-wide rather than Home's, but the
board that shows them is this one, so they are built in Home's stage 1 and sit
over every page.

**Superseded by B18** the day the owner saw them on his own machine: all three
are removed, and the compositor owns them.

## B17 — the card's chapter text waits for a store of its own (2026-11-09)

Stage 1 read the Home card's chapter name and first lines straight out of the
full-text index. That was wrong twice over. The index stores book and chapter
as *unindexed* columns — only words are indexed — so "one chapter of one
book" is not a lookup but a scan of every word of every indexed book, up to
five scans per Home visit. And the catalog is one connection behind one lock,
so a scan holds the whole app's database access: on the owner's real library
it froze startup at the splash screen. CI's seeded library is a few chapters
wide and hid it completely; every smoke test passed over the fault.

**Ruling:** Home never queries the FTS index by book or chapter again — the
scan is banned on principle, not on size. The chapter heads get their own
small table keyed (book, chapter), filled at index time and backfilled once
in the background, the shape the thumbnail backfill already set. Until that
lands the card shows "Where you left off" and a quiet line naming what will
stand there. The trap itself is recorded as pitfalls §105, because "just
read it from the FTS table" will sound reasonable again someday.

---

## B18 — the window's three buttons are gone; the compositor owns them (2026-11-09)

Owner ruling, on seeing §65's buttons for the first time on his own machine:
*"listen, I am using a tiling manager. sway. so two of those three buttons were
redundant anyway! remove those three buttons. they will not be missed! ALL
three."*

So they are removed rather than redrawn. On a tiling compositor minimize and
maximize have no meaning, and close is a keybinding; drawing three discs over
every page bought nothing and cost a strip of the design's edge-to-edge surface.
The window stays undecorated (`set_decorated: false`) and Kalam's own way out
stays Ctrl+Q, which the shortcuts pane lists and the close request already
honours by cancelling running jobs.

Removed with them: the overlay slot in the shell's view, the two button builders
in `app.rs`, the `.kalam-window-btn` rules, and the four Lucide glyphs bundled
for them (`minus`, `x`, `maximize-2`, `minimize-2`) along with their SVGs — all
recoverable from history if a decorated window ever comes back. The Home card's
`play` glyph stays: it is §60's, not §65's, and the glyph test that covered both
now covers it alone.

Superseded: B16. The design files are not touched (§65 still draws the buttons);
this entry is the record of why the build does not.

---

## B19 — catch the freeze before drawing anything else (2026-11-09)

Asked what to do next, with Home's stage 1 built and the two startup freezes
fixed, the owner chose *"Catch the freeze first"*: add the instrument that
names the job holding the database, before the chapter-head store (Step B) and
before stage 2's shelf ruler.

The reason is the log he sent. One block of 4657 ms, blamed on a task that the
same line said had ended 27 seconds earlier, and a final "still blocked" line
that never resolved. Three freezes had already been found and fixed by reading
his terminal (§105, §106, §107); a fourth was sitting in the same output and
could not be named, because the watchdog's activity labels describe what the
screen is *doing*, and a screen waiting for the database is doing nothing.

So `src/lockwatch.rs` records who holds the catalogue's one connection lock and
since when. A stall line printed during a block now ends with that lock's
state — a name and a duration, or `free`; a waiter made to wait over 100 ms
prints its own `[lock]` line naming both sides; a hold that runs past two
seconds is named once whether or not anything waited for it; and a block longer
than two seconds gets a fresh line every two seconds rather than one snapshot
at the start. The
CI smoke report prints `[lock]` beside `[stall]`, so contention on a seeded
library is visible without anybody's terminal. Pitfalls §109 carries the
reasoning and the costs.

The instrument was an instrument first, and it paid for itself in three runs.
The first named a holder at all. The second proved the screen's blocks really
were waits for the database rather than its own work — 427 to 1110 ms of a
block spent waiting, in one wait each. The third named the shape: half a second
of waiting behind holds of 0 to 4 ms, which is a queue rather than a slow job.
Linux hands an unfair mutex to whoever asks next, and the two housekeeping jobs
that start when the window is realized — the bundled-dictionary install and the
thumbnail backfill — asked a hundred times each while the screen asked once.

So the fix is that the screen asks to go first: `Catalog::conn` raises a flag
before it blocks, a worker about to take the lock sleeps two milliseconds at a
time while that flag is set, and the screen lowers it the instant the lock
lands. The sleep is capped at 50 ms per acquisition and the cap is pinned by a
test, because an unbounded one would turn a single stuck flag into a library
that never loads. This is the owner's principle from roadmap 7.1 applied to the
lock rather than to the work: the screen does not queue behind housekeeping.

The work B19 interrupted is queued and unchanged — Step B's chapter heads, then
stage 2.

## B20 — the screen goes first at the catalogue lock (2026-11-09)

The ruling: at the catalogue's one connection lock, the screen is served
first. A worker about to take the lock gives way while the screen is waiting —
two milliseconds of sleep per look at the flag — and stops giving way after
50 ms, cap pinned by a test, so that a flag left set by a bug costs the workers
a fiftieth of a second and not the session.

This is recorded as a decision rather than as a fix because the opposite was
never decided either. Nobody had written down who wins that lock, in any
document, at any time:

- The comment above the lock itself said the app is *single-threaded for the
  UI, and imports run sync on the UI* — a P1-era truth, and the reason an
  ordering rule looked like overhead. Under it there is nobody to queue behind.
  That comment is corrected now; it was still there this morning.
- Roadmap 1.2, *"move blocking SQLite off the UI thread"*, is marked done
  2026-09-19, and `docs/WORKING.md` carries it as a standing rule: catalog
  queries go to `tasks.rs` workers. From that day the screen and its workers
  have shared one lock.
- The same roadmap decided, deliberately and with reasons, which reads *stay*
  on the screen: *"a page asks and blocks for ~1 ms, which is invisible"*, so
  five bounded queries stay synchronous, and *"every page's first read in
  `init`"* stays synchronous because the page is not on screen yet. That entry
  ends with a standing invitation — *"revisit when a large library makes those
  reads measurable"*.
- What no document said is what happens when a read that stayed on the screen
  meets a worker holding the lock. Linux answered it by default: an unfair
  mutex goes to whoever asks next, not to whoever has waited longest.

So the queue was inherited, not designed. It was sighted once, in §105 — *"a
scan on a worker thread queues every UI-thread read behind it; at startup that
presents as 'the splash screen never goes away'"* — and the fix then was to
kill the slow query in front of it, which removed the evidence and left the
ordering untouched. §106 pinned a different shape entirely (a guard held across
a call that locks again). The queue had no rule and no instrument until §109
built one.

The numbers that settled it, from three CI runs and then the owner's machine.
Before: waits of 370 to 1110 ms per block, measured two ways — the accumulated
counter and then the waiter's own line — against holds of 0 to 4 ms, and 116
`[lock]` lines in a single run. Every hold polite; the total a freeze. After: a
10 ms worst case in CI and no `[lock]` lines at all; and in the owner's own log
not one line of reported waiting, with the eleven samples that caught the lock
occupied finding it empty eight times, held by the screen itself twice, and
held once by a worker for 2 ms.

**What this ruling does not say.** It makes the ordering fair; it does not make
holding fast. A long hold on the screen thread still freezes the screen — the
worker cap exists precisely so that it does not also starve the workers behind
it. The owner's log still carries one such hold, the startup schema pass in
`Catalog::open`, and it is Still open item 4, not something this settles.

Deliberately not adopted, with the reason, so the question is not re-litigated:

- **A connection pool, or a second read-only connection.** WAL would allow
  readers alongside a writer and `PRAGMA busy_timeout = 5000` would finally
  mean something — today it cannot help the screen at all, because SQLite's
  patience sits behind this mutex. Not adopted: the measured problem was
  ordering, not capacity. Holds were 0 to 4 ms and reads about 1 ms, so a pool
  would trade one named lock for SQLite's own, where nothing can say who is
  holding what; it would need its own PRAGMAs, statement cache and page cache
  per connection; and it would put the memoised `library_stats` cache and its
  `total_changes()` token behind a second view of the database. One connection
  is also what makes §106's nested-lock shape a single thing a scanner test can
  find. Revisit only on a measurement the flag does not explain.
- **A fair or ticketed mutex.** Not adopted: fairness is the wrong requirement.
  Two housekeeping jobs alternating politely in front of the screen would still
  keep it out for as long as they have work between them, which is exactly the
  half second that was measured.
- **Coarser yielding — workers pause between task items instead of before each
  acquisition.** Not adopted: an item can be a hundred covers, so the screen
  would wait for the whole of one. Yielding at the lock is the only place where
  the wait is bounded by somebody else's hold rather than by somebody else's
  batch.
- **Rewriting the two housekeeping jobs to take the lock less often.** Not
  adopted as the fix, though it remains good hygiene: it would shorten this
  queue and leave the next one, and the flag covers jobs that have not been
  written yet.

Cost, stated because it sits on the hot path: on the screen's path, a handful
of relaxed atomic loads and stores and two monotonic clock reads for the whole
acquisition, against a query that costs microseconds. A worker pays the same,
plus up to 50 ms of sleep when the screen happens to be waiting — the only part
of this that costs anything measurable, and the point of it. The cap and
the wait line's wording are both pinned by tests in `src/lockwatch.rs`, because
that line is the evidence and it has to stay readable.

## Still open

1. **The tasks box needs drawing.** B5 puts running and failed tasks in a
   nested box at the bottom of the bell's panel. §45 and decision 148 describe
   a panel with no such box. It is built in the panel's own language — a group
   heading, hairline-separated rows, an Open link — until the design side draws
   it. Design chat 17 owns background work and should be told.

2. **The All books title dropdown.** §47 and decisions 159 to 172 design it and
   nothing builds it yet. Until it exists, B9's quick link is the shelves door
   the rail no longer provides.

3. **The Home quote's real source** (B13) and **the customization area in
   Settings** that will carry the greeting's name (B14). Both are to be
   discussed in this session; both have placeholders until then.

4. **The startup database pass runs on the screen.** The owner's log from the
   build carrying B20 shows one long block per launch — 4.9 s, then 2.5 s —
   while the splash is up, and no freeze once the app is running (150 to
   250 ms hiccups opening Settings, a book, the reader, Home). The part the log
   can name is the screen opening the library itself: `Catalog::open` runs the
   whole startup schema pass on the UI thread holding the lock, at least
   1558 ms in the first launch and 109 ms in the second. The rest of the block
   has the lock empty, so it is the app being built and painted for the first
   time with the startup jobs competing for the machine — and the log cannot
   name it, because startup has no activity labels at all, so every line from
   that phase points at whatever finished last. Two small steps are on offer
   and neither is ruled on: label the startup phases, and record for each block
   how much was processor time and how much was waiting, which settles "busy"
   from "starved" in one number. Roadmap 1.2's own note — *revisit when a large
   library makes those reads measurable* — is the standing invitation this
   answers.

Settled while the rail was built, and built: the entries and their order (B1),
the "My Library" label (B1, decision 173), the active glyph (B8), the rail's
fill and divider (§56: no fill, no border, a 2px rounded divider inset 28 from
the top and 24 from the bottom), the bell dot (§56: 8 from the top, 9 from the
right), the bundled glyphs (B6, B10) and the drawn tooltip (B11).
