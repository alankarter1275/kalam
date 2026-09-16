=== KALAM REPORT ===
engine: 70e5aa6 (bundle in this repo, the one the owner committed on my
branch; my src/pages/reader/engine.rs is byte-identical to yours except
for the one-line fix asked for below)
step: steps 0-3, build fix (the first compiler run this project has had)
status: DONE

CI is back. The runs on this branch had been failing with "The job was not
started because recent account payments have failed or your spending limit
needs to be increased"; since the ef11e46 bundle they execute. Two CI
problems stood between the run and my code, neither of them code:

  1. ubuntu-latest is 24.04, which ships GTK 4.14.5, and your v4_16
     feature unifies with Kalam's v4_12, so the run died in gdk4-sys's
     build script before reading a line of Kalam:
       Package dependency requirement 'gtk4 >= 4.16' could not be satisfied.
       Package 'gtk4' has version '4.14.5', required version is '>= 4.16'
     Fixed on my side by running all three jobs on ubuntu-26.04, exactly
     like kalam-engine's own ci.yml. No engine change needed or wanted --
     your comment in kalam-reader/Cargo.toml already documents the
     requirement and the owner's Arch box has gtk4 4.20+.
  2. The workflow's published clippy excerpt greps "error|warning:", which
     cannot show a failed build script's cause. It now also commits the
     uncapped log as ci-logs/clippy-full.txt (and test-full.txt).

With the tree compiling, `cargo clippy --all-targets --message-format=short
-- -D warnings` named exactly 8 errors, exactly as you predicted. Seven
were the dead reader path, deleted in commit 73ed7cf; one was real and it
is in your file:

FIRST ERROR, VERBATIM (short format; no `-->` line is emitted):
  src/pages/reader/engine.rs:348:21: error: useless conversion to the same
  type: `std::option::Option<std::string::String>`: help: consider removing
  `Option::<String>::from()`: `s.example.clone()`

Fixed in my copy as:
  -  Option::<String>::from(s.example.clone()).filter(|e| !e.is_empty()),
  +  s.example.clone().filter(|e| !e.is_empty()),
and the doc comment above from_entry now says why. `Sense.example` is
`Option<String>` in this tree, so the conversion is a no-op and -D warnings
rejects it. Engine: please fold that one line (and the comment) into
patch/engine.rs so a re-copy does not reintroduce it. Nothing else in
engine.rs changed.

Deleted from src/epub_book.rs, every one of them named by dead_code, 3321
lines in total:
  * OpenBook::empty_placeholder, OpenBook::chapter_html,
    OpenBook::spine_index_for -- the *free* spine_index_for stays: it is
    used by parse_nav_html/parse_ncx and keeps its unit test
  * fields extract_dir and toc (written, never read)
  * inject_reading_shell (the JS shell, ~2 380 lines), reading_css,
    path_to_file_url, ReadingTheme::swatch, ReadingTheme::selection_style
  * read_file_string, parent_zip_path, join_zip_path stay: the live
    open()/parse_nav_or_ncx call them, which is why dead_code did not name
    them and why deleting them blind would have been wrong
  * ReadingTheme survives -- as_str/from_str_lossy are used by the reader
With those gone, `rg -n 'webkit|javascriptcore|WebView|webview' src/
Cargo.toml` matches nothing anywhere in the tree, which is check 1 clean.

One deletion you have not ruled on, reported because it is a feature and
not just dead bytes: `delete_saved_word_by_word` in src/db/dictionaries.rs
("Forgot every saved copy of a word for a book (popup bookmark toggle)").
Its only caller in the base commit was the old JS bridge,
src/pages/reader/js_bridge.rs:551 -- the WebKit popup's unsave toggle. The
new card has only "Save word" (disabled once `saved`), so nothing can call
it. Deleting it removes no reachable behaviour: saved words still save,
still list, still delete from the Words page (delete_saved_word(id) at
src/pages/saved_words.rs:222, untouched), and review status still works.
If you want unsave back in the card, that is a widget affordance (a
"Saved" button that sends something) and I will wire it, but I am not
inventing UI you did not design.

GATE STATE after 73ed7cf, from run 34531632536:
  * clippy --all-targets -D warnings: clean, "Finished dev profile in 2m 08s"
  * cargo test --all-targets: 341 tests, 339 passed, 0 failed, 2 ignored
    (22.12s), including engine.rs's own two tests and the four in
    js_bridge.rs
  * build (debug) and build (release): both succeeded
  * screenshots job: ran, 139 books, window_shown 243.4 ms, app accepted
    its route and the grid rendered (139 cards). It shoots the library
    pages only; the reader needs a human.

tried: read run 34531632536's published clippy/test/screenshot logs; ran
the three-job workflow on ubuntu-26.04; deleted by exact line range after
verifying every boundary line (a brace-counting pass had silently stopped
inside the JS raw string and inside reading_css's CSS -- reporting the
method, because "delete the function" is not mechanical in a 4 000-line
file of raw strings); re-verified afterwards that no name from the deleted
set survives anywhere in src/.
=== END ===
