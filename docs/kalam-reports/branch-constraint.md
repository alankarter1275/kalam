# The branch constraint (owner's instruction, 2026-09-14)

**Arena gives this chat exactly one branch: `arena/01a08cfb-calibre-alt`.**
It cannot create another one. So this branch is not disposable, and the
engine's earlier line — "after step 4, tell the owner the branch can be
deleted" — is superseded for our side: **it must not be deleted.**

What that means in practice:

* Do not delete the branch, locally or on the remote. Do not delete it
  from the pull-request page after a merge either (GitHub offers a
  "Delete branch" button there even when auto-delete is off).
* Do not force-push it away, and do not rename it.
* Every later change — the `v0.1.0` pin line, anything the owner's card
  review turns up — is committed **on this branch** and reaches `main`
  through a **fresh pull request from the same branch**. GitHub allows a
  new PR from a head branch whose previous PR is already merged; no new
  branch is needed.
* Pushing to this branch itself is fine and expected. `main` is the branch
  that is protected and PR-only.

## Checked, and it matters

`gh api repos/alankarter1275/calibre-alt` reports
`delete_branch_on_merge=false`, so merging PR #13 will **not** delete the
head branch automatically. If that setting ever changes, or the branch is
deleted by hand, the chat loses its only branch and the pin change has
nowhere to land.

## The sequence from here, using only this branch

1. Owner merges PR #13 (`Create a merge commit`; not squash, not rebase).
2. Merge `main` back into this branch, so the next pull request is clean.
3. Engine merges PR #1, tags `v0.1.0`, and sends the line. Commit it here:
   `Cargo.toml`'s `branch = "arena/01a07f3f-kalam-engine"` becomes
   `tag = "v0.1.0"`, plus the lock's source lines.
4. Open a second pull request from this branch to `main`; the owner merges
   it; CI must be green before that (the tag has to resolve for a fresh
   clone).
5. The branch stays. It is the working surface for this chat until the
   chat ends.

The engine's branch `arena/01a07f3f-kalam-engine` may be deleted after
step 4 is green — but the owner should keep it too if in doubt: their
branch name is part of how our `Cargo.toml` currently names the
dependency, and only the tag pin makes it safe to remove.
