# Pitfalls — mistakes already made in this repo, and how they were fixed

**Read this before writing code.** Every entry below is a mistake that was
actually made here, caught, and fixed. They are written down so the next agent
does not spend a CI cycle — or ship a bug — rediscovering them.

Format: **what went wrong** → **why** → **what to do instead**.

Related reading: `ROADMAP.md` ("Read this first"), `docs/conversation.md`
(settled design decisions), `docs/ci/README.md` (how CI is installed).

---

## 1. Error paths that have never once executed

**Both real bugs found during the A0 step-2 pass had the same shape: an error
branch nobody had ever run.**

### 1a. A fallback that was a guaranteed panic

`AppModel::init` "handled" a failed `Catalog::open()` like this:

```rust
Err(err) => {
    notify::error("Could not open the library", …);
    Arc::new(Catalog::open().expect("catalog open"))   // ← retries the call that just failed
}
```

It retried the call that had *just* failed and `.expect()`ed the result. On a
corrupt database that is a certain panic — and because `init()` runs inside a
GTK signal callback, the panic **cannot unwind**, so it escalated to
`panic in a function that cannot unwind` → `Aborted (core dumped)` with a raw
backtrace instead of a message.

**Do instead:** an error branch must do something *different* from the thing
that failed. `main()` now opens the catalog **before** `app.run()` and, on
failure, prints the error, the database path and the exact `mv` command to move
the broken file aside, then exits 1.

### 1b. `SELECT MIN(x)` over zero rows returns NULL, not zero rows

`book_first_opened` read `SELECT MIN(at) …` into a `String`. Over zero rows
that query returns **one row containing NULL**, so `.optional()` does not help
— the *value* must be nullable. "Never opened" was therefore a hard error.

Nobody noticed because every caller wrote `.ok().flatten()`, which turned the
error into `None` and produced the correct screen **by accident**.

**Do instead:** read aggregates into `Option<T>`, or wrap them in `IFNULL`. And
treat a swallowed error as a place a bug can hide, not merely as untidy code.

---

## 2. GTK: things a `gtk::Window` was doing for you

When the A1 track moved dialogs from `gtk::Window` into an in-app overlay,
three things silently stopped happening. All three had to be replaced by hand.

| A window gave you | Why it matters | Replacement |
|---|---|---|
| **Teardown** — `close()` destroys the widget tree | A button's callback holds the dialog; the dialog holds the button. Destroying the tree breaks that loop. Removing an overlay child does **not**. | `teardown()` also empties the host, so a closed dialog is actually freed instead of leaking on every open |
| **A height bound** — a window has a default height | A panel centred in an overlay is sized by its content, so a long description or twenty rule rows push the action bar off a 768px screen | `max_content_height` + `propagate_natural_height` on the scroller |
| **A real top-level** for portal dialogs | `gtk::FileDialog` is portal-backed and needs a genuine window as parent | Resolve it from the anchor's root, not from the dialog |

## 3. `set_size_request` is a FLOOR, not a size

This one caused a user-visible bug that took four separate fixes.

The book float had `set_size_request(720, 420)`, and the panel still changed
size from book to book. GTK grows a widget past its size request whenever the
content inside needs more room. So **any unbounded child can resize the
panel**. In that one float there were four:

1. Tags in a `gtk::FlowBox` (`max_children_per_line: 8`, up to 12 chips) —
   9+ tags wrapped to a second row and made the panel taller.
2. Title and series labels with `set_wrap: true` — a long title took 2–3 lines.
3. Authors, filled by the shared `replace_author_links`, which builds a
   wrapping FlowBox.
4. The description section's "no Read more" branch left the section entirely
   unbounded — `height_request(-1)`, natural height, no max — so a short blurb
   gave a short panel and a nearly-long-enough one gave a tall panel.

**Do instead:** if a panel must be a fixed size, every variable-length child
needs an explicit bound — ellipsise, clip, or scroll. Do not assume the size
request is doing it. And when a shared helper (like `replace_author_links`)
behaves correctly elsewhere, constrain it **at the call site**, not in the
helper.

## 3b. A view property and a fill function are two writers

The book float's "Read more" button made the description **shrink to one
line**. The view declared `set_vexpand: true` on the description scroller;
`fill()` — which runs on every update, not just the first — called
`set_vexpand(false)` on the same widget. The fill function always wins, because
it runs last.

The bug hid for a while because the collapsed state used
`PolicyType::Never`, and `Never` forces a scroller to show its content at full
height (see 4b). That masked the missing `vexpand`. Expanding switched the
policy to `Automatic`, which released the scroller back down to its own
minimum — one line.

**Do instead:** decide where each property lives and keep it there. If a value
is static, set it in the view and never touch it in the update path. If it
varies, set it *only* in the update path so there is one writer. When a widget
misbehaves, grep its name across the whole file before assuming the view is the
source of truth.

Corollary: a control that changes a panel's shape does not belong in a panel
whose defining property is a fixed shape. The toggle was deleted rather than
repaired — scrolling already solved the problem it existed for.

## 4. Hiding a widget removes its space

Fixing (3) introduced a new bug: `fill()` hid the tag row when a book had no
tags, and a hidden widget occupies no space — so the action buttons below it
shifted up on untagged books.

**Do instead:** when a widget's job is to reserve space, show it
unconditionally. An empty row is invisible anyway.

## 4b. Reserving space is not the same as anchoring

Fixing (4) by keeping the tag row permanently visible **did not work** — the
user reported the buttons still moving. Two reasons, and the second is the
general lesson.

**`ScrolledWindow` + `PolicyType::Never` ignores your height.** `Never` is a
promise to GTK that the content is fully visible in that direction, so GTK
propagates the child's *whole* minimum height and `min_content_height` /
`max_content_height` cannot shrink it. A chip taller than the requested row
height still grew the row. Use `External` when you want a hard height *and*
scrolling; `Never` is only safe when the content genuinely cannot overflow.

**Reserving space for one child only fixes that child.** Nothing in the float
body had `vexpand: true`, so all leftover height pooled *below* the action row
and its position tracked whatever happened to sit above it. Every variable
child would have needed its own bound, forever.

**Do instead: anchor the thing that must not move.** One `vexpand: true` spacer,
plus `valign: End` on the rows below it, makes the slack collect *above* them.
In a fixed-height panel that pins them absolutely, regardless of what changes
higher up.

**Where you put the spacer decides what gets separated.** Placing it directly
before the action row anchored the buttons but shoved the tag row up with the
slack, leaving a gap between two things that belong together. If several
trailing rows should stay as a group, the spacer goes **above the whole group**,
not between its members.

## 4c. Ellipsising a label does not stop it widening its parent

The book float kept growing sideways for long titles even though the title
label had `EllipsizeMode::End` set. Ellipsize only decides how overflow is
*drawn*; the label still reports its **entire** string as its natural width,
and since `set_size_request` is a floor (see 3), the panel grew to grant it.

**Do instead:** set `set_max_width_chars(n)`, which is what actually caps the
natural width, and pair it with `hexpand: true` so the label is still allocated
the real column width rather than being squeezed to `n` characters. Ellipsize
or wrap then applies within that cap. The same applies to *wrapping* labels: a
wrapping label with no cap asks for its whole text on one line.

Backstop for a whole region: `set_overflow(gtk::Overflow::Hidden)` on the
container, so a label added later cannot silently widen it again.

## 4d. Slack is a resource — give it to something useful

The first anchoring fix parked all the leftover height in a blank `vexpand`
spacer. That worked, but it meant a panel with a short title showed a band of
empty space while the description sat in a cramped scroller right next to it.

**Do instead:** make the element that *benefits* from extra room the expanding
one. Here `desc_section` carries the only `vexpand`, so it does both jobs at
once — it pins everything below it to the bottom, and it donates the spare
height to the description. A dedicated spacer is only right when nothing in the
layout actually wants the space.

## 2b. An overlay is not a focus scope

Related to 2, and missed when the dialogs were first converted. A
`gtk::Window` confines Tab: focus cycles within the window and stops at its
edge. A panel in a `gtk::Overlay` gets no such thing — the page underneath is
still in the same widget tree and still focusable, so Tab walked out of a
"modal" dialog and into the sidebar behind it. You could focus a button you
could not see and activate it with Enter.

The scrim hides this in testing because `can_target` blocks the **mouse**.
Nothing was blocking the **keyboard**.

**Do instead:** `crate::widgets::focus_trap` — a capture-phase key controller
on the window root that owns Tab/Shift+Tab while the panel is visible, moves
focus with `child_focus`, and wraps at the ends by clearing the root focus and
searching again. Attach it to the **root**, not the panel: when a dialog opens,
focus is usually still on the page widget that opened it, so a controller on
the panel would never see the keypress that walks away from it.

Remove it when the dialog closes, for the same reason as the Esc controller —
an orphan keeps swallowing Tab for a panel that no longer exists.

## 4e. `thread_local!` state silently swallows cross-thread calls

`notify` keeps its queue and history in `thread_local!` cells. Worker threads
called `notify::error` anyway — the import loop reports an unreadable file that
way. There was no panic and no warning: the toast was appended to *that
thread's* copy of the queue, which nothing ever renders, and the message was
lost. A module whose entire purpose is "no failure goes unreported" was
dropping reports on the floor.

**Do instead:** make the entry point thread-safe rather than auditing every
caller — but bounce only the part that actually needs the main thread. `push`
records the history entry synchronously (plain data) and defers only the
*display* via `MainContext::invoke`, which runs inline when already on the main
thread.

**And the history itself was `thread_local!` too.** Fixing the display was only
half of it: `HISTORY` was a thread-local `RefCell`, so a worker's entry was
filed in that thread's own copy, invisible to Settings → Notifications (which
reads it from the main thread) and discarded when the thread ended. Background
work is precisely where unattended failures happen, so that was the worst half
of the app to lose. It is now a process-wide `static Mutex<VecDeque<Entry>>`.

Rule of thumb: `thread_local!` is right for *UI-owned* state (the widget host,
the pending-display queue) and wrong for anything a different thread might
legitimately produce or a different thread might read back.

**The first attempt bounced the whole function, and that broke two tests.**
Cargo's test harness runs each test on its own thread, so nothing under `cargo
test` is the main-context owner: every `push` got deferred to a main loop that
never runs, and the history stayed empty. Splitting data from display fixed it,
and a regression test now pushes from a `thread::spawn` and asserts the entry
is in the history. Lesson: "is this the main thread?" is false in unit tests
too, so any bounce must leave the testable bookkeeping on the calling thread.

The general rule: if a free function touches `thread_local!` state or GTK, it
must either be documented main-thread-only *and* enforced, or it must bounce.
"Documented and not enforced" means it will be called from a worker eventually.

## 4f. Two futures racing to report the same job

The task manager first drained progress and awaited the result concurrently.
That reads naturally and is wrong: the result usually arrives while a progress
update is still queued, so the finished toast appeared and *then* a stale
"importing 3 of 5" overwrote the status line.

**Do instead:** drain progress to exhaustion, then take the result. Dropping
the `Reporter` closes the progress channel, so the loop ends on its own and the
ordering is guaranteed rather than lucky.

## 4f2. Making state process-wide makes its tests racy

A follow-on from §4e, and it took a CI failure two commits later to show up.

`HISTORY` moved from `thread_local!` to a process-wide `Mutex` so a worker's
message would be readable from the main thread. That was right. But `cargo
test` runs tests on **parallel threads of one process**, so four tests that had
each been working on their own private copy were suddenly sharing one.

`clear_history()` at the top of a test is not isolation. Another test can push
between that call and the assertion, and
`report_passes_ok_through_and_flags_errors` failed on exactly that: it asserted
the history was empty, and it was not.

**Do instead:** give the shared state a test-only `Mutex<()>` and take it at
the top of every test that touches it. Poison-tolerant, so one failing test
reports its own failure instead of turning every later test into a mutex panic
that hides it.

**The general rule:** whenever you widen the scope of some state — thread-local
to global, per-instance to shared — re-read its tests. They were written under
the old scope and may have been relying on it for isolation without saying so.

## 4g. Overriding `update_with_view` turns off every `#[watch]`

Found while migrating the import loops, not while looking for it.

relm4's default `update_with_view` calls `update` **and then** `update_view`.
Override it and you replace both halves, so unless the override ends with
`self.update_view(widgets, sender)` nothing ever re-evaluates the `#[watch]`
bindings in `view!`. The docs say so plainly: *"you must remember to call
`update_view` in your implementation. Otherwise, the view will not reflect the
updated model."*

Two pages were getting this wrong:

- `home.rs` — four `#[watch]` bindings, including the import status line and
  the `+ Add books` button's `set_sensitive` / `"Importing…"` label. The page
  looked fine only because `rebuild()` repaints the parts it owns by hand.
- `lookup_history.rs` — one `#[watch]`, the "N lookups" count, which never
  moved after a search or a clear.

`book.rs` and `series_float.rs` also override without calling it, and those are
**fine**: neither has a single `#[watch]`, so there is nothing to refresh.

**Check, when overriding:** does this file contain `#[watch]`? If yes, the
override must end by calling `update_view`. Watch for a `return` inside a match
arm — it skips that tail, which is sometimes deliberate (`all_books.rs` returns
early per file so a 300-book import does not rebuild the grid 300 times) but is
easy to do by accident.

## 4h. A GObject cannot cross a thread — its raw bytes can

The obvious way to preload covers is "decode on a worker, return the texture".
It does not compile, and that is the seam doing its job: `gdk::Texture` is a
GObject owned by the main thread, so `tasks::spawn`'s `T: Send` bound rejects
it.

The fix is to move the boundary rather than fight it. Split the job at the
last point where the data is still plain:

- **worker** — read the file, decode it, resize it, hand back `Vec<u8>` of RGBA
- **main thread** — wrap those bytes in a `gdk::MemoryTexture`

The expensive part is all on the left. The wrap is a pointer copy.

Two things to get right when doing this:

- **The buffer must match the dimensions exactly.** `MemoryTexture::new` takes
  a stride and trusts it; a short buffer is a garbled image or a crash inside
  GDK, not a Rust panic. Check `len() == w * h * 4` before wrapping.
- **Hold widget references weakly.** A page can be destroyed long before its
  covers finish decoding. A strong reference leaks the widget *and* lets a
  finished preload write into a dead page.

Related: anything you park in a list waiting for an async result needs a way to
be reaped when the result never comes. A cover with a missing or corrupt file
is never swapped in, so its entry is only removed by the periodic sweep — not
by the success path, which is the one that is easy to remember.

## 5. Never use `opacity` on a scrollbar

`src/style.rs` opens with a warning block explaining that `opacity` below 1
makes GTK render through an offscreen surface, and a collapsed overlay
scrollbar's surface is zero-sized:

```
*** BUG *** In pixman_region32_init_rect: Invalid rectangle passed
```

**I wrote `opacity: 0` anyway** while hiding the tag scrollbar, and only caught
it by re-reading that block. Hide a scrollbar by making its **background
transparent**.

More generally: `src/style.rs` says every line in its scrollbar block exists
because of a specific bug. Believe it.

## 6. To hide a scrollbar, use `PolicyType::External`

`Automatic` reserves space for a bar that appears only sometimes — which is its
own version of the "size varies" complaint. `External` keeps wheel, touchpad
and drag scrolling while GTK draws and allocates nothing.

## 7. `.focus()` is ambiguous on a `gtk::Window`

With `gtk::prelude::*` in scope, both `WidgetExt::focus` and
`GtkWindowExt::focus` apply:

```
error[E0034]: multiple applicable items in scope: multiple `focus` found
```

Name the trait: `gtk::prelude::GtkWindowExt::focus(&window)`. Note also that
`gtk::Text` is the inner widget of a `gtk::Entry` and is what actually holds
focus — test for it first.

## 8. A keyboard shortcut on the window root fires while you are typing

The float close handler in `app.rs` fired on `q`/`Q`/Esc whenever a float was
visible, without checking focus. The tags panel has a text entry, so **typing
the letter `q` into it dismissed the panel**.

**Do instead:** a letter shortcut must check whether a text widget has focus.
Esc is safe; letters are not.

---

## 9. Dead code fails the build

This is a **binary crate** and clippy runs `-- -D warnings`. An enum variant
that is only *matched* and never *constructed*, a helper that lost its last
caller, or an import left behind by an edit will all fail CI.

Real examples from this session:

- Removing the ✕ from the series float made `SeriesFloatMsg::Close` and
  `SeriesFloatOut::Close` unreachable.
- Converting the pickers left three copies of a `window_of()` helper unused.
- `shelf_editor.rs` kept `use relm4::RelmWidgetExt;` after its last
  `set_margin_all` went away.

**After deleting a call site, grep for what it used**, including enum variants,
helpers and imports. Do not add an import to `src/db.rs` for a symbol used only
in one submodule.

## 10. Do not write "helpful" defaults that hide failures

The whole A0 step-2 pass exists because pages did
`list_x().unwrap_or_default()`. The result was **the empty-state lie**: a
broken database rendered as "your library is empty", "no dictionaries
installed", "no shelves yet" — each indistinguishable from the real empty case.

Specific traps recorded during that pass:

- `self.x = ….unwrap_or_default()` in a *reload* **overwrites live on-screen
  data**. On error, report and keep what is displayed.
- A silent `return` on a failed read is the same defect with different syntax.
- `unwrap_or(false)` on a uniqueness check silently assumes "the name is free".
- Distinguish "row absent" from "read failed". They are different messages.
- Raw grep counts of `unwrap_or_default()` **over-count** — many are string,
  path or date defaults, or `get_pref(key, default)` calls that default *by
  design*. In `reader.rs`, 35 matches were 4 real ones.

---

## 11. Working with CI in this repo (no local toolchain)

There is **no `cargo` in the sandbox** — no `~/.cargo`, no `pkg-config`, no
gtk4, and no network to `static.rust-lang.org`. **CI is the only gate.** Runs
take roughly 5.5–9 minutes.

Practical consequences:

- **Check the failing *step* before assuming your code is broken.** Twice a
  "failed" run was the rustfmt step alone, whose formatting commit had in fact
  landed. Use `gh run view <id> --json jobs`; `gh run view --log` returns
  nothing here.
- Read the published logs: `ci-logs/clippy-latest.txt` for clippy,
  `ci-logs/test-latest.txt` for tests. **`clippy-latest.txt` can be stale** —
  it keeps a `--- run <id> ---` footer from an older run.
- **CI's rustfmt step auto-commits and pushes**, so your next push is often
  rejected. Recover with `git fetch origin <branch>` then
  `git rebase FETCH_HEAD`. Use `FETCH_HEAD`; `origin/<branch>` may not exist
  locally in a shallow clone.
- **Local HEAD silently drifts between turns.** Always
  `git fetch origin <branch> && git reset --hard FETCH_HEAD` before editing.
  The sandbox can also be replaced wholesale — it came back once as a fresh
  clone at old `main`, and the branch had to be re-fetched. Your pushed work is
  safe; your local checkout is not.
- **Two pushes in quick succession race CI's rustfmt.**
- **Never `git commit --amend` after pushing.** Use `git reset --soft <sha>`.
- **`echo "push=$?"` after `git push … | tail` reports `tail`'s status**, not
  git's. A rejected push can print `push=0`. Read the hint text or check
  `$PIPESTATUS`.
- On a rebase conflict in `ROADMAP.md`, it is almost always duelling appended
  changelog rows. Keep both.

## 12. Editing files you cannot compile

Because nothing can be built locally, scripted edits need their own safety net:

- Python heredocs must `assert old in s` **and** assert the occurrence count,
  then verify brace/paren balance afterwards. Compare against
  `git show HEAD:<file>` — `reader.rs` has a pre-existing `+1` brace delta from
  a `{` inside a string literal, and so do `dictionaries.rs` and
  `shelf_rules.rs`.
- **Do not chain `python3 <<'PY' … PY && git commit && git push`.** A failed
  `assert` exits Python but the `&&` chain still proceeds.
- **After a CI rustfmt commit, re-read the file before scripted edits.** An
  anchor matching a one-line expression will fail once rustfmt has reflowed it
  into a 4-line chain.
- **A flat `grep -n "self\.catalog"` misses multi-line method chains.** Sweep
  with `\b(self|model)\s*\n?\s*\.catalog\b`. A single-line grep cost one CI
  failure.
- `awk 'length>100'` counts **bytes, not characters** — curly quotes and em
  dashes trip it. Check with Python before "fixing" a line.
- **Writing new code from memory is unsafe.** Four API mistakes came from it:
  there is no `insert_book_for_test` (use the 10-arg `insert_book`), `Book` has
  no `Default`, `Book.progress`/`rating` are `u8`, and `Book` lives in
  `crate::models`. Also: `Cargo.toml` sets no `rust-version`, so avoid recent
  std APIs (`repeat_n` needs 1.82).
- `catalog()` already returns `&Catalog`, so `&self.service.catalog()` is a
  type error and a needless borrow. And `let cat = self.service.catalog();`
  followed by assigning to `self.<field>` is a borrow conflict — finish reads
  into locals first.

- **Four more API traps, all caught by CI (2026-10-01, PDF async step 1),
  all from writing new code without a compiler:**
  `Option::is_some_and` has no `Result` twin — for a `Result`, use
  `matches!(&x, Ok(t) if ...)` (which also keeps `x` assignable inside the
  block, unlike `if let Ok(ref t) = x`); building a tuple scrutinee like
  `if let (Some(p), Some(tx)) = (self.a.clone(), self.b)` moves `self.b`
  out from behind `&mut self` — write `self.b.as_ref()` and bind `tx`
  without `ref`; `Option::zip` **consumes** both options, so a value needed
  afterwards must be compared by reference
  (`match (&started, &current) { (Some(a), Some(b)) => a == b, _ => false }`);
  and a struct field declared `PathBuf` needs an owned value — `path.clone()`,
  not the `&PathBuf` a `ref` binding gives you.

- **A private type cannot appear in a `pub` item (2026-10-01, step 3 of
  the PDF plan; one failed CI run).** A private enum used as a `pub`
  model field / `pub` method parameter trips the `private_interfaces`
  lint, which `-D warnings` turns red. The whole file's convention is
  `pub` types anyway -- match it (or make the field and method private;
  same-module callers do not need `pub`). And a second lesson from the
  same run: **do not report a CI result from `gh run watch`'s exit code
  alone.** The watch exited 0 while the run had failed, and "step 3 is
  green" was said on that basis. Always confirm with an explicit
  `gh run view <id> --json conclusion` (or the run's own ci-logs commit)
  before calling a run green.

- **Second batch of the same (2026-10-01, step 4 of the PDF plan; one
  failed CI run):** a `crate::` path written from memory pointed at
  `crate::pdf_reader::...` — the module lives at
  `crate::pages::pdf_reader` (and an unresolved path also produces a
  misleading secondary error about `str` not being `Sized`, which
  disappears with the path); and in `watch_folder.rs` a `res.title.clone()`
  was inserted *after* existing code that does `last_title = res.title;`
  — the move happens on someone else's line, far above the new code.
  Both are catchable in seconds before pushing: grep every new
  `crate::foo::` path against `main.rs`'s module tree, and when editing
  inside an existing function, re-read the whole function for moves of
  the values the new code touches — the borrow checker sees the whole
  function, not just the diff.

- **A helper used only by tests is dead code in the bin (2026-10-01,
  step 4; one failed CI run).** `cargo clippy --all-targets` compiles the
  test target where the helper is used, but the plain bin target is
  compiled too and `dead_code` fires there — test-only reachability does
  not count. Assert through the production API instead (here: draining
  the queue the way the worker does), or make the production code
  genuinely call the helper.

- **`255.0 / 255.0` is `eq_op` under `-D warnings` (2026-10-01, step 5;
  one failed CI run).** Writing a colour channel as `v / 255.0` reads
  nicely, but when the channel is full-scale the two operands are equal
  and clippy fails the build. Write `1.0`.

## 13. A route existing in `app.rs` does not mean the user can reach it

`ReadingList`, `Tags` and `Analytics` had complete pages, `PageSlot` variants
and `Route` arms — and **nothing in the UI ever emitted those routes**. Three
finished pages that could not be opened at all. `AllBooks` was linked from
exactly one place: inside the `if total_books == 0` placeholder, so importing
your first book removed the only link to the full grid.

**Reachability means grepping for who *emits* the route**, not who handles it.

## 14. You cannot see the screen

The user is the QA loop for anything visual. Do not ask for screenshots — you
cannot view them. Ask for error text, and say precisely which two states to
compare ("a book with no tags versus one with many — do the buttons sit at the
same height?").

Corollary: **verify a claim before repeating it.** I told the user the series
float "skips the book float you came from"; it opens from the **book page**
(`book.rs:695`), and the book float's series line is a plain label. The
correction is in the ROADMAP rather than quietly dropped.

---

## 15. Do not skip a file on a shallow check

Rejected reasoning, in the user's words: *"it already reports its main error,
so it's fine."* A proper check also covers secondary/enriching reads, N+1 query
patterns, and whether the page could later move off the UI thread.
`saved_quotes.rs` and `saved_words.rs` were skipped on exactly that shallow
basis and had to be revisited.

If a file genuinely needs nothing, say **why** precisely. `author.rs` is the
model: it makes no database reads at all, it only passes the `Arc` to its
children, and its one `unwrap_or_default()` is on a local helper.

---

## 16. A preloader wired into one call site leaves every other page blank

Step 5 added `cover_widget_deferred`: a card draws a placeholder immediately
and a worker fills it in later. The thing that starts that worker,
`warm_covers`, was called from exactly **one** place — `build_book_grid`.

But cards are not only built by the grid. `home.rs` and `author.rs` call
`build_book_card` directly to lay out their own strips. Those cards happily
drew placeholders and then waited for a decode that nobody had asked for, so
**Home's covers never appeared at all** — not slowly, never. It looked like a
loading bug and was really a missing function call.

Two rules came out of it:

1. **Pair the deferral with the warm-up in one function.** `preload::warm_books`
   is now the only thing a page calls; it cannot be given the list without also
   queueing it. A page that builds deferred cards and does not call it is the
   bug, and the fix is one line rather than three.
2. **When you make something lazy, grep for every builder of the lazy thing**,
   not every caller of the function you edited. The grid was the obvious
   caller; the two that mattered were the ones that had quietly bypassed it.

### The sibling mistake: a budget that was never topped up

`ahead_of` ended in `.take(PRELOAD_AHEAD)` — 24 covers. The intent was "decode
what is visible first". The missing half was anything to request the other 115,
because scroll-driven re-queueing was never wired up. On a 139-book library the
first two rows filled in and the rest kept placeholders for good, which is
worse than the slow-but-complete behaviour it replaced.

An optimisation that drops work must say **who picks the work back up**. If the
answer is "nothing", it is not a budget, it is a cap, and the feature is
half-finished. `ahead_of` now returns everything, nearest-first, and the
batching moved into `warm_covers`, where the yield between off-screen covers
keeps the UI thread free without ever abandoning a cover.

Both defects survived a green CI run and were found in ten seconds by a human
looking at the actual screen. CI cannot see a placeholder.

---

## 17. "If it is too big, empty it" is not a cache bound

`COVER_CACHE` had a bound. It was:

```rust
if cache.len() > 400 { cache.clear(); }
```

That is worse than it looks. It throws away *everything*, including the covers
on screen right now — so the moment a library crosses the limit, the visible
grid has to decode itself all over again. The cache stops helping at exactly
the size where it starts to matter, and the user sees a stall they did not see
at 399 covers.

It also nearly escaped notice because of an unrelated bug. Before A0 step 5's
fix the preloader stopped after 24 covers, so nothing ever approached 400.
Removing that cap — correct on its own terms, the covers were not loading —
quietly turned a dormant flaw into a live one. **When you remove a limit,
check what else was relying on it.**

The replacement drops only the least-recently-used entry, which keeps what the
user is looking at. Two details that are easy to get wrong:

1. **The order list has to be pruned everywhere the map is.** A key left behind
   in the bookkeeping will evict a live entry later, and that bug would surface
   as an occasional unexplained re-decode rather than anything obvious.
2. **A probe is not a use.** `is_cover_cached` is the preloader asking whether
   it needs to decode something — if that counted as a use, a background sweep
   across a whole library would reorder the cache away from what is on screen,
   which is precisely backwards.

Both are covered by tests, which was only possible after making the cache
generic over its value type: a `gdk::Texture` cannot be built without an
initialised GTK display, and CI has none. Parameterising the type was cheaper
than leaving the eviction logic untested.

### The related reporting failure

`install_bundled_dictionaries` ran on the UI thread for the whole of A0 step 4
and was in neither the "converted" nor the "deliberately left alone" list I
gave the user. The reason is the search method: I looked for `thread::spawn`
call sites, because the task was "move the threads onto the seam". But the
acceptance criterion was **"all slow work off the UI thread"**, and this is
ordinary blocking code in the startup path with no thread anywhere near it.

Searching for the mechanism instead of the requirement cannot find work that
was never threaded in the first place. Same shape as §16: checking a
convenient proxy rather than the actual property.

---

## 18. Cheap work repeated for ever is not cheap

The thumbnail backfill existed to give books imported before A0 step 3 a
thumbnail without re-importing. It was written as "only missing files are
generated, so it is cheap after the first pass" — and that sentence was true
about the *generating*, which is why nobody looked further.

What it actually did on every launch of a settled library:

1. `list_books()` — every column of every book, plus a second query joining
   `tags`, building and dropping a full `Book` struct per row. The backfill
   reads exactly two fields: `uuid` and `cover_path`.
2. One `is_file()` per book, to confirm thumbnails that were already there.

At 139 books nobody notices. At 2,000 that is two table scans, 2,000 structs
and 2,000 stat calls, every single start, to do nothing at all. The CI report
made it visible: `thumbs_backfilled 50/100/…/2000` scrolling past on a library
where every thumbnail already existed.

Two fixes, and the second is the one that matters:

- A narrow query (`books_with_covers`) returning just the two columns, with
  cover-less books filtered in SQL rather than skipped in the loop.
- A skip marker, so a settled library does no work at all.

### Getting the marker right

A plain "done" flag would be wrong — importing a book has to trigger a new
pass. Storing the **book count** at the last complete run gives that for free:
any import changes the count.

Three cases that had to be reasoned about rather than assumed:

- **A cancelled pass must not record the marker.** It has not verified the rest
  of the library, and claiming otherwise leaves those books without thumbnails
  permanently.
- **A cover that fails to thumbnail must not count as covered**, or the marker
  promises a complete library that is not one.
- **Delete-then-import nets to the same count.** The count alone cannot see it,
  so `delete_book` clears the marker explicitly. An unnecessary pass costs one
  query; a missed one costs a book its thumbnail for good.

The general shape: **an optimisation that skips work must be wrong in the safe
direction.** Every ambiguous case here re-runs. Compare §16, where a preloader
dropped work and nothing picked it back up — same failure, opposite cause.

## 19. A test that passes on a machine where the bug cannot appear is not a test

`docs/testing-a0-step5.md` told the user to check that `startup_dicts` prints
*after* `window_shown`, to confirm the dictionary import had moved off the UI
thread. They ran it and reported `startup_dicts 0.6 ms` printed **before**
`window_shown 711.6 ms`.

The build is correct. The instruction was not.

The line prints when the work *finishes*. On a settled machine the packs
installed months ago, so `install_bundled_dictionaries` early-outs on a pref in
well under a millisecond — long before the window appears at ~700 ms. It
therefore prints before `window_shown`.

The fatal part: **it would have printed before `window_shown` on the broken
build too.** Sub-millisecond work does not delay anything whether it runs on
the UI thread or a worker. So the check produced the same output for a fixed
build and a broken one — it had no power to distinguish them, which means it
was never a test, just a line to read.

What makes it a real test is forcing the condition the fix addresses:

```bash
XDG_DATA_HOME=/tmp/kalam-dicttest KALAM_TIMING=1 cargo run --release
```

An empty data dir means the dictionaries genuinely install (~2–3 s), so the
ordering finally carries information. CI gets this for free by seeding a fresh
library every run, which is exactly why CI *could* prove this fix while the
user's machine could not.

### The general rule

Before writing a manual check, ask: **what would this print if the bug were
still there?** If the answer is "the same thing", the check is worthless no
matter how sensible it reads. A test needs a failing case that is reachable on
the machine it runs on.

Note the symmetry with §18. The thumbnail skip is the mirror image — it can
*only* be verified on a settled library, and CI (always a first launch) can
never show it. Two fixes in the same commit, each provable in exactly the
environment where the other is invisible. Neither environment is "the" test
environment; the question is always which one can make the bug appear.

---

## 20. Answering a question the user did not ask

**The mistake.** The user was asked how far extensibility should go, and
answered about **scope**:

> *"sources and metadata and maybe a few more, not an ecosystem, because it's
> for personal use. plugin system makes sense if there is a community, which
> isn't the case here."*

I turned that into a decision about **implementation language**: removed Lua
from the plan entirely, renamed the user's "P12 — Lua plugin system" to "P12 —
Extension surfaces", wrote "deferred, probably indefinitely" and "No scripting
runtime of any kind", and swept eleven documents to match.

The user had said *no ecosystem*. I heard *no scripting runtime*. Those are
different claims:

| The user's claim | What I wrote |
| --- | --- |
| Do not court third-party authors, no marketplace, no API-stability promises | There will be no Lua |
| About **who else** uses the extension points | About **what language** they are written in |

An ecosystem is about *other people*. A scripting runtime is about *how fast
you can fix something*. The second applies to one developer just as much as to
a thousand — which is the whole argument I had thrown away.

**The user's correction, which was checkable and correct:**

> *"have you seen how metadata plugins in Calibre work?? there are many, many
> plugins in Calibre just for metadata sources."*

Calibre's index carries 20+ third-party metadata-source plugins — Goodreads,
Amazon, Kobo, StoryGraph, ISFDB, Douban, DNB, moly.hu, databazeknih.cz, Skoob,
Kitapyurdu, noosfere — mostly regional or niche, and almost all HTML scrapers.
I had written in `source-seam.md` §12a that metadata was "already extensible,
done" because we ship two providers. Two providers is not the same as
extensible; Calibre ships several built in *and still* needed the plugin tail.

### Three separate errors, worth naming individually

1. **Under-weighted how often scrapers break.** Tachiyomi's entire extension
   architecture exists for this: *"extensions are parsers; if a website
   changes its structure, the extension breaks. The core app stays stable;
   extensions change constantly."* I treated site changes as rare.
2. **Asserted a cost without measuring it.** I said recompiling was "a few
   minutes" without opening `Cargo.toml`. It is `lto = true` with
   `codegen-units = 1` over 44k lines and 36 dependencies — the slowest
   possible configuration, a full relink for a one-character change, and the
   one most likely to be OOM-killed on the user's 4 GB machine. **A number I
   have not measured is not evidence, and this repo has a measurement culture
   precisely so I do not have to guess.**
3. **Scope creep in reverse.** Deleting a feature is a change like any other.
   Renaming a phase the user named, and writing "indefinitely" on their idea,
   needed their agreement first.

### The rule

**When an answer settles one variable, change only that variable.** If a
second decision seems to follow, say so and ask — do not ship it. Watch for
the shape of this error: the user answers question A, and the next commit
message explains a decision about question B.

And the specific form it took here: **"no community" does not imply "no
tooling for ourselves."** Ask who a constraint protects. If the answer is
"strangers", it says nothing about what the maintainers should use.

### Related

§19's rule was "what would this print if the bug were still there?" The
analogue: **what did the user actually say, and would their sentence still be
true if I had decided the opposite?** Here it would — "not an ecosystem" is
equally true with or without Lua, which is the tell that the sentence never
settled the question.

---

## 21. When you fix the thing a check was watching, re-derive what the check proves

The CI screenshot job had never actually navigated. It sent `Tab Tab Return`
and hoped the focus order matched, and when it did not, three byte-identical
photographs of Home were reported as a successful run. The fix was to let the
app be told where to go (`KALAM_ROUTE=all-books`), and it worked on the first
try: route accepted, `grid_build` emitted, 139 grid cards.

The same run reported:

```
navigation: FAILED -- 01-home and 03-after-nav-settled are byte-identical.
  The app never left Home; every screenshot below shows the same page.
```

That verdict was wrong, and I wrote it. The check compared the first
screenshot to the last, which was the right question **while navigation
happened part-way through the run** — the app started on Home, so a difference
meant it had moved. `KALAM_ROUTE` navigates at *startup*. Every shot in the
run is now the requested page, so the file named `01-home` was already
All-books and all three fingerprints matching is the **expected** result.

I changed the mechanism and carried the old oracle across without re-asking
what it was testing. The check still ran, still printed a confident verdict,
and the verdict was noise.

Two things had to change, and the second is the one that matters:

- **The filenames.** `01-home` described a page that was no longer Home, so
  the report lied twice — wrong page name, and a correct result presented as
  a failure.
- **The comparison.** Proving "we are on All-books" needs something that is
  *not* All-books to compare against. The job now relaunches with
  `KALAM_ROUTE=home`, photographs Home as `04-home-for-comparison`, and
  asserts the two differ. That catches the real failure — the log claiming it
  navigated while the screen never changed — which comparing a page to itself
  never could.

### The rule

**A check is a question about a mechanism. Change the mechanism and the
question may no longer parse.** After any fix, re-read every assertion that
watched the old behaviour and ask what each one now proves. An assertion that
survives a refactor unexamined is not evidence that it still works; it is
evidence that nobody looked.

### Related

This is §19 wearing different clothes. There the check could not distinguish a
fixed build from a broken one; here it could not distinguish a fixed build
from a broken one *either* — it just failed in the flattering direction
instead, crying wolf rather than staying silent. Both failures come from not
asking what the output would be in the other case. A check that reports
failure on correct code teaches people to ignore it, which costs more than
having no check at all.

---

## 22. Do not let a container compute geometry from children you are removing

The windowed grid (A0 step 6) builds only the book cards you can see. First
attempt kept the existing `GtkGrid` and added one tall spacer widget in the
last row to hold the full height open. It shipped behind a switch, the user
turned it on, and hit three bugs in about a minute:

1. The page scrolled roughly **twice as far as it should**, and everything past
   the books was blank.
2. **Books were missing.**
3. After a few rows, **the covers and the scrollbar jumped around** while
   scrolling.

Three symptoms, one cause.

**A `GtkGrid` row is as tall as its tallest child.** The spacer for 144 books
was 6,532 px, and it was placed *inside* the last row — so that row became
6,532 px tall, on top of the 23 normal rows above it. 6,796 px of content
became 13,064 px of scrolling. That is bug 1.

**The spacer occupied a real cell**, column 0 of the last row. The book that
belonged there had nowhere to go. That is bug 2.

**Rows holding no mounted cards collapsed to zero height.** As cards mounted
and unmounted during a scroll, row heights kept changing, so the grid's total
height changed underneath the scrollbar. That is bug 3.

### The general rule

**A container that derives its size from its children cannot be used to
virtualize those children.** The entire premise of windowing is "most children
do not exist right now", and `GtkGrid`, `GtkBox` and friends answer "how big am
I?" by asking the children that do. Those two facts are in direct conflict, and
no arrangement of spacers fixes it — a spacer is just another child feeding the
same broken calculation.

Use a container that does **not** infer geometry: `GtkFixed`, where every child
is placed at an explicit x/y and the overall size is set once from the data.
Then positions and total height depend on the *book count*, never on what is
mounted, which is exactly the property windowing needs.

### What made this expensive

The arithmetic was checked before pushing — the *row* maths (which rows are
visible) had seven tests and was correct. What was never checked was the
**pixel** maths, because it was one line inside a function that needs a display
and therefore "could not be tested". That was wrong: `grid_height(books)` and
`card_position(index)` are pure integer functions. Pulling them out of the
widget code made them testable, and the tests now written fail against the old
implementation (13,064 px vs 6,796 px for 144 books).

**If a function needs a display, the arithmetic inside it usually does not.**
Extract the numbers and test those. See also §19: the seven row tests passed
throughout, which made the change *feel* verified while the part that actually
broke had no coverage at all.

### Related

§21 was "when you fix the thing a check was watching, re-derive what the check
proves". This is the neighbouring failure: **having tests for one half of a
change is not having tests for the change.** The half with coverage was the
half I found interesting, not the half most likely to be wrong.

---

## 23. A CI step that pushes must rebase, or it fails on someone else's commit

Two runs in a row failed at `rustfmt (auto-fix and push if needed)`. Nothing
was wrong with the code, and nothing was wrong with the formatting — the
formatting had already been applied successfully. The step ended with:

```yaml
            git commit -m "style: rustfmt auto-fix"
            git push
```

A bare `git push`, no rebase. Meanwhile the `screenshots` and `scale` jobs
commit `ci-logs/` to the same branch. When one of those lands between this
job's checkout and this step, the push is rejected, the step exits non-zero,
and — because it has no `continue-on-error` — **the entire run fails.**

Every other auto-committing step in the same workflow already got this right:

```yaml
            git pull --rebase --autostash origin "$GITHUB_REF_NAME" || true
            git push origin "HEAD:$GITHUB_REF_NAME" || true
```

The rustfmt step predates them and was never brought in line.

### The second cause, which was mine

The race explanation above is real but incomplete, and the incomplete version
sent me down another wrong path. There is a second, *deterministic* collision:
the step immediately before rustfmt is **`Cargo.lock (generate and push if
missing or stale)`**, which also pushes — a step I added earlier the same day.
When it commits, rustfmt's bare push is rejected **every single time**, not
intermittently.

So an earlier fix of mine turned a rare flake into a reliable failure. Worth
sitting with: adding a second writer to a branch is not a local change, it
changes the failure rate of every other writer.

### Why it cost more than it should have

The failure surfaces as "rustfmt failed", which reads as *your code is
badly formatted*. I spent two pushes guessing at what rustfmt wanted and
reformatting code by hand — including one commit whose entire message was a
theory about a `matches!` arm. Both guesses were wrong, because there was
nothing to fix.

Two things would have cut that short:

1. **Check whether the step's own action succeeded before assuming its subject
   did.** `cargo fmt` ran fine; the *push* failed. The step name conflates the
   two.
2. **The diff was never published.** The sandbox cannot download Actions logs
   (§ the whole reason `ci-logs/` exists), so a formatting failure said only
   "failed" with no detail. Now the step writes
   `ci-logs/rustfmt-latest.diff` — a check whose output cannot be read is
   barely a check.

### Attempt two also failed, and the lesson is bigger than the rebase

Adding the rebase did not fix it. The step kept failing, no `rustfmt auto-fix`
commit landed, and no diff was published — so `git push` was still the thing
exiting non-zero, for a reason I could not see, because the sandbox cannot
download Actions logs.

At that point I had spent **four runs** on a formatting step. The mistake was
treating it as a puzzle to solve rather than asking why formatting was allowed
to fail a build at all.

**The right fix was to remove the failure mode, not diagnose it.** rustfmt now
formats, prints and publishes the diff, restores the tree so clippy sees the
real source, and cannot fail the run. The agent applies the formatting in its
next commit — which is where it belonged anyway, rather than arriving as a
drive-by commit from CI that everyone then has to rebase around.

### And then the real cause, which was none of the above

With the report-only step installed, it *still* failed — a step that only
formats and prints. That is only possible if `cargo fmt` itself errors, and
`cargo fmt` errors when the code **does not parse**.

It did not parse. My script for appending tests finds the file's last `}` and
splices before it. That is fine when the file ends with `mod tests`. This file
had since grown new functions *after* its test module, so the last `}` belonged
to `set_global_pref` — and 70 lines of `#[test]` functions were spliced **inside
that function's body**.

Braces still balanced, so my balance-checker said OK. Nothing looked wrong in a
`tail`. It was invisible to every check I had, and the only thing that could see
it was a parser.

**Six runs**, and the first five were spent on a formatting step that was
correctly reporting "this file is broken" in the only way it could.

### The rules

1. **Any CI step that pushes must rebase first, and must not fail the build if
   the push loses a race.** Better still: **a cosmetic check should not be able
   to fail the build at all** — while rustfmt could fail the run, it masked
   clippy and the build entirely, so a syntax error hid behind a formatting
   error for five runs.
2. **Do not append code by locating the last brace.** Anchor on something that
   identifies the *place* — the `mod tests {` line, a named marker — because
   "the end of the file" stops meaning "the end of the test module" the moment
   anything is added after it.
3. **A balanced-brace check does not mean it parses.** Text surgery on source
   needs a parser or a compiler; everything short of that will confirm a broken
   file looks fine. More generally: when several jobs write
to one branch, every writer needs the same conflict discipline. One that does
not have it will fail intermittently, on a schedule that looks random and
correlates with nothing in the diff.

### Related

§21 was about a check whose *verdict* stopped meaning anything after a change.
This is the sibling: a check whose *failure mode* has nothing to do with what
it checks. Both waste time by pointing the investigation at the diff.

---

## 24. Making a setting global makes every test depend on the developer's machine

P6.5 moved app-level preferences out of the library database into
`~/.config/kalam/prefs.json`, so switching library would stop resetting your
theme and reader settings. Correct change. It immediately broke a test that had
nothing to do with libraries:

```
service::tests::history_and_lookup_history_return_rows_without_errors ... FAILED
```

`log_dict_lookup` skips logging when `dict_history_enabled` is off. That pref
had just become global — so the test was now reading a JSON file in a real home
directory, outside the repository, that the test had never heard of and could
not control. On a machine where that setting happened to be off, the test
failed. On a fresh CI runner it might pass. Nothing about the *code under test*
decided the outcome.

That is §19's rule from the other direction: not "a test that cannot fail", but
**a test whose result is decided by something outside the test.** Both are
tests in name only.

### The fix

Global prefs are inert under `cargo test`: reads return empty, writes are a
no-op. A `cfg!(test)` guard rather than a temp directory, because these
functions are called from everywhere and threading a base path through every
caller to serve the tests would be worse than the problem. The classification
rule itself (`is_global_pref`) is pure and keeps its own tests.

There is a test asserting the guard works, which matters more than it looks —
if it ever regresses, the symptom is not a failure but a suite that quietly
starts depending on whoever runs it.

### The rule

**When you move state outside the repository — a config file, an environment
variable, a keyring, a server — every test that touches it becomes a test of
the machine.** Decide at that moment how tests will be isolated from it. Not
after CI goes red for a reason that looks unrelated to the change.

---

## 25. A log published only on failure will outlive the failure

`ci-logs/test-latest.txt` and `ci-logs/clippy-latest.txt` were written only
when their step failed. That sounds economical and is a trap: once the problem
is fixed, the *old failing log stays committed*. Every later green run still
shows a red file.

It misled me three times in one session. The last was reading
`test result: FAILED. 301 passed; 1 failed` and starting to investigate,
before noticing the run id at the bottom belonged to a run two hours dead while
the current one was green.

The `--- run <id> ---` footer is what saved it each time, and it only worked
because I thought to check. A file that requires you to remember to check
whether it is current is a booby trap, not a diagnostic.

### The rule

**A published artifact must always describe the latest run.** Write it on
success too — "clean" is information. If a file can be stale, someone will
read it as current, and the more convincing it looks the longer they will
believe it.

### Related

Same shape as §21 and §23: a check whose *output* stops corresponding to
reality. §21 was a verdict that no longer matched what it measured, §23 a
failure whose cause was unrelated to what it checked, and this is a result that
outlives the run that produced it. In all three the investigation goes to the
wrong place, and in all three the fix is to make the signal honest rather than
to get better at interpreting a dishonest one.

---

## 26. A test helper placed between test functions is invisible to the panic guardrail

The guardrail that keeps production `.unwrap()`/`.expect()` counts from growing
(`tests/guardrails.rs`) walks brace depth and suppresses everything inside a
`#[cfg(test)] mod tests`. But the suppression is per *item*, not per module:
when a `#[test]` function inside the module closes, the module-level
suppression has already been overwritten by the function's own, and it is not
restored. The practical effect: **code inside `mod tests` but sitting after the
first test function's closing brace is counted as production code.**

This repo's test modules have always placed their helpers (like `seed` in
`db.rs`) *above* the first `#[test]`, so the baseline of 12 never moved. A new
helper written in the natural reading order — helpers at the bottom, or between
tests — silently pushes the count over the cap, and the failure names the
guardrail, not the placement.

Found while adding `scratch_library` to `src/paths.rs`: the count went 12 → 13,
the debug scan showed the helper's `.expect("scratch dir")` as the only new
hit, and no amount of reading the helper explains it — the position was the
whole bug.

### The rule

**In `mod tests`, helpers go above the first `#[test]` function.** If a helper
must live elsewhere, it must not contain `.unwrap()` or `.expect(` — use
`match`, `?`, or `unwrap_or` — or the guardrail will count it no matter how
test-only it is.

## 27. Status claims come from the repo, never from memory

The owner asked "what is P7/P9/P12?" and "isn't the custom renderer done?" —
and was right to be angry. The reply had cited the **archived** P0–P12 phase
plan (fiction sources, manga sources, Lua plugins, "renderer research parked")
as if it were current. ROADMAP.md's first paragraph says it replaced that plan
on 2026-09-18 and that the old write-ups live in
`docs/archive/roadmap-phases-p0-p12.md` "for their lessons, not for planning."
The custom renderer is not research — it shipped with the engine swap and is
the core of the app.

Two failure modes stacked up:

1. **Condensed session memory carries old vocabulary.** A conversation summary
   preserves phase numbers from weeks ago but not the sentence that says they
   are dead. Speaking from that summary without re-reading the roadmap
   resurrects the old plan.
2. **The moved-base fault makes git lie.** The sandbox HEAD had silently
   fallen back to the branch point `29788b4` (documented in the 2026-09-19
   changelog row, and it recurs). `git ls-files assets/models/` answered
   "empty" against that old commit, which became a confident "the OCR models
   are not in git" claim and a proposed fix for a problem that did not exist —
   the models are committed (`b482d03`).

### The rule

- **Before any claim about what is shipped, what is next, or what a phase
  number means: read the "What is really shipped" table and the changelog tail
  of ROADMAP.md in this session.** Never cite a phase number from memory.
- **After any environment reset or surprising git output, run `git fetch`
  followed by `git reset --mixed origin/<branch>` before believing `git
  ls-files`, `grep`, or `git log`** — they all answer for whatever commit HEAD
  happens to be on, and a fallen-back HEAD makes committed work look missing.
- The cost of this one was a full session's worth of owner trust in the plan.
  The fix is cheap: read first, then talk.

## 28. A removal leaves orphans: helpers whose only non-test user was the feature

**The fault (2026-09-30, comic-OCR removal).** Deleting the comic OCR
feature took its call sites with it, but three survivors looked innocent:
`compute_comic_page_layout` in the comics reader (pure geometry, "surely
still used"), the `k-sel-*` CSS classes in `resources/style.css`, and
`select_rect`/`word_at`/`line_at` on `PdfPageText`. The layout helper had
in fact lost every non-test caller — clippy runs with `-D warnings`, so the
first CI run would have failed on a dead-code warning for a function that
still *looked* alive because a unit test called it. The CSS classes and the
`PdfPageText` methods were the opposite trap: they looked comic-specific
but are shared with the PDF reader's selection toolbar, so deleting them
would have broken live code.

### The rule

- **After any removal, grep every helper the removed code called for
  non-test users (`grep -rn ... | grep -v "mod tests"` region) before
  pushing.** A helper kept alive only by its own unit test is dead code
  under `-D warnings`; a helper that *looks* owned by the removed feature
  may be shared with a sibling reader.
- Check CSS classes the same way before deleting them — class names are
  global, and the PDF reader reuses the comics reader's toolbar styling.

## 29. Runtime warnings are findings, not noise — record and chase every one

**The fault (2026-10-01).** The owner's terminal showed three
`Pango-WARNING: failed to create cairo scaled font ... the offending font
is 'Noto Color Emoji 8.8' ... scaled_font status is: out of memory` lines
while using the app. Nothing crashed, so nothing forced a look — but the
warning carried a complete diagnosis if read: the `8.8` is a font size,
and the only labels in the app at 0.88rem are the reader pill counter and
a handful of small-text classes. Cross-checking which of those can contain
emoji pinned it in minutes: the comics reader's "Series Completed" pill
label. Five color-emoji strings across three files (comics end card ×2,
streak strip, watch-folder checkboxes ×3) were sending Pango to Noto Color
Emoji, whose CBDT bitmap font cairo cannot scale on the owner's system.

### The rule (owner directive, 2026-10-01)

- **Every runtime warning the owner reports, and every mistake the agent
  makes, gets a pitfalls entry — every instance, no exceptions.** A warning
  that is "just cosmetic" is still the app telling us where it is fragile.
- **Diagnose to the exact line before fixing.** A warning text usually
  contains its own coordinates (here: a font name and size that matched one
  CSS class). Guessing produces fixes for the wrong string.
- **Re-read this file at the start of each phase** (and whenever a similar
  problem appears), so the same fault is never paid for twice.
- **Color emoji never go in GTK label strings.** Pango's fallback to a
  color bitmap font + cairo's inability to scale it equals three warnings
  and a missing glyph on affected systems. Use plain text, a monochrome
  dingbat (✓ ✕ ★ — not covered by Noto Color Emoji), or a symbolic icon.

## 30. Check HEAD before committing — the moved-base fault can strike between edits and commit

**The fault (2026-10-01, 5th occurrence of the moved-base fault).** The
sandbox HEAD fell back to the old base (`29788b4`) at the start of a turn
— after the previous turn's edits were already committed and pushed, so
nothing looked wrong. The turn's own edits were then made and committed
with `git add -A` **while standing on the stale base**, producing a commit
whose tree was correct but whose parent was a month old. The push failed,
and the rebase produced conflicts in every file changed since the old
base, plus a stale `Cargo.lock` (an older transitive-dependency version)
riding along in the commit.

### The rule

- **Before every `git add -A && git commit`, run `git log --oneline -1`
  and confirm it is the expected tip, and check `git status --short` lists
  only the files this turn intentionally touched.** A commit on the wrong
  base is not a fast-forward away — it is a rebase conflict festival and a
  vehicle for stale files.
- **Recover by re-parenting the tree, never by re-editing files:** if a
  commit with the correct tree has the wrong parent, `git reset --soft
  origin/<branch>`, restore `ci-logs/` (and any other CI-owned files) from
  origin, then commit again. The tree was right; only the parent was wrong.
- Turn-start hygiene (already the rule, now with a reason): fetch +
  `reset --mixed origin/<branch>` **before editing anything**, because the
  fallback can happen between turns, not only between sessions.
- *Sixth occurrence, later the same day:* the pre-commit check above
  caught it — `git log -1` showed the old base after `git add -A`, before
  anything was committed. Recovery as described: fetch, `reset --mixed`
  onto the tip, restore `ci-logs/` from origin, commit the two intended
  files. The check works; keep running it.

## 31. An async rewrite must re-home every side effect of the sync path it replaces

**The near-miss (2026-10-01, PDF plan step 1; caught before commit, never
shipped).** Moving the PDF open off the UI thread meant rewriting
`ensure_page_text`, whose old body did three things: extract the vector
text, **look up the persistent OCR cache** on a scanned page
(`load_page_ocr`), and only then trigger background OCR. The rewrite
initially sent just "extract text" to the worker and fell back to OCR on
an empty answer — the cache lookup had silently vanished. It would have
compiled, passed every test, and regressed exactly one behavior: a page
OCR'd on a previous visit would re-run seconds of neural inference
instead of loading instantly. Nobody would notice until an owner field
report.

### What caught it

Not the compiler, not the tests — a **diff read against HEAD before
committing** (`git show HEAD:file | grep load_page_ocr` showed a
production call site that no longer existed anywhere in the tree).

### The rule

- Before replacing a synchronous function, **enumerate every effect it
  has** (returns, caches read, caches written, triggers fired, state
  set) and tick each one off in the new design — off-thread is not a
  reason for any behavior to disappear; the DB read just moves into the
  worker.
- End every multi-pass edit with a full `git diff` read plus a grep of
  the old function's callees in the new tree. Compile-clean proves
  nothing about dropped logic.
- State the old path's behaviors in the plan before rewriting, so the
  diff can be checked against a written list, not memory.

## 32. Two gestures on one widget must coordinate — a drag-end can eat a double-click

**The bug (2.18, found by the owner's field test 2026-10-01 after the plan's
step 3 shipped).** "PDF double/triple-click selection broken" was diagnosed
in planning as the dead-click-on-unready-page problem — text not extracted
yet, so the click had nothing to select. That was real but secondary. The
actual root cause: the PDF reader attached `GestureDrag` and `GestureClick`
to the same overlay with no coordination, and `SelectionDragEnd`'s
no-movement path clears the active selection. On the second press of a
double-click, the click gesture selects the word — and the drag gesture's
own release then runs `clear_selection()` on it, instantly. The selection
was made and destroyed within one press/release pair, so it looked like
double-click "never worked". It never had, since the gestures were written.

**The fix, copied from the EPUB reader's view** (`kalam-reader/src/view.rs`),
which has worked from day one:

- Attach the click gesture BEFORE the drag gesture — controllers run in
  addition order, so the click has counted the press by the time the drag
  decides what to do with it.
- Share a `multi_click: Rc<Cell<i32>>` between them; `drag-begin` stands
  down entirely (no message, no drag state) when `multi_click >= 2`.
- `drag-end`'s no-movement "clear the selection" path only runs when that
  press actually stored drag state — a stood-down press's release must do
  nothing.

### The rule

- When two controllers can both react to the same press, write down the
  full press/release interleaving for every click pattern (single, double,
  triple, drag, tap-on-selection) before trusting the wiring — the bug
  lived in the *sequence*, not in either handler alone.
- A "tap clears the selection" release handler is incompatible with
  multi-click selection unless something tells it which press it belongs
  to. The absence of that coordination is invisible until a human
  double-clicks.
- Copy the working reader's gesture arrangement rather than inventing a
  parallel one; the EPUB reader's `multi_click` gate is the reference.

## 33. `Some(ref x)` on a `HashMap::get` result is a reference to a reference

**The mistake (CI run 36831842897, 2026-10-01).** Two new lines in the PDF
reader's drag handlers wrote `if let Some(ref ov) = self.page_overlays.get(&slot)`
— but `get` already returns `Option<&Overlay>`, so `ref ov` binds `&&Overlay`,
and clippy under `-D warnings` rejects it ("this pattern creates a reference
to a reference") before the build ever runs. The codebase's own idiom, two
hundred lines above, is `if let Some(ov) = self.page_overlays.get(&slot)`.

### The rule

- Match what the expression already is, not what it was at its definition:
  `.get()`, `.iter().next()`, and friends hand you a reference — bind it
  plainly, never with `ref`.
- Same class as §31's `255.0/255.0`: a one-token slip that only CI can see,
  because there is no local compiler. Before writing a match pattern on a
  method's return, say the type out loud.

## 34. A cursor lifecycle must be closed: every release needs a resting state

**The bug (owner field re-test 2026-10-01, one day after the hand cursor
shipped).** After dropping a dragged selection handle in the PDF reader the
cursor glitched: the closed hand vanished and the plain arrow sat over the
grip until the pointer moved. Two mistakes in the new cursor code:

1. The drag-end handler reset the cursor to the default (`None`) -- but no
   motion event arrives after a drop until the pointer moves, so whatever
   the release sets is what the user stares at. `None` also clobbered the
   page's own base cursor.
2. The PDF page overlay already had a resting cursor, the text I-beam,
   set once in `wrap_page`. The motion and leave handlers wrote `None`
   anywhere that was not a grip, so the first mouse move silently replaced
   the I-beam with the plain arrow page-wide.

**The fix.** Every cursor path lands on an explicit named state: I-beam
over the page, open hand over a grip, closed hand during a grip drag. The
drag-end parks the cursor on exactly what the motion controller would
compute at the release point (`press + offset`, same hit function, same
coordinate space), so resting and moving can never disagree.

### The rule

- A widget whose cursor you touch has a life beyond your gesture. Find its
  resting cursor (the one set at creation) before writing `None` anywhere;
  `None` means "the parent's cursor", not "no cursor", and it overrides
  whatever the widget set for itself.
- Events stop when the pointer stops. Any state you compute from motion
  events (hover, cursor) must be recomputed at gesture end from the
  gesture's own coordinates, or it goes stale in front of the user.
- Copying a working handler from another widget (`view.set_cursor(None)`
  in the EPUB reader, whose area has no base cursor) without asking what
  the target widget's resting cursor is -- the two `None`s meant different
  things.

## 35. Port the reference gesture pattern completely -- the multi-click gate belongs on begin, update AND end

**The bug (owner field re-test 2, 2026-10-01).** After the gesture fix
(pitfalls 32) the PDF reader's triple-click selected a line, but a
double-click showed the action chip with no selection behind it. Root
cause: the EPUB reader gates its drag gesture on the live `multi_click`
count in all three callbacks -- begin, update, and end, each reading the
count fresh at that moment -- while the PDF port gated only the begin
("begin stood down, so there is no drag state, so the release is safe").
That inference is not one GTK makes: a press's release can fail to arrive
at the drag gesture (sequence claiming by the click gesture), leaving a
stale drag state behind. The next press's movement then ran a selection
drag from the stale state -- replacing the word the double-click had just
selected with an empty one -- and the next release consumed the stale
state as a tap and cleared it. The chip stayed up because only
`clear_selection` dismisses it, and the update path's empty-rects branch
did a bare `replace(None)`. The triple-click survived only because
release two had already consumed the stale state: the line its third
press selected had nothing left to destroy it.

**The second half of the same report:** word and line selections stored
the *click point* as `anchor_pt`/`active_pt`. Grabbing the end handle
then ran `select_between(click_point, cursor)`, restarting the selection
from the middle instead of extending it -- "the end handle became the
starting handle". Selections made by dragging were fine (their anchor is
the drag start), which is why it only *sometimes* failed.

### The rules

- When porting a proven gesture arrangement, port all of it. Every
  callback that can act on a press (begin, update, end) gates on the
  live multi-click count read at that moment; "the begin stood down, so
  nothing else can happen" assumes a signal ordering GTK does not
  guarantee.
- A stored gesture state is a liability across presses: any release that
  fails to arrive leaves it alive. Consumers must be defensive -- gate on
  what the press *was* (the count), not on what was stored.
- A selection's drag anchors are the selection's ends, never the click
  that made it. Anything that can later drag from a handle reads them.
- The chip must never outlive its selection: every path that removes a
  selection dismisses it, including the drag-update empty-rects branch
  that bypasses `clear_selection`.

## 36. The moved-base fault recurs — check HEAD before every commit

**Third occurrence, 2026-10-02.** Between turns the sandbox's git state
fell back to the branch point `29788b4` (fresh clone, upstream unset)
while the working tree kept the full session state. A commit made
without looking first captured 173 files / 37,500 insertions — the whole
session re-committed on the base — and only the unset upstream (push
refused) kept the mess off the remote. Same fault, same recovery as
2026-09-19: fetch, `git diff FETCH_HEAD` to isolate the genuinely
intended changes (here: two files), `git reset --hard FETCH_HEAD`,
re-apply, commit, push.

**Fourth occurrence, 2026-10-02 (afternoon), same day as the third.**
Again the sandbox's git metadata fell back to the branch point between
turns — this time without the unset-upstream tell: the push target
still resolved, so the first symptom was the push being *refused*
(non-fast-forward: the remote held the session's pushed commits plus
CI publish commits). The commit had already been made blind on the
wrong base; the tell was in `git commit`'s own output — `create mode
100644 plan.md` for a file known to be tracked, and an insertions
count far too large. Recovery: `git fetch origin`, then
`git reset --mixed <remote tip>` — mixed, not `--hard`, because the
working tree held the only copy of the uncommitted changes — then
`git checkout -- ci-logs/` for the stale generated logs, re-commit the
two real files, push. Nothing lost; the misplaced commit became
dangling. The rule below caught it one step later than it should have:
the status check happens before the commit, and it did not run first.

### The rule

- At the start of every turn, before any commit: `git log --oneline -1`
  and `git status`. If HEAD is the branch point instead of the session's
  last commit, or the upstream is unset, stop and recover — never commit
  blind, and never trust a silent push: check that the push actually
  moved the remote and that a CI run started for the new head.
- Two more tells of a fallen base: a commit output line saying
  `create mode` for a file you know is tracked, and a file-count/
  insertions count that dwarfs the edit you just made. If either
  appears, stop before pushing — the commit is on the wrong base.

### Eighth occurrence, 2026-10-02 — the reset itself destroyed work

Occurrences four through seven were all caught harmlessly by the HEAD
check. The eighth introduced a new failure mode: the check ran at
**commit time, over a dirty tree**, and the remote had moved *during* the
turn (CI's smoke-report publish job pushed `ca582cd` while the edits were
being made). The recovery reflex — `git reset --hard FETCH_HEAD` — ran
over a working tree full of uncommitted edits and **wiped them all**.
Only the new, untracked `src/stall.rs` survived (reset does not touch
untracked files), so the commit that followed contained one file out of
nine and pushed a half-step: an unreferenced `.rs` file that cargo never
compiles, on a green CI run. The missing eight files were rebuilt from
the session record and pushed as the follow-up commit.

**The added rule:**

- **Never run `git reset --hard` over a dirty tree.** If a moved base is
  discovered with uncommitted work, first `git stash` (or commit locally,
  then rebase), *then* reset/rebase, then restore. The reset-recovery
  steps in this entry were written for a clean tree; with edits in
  flight they are a shredder.
- The HEAD check protects a **commit**; it cannot protect **edits**. On
  any multi-edit turn, expect the remote to have moved underneath you
  (the CI publish job commits on its own schedule) — fetch before the
  final commit, and if it moved, stash-rebase-restore rather than
  reset-and-lose.
- A green CI run proves nothing about *completeness*: a half-landed
  step whose missing files are all unreferenced compiles clean. Read the
  committed diff stat against the intended file list before calling a
  push done.

## 37. "No precedent" claims need a repo-wide grep, not a page-local one

**2026-10-02, during 2.21.** The 2.21 plan recorded that the confirm
dialog would need "a gtk::MessageDialog; no existing dialog precedent
to copy". There was a precedent: `src/widgets/in_app_dialog.rs` is the
app's modal-dialog system (its module doc explains exactly why a
`gtk::Window` dialog is wrong on a tiling compositor, and pitfalls §2
records the teardown rules), and `src/widgets/remaster_dialog.rs` is a
complete confirm dialog — Cancel/confirm buttons, CSS classes, and the
Rc-shared callback pattern GTK's `Fn` handlers need. The planning
research grepped for dialogs only inside the reader page and concluded
from the absence there. The shipped dialog uses the in-app system; the
plan's parenthetical was simply a research miss.

### The rules

- Before writing "the codebase has no X", grep the whole tree for X.
  `grep -rn "dialog" src/` would have found both files in seconds;
  check `src/widgets/` first — shared UI machinery lives there.
- The first PDF write path in the app also settled some mupdf-rs 0.8
  facts worth keeping (docs.rs re-export pages can 404; the raw source
  at raw.githubusercontent.com/messense/mupdf-rs/v0.8.0/ is the
  reliable reference): `PdfDocument::open` takes `&P where P:
  AsRef<FilePath>` — pass a `&str`, the same convention as
  `Document::open` in `src/pdf.rs`; saves are
  `save_with_options(filename: &str, options)` and
  `write_to_with_options(&mut W, options)` with `PdfWriteOptions` at
  `mupdf::pdf::PdfWriteOptions`; `shape::TextOptions` has a lifetime
  (the `fontfile` field), so build it with `..Default::default()`;
  `Shape::insert_text` maps the insertion point through the inverse
  page CTM, so points are view coordinates (top-left origin, y down) —
  the same space the OCR quads already use; and glyphs are emitted
  along the unrotated axis with the line clip measured against the
  unrotated media box, which is why pages rotated 90°/270° must be
  skipped rather than embedded.

**Same feature, first CI round (16 errors, all caught by clippy).**
Three are general and cost a full CI round trip each:

- With `use gtk::prelude::*` in scope, a bare `writer.flush()` is
  ambiguous against gdk's `DisplayExt::flush`, and the error is
  reported as `IsA<gdk::Display>` not implemented for your writer.
  Use the fully qualified `std::io::Write::flush(&mut w)` in files
  that import the GTK prelude.
- `MainContext::default().invoke` requires a `Send` closure, which
  sinks any non-Send callback parameter the closure carries. When
  every caller is a main-thread button handler, call
  `tasks::spawn` directly (it is main-thread-only anyway) instead of
  wrapping in `invoke`; reserve `invoke` for worker-thread entry
  points like `pdf_ocr::enqueue_import_scan`.
- A confirm dialog's callback is an `Fn` closure (GTK's handler
  kind), so it cannot move its captures into the task starter —
  exactly what `remaster_dialog.rs` already documents, and what its
  `Rc` pattern exists for. Rc the moved values and clone them out per
  invocation. Reading a precedent is not the same as applying it:
  the handler was written without the Rc and failed with E0507.
- Renaming an `_`-prefixed parameter (here `_root` → `root`) breaks
  body references that were legal despite the prefix — grep the old
  name before renaming.

## 38. Green in CI is not correct in the field — and "done" waits for the field

**2026-10-02, 2.21's whole life cycle: planned, shipped CI-green, and
withdrawn the same day.** The embed feature's unit tests included a
full round trip — write the invisible text layer, save, reopen,
extract, find the word back. It passed; CI was green; the item was
reported as done. The owner's field test on a real book found the text
"not embedded correctly" (his words; no further detail — he chose
removal over debugging, and the feature is parked until the app is
complete). Three separate "done?" answers had been given while his
test was still pending.

### The rules

- A round trip through the same library that wrote the file proves
  self-consistency, nothing more. "Any application can search this
  file" is an interop claim; it is only tested by a different
  implementation — pdftotext, pdfium, a real viewer. If this feature
  returns, the CI test must run the written file through a second
  renderer, and the field test comes before the item is called done.
- When a defect report arrives with no detail and the owner chooses
  removal, record exactly that — the report, the decision, the missing
  detail — so the retry starts by reproducing the failure instead of
  re-deriving a design that already failed once.
- An item's done state includes the owner's visual QA for anything he
  can see or run. The honest status before that is "awaiting field
  test", never "done".
- Scope honesty cuts both ways: when a proposed item is a nice-to-have
  rather than core reading flow, say so at planning time. The owner's
  verdict on 2.21 was "we are straying again" — a scope call he should
  not have had to make after implementation.

**Fourth occurrence, 2026-10-02, later the same day.** The base fell
back to `29788b4` again between turns; this time the rule was not
followed — a plan.md commit was made without a HEAD check and landed
on the branch point ("create mode 100644 plan.md" was the tell: the
file already existed on the session's tip). The unset upstream again
kept it off the remote (push refused, non-fast-forward). Same recovery:
fetch, `git diff FETCH_HEAD <bad-commit>` to isolate the one intended
file, `git reset --hard FETCH_HEAD`, restore that file from the bad
commit, commit, push. The tell is worth stating plainly: a commit that
"creates" a file the session already tracks, or a diff whose size
suddenly equals the whole session, means the base moved — stop, do not
force anything, recover.

**Fifth occurrence, 2026-10-02, minutes after the fourth.** The §36
check was run and *ignored*: `git log --oneline -1` printed the branch
point `29788b4`, and the commit went ahead anyway — the check had
become a ritual, not a gate. The tell fired again ("create mode" for
plan.md, a tracked file). Same recovery. The rule, sharpened: the HEAD
check is not a step to perform before committing — it is a condition
for committing. Branch point shown means no commit, full stop, recover
first.

## 39. A value moved by one arm of a match is moved for the whole match

**2026-10-02, item 2.22's first CI round: two E0382s of the same shape.**
A `match` produced `(dir, file_part)` in one arm and
`(other_dir, format!("…{file_part}…"))` in the other — the inline
`format!` captures borrow, so that arm looked like the safe one, but the
tuple arm *moved* the value. Every use after the match —
`dir.join(&file_part)` two lines down — failed with "borrow of moved
value", even though "the arm that moved it is not the one my code took"
feels like it should not matter. Move semantics are static: if any arm
can move it, it is moved.

**Do instead:** when a value is needed after a match that must produce
it in one arm, `clone()` in that arm (and say why in a comment), or
have every arm borrow and clone once at the end. And when editing
without a compiler, read each new `match` arm asking one question:
*does this arm move anything used below?* — the borrow checker reads
the whole match, not just the diff.

The stale-log trap from §11 recurred in the same round:
`test-latest.txt` still ended with the *previous* run's footer, and only
the commit list showed which publish commit carried the real failure.
Read the `--- run <id> ---` footer before believing a log is current.

## 40. The disk is the truth — and a collapsed card deletes at its entity's scope

**2026-10-02, item 2.22's first field test on the owner's real library
found two defects the CI-green suite could not:**

**Defect one: every comic cover stayed behind in the old per-book
folders while the chapter files moved.** The placement pass moved the
cover by the row's `cover_name` — resolve the row's name, rename that
file — and when the row and the disk disagreed (a name that resolved
nowhere, or a move that failed), the error went through `let _ =` and
the chapter simply lost its cover, its old folder staying behind with
the orphan. The exact divergence on the owner's machine was never
confirmed — no access to his database — which is itself the lesson:
the fix could not be "correct the row"; it had to stop trusting the
row. The sweep now lists the old folder and moves what is *there*
(cover images, `kalam.json`), letting the row catch up; a repair pass
adopts the same leftovers for chapters already placed; and every file
operation in the pass logs instead of being discarded.

**Defect two: deleting a comic from All Books removed exactly one
chapter.** The grid collapses a series to one card backed by one
representative book row, and the delete flow deleted *that row*. An
action on a collapsed entity must operate at the entity's scope:
`delete_book` is now series-scoped for comics, the confirmation counts
the chapters (`delete_scope_count`), and the toast reports the real
number.

### The rules

- A migration that moves files must be driven by the directory
  listing, never by metadata that claims to describe it. The disk is
  the truth; rows catch up.
- Never `let _ =` a file operation in a migration. A swallowed error
  is a stranded file that nobody will see until the owner browses the
  folder.
- Unit tests seeded from the same assumptions as the code cannot catch
  row/disk divergence — the tests built rows whose names matched the
  files, so the move always worked. A migration test needs at least
  one case where the row lies (a cover with no name, a name with no
  cover).
- When the UI presents a collapsed entity (a series card, a drawer
  entry), every destructive action on it goes through the entity, not
  the representative row behind it.
- A feature that already ran in the field needs a healing path, not
  just a corrected forward path: the adoption pass repairs libraries
  already in the broken state, on the next launch, with no user
  action.

**Sixth occurrence, 2026-10-02, later still.** The base fell back to
`29788b4` once more between turns — and this time the rule worked as
intended: the HEAD check ran before any edit, saw the branch point, and
no commit was made on the wrong base. Recovery was the plain three
steps (fetch, `reset --hard FETCH_HEAD`, redo the intended change). No
new lesson; the record stays complete because the fault itself keeps
recurring even when it is caught.

---

## 41. The guardrail ratchets count your new code — check them before you push

**2026-10-02, 7.1 step 0.** The step added two `.expect("activity lock")`
calls to `src/timing.rs` — natural-looking, matching the file's existing
span-lock style. CI failed on `tests/guardrails.rs:
production_code_panics_do_not_grow`: the repo ratchets production
`.unwrap()`/`.expect(` at 12 (WORKING.md §1 says zero; the ratchet holds
the real number and may only go down), and the two pushes had grown it to
14.

The first wrong response would have been raising `MAX_PROD_PANICS` — that
is the ratchet running backwards. The second wrong response would have
been restructuring the code to dodge the string match. The right response
was asking what the check proves: an unhandled panic on a *diagnostics*
path (the stall watchdog's activity stack) is strictly worse than a
poisoned lock carried through — a poisoned lock means another thread
already panicked, and the diagnostic must not answer with a second panic
on the UI thread. Both sites now use `.unwrap_or_else(|e| e.into_inner())`
— the same pattern the watchdog's read paths already used — and the count
is back to 12 with the documented per-file distribution.

### The rules

- `tests/guardrails.rs` is part of the repo's contract, not CI trivia.
  Before pushing anything that adds `.unwrap()`/`.expect(` to `src/`,
  `Arc<Catalog>` to `src/pages/`, or literal hex colours to
  `resources/style.css`, count first — the ratchets are the same three
  shapes and they never go up.
- When a ratchet catches your change, the fix is almost never the
  constant. Ask what the check proves; on a diagnostics path the answer
  is usually "carry on with `unwrap_or_else(into_inner)`", because
  instrumentation that can crash the app is worse than no instrumentation.
- Re-run the count locally (it is a 20-line string match, reproducible in
  one python snippet) rather than spending a CI cycle on the discovery.

---

## 42. A recurring GTK critical means the mitigations cover only some paths

**2026-10-02, the owner's 7.1 timing run.** Mid-session, during navigation
away from the EPUB reader:

```text
(kalam:272759): Gtk-CRITICAL **: gtk_widget_is_ancestor:
assertion 'GTK_IS_WIDGET (widget)' failed
```

This is the **third appearance** of this critical, and the first two were
fixed with in-code mitigations that are still there, each with a comment
naming this exact message:

- `AppModel::detach_current` (`src/app.rs`): unparent the page *before*
  dropping the controller — dropping first makes GTK probe a disposed
  widget.
- The float-close path (`src/app.rs`, `CloseBookDialog` handler): the
  rebuild of the page underneath a closing dialog is deferred to an idle
  callback — rebuilding while GTK unwinds the close signal is what
  produced the criticals there.

**This instance:** it fired inside a ~250 ms UI block about 8 s after
`route_open:reader`, at the moment the session navigated reader → home.
The reader page is not a cacheable route, so Back drops it through the
mitigated detach path — meaning something *else* still held or touched a
finalized reader widget (a gesture, an engine view callback, a task
applying to a dead page). Not reproducible from the log alone.

### The rules

- Every appearance of a runtime warning gets recorded, even when a fix
  for a previous appearance is already in the tree — the count is the
  signal that the mitigations do not cover every path.
- The chase is now owed: whoever next touches reader teardown, float
  close, or page-cache dropping runs the session again and attributes
  this critical to its actual call site before calling that work done.
  Two mitigated paths and a third appearance means the pattern, not the
  two sites, is the problem.

## 43. Know the edition before you reach for new syntax — let-chains are not in this repo

**Date:** 2026-10-02, 7.1 step 2a (book page + book float migration).

**What happened:** while converting `SetRating` in `src/pages/book.rs`, the
first draft used a let-chain:

```rust
if saved && let Some(book) = self.book.as_mut() { ... }  // does not compile here
```

The workspace is **edition 2021**; let-chains in `if` conditions stabilized
in edition 2024. The code would not have compiled — and because this
sandbox has no Rust toolchain, nothing local would have said so. The
mistake was caught reading the diff, rewritten as a nested `if` inside
`if saved { ... }`.

**The rules:**

- Check `Cargo.toml`'s `edition` before using any syntax newer than the
  repo's baseline. Anything stabilized after edition 2021 is suspect
  here: let-chains, `gen` blocks, `unsafe extern`, if-let temporaries in
  match guards.
- With no local compiler, the diff read is the compile pass. Read new
  code as the compiler would — borrow by borrow, move by move — before
  pushing, because CI is a round-trip away.

## 44. §31's enumeration must run to the closing brace — a read can hide below the line you are editing

**Date:** 2026-10-02, 7.1 step 2a.

**What happened:** the §31 pass ("an async rewrite must re-home every side
effect of the sync path") enumerated the book page's reads and believed
`fill_author_card` performed two: the author profile and the avatar
photo. After the conversion, a re-read of the function found a third,
sitting ~70 lines below the signature: the other-books thumbnails row
called `crate::author::owned_books_for_author(catalog, first_author)` —
a full books-for-author query inside a fill function, invisible to the
line-level scan that found the first two.

It was folded into `BookPageSnapshot.author_other_books` (the service
reads it once; the page filters out the book itself when drawing), but
the miss proves the process hole: enumeration that stops at the "interesting"
lines is not enumeration.

**The rules:**

- Enumerate a function's reads **to its closing brace**, sequentially,
  before declaring it converted. The expensive call is wherever the
  author last needed data, not near the top.
- The same is true for any "no more X in this function" claim made from
  a diff: run the grep against the whole function body, not the changed
  lines.

## 45. The watchdog can only name what has a span — and an async apply path is a new blind spot

**Date:** 2026-10-02, 7.1 step 2a field test (owner's machine).

**What happened:** the 2a migration worked exactly as built —
`route_open:book` fell from 96-686 ms to **1 ms**, the float's dialog
span from 31 ms to **6 ms** — and the owner still reported "seems
faster, but still not instant", with the window and the information
appearing at once (no visible skeleton) and 751-1355 ms `[stall]`
blocks landing right after the spans ended.

Two lessons, one entry:

1. **A skeleton that fills before the first frame draws never exists
   for the user.** The worker round-trip (~100 ms on his machine) plus
   the apply handler's work block the main loop through the first
   paint, so the first frame the user sees already carries the data.
   That is acceptable — skeleton visibility was never the goal — but
   it means the *metric* is time to first painted content, and that
   number did not move enough: the remaining 0.7-1.4 s is the apply
   plus GTK realize/style/layout/paint of the filled tree, not the
   reads the migration removed.
2. **The apply path was born unattributed.** The `Loaded` handlers
   run outside any route or dialog span, so the watchdog attributed
   their blocks to whatever span ended last — a guess, and it looked
   like one ("after route_open:book (took 1 ms)"). The same blindness
   covers the idle work: `icons::init` at the first idle moment, the
   thumbnail backfill, the warm-cover swap stream — all main-thread
   costs with no label, which is why "no route or dialog span was
   open" appears in the log at all.

**The rules:**

- Every async apply path gets an `activity` guard (and a timing span
  under `KALAM_TIMING`) from the day it is written — an apply handler
  is a new place the UI thread can block, and the watchdog cannot
  name a place it cannot see. This is now part of the screen-open
  recipe.
- A migration step is not done when the reads move; it is done when
  the field log shows where the *remaining* block went. "Seems
  faster" from the owner is a measurement request, not a pass.
- Before stamping a recipe on more screens, prove it moves the number
  the user feels (time to first painted content), not the number the
  spans report (construction time).

## 46. Copy-ness can be load-bearing — a Copy-to-owned refactor breaks code that looks perfectly general

**Date:** 2026-10-02, 7.1 step 2a.1 (caught by CI, not by review).

**What happened:** widening the activity stack's labels from `&'static
str` to `String` (so a task's runtime name could ride along) broke
`last_ended_activity()` with E0507 — *cannot move out of dereference
of `MutexGuard`*. The getter's shape, `state.last_ended.map(...)`,
looks perfectly general; it only compiled before because the old tuple
`(&'static str, Duration, Instant)` was `Copy`, so `.map()` silently
**copied** the `Option` out through the guard's `Deref`. Make one field
owned and the copy becomes a move out of a borrow, and the failure
surfaces in a getter far from the type change. The same refactor broke
a test that constructed `ActivityGuard { label: "literal" }` — a
struct-literal site the signature change never touched.

The fixes were mechanical — `.as_ref().map(|(l, took, ended)| (l.clone(),
*took, ended.elapsed()))` and `.to_string()` on the literal — but both
were invisible to the diff read, because the broken lines were **not in
the diff**. Only the field types changed; the compile errors were in
callers whose text was untouched.

**The rules:**

- When a type change makes a value non-`Copy`, grep for every consumer
  that binds, moves, or pattern-matches it — especially behind a
  `Deref` (guards, locks, iterators). "It compiled before" is not
  evidence it was written to be general; it may have been riding on
  `Copy`.
- The diff read is the compile pass only for the lines in the diff.
  For signature/type changes, the review surface is every use site,
  and CI compiling `--all-targets` (tests included) is the safety net
  that catches what the read cannot.

## 47. A flaky test is a distribution with a cause — this one was a real bug that truncated series titles

**Date:** 2026-10-02, 7.1 step 2a.1 CI round two.

**What happened:** the branch's tests failed once in a test untouched by
the change — `importing_a_comic_lands_it_in_its_series_folder`, which
passes a uuid-padded series name through the comic import. The series
came out as "Import Series" instead of "Import Series cd272d85-…": the
chapter-marker heuristic's `" c"` keyword (for `c12`-style notation)
matched the space before the uuid, and the number scanner skipped *any*
characters until it found digits — so `d272d85-…` became "chapter 272"
and everything after "Import Series" was deleted as a "marker".

The activation probability is what made it look like noise: the random
uuid must begin `c` + digit for the `" c"` path (about one run in
sixteen), or end in an all-digit tail group for the trailing-digits path
(about one in two hundred). Roughly a 6-8 % flake — and a **real,
user-facing bug** the whole time: a series called "The Chronicles 1950"
would have its title truncated to "The" with a phantom chapter 1950 on
import.

**The fix:** the marker parser now requires a complete word — an
optional short alphabetic prefix ("c12", "vol2", "Ch. 5") followed by
digits, at most five of them (the app's own ceiling: chapter stems are
four digits, five only past 9999), with nothing alphanumeric after the
digits — applied at every keyword and delimiter site in both the series
sanitizer and the filename parser, plus the same five-digit cap on both
trailing-digit rules. Regression tests pin the exact uuid shapes and
every marker form that must keep parsing.

**The rules:**

- A test that fails intermittently is not weather. Compute or bound the
  probability, find the input that varies (here: a random uuid), and
  read the code path until the failure is *deterministic in your head*.
  "Rerun until green" hides exactly the bugs users eventually find.
- Lenient scanners — skip-any-characters-until-digits — turn every
  nearby word into a potential false match. A marker (chapter number,
  volume, issue) is a *word*: parse it as one, and bound it by the
  domain's real range.
- The published CI logs carry the panic line but not the assertion's
  left/right values; `ci-logs/test-full.txt` (the unfiltered log, same
  publish commit) does. Read the full log before diagnosing.

## 48. A 0 ms callback can still freeze the UI — never swap widgets by container mutation

**Date:** 2026-10-02, 7.1 step 2a.1 field run; fixed in step 2a.2.

**What happened:** after the async migration, every main-thread callback
measured 0 ms in the timing log — and the owner still saw 250 ms-3.4 s
freezes, three of six ending exactly on a cover arrival. The deferred
cover frame filled itself by removing its placeholder child and
appending a picture (`book_row.rs` `swap_in_cover`): a container
mutation, which schedules a relayout of the frame's ancestors. One
arrival is nothing; a home screen streaming two dozen covers is a
relayout storm — 0 ms of "our" code, seconds of GTK layout, and the
watchdog could only say "after task_item:Preloading covers".

**The fix:** the deferred frame now has exactly one child, forever — a
fixed-size picture whose paintable is swapped in place when the cover
arrives. The placeholder gradient is the frame's own background (no
class is ever toggled); the paintless picture is transparent over it.
A paintable swap on a fixed-size picture is a repaint, not a relayout.

**The rules:**

- "My callback is fast" is not the question. The question is what GTK
  work the callback *schedules*: `remove`/`append` on a realized
  container relayouts its ancestors, a css-class toggle re-styles the
  node, and a paintable swap on a fixed-size widget repaints. Pick the
  cheapest one that can express the change, always.
- The second half of the fix is ordering: a page that decodes its own
  covers on its snapshot worker applies them from the cache before any
  frame is built — the swap path is never even reached. Doing work
  before the widgets exist beats doing it cheaply after.
