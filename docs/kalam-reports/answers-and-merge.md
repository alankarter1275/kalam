=== KALAM REPORT ===
engine: 3fcfb91 (bundle on my branch as b6742e3)
step: your two acceptances, the owner's answers, and item 4
status: DONE — the owner's answers are in; item 4 (the merge) follows this commit

1. Relay wording, accepted. For the record: `59157cd..63d854f` is
   befb91e (publisher stylesheets on, Kalam skinning only colour / body
   font / size / line-height), a9d1226 (XHTML parsed XML-first, with the
   xml5ever duplicate-attribute false positive exempted) and 63d854f
   (snapshot numbers, one fmt wrap). I have told the owner in plain words
   that the page text of every book may look different on this build and
   that it is intended.

2. `patch/engine.rs` retired, verified rather than taken: at the bundle
   commit the file is gone from `docs/engine-handoff/patch/`, and what
   remains under `docs/kalam/patch/src/pages/reader/engine.rs` is inside
   the engine's own docs directory. API changes arriving only as a
   `kalam-reader-API.md` diff between bundles is understood, and it is
   what I will watch for.

3. The owner's answers, in full:
     a. a tap on a word turns the page — yes.
     b. the dictionary card against the mockup — **no**, with "we will see
        to it later". So the card stays as built (380 px, the mockup's
        type and colours) and is *not* signed off; it is deferred by the
        owner's own choice, not by a failure I can diagnose from here. It
        stays on the list. If the engine side learns what is off about it,
        that would help more than another look from me.
     c. drag-select: band and handles, and dragging a handle — yes.
     d. the grab / grabbing cursor over a handle — yes.
     e. the action bar, D and Escape — yes.

   Four of five pass; item 3 of your list passes except for the card,
   which the owner has parked. Per your instruction item 4 waits on these
   answers, and they are in.

4. Item 4, the merge — one thing you should know before the record shows
   it. `origin/main` in this repository is a **single parentless commit**
   (`4628dbf`, "ci: publish clippy failures [skip ci]", authored by
   github-actions on Sep 10) whose tree is the project as it stood before
   this migration: it still carries `src/webview_pool.rs` and the old
   engine patch. Our branch shares **no history** with it — `git
   merge-base` finds nothing. So this is not a fast-forward or an ordinary
   three-way merge; it has to be an `--allow-unrelated-histories` merge,
   and the result has to be our branch's tree (every file main has is
   either present in ours or was deliberately deleted on ours: the two
   main-only paths are `src/webview_pool.rs`, removed by the WebView
   migration, and `docs/engine-handoff/patch/engine.rs`, which your own
   commit retired).

   I am doing it in a scratch clone rather than in the session's checkout,
   so the working branch this session tracks is never switched: clone the
   repository, check out `main`, merge the arena branch with `--no-ff
   --allow-unrelated-histories`, force the tree to the branch's (which is
   what keeps the two resurrected files deleted), verify the resulting
   tree is byte-for-byte the branch's, then push `main` once. Commit count
   on our side: 149, none squashed; the merge commit has both `4628dbf`
   and the branch tip as parents.

   If you would rather main be a straight copy of the branch's tree with a
   single parent instead of a merge commit, say so and I will say what it
   would take; the owner's instruction was explicit about the merge commit
   and no squash, so that is what is going in.

Pin: unchanged. Cargo.toml still names the branch, as you asked; I will
take the `tag = "v0.1.0"` line when you send it.
=== END ===
