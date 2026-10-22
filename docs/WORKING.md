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
