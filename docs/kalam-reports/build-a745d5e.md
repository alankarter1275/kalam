=== KALAM REPORT ===
engine: a745d5e (bundle on my branch as 84804b6)
step: the scrolled-mode shimmer fix (your step 3)
status: LANDED — the branch builds a745d5e; verification run in flight

The lock on this branch was already at a745d5e before I touched anything:
the repository's own CI has an "update Cargo.lock" job that bumps the
engine revs when a bundle lands, and it had run. All seven engine crates
name `a745d5e87b6907074551dd50f204ae7c23979352`, which is your tip exactly.

Two things worth recording, the second at my expense.

1. Verified your "no API changes" claim rather than trusting it, since the
   last "rebuild only" note was wrong about behaviour: `3fcfb91..a745d5e`
   is `crates/kalam-reader/src/scroll.rs` (the strip's bands and its frame
   origin on the device-pixel grid, `view_top` replacing `scroll_y`),
   `src/view.rs` (the Strip constructors gained the dpi scale, the draw
   calls the new accessor) and two docs. No `pub` item's signature moved,
   so Kalam needs no edits — your claim holds this time.

2. My own attempt to bump the lock was wrong twice over and CI caught it:
   I replaced the seven-character prefix `3fcfb91` in a file that actually
   carries full forty-character hashes, which produced a hash that was the
   new rev with the old rev's tail stuck on it. The rebase conflicted, I
   read the conflict, saw that the branch's own lock was already correct,
   and abandoned my commit. The lesson is mine: read the full hash out of
   `Cargo.lock`, never a `grep -o` prefix.

Because the bot's lock commit carries `[skip ci]`, no run has yet compiled
the a745d5e lock — every green run so far built 3fcfb91. The commit this
report rides on is what triggers that run; its logs are the proof, and the
PR's own checks are re-running with it.

For the owner's question about merging, this matters: pr #13 now carries
63d854f, 3fcfb91 and a745d5e, and it is still open and unmerged, so this
was landable. After the merge, nothing in this chat can be added — so the
sequence wants the v0.1.0 line in the same pull request, not in a new chat.
=== END ===
