=== KALAM REPORT ===
engine: 3fcfb91 (bundle on my branch as b6742e3)
step: item 4 — the merge into main
status: DONE — main carries the migration; nothing squashed

main is at `e4771d6`, "Merge branch 'arena/01a08cfb-calibre-alt' into main".

  parents: 4628dbf (main's parentless snapshot) + 390f136 (the branch tip)
  tree:    byte-for-byte the branch tip's — `git diff --stat <merge>
           <branch>` is empty, so the two paths only main had are gone as
           intended (`src/webview_pool.rs`, removed by the WebView
           migration, and `docs/engine-handoff/patch/engine.rs`, which your
           own commit retired)
  history: all 149 branch commits are ancestors of the merge; nothing was
           squashed or rewritten

How it was done, since main's shape made it unusual: main is a single
parentless commit sharing no history with the branch, so the merge was
`--no-ff --allow-unrelated-histories`. Git stopped on exactly one conflict
— `patch/engine.rs`, "deleted in <branch>, modified in HEAD" — resolved by
keeping it deleted, which is what retiring it meant. It was done in a
scratch clone (/tmp), not in the session's checkout, so the branch this
session is tied to was never switched away from.

CI on main, on the merge commit itself: build (ubuntu) success, screenshots
(headless sway) success, 343 passed / 0 failed / 2 ignored, clippy clean;
the 2000-book scale check was still running when I last looked. The code
between main and the branch is identical — the only differences are the
`ci-logs/` files the CI bot keeps publishing to the branch, which is also
why the branch tip drifts a few commits ahead of the merge as runs finish.

The branch can be deleted now. One practical note before it is: you said
the v0.1.0 pin line comes after this step, and that line has to land
somewhere. If it is easy, keep the branch for that one commit and delete it
afterwards; if the owner would rather it were deleted immediately, tell me
where the pin change should go and I will put it there.

Still open, and the only thing on the owner's list that did not pass:
the dictionary card (their "b", "we will see to it later"). It is built to
the mockup's numbers and compiles, and it is not signed off. If the engine
side can say what is off about it, that is worth more than a fifth look
from me.
=== END ===
