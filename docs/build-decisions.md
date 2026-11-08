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

**To confirm:** All books is listed third, between My Library and Authors,
because it is the grid behind My Library and shelves now hang off it. The
owner added the entry without naming its position.

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

## Still open — needed before the rail can be built

Recorded so nothing is invented silently. None of these is ruled on yet.

1. **The active glyph.** Design system §56 says the icon on the filled disc is
   the page colour, "so it reads as cut out". §11 says anything on an
   `accent-fill` surface is white and must reach 4.5 : 1. Measured on
   Terracotta:

   | Glyph colour | Dark | Light |
   |---|---|---|
   | white | 4.64 : 1 | 5.12 : 1 |
   | page colour (cut out) | 3.90 : 1 | 4.63 : 1 |

   The cut-out look clears the 3 : 1 an icon needs and misses the 4.5 : 1 text
   needs. It is a taste call, not a rules call. The code paints white today.

2. **The rail's fill.** §56 says "no fill, no border". In dark mode the rail
   carries its own darker fill; in light mode it already matches the page.

3. **The divider.** §56 wants 2px with rounded ends, stopping 28 from the top
   and 24 from the bottom. The code draws a 1px line the full height.

4. **The tooltip.** Decision 180 wants a drawn card, 32 high, with the name
   and its shortcut. The code uses the system tooltip with the name only.

5. **The bell dot.** §56 places it 8 from the top and 9 from the right; the
   code has 6 and 6.

6. **Window controls.** Decision 181 wants three 32 round buttons at the top
   right over the page. The app has no title bar — which the decision also
   wants — and no buttons at all, so Alt+F4 is the only way to close. Is that
   part of this shell work or a separate pass?

7. **The label.** Decision 173 proposes the entry reads "My Library"; the code
   says "Library". B1 assumes the design's wording, and the checklist already
   asks the owner to confirm it.
