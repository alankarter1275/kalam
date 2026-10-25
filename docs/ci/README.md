# Continuous integration

The workflow that runs is [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml).
This document describes that file; it is the only maintained copy of the
workflow.

## Jobs

| Job | Required for success? | Purpose |
|---|---:|---|
| `build` | Yes | Format report, Clippy, workspace tests, release build and OCR diagnostic. |
| `screenshots` | No | Launch the release binary under headless Sway in six parallel groups and capture the reader, comics, PDF, page and Settings proof frames. |
| `publish-screenshots` | No | Merge the screenshot groups' short-lived artifacts and commit their reports and settled redesign frames to `ci-logs/`. |

All jobs run on Ubuntu 26.04. The GTK4 bindings require GTK 4.16 or newer.

## Build job

The build job installs GTK4, libadwaita and the native build
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
not block a correct code change. Its 74 captures run as six parallel
matrix groups — EPUB, comics, PDF, general pages, and two halves of
Settings — rather than as one serial list. Each group uploads its
`report.txt` files and settled PNGs as a one-day internal artifact;
`publish-screenshots` then merges those artifacts and performs the
single `ci-logs/` commit, so parallel shards never race to push.

The workflow remains the source of truth for the exact route/action
list. In broad terms, the EPUB group uses the 139-book seed, the comics
and PDF groups use their 20-book fixtures, the pages group covers
tasks/components/book/series/author, and the Settings groups cover all
23 pane and notification states.

The seeder creates valid EPUB files in folders named with the same readable
name and short id as the app. The seed database begins at schema version 1 and
is migrated by the app on open. The reader route is not counted as successful
if the app logs `open epub failed`.

Each run waits for both the app's route line and a real compositor
window, applies a short route-dependent minimum dwell, and then looks
for two coarsely identical frames. `SETTLE` is the upper readiness
budget, not a fixed sleep; if no stable pair arrives in time, the
latest frame is retained and the report says so. Home receives the
same readiness treatment before it is used as the comparison negative.
A route-accepted log line proves only that the route was parsed; it does not
prove that the reader opened its book or that the UI is visually correct.

Screenshot groups upload only their reports and settled PNGs as
short-lived artifacts for the publish job to merge; seeded config
homes, compositor logs and intermediate candidate frames stay on the
shard. The workflow commits the text summaries
to `ci-logs/*-latest.txt`, including on successful runs. The settled
frames of the redesigned surfaces are the exception to the no-PNG rule:
the component gallery, the book page and its dialogs, the series page,
the author page and its dialogs, and the 23 Settings/notification
captures are committed as `ci-logs/*.png`, because a redesign nobody can
look at is not verified and the agent cannot download Actions artifacts
from its sandbox. See [`README-screenshots.md`](README-screenshots.md)
for the full list. The build job also
publishes rustfmt, Clippy, test and OCR-probe output under `ci-logs/`.

The summaries are evidence to inspect, not a visual approval. See
[`README-screenshots.md`](README-screenshots.md) for their limits and local
reproduction steps.
