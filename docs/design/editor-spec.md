# Deep editor: design spec

Source boards: https://claude.ai/artifact/2nMT5SDCKGPKjBNPs3YKBe (Terracotta, light and dark). Tokens are from `design-system.md` (sections 32 to 40 are new in v0.7). Decisions 118 to 143 in `decisions.md`. The in-reader editor (decision 118) is not covered here. Neighbouring specs: `epub-reader-spec.md` and `book-details-spec.md`.

## 1. Layout (1440 x 860 in the mockups)

| Area | Position | Notes |
|---|---|---|
| Top bar | full width, 56 high, 1px `border` below | Back pill, title block, then Tools, History (ghost), Save (filled) |
| File tree | left, 240 wide, 1px `border` on its right | Sections: Text, Styles, Images and Fonts, Contents; each collapses |
| Editing area | centre, flexible | formatting toolbar (48), tab strip (40), then the file |
| Right panel | right, 440 wide, 1px `border` on its left | tab row 48 high: Preview, Outline, Problems, Live CSS |
| Status bar | bottom, 32 high | position, file type, problem count, snapshot note, Code / Split / Preview |

Panels are docked: no radius, fill or shadow (section 32). The file tree can be hidden. **Split** is the default view (decision 122): Code gives the editing area the full width, Preview gives the right panel the full width.

## 2. Components

- **Flat tabs and ghost buttons** (section 33): used for the view switch, the right-panel tabs, the tab strip and every top-bar button except Back and Save.
- **File tree** (section 36): 36 high rows, collapsible section headers with a chevron, the open file with the accent bar, a changed file with a dot. Right-click menu (decision 125).
- **Editor tabs:** pinned, changed (dot) and temporary (italic) (decision 126). Images open an image tab; fonts open a specimen tab.
- **Source editor** (section 34): `GtkSourceView`, syntax from tokens only; formatting toolbar (Bold, Italic, Link, Heading, List) writes real tags.
- **Right panel:** Preview with its menu ("As the book ships" / "As in Kalam"; arrow only on the active tab); Outline (heading tree, current heading marked); Problems (section 37); Live CSS (selector, file and line, properties; overridden ones struck through).
- **Edit contents tab** (decision 136): a list of entries (36 high rows, parts medium weight, chapters indented 24, target file in Meta at the right, accent bar on the selected row), buttons "+ Add entry" and "Generate from headings" in the toolbar position, a hint line (Tab indents, Shift+Tab outdents, drag to reorder); the Preview on the right marks the target heading with a 2px `accent` outline.
- **Find and replace:** a floating panel, 440 wide, `card` fill, 1px `border`, radius 14, padding 24 (decision 131).
- **Command palette** (section 38) and **Tools menu** (decision 129, design system section 17): 240 wide with four submenus (Insert, Structure, Clean up, Fonts and images), the open one 260 wide.
- **After-tool pill** (section 39) and **scope question** (a 420 dialog with two option rows, "This file" and "Whole book").
- **Compare:** two columns with line numbers 32 wide; removed lines `match`, added lines `diff-add` (section 35); header with Back to editor and Restore this version.
- **Picker page** (section 40): Editor label, Display title, underlined search field, "Edits waiting" and "All EPUBs" groups, rows 72 high.

## 3. Dialogs (on the `overlay` scrim, decision 75)

| Dialog | Size | Contents |
|---|---|---|
| Unsaved changes | 420 | "Save changes to 3 files?", ghost Discard, ghost Keep editing, filled Save |
| Save with errors | 420 | "2 errors found", first error, ghost Save anyway, filled Show problems |
| History | 560, padding 24 | snapshot list (64 high rows), Compare, Restore this version |
| Spell check | 420 | sentence with the word dotted-underlined, suggestions, Add to dictionary, Ignore, Change |
| Personal dictionary | 420 | add field, words with ✕, Close |
| Insert character | 420 | search, 8-column glyph grid (36 buttons), name and code, Close, Insert |
| Insert link | 420 | In this book / Web address, search, list, Insert |
| Insert image | 420 | thumbnails, Add from file, alt text, Insert |
| Insert inline contents page | 420 | insert after, Parts and chapters, add to reading order |
| Split chapter | 420 | At the cursor, At every heading, By size (max size field), count, Split |
| Reorder files | 560 | reading order with drag handles, 3px accent drop bar, Save order |
| Replace cover | 420 | current cover, Choose image, Replace (disabled until chosen) |
| Export for editing | 420 | file, "Changed 2 min ago", Show in folder, Open in app, Import changes |
| Reports | 640 | tabs Files, Words, Images, Links, CSS, Fonts; sortable table |
| Subset fonts, Embed fonts | 420 | fonts with tick circles and sizes, licence note |
| Compress images | 420 | Lossless / Smaller, estimated saving |
| Check external links | 560 | progress bar, results (Works, Not found, Moved), Stop, Done |
| Tag mapper | 560 | rules (from to), count, Add rule, Apply |

Buttons in dialogs: ghost with a 1px `border`, then the main action filled (`accent-fill`, 40 high).

## 4. Tokens used

`bg`, `text`, `text-2`, `accent`, `accent-fill`, `card`, `border`, `overlay`, `match`, and the new `diff-add` (design system section 35). `text-3` is not used. Fonts: Playfair Display, DM Sans and a monospace font (proposed, section 34). Radii: 4, 6, 8, 10, 14, 999. New sizes: 36 tree row, 48 tab row, 56 top bar, 32 status bar, 440 right panel, 240 file tree, 72 picker row.

## 5. States

Drawn (dark and light): the workspace in Split with the Preview tab and its menu open; Edit contents tab; Problems (errors and warnings), Outline and Live CSS tabs; Problems with a fix preview; History and Compare; Tools menu with the Clean up submenu open; command palette; Find and replace (matches found); scope question and the after-tool pill; tab strip with pinned, changed and temporary tabs; image tab and font tab; collapsed and open file tree sections; picker page; no EPUBs; EPUB cannot be opened; unsaved-changes and save-with-errors dialogs; Problems empty; control states for the primary button (idle, hover, focus, disabled); the dialogs in section 3.

Not drawn: see "Not designed yet (deep editor)" in `decisions.md`.

## 6. GTK widgets and custom drawing

- **Layout:** a `Gtk.Paned` pair (file tree, centre, right panel) under a `Gtk.Box` top bar and above a `Gtk.ActionBar` status bar. The Code / Split / Preview switch is linked `Gtk.ToggleButton`s in the action bar. The file tree can be hidden with a `Gtk.Revealer`.
- **Source editor:** `GtkSourceView` 5 with a custom style scheme generated from the tokens. The current-line tint and cursor come from the view. No visible scrollbars (decision 90): external scrollbar policy.
- **File tree:** a `Gtk.ListView` over a `Gtk.TreeListModel`; section headers are expander rows. Drag to reorder uses `Gtk.DragSource` and `Gtk.DropTarget` with a custom drop bar.
- **Tabs:** a `Gtk.Notebook` styled flat, or a custom strip with `Gtk.ToggleButton`s; the temporary tab is italic via CSS. Each tab owns its undo stack (a `GtkSourceBuffer` each).
- **Right panel:** a `Gtk.Stack` with flat tab buttons. Preview uses the reader's engine; click-to-jump needs it to report the clicked element. Outline, Problems and Live CSS are `Gtk.ListView`s.
- **Compare:** two `GtkSourceView`s with linked scrolling; the tinted lines are line-background marks.
- **Find and replace, command palette, Tools menu:** overlay widgets or `Gtk.Popover` with no arrow and a custom list (not `Gio.Menu`). Submenus are a second popover beside the first.
- **Reports table:** a `Gtk.ColumnView`. Check external links and fixes run in a background task with a progress bar.
- **Export for editing:** `Gio.FileMonitor` on the exported file.
- **Dialogs:** a modal window or overlay widget on an `overlay` scrim.
- No conic gradients, `mask-image`, `backdrop-filter`, widget rotation or blurred glows.

## 7. Notes for the builder

- Edits change Kalam's copy only. Take a snapshot before every save, tool, restore and import (decision 135). The original is backed up per the Library setting.
- Validate before saving. Errors ask first (decision 143); warnings never block.
- A tool with scope (Smarten punctuation, Reformat code, Remove unused CSS) asks "This file or Whole book?" first and shows an Undo message after (decision 128). One tool run is one undo step.
- Fix in Problems must show the exact line change before Apply (decision 132). A problem with no safe fix only offers Go to.
- Split and merge must fix links to the moved content automatically. Reordering files edits the reading order only.
- Edit contents writes both `toc.ncx` and `nav.xhtml` and saves with the book. It is not the inline contents page.
- Opening a book never touches the network. Check external links runs only when started.
- Export for editing: confirm the folder (`~/Kalam/Editing/`), and say what happens to the copy after Import changes.
- The formatting toolbar writes `strong` and `em`, never `b` and `i`; pressing again removes the tag. There is no Underline button.
- Credits and rows that name files must use the real file names; sample names in the boards are invented.
- Every icon-only button needs a tooltip and an accessible label. Add keyboard focus rings everywhere (2px accent, 2px offset). Hover uses `border` colour as a fill.
- The boards use placeholder text glyphs for icons. No tap-to-look-up (decision 9).
