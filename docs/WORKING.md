# Working rules

This file records current development boundaries and the checks that enforce
them. The application source and tests are authoritative; update this guide
when they change.

## Data and sidecars

- SQLite is the source of truth for catalog and reading state.
- A book's `kalam.json` is a portable backup snapshot, not a live database.
  Reader progress is written to SQLite; page turns do not refresh the sidecar.
  Metadata, annotation and bookmark changes refresh it. Legacy EPUB patch
  fields are preserved for older data but are not active edit state.
- Keep database migrations and persistence changes covered by tests. Do not
  describe the sidecar as synchronizing after every database update.

## UI and work

- Keep GTK objects on the GTK thread. Do not move widgets or widget references
  into background work.
- Use `src/tasks.rs` for work that may block the interface, such as catalog
  queries, filesystem operations, EPUB processing, imports and OCR. Send owned
  results back to the UI through messages.
- Pages should request data through `LibraryService` snapshots rather than
  acquiring the catalog directly. This is a ratcheted boundary: the current
  code still has exceptions, and the guardrail test prevents the count from
  growing.
- Keep worker results independent of GTK types. A worker may use the catalog,
  file paths and parsing libraries, but must not manipulate widgets.

## Error handling and tests

- Avoid production `unwrap()` and `expect()`. A guardrail test tracks the
  existing production sites and prevents them from increasing; the current
  code has not yet reached zero.
- Do not weaken or raise a guardrail budget to accommodate new violations.
  When removing violations, lower the corresponding limit.
- Add focused tests for changed persistence, parsing and compatibility behavior. Use
  `cargo test --workspace --all-targets` for workspace coverage and
  `cargo clippy --workspace --all-targets -- -D warnings` for linting when a
  Rust toolchain is available.

## Repository hygiene

- Keep temporary files, build outputs and generated reports out of the source
  tree; use `target/` for build output and the established locations under
  `docs/` for maintained documentation.
- Do not add a new dependency, abstraction, environment switch or page without
  explaining why it is needed and how it fits the existing design.
- Update active documentation when user-visible behavior or architecture
  changes. Keep archival notes clearly identified as historical.

## Guardrails

`tests/guardrails.rs` contains count-based limits for production panic sites,
`Arc<Catalog>` use under `src/pages/`, and color literals in the stylesheet.
They are regression ratchets, not proof that the desired architecture is
fully implemented. `src/perf.rs` holds query-count checks; the stall monitor
is implemented in `src/stall.rs`.

## Reader boundary

`crates/kalam-reader` supplies the GTK EPUB reader widget used by the app.
Kalam connects its reader events to app features. Dictionary lookup is driven
by explicit selection/actions; tap callbacks are used for Word Memory terms,
not for generic tap-to-dictionary lookup. Keep engine changes aligned with the
current public API in `crates/kalam-reader/README.md`.

## Style conventions

Recorded on 31 October 2026 after reading Google's Rust style guide (published
21 October 2026, `google.github.io/styleguide/rustguide.html`) against this
codebase. The guide is written for codebases with thousands of contributors and
says so: its stated extra goal is smoothing Large-Scale Changes — codebase-wide
refactors run by tooling — and its "Apply API design rules in context" section
makes API rules binding for libraries but only a recommendation for applications,
which it tells to favour simplicity and directness over reusability. kalam is an
application; there is no `src/lib.rs`. So most of the guide does not transfer,
and what does is recorded here rather than adopted wholesale. The guide's own
goal — "style rules should pull their weight" for every reviewer — argues for
that restraint when there is one reviewer.

Adopted:

- **Failure reporting.** `Option` where the cause of a failure is obvious,
  `Result` where the caller needs the reason, `panic!` only for a violated
  invariant that means the code itself is wrong. Tests may panic freely, and the
  guide's exception for developer-facing binaries does not apply here: this is
  the app a reader opens. Already the practice — see "Error handling and tests"
  above — and written down so it survives a contributor who has not inferred it,
  human or otherwise.
  The ratchet is exactly tight, which is worth knowing before adding anything:
  `MAX_PROD_PANICS` is 8 and the measured count is 8 (two in `src/timing.rs`,
  two in `src/pages/pdf_reader.rs`, one each in `src/pages/shelf_editor.rs`,
  `src/pages/pdf_reader/drawings.rs`, `src/pages/comics_reader/mod.rs` and
  `src/pages/browse.rs`). There is no slack, so one new production `unwrap()`
  fails CI. Measured by replicating `count_prod_panics` rather than by searching
  the tree: a plain search over-counts, because prose that mentions `.unwrap()`
  is skipped by the scanner and not by a grep. An earlier count of four was three
  comment lines and one call.
- **Accessor naming.** `foo()` for a getter returning `&T`, or `T` when `T:
  Copy`; `foo_mut()` for `&mut T`; `set_foo()` to write; `take_foo()` when
  reading clears the value; `replace_foo()` when writing returns the old one.
  Implement the expected set together: a live `set_foo` with no `foo` invites
  callers to cache the value and drift out of sync. This is the standing reason
  `Dropdown::selected` is kept under a narrow `#[allow(dead_code)]`.
- **Lend locked state; do not hand out the key.** Where a value sits behind a
  `Mutex` or `RefCell`, new code should prefer an accessor taking a closure that
  runs while the lock is held, so the borrow's scope is explicit and ends where
  it looks like it ends. `Catalog::conn()` returns the guard itself and is called
  about 198 times; those sites are not being rewritten, but holding a guard
  across a call that locks again is how this app freezes, and the closure form
  makes the hazard visible at the call site.

Deliberately not adopted, with the reason, so the question is not re-litigated:

- **Flat file layout and one-file crates.** Tagged `[chrome]` in the guide — a
  Chromium rule about parallel compile times, not a Google-wide one. This
  project's problem is the reverse: `src/pages/epub_editor.rs` is 14,355 lines,
  and directories are how it copes.
- **Avoid `Rc` and `Arc`.** GTK4's object model is reference counted and the Rust
  bindings hand out shared handles by design, so the 252 `Rc` and 119 `Arc` sites
  are the toolkit's shape rather than a habit carried over from another language,
  which is the failure mode the rule targets. The guide's own escape clause covers
  it: allowed where avoiding shared ownership would make the structure
  unreasonably complex. `Arc<Catalog>` under `src/pages/` is separately ratcheted
  in `tests/guardrails.rs`.
- **No glob imports.** 74 of the globs here are `use gtk::prelude::*;`, the
  documented gtk-rs idiom, and 83 more are `use super::*;` inside test modules.
  The 17 in production code are the modules split out of `db.rs`, whose header
  records that they were moved verbatim; naming those imports belongs to finishing
  that split, not to a style rule.
- **`#[non_exhaustive]`, `#[must_use]`, and the library API rules.** These exist
  for downstream consumers. There are none: this is one person's reading app, not
  published anywhere, and the seven workspace crates have no external users.
