# Continuous integration

The workflow that runs is [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml).
This document describes that file; it is the only maintained copy of the
workflow.

## Jobs

| Job | Required for success? | Purpose |
|---|---:|---|
| `build` | Yes | Format report, Clippy, workspace tests, release build and OCR diagnostic. |
| `screenshots` | No | Launch the release binary under headless Sway and report reader, tasks and editor routes. |

Both jobs run on Ubuntu 26.04. The GTK4 bindings require GTK 4.16 or newer.

## Build job

The build job installs GTK4, libadwaita, GtkSourceView and the native build
dependencies, then installs Rust stable. It also generates `Cargo.lock` when
needed and downloads the OCR model files used by the app.

The checks are:

1. **rustfmt:** runs `cargo fmt --all` and publishes the resulting diff. It is
   report-only; this step does not fail the workflow.
2. **Clippy:** runs `cargo clippy --workspace --all-targets --message-format=short
   -- -D warnings`. A separate step fails the job if Clippy fails.
3. **Tests:** runs `cargo test --workspace --all-targets`. A separate step
   fails the job if tests fail.
4. **Release build:** runs `cargo build --release`.
5. **OCR probe:** runs the ignored OCR scan probe as a diagnostic and publishes
   its output. A probe failure is returned as a job failure.

`--workspace` is required. The workspace contains 11 members, and the root
package is not the default for all of them. `--all-targets` selects target
kinds; it does not select workspace packages.

The release binary is uploaded as the `kalam-release-binary` artifact for one
day. The smoke-test job downloads that binary rather than building it again.

## GUI smoke job

The `screenshots` job depends on a successful build and uses
`continue-on-error: true`. A compositor or rendering problem therefore does
not block a correct code change. It runs `docs/ci/screenshot.sh` three times
under headless Sway with 139 seeded books:

- `ROUTE=read-1` opens the first seeded EPUB.
- `ROUTE=tasks` opens the task manager.
- `ROUTE=editor` opens the EPUB editor picker.

The seeder creates valid EPUB files in folders named with the same readable
name and short id as the app. The seed database begins at schema version 1 and
is migrated by the app on open. The reader route is not counted as successful
if the app logs `open epub failed`.

Each run captures the requested page more than once, then launches Home for a
comparison image. Identical captures of the same settled page are expected.
A route-accepted log line proves only that the route was parsed; it does not
prove that the reader opened its book or that the UI is visually correct.

Screenshots and run logs are kept only in the Actions job workspace. The
workflow does not upload PNG screenshots. It commits the text summaries to
`ci-logs/reader-latest.txt`, `ci-logs/tasks-latest.txt` and
`ci-logs/editor-latest.txt`, including on successful runs. The build job also
publishes rustfmt, Clippy, test and OCR-probe output under `ci-logs/`.

The summaries are evidence to inspect, not a visual approval. See
[`README-screenshots.md`](README-screenshots.md) for their limits and local
reproduction steps.
