=== KALAM REPORT ===
engine: ef11e46 (in-tree bundle still says e6a3c25; I verified ef11e46
directly from the engine repo and src/pages/reader/engine.rs already carries
the new pos line, so nothing on my side is stale)
step: static pass before the owner's first build
status: BLOCKED

1. `rg -n 'webkit|javascriptcore|WebView|webview' src/ Cargo.toml` — BLOCKED.
   Cargo.toml, src/app.rs, src/theme.rs and the whole reader are clean after
   commit 22decdd. 28 hits remain, all in src/epub_book.rs:
     567, 568      `window.webkit.messageHandlers` (shell JS)
     1024-2381     more shell JS and comments
     3031-3732     `-webkit-*` CSS properties and comments
   All 28 are string literals / comments inside three items that have had no
   callers since the reader swap: `OpenBook::chapter_html` (120-149),
   `inject_reading_shell` (503-2884), `reading_css` (2938-3795). Grepping
   each name outside its own definition returns nothing.
   Choose: (a) accept this as clean — no live WebKit code, no crate
   dependency; or (b) say "delete them" and I remove the three items in a
   follow-up commit (that also clears the dead_code warnings they now
   produce, which the `clippy -D warnings` job would fail on). (b) needs
   care: path_to_file_url / read_file_string / parent_zip_path /
   join_zip_path sit between them and may orphan, and there is no compiler
   here. Your INTEGRATION.md step 0 says these may stay "for now", which is
   why I did not delete them on my own.

2. Cargo.toml — OK.
     line 31: kalam-reader = { git = "https://github.com/alankarter1275/kalam-engine", branch = "arena/01a07f3f-kalam-engine" }
   zero `webkit6` / `javascriptcore6` lines; zero case-insensitive matches
   for either name anywhere in the file.

3. `rg -n 'JsRaw|JsPayload|WebViewHandlers' src/` — OK. Empty after 22decdd;
   the last hit was a doc comment in the engine's own patch file, and the
   test name there held the last lowercase `webkit`.

Sense.example is `pub example: Option<String>` (src/db/dictionaries.rs:86) —
the String/Option<String> case your comment assumes, so no change needed.

Wording edits in your file (patch/engine.rs), please fold in so a re-copy
does not revert them — all commit 22decdd:
  * line 3: "replaces the WebKit `WebView` + JavaScript bridge" becomes
    "replaces the old in-app browser page and its JS bridge"
  * line 98: "like it did for `JsRaw`" becomes "like it did for the old payloads"
  * test name: webkit_rows_are_not_mistaken_for_locators becomes
    legacy_rows_are_not_mistaken_for_locators

tried: ran all three greps before and after 22decdd; fetched the engine repo
and diffed e6a3c25..ef11e46 (only docs/kalam/{INTEGRATION.md,RESEARCH.md} and
docs/kalam/patch/.../engine.rs; nothing under crates/, so the widget API is
unchanged and my static cross-check stands); grepped every name in the three
remaining functions for callers (none); grepped Cargo.toml case-insensitively.
=== END ===
