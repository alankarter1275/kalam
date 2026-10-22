# Agent workflow

- Read [`ROADMAP.md`](ROADMAP.md), the relevant source files and tests before
  making product or architecture changes. The app source is authoritative;
  correct stale documentation rather than repeating a claim that the code no
  longer supports.
- For substantial or user-visible work, write a focused plan, identify the
  affected code and tests, and resolve important behavior questions before
  implementation. KAL-5's EPUB content editor is deferred and removed; do not
  implement or mock up a replacement unless the owner explicitly reopens it.
- Keep active guidance in current documents. Clearly label archived research,
  issue logs and conversation records as historical; do not present them as
  current implementation status.
- Keep GTK objects on the GTK thread and use the task/message pattern for
  blocking filesystem, database, parsing and OCR work. Follow the current
  constraints in [`ARCH.md`](ARCH.md) and [`docs/WORKING.md`](docs/WORKING.md).
- Avoid production `unwrap()` and `expect()`. The current guardrail is a
  regression ratchet, not a claim that production code has none.
- When Rust is available, run focused tests first, then the workspace checks:
  `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, and
  `cargo test --workspace --all-targets`.
- Do not add temporary artifacts to the repository root. Keep generated
  screenshots and build outputs in their established output directories.
- Use plain, concise language in code comments and documentation. Keep UI
  wording consistent with the app's existing terminology.
