# Plan — 2.21: embed recognized text into the PDF file (searchable PDF)

**Status: RESEARCH DONE. Awaiting the owner's answers to the three open
questions below, then his go, before implementation.**

Per the working agreement: research below is done and cited (code with
file/line evidence; the mupdf-rs 0.8 write API verified against the
published crate source on docs.rs); the pitfalls file has been consulted
(§§12, 23, 27–29, 31–35 — see the end); all owner questions are asked
here, during planning, not during implementation.

---

## Goal

A conscious, explicit action writes the OCR-recognized text back into a
scanned PDF as an invisible text layer, so every application — not just
Kalam — sees selectable, searchable, copyable text. The file is modified
only when the owner asks for it, with a clear warning and an undo/backup
story (owner requirement, recorded in ROADMAP 2.21). A welcome side
effect: once the file carries its own text, the vector-text path serves
the book and our OCR cache is never needed for it again.

## What is true today (verified, with evidence)

1. **The recognized text exists, per page, with per-character
   coordinates, in the database.** `page_ocr_cache` rows are written the
   moment a page finishes (`save_page_ocr`, `src/db.rs:1396`) and read
   back with `load_page_ocr` (`src/db.rs:1417`), each holding a complete
   serialized `PdfPageText` — every line and every character with its
   quad in document points (`PdfTextChar { ch, x0, y0, x1, y1 }`,
   `src/pdf.rs:20`). Rows are keyed by the file fingerprint
   `"{size}:{mtime}"`, so a modified file never reads stale rows.
2. **Those coordinates are in the right space for the writing API.**
   Both text paths store view coordinates — origin top-left, y downward:
   the MuPDF extraction stores structured-text quads raw
   (`src/pdf.rs:555–584`), and the OCR path maps rendered pixel rows to
   points through the same page dimensions (`src/ocr.rs:140–185`). The
   click-to-selection mapping divides widget pixels by the same
   dimensions, which is why selection works identically on digital and
   scanned pages. mupdf-rs 0.8's `Shape::insert_text` takes exactly such
   view-space points: `Shape::new` caches the page CTM and its inverse
   (`shape/mod.rs:79–100` of the crate) and `insert_text` maps the given
   point through the inverse CTM itself (`shape/text.rs:116`). No
   coordinate conversion is needed on our side.
3. **The writing API exists and is safe-shaped.** mupdf-rs 0.8 provides
   `PdfDocument::open` (edit an existing file), `load_pdf_page`,
   `Shape::new` + `insert_text` + `commit(&mut doc, /* overlay: */
   true)`, and `TextOptions { render_mode: 3 }` — PDF text render mode
   3, invisible, the standard searchable-PDF construction. Saving goes
   through `PdfWriteOptions` (full rewrite, not incremental) and
   `write_to(&mut W)`, which can write to a temporary file we verify
   before it ever replaces the original.
4. **One honest limitation: rotated pages.** The crate's `Shape` clips
   inserted text against the unrotated media box (`shape/mod.rs:87` —
   `width`/`height` come from `media_box`), so on a rotated page,
   view-space points beyond the media box's width would be dropped
   silently. Rotated scanned books are rare; the first version skips
   pages whose rotation is not 0 and says so in the result, rather than
   fighting the crate's clip. `PdfPage::set_rotation` exists on the same
   type, so `rotation()` is readable per page.
5. **The base-14 Helvetica font needs no embedding.** Render mode 3
   never paints a glyph; the font is used only for metrics and encoding.
   The crate's `base14-fonts` feature is already enabled
   (`Cargo.toml:158`). Simple-font encoding covers Latin-1 — the
   English-only library scope — including curly quotes and dashes.
6. **The infrastructure to run it exists.** Background tasks with
   progress, cancellation and task-manager visibility are the standing
   pattern (`src/pdf_ocr.rs:114` — `enqueue_import_scan`); the doc
   service already answers page-vector-text questions off the UI thread;
   the 2.20 principle ("the UI thread never touches disk, database, or
   parsing") governs the whole flow.
7. **After embedding, everything downstream just works.** The file gains
   vector text, so `ensure_page_text` extracts it directly, the
   import-time OCR queue skips the book (only vector-empty pages are
   enqueued), and other applications see the text through their own
   extractors. The file's size and mtime change, so the old
   fingerprint's `page_ocr_cache` rows can never be read again — they
   become orphans.

## Design

1. **New module `src/pdf_embed.rs` — pure, testable core.**
   - `words_from_line(&PdfTextLine) -> Vec<PdfEmbedWord>`: group a
     line's characters into words at whitespace boundaries (the same
     scan `word_at` performs, `src/pdf.rs:317–339`), each word carrying
     its text and its quad (min/max over its characters).
   - `word_geometry(quad) -> (Point, fontsize)`: the insertion point and
     size for one word — fontsize `0.8 ×` quad height, baseline at
     `bottom − 0.2 ×` height (the descender allowance keeps the
     invisible glyphs inside the visual line, so other applications'
     selection highlights land on the scanned ink).
   - `embed_pages(doc, pages: &[(usize, PdfPageText)], progress) `:
     open the file with `PdfDocument::open`, and for each page with
     recognized text: skip and record pages whose rotation is not 0;
     otherwise `Shape::new`, one `insert_text` per word with
     `render_mode: 3`, `commit(&mut doc, true)`. Pages that already
     have vector text are never touched.
2. **Verify-then-swap, never in-place blind write.** The document is
   written with `write_to` into a temporary file **next to the
   original** (same filesystem, so the rename is atomic). The temporary
   file is then reopened with MuPDF and verified: page count matches,
   and sample pages (first, last, a middle one) extract the expected
   text. Only then: original renamed to the backup name, temporary
   renamed to the original. Any failure at any point leaves the
   original untouched and removes the temporary file.
3. **A background task, a dialog, and a reload.**
   - The whole embed runs as a task-manager job (progress per page,
     cancellable before the swap), exactly like the import OCR scan.
   - The confirmation dialog states plainly what happens: the file will
     be modified, a backup will be kept, other applications will be able
     to search and copy the text. No emoji, plain text.
   - On success: the reader invalidates its text cache and bumps the
     render generation so pages re-extract from the file's new vector
     text; the orphaned `page_ocr_cache` rows of the old fingerprint
     are deleted (they can never be read again); a plain notification
     reports pages embedded and pages skipped (rotated).
   - The readiness gate: the doc service counts pages with neither
     vector text nor a cached OCR row; while that count is above zero
     the action shows "N pages still to recognize" instead of running
     (see open question 3).
4. **Write options: touch as little as possible.** `PdfWriteOptions`
   with full rewrite (not incremental), no garbage collection, no
   clean-up pass, no image or font recompression — the scanned page
   images and the existing content must pass through byte-faithful in
   every way MuPDF guarantees, and the verification pass is the
   backstop.

## Tests (CI is the only compiler)

- `words_from_line`: grouping at whitespace, punctuation kept with its
  word, an empty line and a whitespace-only line produce nothing.
- `word_geometry`: the descender allowance keeps the glyph box inside
  the quad; degenerate (zero-height) quads clamp to a minimum size.
- **Round-trip integration test**: build a blank multi-page PDF with
  mupdf itself, run `embed_pages` on synthetic `PdfPageText` input,
  save to a temporary file, reopen, extract — assert the words come
  back at the expected positions (render mode 3 text is extracted like
  any other). This exercises the real writing path end to end.
- Rotated page: a page created with rotation 90 is skipped and
  reported, not corrupted.
- The verify-then-swap guard: a corrupted-verification case (write to a
  read-only directory) leaves the original untouched.

## Open questions for the owner (asked now, during planning)

1. **The backup story.** After embedding, the original file must be
   recoverable. Options:
   (a) keep a backup beside it — `Book.pdf` becomes the searchable
   file, `Book.pdf.bak` (or a hidden name) is the untouched original;
   undo is renaming back;
   (b) write a new file beside it — `Book (searchable).pdf`, original
   untouched, but the library entry still points at the original, so
   the owner must re-import or swap manually.
   Recommendation: (a) — the library entry stays valid and undo is one
   rename; the backup name can be made hidden-dot to avoid clutter.
2. **Where the button lives.** Options: the PDF reader's sidebar
   (Settings panel, a "Text" section) — where the need is noticed; the
   library's book context menu; or both.
   Recommendation: the reader's sidebar first (one place, conscious
   action); the context menu can come later if missed.
3. **Complete recognition first, or partial embed?** Options: allow the
   action only when every scanned page has recognized text (button
   shows "N pages still to recognize" until then); or embed whatever
   exists and leave the rest image-only.
   Recommendation: require complete — a half-searchable file is
   confusing in other applications, and the import OCR task already
   runs to completion on its own.

## Pitfalls consulted

§12 (CI is the only compiler; exact code before edit anchors), §23, §27,
§28, §29, §31 (diff read before commit), §32/§35 (gesture coordination
— not touched here, but the reader reload after embed must not fight the
selection state), §34 (cursor lifecycle — unaffected), and the standing
2.20 principle: nothing in this flow runs on the UI thread.
