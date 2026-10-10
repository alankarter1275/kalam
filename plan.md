# Plan — KAL-5: EPUB Editor Overhaul

**Status: IMPLEMENTING — plan approved; ready to begin coding.**

## The Problem
The Phase 6 EPUB editor (shipped 2026-10-09) leaned entirely into a WYSIWYG proofreader layout that was deemed "completely useless" and "horrendous" on first use. It lacked the file-tree structure and powerful capabilities of standard EPUB editors (like Calibre), and its virtual "Patch & Bake" system was too clunky for deep editing.

## The KAL-5 Design (Settled 2026-10-10)

The overarching goal is to achieve **Calibre-level power with a clean, modern GTK4 UI**, prioritizing direct file edits in a dedicated IDE interface, while preserving zero-lag quick fixes inside the reader.

### 1. The Full Editor Layout (IDE-style)
- **Left Sidebar:** A complete File Tree (HTML, CSS, images, etc.).
- **Right/Main Area:** The content editing pane, featuring a toggle between two visual modes.
- **Entry Point:** The editor picker page (polished) and the reader's TOC pencil.

### 2. The Two Editing Modes
1. **Light Mode (Obsidian-style Live Preview):** A clean rich-text view that renders HTML styling (bold, headers) but explicitly shows the underlying Markdown syntax (e.g. `**bold**`). Edits map safely back to HTML tags.
2. **Deep Mode (Raw Code):** A raw HTML/CSS code editor with a live preview (side-by-side or toggleable) for deep structural formatting.

### 3. Save Mechanism: Direct `.epub` Saves
- **Full Editor:** We are dropping the complex "Patch & Bake" system for the full editor. Edits hit Save and directly update/repack the `.epub` file, maintaining an automatic `.orig` backup (Calibre style).
- **Reader Quick Fixes:** In-reader fixes (the pencil tool) will *continue* to use the "Patch" system. Saving directly to an HDD from the reader causes lag, so quick fixes save instantly as patches.
- **Patch Review:** The full editor will detect pending reader patches and show a clear indicator, allowing the user to review and "Bake" them before deep editing.

### 4. Advanced Features (Calibre Parity)
KAL-5 must include these powerful tools:
- **File Manager:** Add, delete, rename, merge, and split HTML/CSS files directly from the file tree.
- **Visual TOC Editor:** A dedicated tool to build, edit, and nest the Table of Contents.
- **Check Book:** An automated validation tool that finds errors like broken links, invalid CSS, and missing files.
- **Automated Tools:** Smarten punctuation, subset/embed fonts, format HTML, etc.
- **Saved Searches:** Save complex regex find/replace queries for later reuse.

## Implementation Plan

1. **Step 1: Direct Save Worker & Backup Safety**
   - Create a background worker that takes a file path (e.g., `OEBPS/chapter1.xhtml`) and new bytes, writes a `.orig` backup of the EPUB if one doesn't exist, and repacks the ZIP directly. This completely replaces the "virtual patch" logic for the full editor.

2. **Step 2: The Pending Patch Guard**
   - Modify the editor entry logic (`epub_editor.rs` init). If the book has pending reader patches, the editor opens a blocking "Review Patches" view (`edits_panel.rs`) first. The user must Bake or Discard these patches before accessing the IDE, preventing raw HTML edits from breaking pending text patches.

3. **Step 3: IDE Shell & File Tree**
   - Strip out the reader engine from `epub_editor.rs`. Build a new GTK4 layout: a resizable `gtk::Paned` with a file tree on the left and a content stage on the right.
   - The file tree (`gtk::TreeView` or `ListView`) reads `content.opf` to categorize files (Text, Styles, Images, Fonts, Misc) just like Calibre.

4. **Step 4: File Manager Operations**
   - Add context menus to the file tree: Rename, Delete, Add Blank HTML/CSS.
   - Implement the repack logic for these operations (updating the ZIP archive, the `content.opf` manifest, and the spine).

5. **Step 5: Deep Mode (Raw Code IDE)**
   - Wire the file tree to open files into a `GtkSourceView` pane (reusing Phase 6.9's `sourceview5` setup) in the main content area.
   - Keep the headless WebKit renderer for the live side-by-side preview.

6. **Step 6: Light Mode (Obsidian-Style Live Preview)**
   - Build a custom `GtkTextView` that parses basic HTML (`<b>`, `<i>`, `<h1>`) into Markdown syntax tags (`**`, `*`, `#`) and applies CSS styles to those spans so they appear formatted while exposing the punctuation.
   - Create a bi-directional mapper that converts this markdown-styled text back to valid XHTML on save.

7. **Step 7: Advanced Tools**
   - Add a toolbar for the Visual TOC Editor (parsing `toc.ncx` / `nav.xhtml`), Check Book (validating EPUB structure), and automated formatting tools.

## Open Questions for the Owner

~~**1. Light Mode Complexity:**~~ **Resolved (2026-10-10):** If the Light Mode encounters complex HTML/CSS it cannot safely represent as Markdown, it will fall back to showing raw HTML tags for those specific elements alongside the markdown. This is the safest approach to prevent data loss.

~~**2. Check Book tool:**~~ **Resolved (2026-10-10):** The validation tool will run automatically in the background on every save, and inform the user if any structural errors (broken links, missing images, invalid HTML) are detected.
