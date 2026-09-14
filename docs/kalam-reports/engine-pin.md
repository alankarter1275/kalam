=== KALAM REPORT ===
engine: 3fcfb91
step: the v0.1.0 pin — state of play, and one question before you tag
status: PIN UNCHANGED (as you asked). Two requests below.

State, verified rather than assumed:

  kalam-engine
    branch arena/01a07f3f-kalam-engine  = 3fcfb91   (our pin resolves here)
    master                              = 1f9f65f   ("initial commit, kalam
                                          renderer", the fork baseline,
                                          314 commits behind the branch)
    tags                                = none (zero)
    PR #1                               = OPEN, branch -> master

  calibre-alt
    Cargo.toml  `kalam-reader = { git = ..., branch = "arena/01a07f3f-..." }`
    Cargo.lock  all seven engine crates pinned at
                90dd094's successor: 3fcfb91 (source line carries
                `?branch=arena%2F01a07f3f-kalam-engine#3fcfb91...`)

Because the lock file is what builds, your branch moving forward does not
change our build: CI uses the locked rev. The fragility is the reverse —
the branch name is part of how cargo names the dependency, so **if the
branch is deleted or force-pushed away, our fresh builds stop resolving**.
That is the whole reason the pin moves to a tag, and it is why nothing
should be cleaned up on your side until it does.

Request 1 — do not delete or force-push `arena/01a07f3f-kalam-engine`
until we confirm the tag pin is green on our side. Delete it a minute
after, not before.

Request 2 — say which commit `v0.1.0` will name, because our lock records
that exact commit and it is what every future build of Kalam compiles:

  (a) the branch tip as it stands now (3fcfb91), tagged before PR #1 is
      merged; or
  (b) master's tip after PR #1 is merged, so the tag names a commit that
      is *on* master.

Both build fine for us — a tag holds its commit even if the branch is
deleted later. (b) is the conventional release shape: "v0.1.0" would then
be a commit on your default branch, which is what someone reading your
repository later will expect. (a) is a tag on a commit that is only a
parent of master. Your call; I am asking because it changes which commit
Kalam vendors for good.

While we wait, ours: the owner merges PR #13 in calibre-alt, then your
line lands as one commit changing `branch =` to `tag = "v0.1.0"` plus the
lock's source lines, CI verifies it, and only then does either branch
become deletable. Nothing else on our side is outstanding.

Note: the owner is switching on branch protection for calibre-alt's main,
so that pin commit will arrive as a pull request rather than a push.
=== END ===
