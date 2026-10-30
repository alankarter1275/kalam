# Settings: design spec

Source boards: https://claude.ai/artifact/82NsKxiZver3BBCiADgzen (Terracotta, light and dark). Tokens are from `design-system.md` (sections 41 to 46 are new in v0.8). Decisions 144 to 158 in `decisions.md`. Facts about today's panes come from `settings.rs`. Neighbouring specs: `editor-spec.md` (flat chrome, dialogs) and `epub-reader-spec.md` (the reader's Reading pane, Shortcuts rules).

## 1. Layout (1280 x 760 in the mockups; Shortcuts 940 high)

A full page on plain `bg`. No band, tint, shadow or glow.

| Area | Position | Notes |
|---|---|---|
| Pane list | left, 240 wide, 56 left and top padding | groups Appearance, Reading, Library, Sources, App (decision 144) |
| Content column | 640 wide, 64 right of the list, top 56 | title (Display 46), Meta line, then Label headings and outlined sections |

**Panes:** Appearance; Reading; Libraries and storage, Data and export, Book files, Watch folders, Dictionaries; Metadata sources, Integrations; Shortcuts, About. The rail is not drawn on the pane boards.

**Notification history** is not a pane. It is the bell at the bottom of the rail and a floating panel (section 2, decision 148).

## 2. Components

- **Pane list item** (design system section 41): 34 high; active has the 3px `accent` bar and bold `text`.
- **Section and row** (section 41): Label heading, then an outlined box with 48 high rows and hairlines; label and optional Meta caption at the left, control at the right.
- **Palette and mode picker** (section 42, decision 145): 2 x 2 boxes, a Light and a Dark mini-page card in each; the selected card has the ring and the tick badge. No Light / Dark / System switch.
- **Text and motion rows** (decision 146): two dropdowns, a Small / Default / Large segmented switch (section 22), a System / On / Off segmented switch.
- **Reading pane** (decision 147): a preview card (Playfair 18 on the current page text), dropdown and field rows, a switch, a four-option segmented switch for Word memory.
- **Dropdown** (section 43): closed and open. Open lists: typeface (own fonts, tick on the current), shelves (No shelf, shelves, divider, accent "New shelf…").
- **Libraries** (decision 149): rows with an accent "Open" chip or Open and Forget buttons; the folder-missing row has an accent Meta line and Locate folder.
- **Watch folders** (decision 151): a folder heading with Remove, a section of three dropdown rows and two switch rows, then Add watch folder.
- **Dictionaries** (decision 152): 56 high rows with a drag handle, the "Speaks first" chip, Remove; Import dictionary and Rescan; dragging state with the 3px drop bar.
- **Metadata sources** (decision 153): Google Books row with a chevron; API key and Country code as indented rows with no box and no line between them; "Not available yet" chips for the comics sources.
- **Integrations** (decision 154): three sections, all disabled.
- **Shortcuts** (decision 155): four outlined groups, 40 high key rows, key chips (section 44), the recording chip and the conflict line.
- **About** (decision 156): the logo in `accent`, Display name, Meta line, three rows.
- **State cards:** dashed cards (design system section 10) for "No watch folders" and "No dictionaries".
- **Notification panel** (section 45, decision 148): heading, accent Clear history, Today and Earlier groups, event rows; empty line.

## 3. Dialogs (on the `overlay` scrim, decision 75)

| Dialog | Size | Contents |
|---|---|---|
| Delete original backups | 420 wide, padding 24 | title, body with the counts, ghost Cancel, filled "Delete backups" |
| Remove dictionary | 420 | title with the pack name, body (will not come back unless imported), ghost Cancel, filled "Remove dictionary" |
| Open library (restart prompt) | 420 | "Open Work?", "Kalam restarts to open this library. Your books are not changed.", ghost Cancel, filled "Restart and open" |
| Export, file and folder choosers | not designed | see "Not designed yet" in `decisions.md` |

## 4. Tokens used

`bg`, `text`, `text-2`, `accent`, `accent-fill`, `card`, `border`, `overlay`. `text-3`, `match` and the highlight colours are not used. Fonts: Playfair Display and DM Sans. Radii 6, 10, 14, 22 and 999 only. New sizes: 640 content column, 240 pane list, 34 list row, 220 dropdown list (design system v0.8). No new colours.

## 5. States

Drawn (dark and light): Appearance; Reading; Reading with the typeface dropdown open; Integrations; Libraries and storage; Libraries with a folder missing; Open library restart prompt; Data and export; Book files; Delete backups confirm; Watch folders; Watch folders with the shelf dropdown open; Watch folders empty; Dictionaries; Dictionaries while dragging; Dictionaries empty; Remove dictionary confirm; Metadata sources; Metadata sources with a rejected key; Shortcuts with a key being recorded and a conflict; About; notification history (events and empty); control states (idle, hover, focus, selected, disabled for the controls in section 46 of the design system).

Not drawn: see "Not designed yet (Settings)" in `decisions.md`.

## 6. GTK widgets and custom drawing

- **Page:** a `Gtk.Box` with a pane list (a `Gtk.ListBox` or linked toggle buttons styled flat) and a `Gtk.Stack` of panes, each pane in a `Gtk.ScrolledWindow` with the external scrollbar policy (decision 90). Rows are a `Gtk.ListBox` inside an outlined `Gtk.Frame`-style box, not libadwaita preference rows, so the hairlines and 48 high rows match.
- **Palette picker:** a `Gtk.Grid` of `Gtk.ToggleButton`s (radio behaviour in code) whose child is a **custom-drawn mini-page widget** (Cairo, using the other palette's colours); the tick badge is an overlay child.
- **Dropdown:** `Gtk.DropDown` with a custom list factory (own-font typeface previews, tick on the current row) and the popover styled as section 17. No arrow.
- **Switches, steppers, segmented switches:** `Gtk.Switch`, two buttons and a label, linked `Gtk.ToggleButton`s.
- **Dictionary list:** `Gtk.ListView` with `Gtk.DragSource` and `Gtk.DropTarget`; the 3px drop bar and the dimmed original are drawn by a custom widget.
- **Key chips and recording:** a button that takes a `Gtk.EventControllerKey` while recording; a refused key shows the conflict line.
- **Dashed state cards:** a custom-drawn widget (Cairo dash), as on the series page.
- **Notification panel:** a `Gtk.Overlay` child in a `Gtk.Revealer` (slide), outside click and Esc close it; the rail item is a toggle button with a small overlay dot.
- **Dialogs:** a modal window or overlay widget on an `overlay` scrim.
- No conic gradients, `mask-image`, `backdrop-filter`, widget rotation or blurred glows. No visible scrollbars anywhere.

## 7. Notes for the builder

- App settings live in `prefs.json` and are shared by every library; each library has its own database and files. Dictionaries are shared and never copied. Say so in the Meta lines as drawn.
- Reading defaults go in `prefs.json`. Per-book (and per-series for comics) overrides are stored with the book, in `kalam.json`, against the book, not a path. One switch per book covers the whole pane (decisions 40 and 147). A book without an override reads the defaults live.
- The Reading pane's controls and the reader's Reading pane must be the same widgets. "Use these settings as my defaults" copies only that pane's values, never across formats.
- Appearance: the app has four palettes in light and dark only. Ignore the README's "13 dark themes". The saved choice is a palette and a mode, not "follow the system".
- Text size scales every size of the type table in `style.rs` together. Reduce motion defaults to the system setting and turns slides into fades.
- Replace the code's up and down dictionary buttons with drag (decision 152). Removed bundled packs stay removed; Restore bundled packs brings back all of them.
- The code keeps the last 25 events for the session. The bell panel shows that list; remove the Notifications pane from Settings.
- Destructive actions: confirm Delete backups and Remove dictionary with counts. Forget library, Remove watch folder, Clear cache and Clear history stay direct.
- Opening a page in Settings never touches the network (decision 110). Metadata sources only switches sources on and off and stores the Google Books key and country; keep the key out of logs. Integrations is disabled until Part 2 exists.
- Rebindable shortcuts reuse the reader's recording and conflict rules (decision 24). The shortcut keys are placeholders until built (decision 76).
- Every icon-only button needs a tooltip and an accessible label. Add keyboard focus rings everywhere (2px accent, 2px offset). Hover uses `border` colour as a fill. The pane list and every row must be reachable with the keyboard.
- No tap-to-look-up (decision 9). The README paragraph that describes it should still be deleted.
