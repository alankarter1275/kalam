# Enabling GitHub Actions for Kalam

The Arena coding agent **cannot push** files under `.github/workflows/` unless the
GitHub App has the `workflows` permission. The workflow body lives here instead:

**Canonical file:** [`github-actions-ci.yml`](./github-actions-ci.yml)

> **Sync note (2026-09-18).** The agent edits the *real*
> `.github/workflows/ci.yml` directly, because that is the file that actually
> runs. This copy exists so that *you* can apply a change with your own
> account when a push of a workflow file gets rejected. It drifts, and it had
> drifted 134 lines before being re-synced today.
>
> **After any workflow change, re-sync it:**
>
> ```bash
> cp .github/workflows/ci.yml docs/ci/github-actions-ci.yml
> ```
>
> If you would rather not carry the duplicate at all, say so and it can be
> deleted — the cost is losing the manual-install fallback described at the
> bottom of this file.

## Slimmed 2026-09-18 — three jobs became two

The `scale` job (2,000 books) was removed and the `screenshots` job was cut to
a single run, now labelled *smoke test*. Reasons and the retired numbers are in
[`README-screenshots.md`](./README-screenshots.md) under "Slimmed on
2026-09-18". `build` is unchanged and is still the only job that can fail a
run.

## ACTION NEEDED (2026-09-04, third attempt) — rustfmt must stop pushing

Sorry, one more copy of the workflow. My previous fix was the wrong shape.

**What happened.** The rustfmt step auto-committed the formatting fix and
pushed it. That push kept being rejected, and because the step had no
`continue-on-error`, **a rejected push failed the entire build** — four runs in
a row, none of them caused by the code. Adding a rebase (attempt two) did not
help, so the collision was not the only problem, and I could not see the real
error: the sandbox cannot download Actions logs.

**The fix is to stop pushing from that step at all.** Formatting is not worth
failing a build over, and a step whose failure mode is unrelated to what it
checks sends every investigation to the wrong place. It now:

- formats, and **reports** the diff instead of committing it
- restores the tree afterwards, so clippy and the build see the real source
- never fails the run
- publishes the diff to `ci-logs/rustfmt-latest.diff` in a **separate** step
  that is `continue-on-error`, so even a failed publish cannot break anything

The agent then applies the formatting itself in the next commit, which is
honest anyway: formatting belongs in the change that caused it, not in a
drive-by commit from CI.

Copy [`github-actions-ci.yml`](./github-actions-ci.yml) over
`.github/workflows/ci.yml` and push.

---

## ACTION NEEDED (2026-09-04) — small follow-up to the rustfmt step

Low priority; nothing is broken. The report-only rustfmt step works, but on a
**clean** run it deletes `ci-logs/rustfmt-latest.diff` locally and never
commits the deletion — so the stale diff from an earlier untidy run stays in
the repository and looks like an outstanding complaint. It already fooled me
once.

Copy [`github-actions-ci.yml`](./github-actions-ci.yml) over
`.github/workflows/ci.yml` when convenient. A clean run now writes
`clean` plus the run id instead of deleting the file, so the published diff
always describes the latest run.

---

## ACTION NEEDED (2026-09-04) — publish the CI logs on success too

Low priority; nothing is broken, but the current behaviour is actively
misleading.

`ci-logs/test-latest.txt` and `ci-logs/clippy-latest.txt` are only written when
that step **fails**. So after a failure is fixed, the old failing log stays
committed and a later green run still shows red. It fooled me three separate
times — most recently reading "1 failed" from a run two hours dead while the
current run was green.

Copy [`github-actions-ci.yml`](./github-actions-ci.yml) over
`.github/workflows/ci.yml`. Both steps now run on success as well, so the
published file always describes the latest run. Same fix as the rustfmt diff.

---

## Retired 2026-09-18: windowed-grid measurement (was applied 2026-09-04)

**This job no longer exists in the workflow.** Kept here as the record of what
it measured and how, because the numbers are still the ones the grid decisions
rest on.

The `scale` job ran the 2,000-book library twice, once with each grid, and
published a side-by-side summary to `ci-logs/scale-2000-comparison.txt`. Both
runs happened in the same job on the same machine on purpose: comparing
against a number from a previous run would be comparing two different rented
VMs, which is what made timing-based tests useless (see the A0 step 7 entry in
the roadmap).

**Result, and the reason it was retired:** windowed 247 MB / 7.1 ms, old
build-every-card grid 352 MB / 139.7 ms. The question was answered on
2026-09-04 and re-answering it on every push produced no new information. The
committed logs are frozen at their last run. If the grid changes, restore the
job from git history rather than extrapolating again.

The one subtlety worth keeping if it comes back: the version installed passed
`WINDOWED=1` on one run and nothing on the other, and the windowed grid later
became the default — so "nothing" would now mean *windowed* too, and both runs
would measure the same thing and report a green, meaningless comparison.
Rather than ask for another manual install, `screenshot.sh` infers the baseline
from the output directory: a run writing to a plain `ci-shots-*` path is the
old grid unless told otherwise. An explicit `WINDOWED=` still wins.

---

## Done: `Cargo.lock` step (applied 2026-09-04)

The lockfile step is installed and has run — `Cargo.lock` is committed and the
dependency graph is pinned. Nothing to do here; kept as a record.

## Working agreement (manual CI handoff)

The Arena agent **cannot** create or update `.github/workflows/*` (GitHub App
has no `workflows` permission, and that cannot be toggled from your side).

So we do this forever:

| Situation | Who | What |
|-----------|-----|------|
| Need a workflow change | Agent | Writes the full file under `docs/ci/` and gives you paste/copy steps |
| Apply workflow change | **You** | Copy into `.github/workflows/ci.yml` (CLI or GitHub UI) and push |
| CI fails | Agent | Says which run/step failed |
| Share the failure | **You** | Paste the failed step log (or the “Diff in …” / rustc error block) |
| Fix code | Agent | Pushes code fixes (not workflow files) |

You already enabled CI once (Option C). Good — leave it.

> **Why the CLI path needs a special token:** GitHub refuses pushes that touch
> `.github/workflows/*` unless the Personal Access Token has the **`workflow`**
> scope (fine-grained tokens need **Workflows: Read and write**). Without it
> you get:
> `! [remote rejected] ... refusing to allow a Personal Access Token to create
> or update workflow .github/workflows/ci.yml without 'workflow' scope`.
> The commit is still created **locally**, but the push fails.
>
> Two fixes: (a) give your token the `workflow` scope (Settings → Developer
> settings → Personal access tokens → regenerate with `workflow` checked), or
> (b) use the GitHub web UI path below — no token involved.
>
> If a local commit was created but the push was rejected, and you then applied
> the same file via the web UI, the local commit is a duplicate: discard it with
> `git reset --hard origin/<branch>` (check first with
> `git log origin/<branch>..HEAD --oneline`).

### If the agent asks you to update the workflow

```bash
cd /path/to/kalam
git fetch origin
git checkout arena/01a0b2ee-kalam
git pull
cp docs/ci/github-actions-ci.yml .github/workflows/ci.yml
git add .github/workflows/ci.yml
git commit -m "ci: update workflow"
git push origin arena/01a0b2ee-kalam
```

Or GitHub UI: edit `.github/workflows/ci.yml` and paste the contents of
`docs/ci/github-actions-ci.yml`.

> Note: if the branch name is not `arena/01a0b2ee-kalam`, run
> `git branch --show-current` and substitute it.

## What CI does

On every push / PR, the **`build`** job — the gate — on `ubuntu-26.04` (not
`-latest`; see the comment in the workflow for the GTK 4.16 reason):

1. Install `libgtk-4-dev` + `libadwaita-1-dev` and friends
2. Generate and push `Cargo.lock` if missing or stale
3. `cargo fmt --check` — **report only**; the diff goes to
   `ci-logs/rustfmt-latest.diff` and never fails the run
4. `cargo clippy --all-targets` with `-D warnings` — publishes to
   `ci-logs/clippy-latest.txt`, then a separate step fails the run if it found
   anything
5. `cargo test --workspace --all-targets` — compiles **and runs** the unit
   tests (in-memory SQLite, headless), failures to `ci-logs/test-latest.txt`
6. `cargo build` and `cargo build --release`

The **`screenshots`** job then runs as a `continue-on-error` smoke test: one
headless-sway run that opens a book and commits a text report to
`ci-logs/reader-latest.txt`. It is a diagnostic and can never block a change.

Visual judgement still stays on your Arch box at phase end. A green smoke test
means "it launched and did not panic", not "it looks right".

> **`--workspace` matters.** Without it cargo builds only the root package, so
> the eleven engine and tool crates never compile and ~420 tests never run.
> Fixed 2026-09-18 in both the workflow and the `Makefile`; it is not yet
> confirmed green by an actual run.

## After it’s enabled

You don’t need to do anything else for CI. The agent watches failures, fixes
them, and only asks you to run the app when a **phase** is done.
