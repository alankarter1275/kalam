=== KALAM REPORT ===
engine: 63d854f (bundle on my branch as 45b6277)
step: your rebuild-only action
status: REBUILT, with one correction to the summary

Correction first, because it changes what the owner should be told:
`59157cd..63d854f` is not formatting. It is three commits, and two of them
change what the reader does.

1. befb91e "the book's stylesheet wins; Kalam skins it" — publisher styles
   are back on in `KalamPrefs::reading_settings()`. The book's indents,
   alignment, margins and relative sizes stand, and Kalam claims only the
   colours, the body typeface, the base size and the line spacing, through
   the same `!important` sheet appended after the book's that the old
   WebKit reader used. That is a visible change to the page text of every
   book in Kalam — exactly the surface the owner is being asked to judge
   this round.

2. a9d1226 "the XML pass was silently off for most books" — xml5ever 0.39
   reports the `xml:lang="en" lang="en"` pair on `<html>` as a duplicate
   attribute, so every book carrying it (most of them, and every fixture)
   was parsed as HTML, and the self-closed-anchor bug was still live for
   those books; `Sink::parse_error` now disregards that one report until
   the 0.40 bump. Goldens carry the +1 arena-key offsets that follow.
   Also `chapbook_core::is_dependency_noise` now names xml5ever's
   "stop_parsing for XML5 not implemented" and html5ever's stale
   foster-parenting line for shells with their own log backend — Kalam
   installs no log backend of its own, so there is nothing for us to do,
   and the reader pass' stderr stays as clean as it was.

3. 63d854f — the illustrated snapshot's numbers and one fmt wrap.

Under a "formatting fix" note those would have gone in unseen, and the
first of them moves the pixels the owner is looking at. Flagging it rather
than arguing about it: if the cascade change is deliberate (the commit
message reads as a decision, "the old WebKit reader respected them"), then
all that is needed is that it is named when the owner gets the build — that
is what I am doing below.

No host API was removed or renamed, so Kalam needed nothing but the lock.
`reading_settings()` keeps its signature and its doc comment now carries
the new rule; the bundle's API reference is regenerated from 63d854f.

---- the build, and a sequencing note --------------------------------

The lock on the branch was already at 63d854f when I picked this up: the
repo's own "ci: update Cargo.lock" commit (c564fed) had bumped all seven
entries, so I did not touch it. Which means the green run on the bundle
commit — build success, screenshots success — was still building 59157cd;
at that commit the lock said so. This report's own run is the first that
compiles 63d854f, and its logs are the record of it: clippy, 343 tests,
reader pass, scale check.

---- one line, third round -------------------------------------------

`docs/engine-handoff/patch/engine.rs` is still the pre-look version: 275
lines behind this tree, still carrying `connect_word`, no chip rebuild, no
handle headroom. It was not touched by this bundle either. Still a
re-copy hazard; still not mine to edit.
=== END ===

=== owner's copy ===

The reader has just been rebuilt against the engine's newest version, and
one thing changed that you will see: **the books now use their own
publisher's styling again** — their indents, alignment, margins and
relative sizes — the way the old WebView reader did. Kalam still sets the
colours, the main typeface, the text size and the line spacing from your
settings.

So please judge the look list on *this* build: the page text may look
different from the last build you saw, and that difference is deliberate.
