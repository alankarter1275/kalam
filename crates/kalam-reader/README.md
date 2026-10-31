# kalam-reader

`kalam-reader` is the GTK4 widget Kalam uses for reflowable EPUBs. Fixed-layout
EPUBs are not supported. The public widget is `ReaderView`; the host owns
navigation chrome, persistence, dictionary UI and other application features.

## Seeing the widget on its own

The standalone demo that used to live at `tools/kalam-reader-demo` was deleted
on 2026-10-30 once nothing depended on it. The full app integration is in
[`src/pages/reader/engine.rs`](../../src/pages/reader/engine.rs).

## Open and embed a book

`ReaderView::open(path, prefs, options)` opens an EPUB and returns a GTK
`DrawingArea` wrapper. Opening is synchronous and performs archive/package
loading; call it on the GTK thread because it constructs GTK objects. Chapter
layout and image loading continue as the widget draws the book. `ReaderOptions`
controls host-font scanning, additional font files and the cache budget.

```rust,ignore
let prefs = kalam_reader::KalamPrefs::default();
let options = kalam_reader::ReaderOptions::default();
let view = kalam_reader::ReaderView::open("book.epub", prefs, &options)?;
container.append(view.widget());

view.connect_position(|position| {
    // Persist the current chapter and fraction in the host application.
});
```

Call `view.close()` when the reader page is destroyed. It releases the session
and breaks the callback/controller references that otherwise keep the book's
layouts and decoded images alive.

Callbacks execute on the GTK main thread. Keep them short; send owned values to
the host model or defer expensive work. The widget itself has no catalog or
application database.

## Reader features

- EPUB pagination with facing pages when the widget is wide enough, and a
  single long continuous-scroll mode. `ReaderView::set_mode()` changes modes;
  `vadjustment()` supplies the scroll position for a host scrollbar.
- Nested publisher table of contents through `toc()` and `goto_toc()`.
- Chapter/fraction navigation, back stack, and `LayeredLocator` capture and
  restoration.
- Text selection with move notifications and selection geometry for a host
  action chip.
- Host-managed highlights: capture, display, update, recolor, remove and jump
  to highlights. Highlight IDs belong to the host.
- Whole-book text search through `search()`. Queries shorter than two
  characters return no results; results are capped at 500.
- Internal links and footnotes through `connect_note`; external links through
  `connect_external_link`; image taps through `connect_image_tap`.
- Reader preferences for theme, font, line height, column width, justification,
  hyphenation and publisher styles.
- Bundled reader fonts, optional host-font scanning, and optional extra fonts
  from `ReaderOptions::fonts_dir`.

`DEFAULT_CACHE_BUDGET` is 32 MiB. Set `ReaderOptions::cache_budget` to override
it; `cache_bytes()` reports current use.

## Word Memory and dictionary lookup

The widget has no general tap-a-word-to-dictionary behavior. The host supplies
saved vocabulary with `set_word_memory()`. Those terms receive a dotted
underline; `connect_word_tap()` and `connect_word_hover()` report interactions
with those marked terms. Kalam uses selection and an explicit Dictionary action
for ordinary word and phrase lookup.

## Persistence

The widget does not persist progress, highlights, bookmarks or preferences.
The host saves the `chapter` and `fraction` from `connect_position()` and may
also store a locator for more durable text-based restoration. The host owns all
highlight ids and rows.

## Dependencies

The crate uses gtk4-rs 0.11 and requires system GTK 4.16 or newer. It wraps the
local Chapbook workspace crates, which provide EPUB parsing, text layout,
pagination and CPU rendering. PDF and CBZ/CBR reading are implemented by
Kalam outside this widget.
