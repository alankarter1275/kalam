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

Settled while the rail was built, and built: the entries and their order (B1),
the "My Library" label (B1, decision 173), the active glyph (B8), the rail's
fill and divider (§56: no fill, no border, a 2px rounded divider inset 28 from
the top and 24 from the bottom), the bell dot (§56: 8 from the top, 9 from the
right), the bundled glyphs (B6, B10) and the drawn tooltip (B11).
