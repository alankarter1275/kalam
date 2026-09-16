=== KALAM REPORT ===
engine: v0.1.0 -> 1b20f3558788abb4ff99611fcaba34b11c443228
step: your request — confirm CI green on the pin commit, PR #13 mergeable as-is
status: DONE

Answer: **DONE.** Verified rather than assumed, item by item.

The pin, as it stands on the branch (the owner's own commit `9e9f31c` "pin
kalam-reader to tag v0.1.0"; I did not touch either file, as instructed):

    Cargo.toml:31
      kalam-reader = { git = ".../kalam-engine", tag = "v0.1.0" }

    Cargo.lock, all seven engine crates:
      .../kalam-engine?tag=v0.1.0#1b20f3558788abb4ff99611fcaba34b11c443228

and the tag really is that commit:
`git ls-remote --tags .../kalam-engine` -> `v0.1.0 ->
1b20f3558788abb4ff99611fcaba34b11c443228`. Name and hash agree with the
lock, so a fresh `cargo build` resolves the tag, not a branch.

CI on `9e9f31c`, the pin commit, both events, all three jobs **success**:

    build (ubuntu)               success
    screenshots (headless sway)  success
    scale check (2000 books)     success

with 343 passed / 0 failed / 2 ignored, clippy clean, and the reader pass
opening the book in 4.5 ms on clean stderr. The build job is the one that
matters most here: it clones the repository fresh and therefore had to
resolve `tag = "v0.1.0"` over the network and compile the engine at
1b20f355 — the path that could not be tested until the tag existed. It
passed.

PR #13 — READY TO MERGE AS-IS.

    OPEN, mergeable = MERGEABLE, status = CLEAN
    arena/01a08cfb-calibre-alt -> main, 63 files, +6292 / -5727

The three tagged engine changes you list (7574797 images in inline
wrappers, a43eb8f monochrome plates on Dark and Sepia, 1b20f35 rustfmt)
are inside 1b20f355 and therefore inside what this branch compiles. None
is host-visible, so Kalam needed nothing for them; the plate decision
follows the palette Kalam already passes, and scrolled-mode's grid fix
from a745d5e is a feel-it-in-hand thing for the owner.

Understood on the bundle staying at a745d5e and being regenerated in a new
session: I have not touched `docs/engine-handoff/`.

For the record, since this is the last report before the merge: the
dictionary card remains the one open item, parked by the owner ("we will
see to it later") — it is built to the mockup's numbers, compiles, and has
never been signed off. It is the first thing for the new session.

Merge instructions for the owner are in the PR body: **create a merge
commit** (no squash, no rebase), and the two pre-migration paths
(`src/webview_pool.rs`, `docs/engine-handoff/patch/engine.rs`) stay
deleted. Nothing in this report asks for anything further from you.
=== END ===
