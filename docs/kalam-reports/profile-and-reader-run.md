=== KALAM REPORT ===
engine: 1058af3 (bundle on my branch as 3ca25ee; my src/pages/reader/engine.rs
now matches yours exactly, including the folded conversion fix)
step: steps 0-3, profile correction + the first real run of the reader
status: DONE

1. [profile.release] changed as instructed, one commit (990fab7):
   `lto = "thin"`, `codegen-units` removed, `strip` and `opt-level`
   untouched, and a comment saying why (stylo's peak memory vs a 4 GB
   machine). CI's release build is green with it, so the profile is not
   just locally plausible. The owner's next `git pull` picks it up before
   their build.

2. The reader now really runs, headless, in CI. Two preconditions were
   missing and both are in now:

   * KALAM_ROUTE had no way to name a page that needs a book. It now takes
     ids: `book-<id>` -> Route::BookPage, `read-<id>` -> Route::Reader. The
     bare words still refuse (a route that guessed an id would photograph a
     different book and call it the right one).
   * the CI seeder wrote `b"not a real epub -- screenshots only"` into every
     book.epub, so the first reader pass mounted the reader, opened the
     file, and landed on the open-error label:
       kalam: open epub failed: failed to open book .../book.epub:
       [UnreadableArchive - ...]: invalid Zip archive: Could not find EOCD
     Worth knowing that path renders without a panic (it is ours, from step
     3) -- but it says nothing about the reading page. The seeder now writes
     a valid EPUB: uncompressed `mimetype` first, META-INF/container.xml, an
     OPF with metadata/manifest/spine, EPUB 3 nav + EPUB 2 NCX so either
     parser finds the contents, and six chapters of seeded prose (~1,900
     words each, so pagination is exercised rather than a one-line chapter).

   With both, run 34538345557's reader pass -- headless sway, Cairo, 139
   books, route read-1, commit 2b3050b:

   * stderr is GTK's a11y-bus warning and "KALAM_ROUTE=read-1 — navigating".
     No open failure, no panic, no warning from the widget.
   * 01-read-1.png is 203,690 bytes against 47,603 for the open-error page,
     its fingerprint differs from Home's, and the three samples are
     byte-identical to each other -- the page settles rather than repainting.
   * [timing] window_shown 161.7 ms, startup_first_page 2.8 ms,
     book_open 4.2 ms (open -> first page inside the widget).
   * peak RSS 121 MB with 139 books and a book open; the library-only run in
     the same job peaks at 83 MB, so reading costs ~38 MB, not ~100.

   Read this as "the widget lays out text under Cairo and does not panic",
   not as "it looks right on a desktop". Step 4 on the owner's machine is
   still the acceptance test for colour, page turns, the dictionary popover
   and external links.

3. Nothing here needs engine action. Open items on the two sides: your
   deferred "unsave from the card" (recorded, not built), and the owner's
   step-4 pass.

tried: validated the generated EPUB in python before trusting CI with it
(mimetype first and stored, every XML part well-formed, every manifest href
present in the archive); confirmed the new route unit tests run and pass
(app::tests::route_names_with_a_book_id_resolve_and_bad_ids_do_not ... ok);
read the published clippy log (Finished `dev` profile in 23.98s, no
diagnostics) and test log (340 passed, 0 failed, 2 ignored).
=== END ===
