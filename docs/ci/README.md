# Kalam's CI

The agent edits `.github/workflows/ci.yml` directly and pushes it. No manual
install step, no duplicate copy to keep in sync.

> **This used to be a manual handoff, and that is now removed (2026-09-18).**
> This file used to carry a second copy of the workflow at
> `docs/ci/github-actions-ci.yml`, which you had to copy over
> `.github/workflows/ci.yml` and push yourself, because the agent's token
> lacked GitHub's `workflows` permission. You enabled that permission, so the
> agent pushes workflow files directly now — proven by commit `a4c4d54` and run
> `35362407132`. The copy was deleted; it had already drifted 134 lines from
> the file that actually runs, which is what a duplicate you have to remember
> to re-sync eventually does.
>
> If a workflow push is ever refused again, the fix is to re-grant **Workflows:
> Read and write** on the GitHub App, not to reinstate the copy.

## What CI does

Two jobs. **`build` is the only one that can fail a run.**

### `build` — the gate

On `ubuntu-26.04`, not `-latest`: `kalam-reader` asks gtk4 for the `v4_16`
feature, and 24.04 ships 4.14.5. The workflow comment has the full story.

1. Install `libgtk-4-dev`, `libadwaita-1-dev` and friends
2. Generate and push `Cargo.lock` if it is missing or stale
3. `cargo fmt --all` — **report only.** Publishes the diff to
   `ci-logs/rustfmt-latest.diff` and restores the tree. Never fails the run
4. `cargo clippy --workspace --all-targets` with `-D warnings`, publishing to
   `ci-logs/clippy-latest.txt`; a separate step then fails the run if it found
   anything
5. `cargo test --workspace --all-targets` — compiles **and runs** the tests,
   publishing to `ci-logs/test-latest.txt`
6. `cargo build` and `cargo build --release`

**`--workspace` is doing real work in steps 4 and 5.** Without it Cargo selects
only the root `kalam` package, because the workspace has 11 members and no
`default-members` key — `--all-targets` picks *targets*, not *packages*. Before
that flag was added on 2026-09-18, one test binary ran. After it: 52 binaries,
751 tests, and four rounds of failures that had been sitting undetected.

### `screenshots` — labelled *smoke test*, and not a gate

`continue-on-error: true`, and it `needs: build`, so it never runs on a broken
tree and can never block a correct change. One headless-sway run that opens a
book on the `read-1` route and commits a text report to
`ci-logs/reader-latest.txt`.

A green smoke test means "it launched and did not panic", not "it looks right".
Visual judgement still happens on your Arch box at phase end. Read
[`README-screenshots.md`](./README-screenshots.md) before trusting any of it.

### Why the logs get committed back

The Arena sandbox cannot download Actions logs or artifacts — both hosts are
unreachable from it. So each step publishes its output into `ci-logs/` and
commits it to the branch, **on success as well as failure**. Publishing only on
failure leaves the previous failure committed, so a later green run still reads
as red; that misreading happened three separate times before it was fixed.

`ci-logs/` is therefore the agent's only window into a build. Do not clean it up.

## Design decisions worth keeping

**rustfmt reports, it never pushes.** It used to auto-commit its fix, and a
rejected push failed four runs in a row for a reason unrelated to the code —
which sends every investigation to the wrong place. Formatting now belongs to
whoever caused it, in the same commit.

**Clippy and the test step are `continue-on-error` with a separate fail step.**
Not to soften them — they still fail the run. It is so the *publish* step runs
first. A step that fails immediately would skip publishing the very log needed
to explain the failure.

**The toolchain is `stable`, unpinned.** This is a real trade-off: a new Rust
release that adds a lint can turn the run red without anyone changing the code,
and that is exactly what happened on 2026-09-18 (8 findings, all new lints).
The alternative is pinning a version and choosing when to update it. Not
decided; the current behaviour is "find out on push".

## Retired: the `scale` job (2026-09-04 → 2026-09-18)

Kept as the record, because the grid decisions still rest on these numbers.

The job ran a 2,000-book library twice, once with each grid, and published a
side-by-side summary to `ci-logs/scale-2000-comparison.txt`. Both runs happened
in the same job on the same machine on purpose: comparing against a number from
a previous run means comparing two different rented VMs, which is what made
timing-based tests useless (see A0 step 7 in the roadmap).

**Result:** windowed grid 247 MB / 7.1 ms, old build-every-card grid 352 MB /
139.7 ms. The question was answered on 2026-09-04, and re-answering it on every
push produced nothing new. The committed logs are frozen at their last run.
If the grid ever changes, restore the job from git history rather than
extrapolating.

One subtlety, if it comes back: the installed version passed `WINDOWED=1` on
one run and nothing on the other, and the windowed grid later became the
default — so "nothing" would now mean *windowed* too, and both runs would
measure the same thing and report a green, meaningless comparison. Rather than
ask for a manual install, `screenshot.sh` infers the baseline from the output
directory: a run writing to a plain `ci-shots-*` path is the old grid unless
told otherwise. An explicit `WINDOWED=` still wins.

## Retired: the `Cargo.lock` step note (applied 2026-09-04)

The step is installed and has run; `Cargo.lock` is committed and the dependency
graph is pinned. Nothing to do.
