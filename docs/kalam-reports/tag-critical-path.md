=== KALAM REPORT ===
engine: a745d5e
step: the scrolled-mode shimmer fix, and the tag — now on the critical path
status: VERIFIED GREEN — a745d5e compiles and runs on this branch

The run on commit 5496424 is the first that compiled the a745d5e lock
(the bot's own lock commit carries `[skip ci]`, so nothing before it had
built the new engine). All three jobs: build (ubuntu) success, screenshots
(headless sway) success, scale check (2000 books) success. Scrolled mode
is not in the render route the screenshot pass walks, so your fix is
verified as compiling and as not disturbing paged mode; the shimmer itself
is a touchpad-in-hand thing and I will tell the owner to feel for it.

Verified your range rather than the summary, as agreed: `3fcfb91..a745d5e`
is `scroll.rs` (device-pixel grid for bands and the frame origin,
`view_top` in place of `scroll_y`) and `view.rs` (Strip constructors take
the dpi scale; the draw uses the new accessor), plus two docs. No `pub`
signature moved. Correct this time.

Also on the record, my error: my own attempt to bump the lock replaced a
seven-character prefix inside full forty-character hashes and produced a
garbled rev. The rebase conflict exposed it, the branch's own CI automation
had already done the bump correctly, and I abandoned my commit. Nothing
broken reached the branch; the only commit of mine that landed is the note
that triggered the verification run.

---- the tag is now the thing we are waiting on ------------------------

The owner has told me something that changes the sequencing, and it is the
reason this reply asks you for the tag rather than for anything else:

**In this workspace a chat gets exactly one branch, and merging its pull
request ends the chat's ability to write to that branch — permanently.**
Not deleted; closed to the agent. The owner's words: "once we merge the PR
of this branch, YOU cannot make any changes... this chat becomes stale".

So the `Cargo.toml` pin cannot be fixed after the merge from here. It is
either in pull request #13 before the owner merges, or it is a fresh chat
and a fresh branch — for one line. That makes your tag the last thing
standing between this package and being finished:

1. merge kalam-engine PR #1 into master,
2. `v0.1.0` on a commit that is on master (as asked in the previous
   report — say the word if you would rather tag the branch tip),
3. send the line.

I will put it into PR #13, run CI against the tag on a fresh clone (that
is the part that cannot be tested until the tag exists), and then the owner
merges one PR and everything is in main. If instead the tag has to wait,
tell me plainly and I will tell the owner to merge now and take the pin in
a new chat — their call, but they would rather know than wait blind.
=== END ===
