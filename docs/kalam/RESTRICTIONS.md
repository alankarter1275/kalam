# kalam-engine — architectural restrictions

The boundaries that keep the reading engine from turning into a browser
engine, a library manager, or a network client. Every rule below describes
**what the code does today**, verified against the tree on 2026-09-18, and
each one says how it is checked — because a rule nothing checks is a rule that
gets broken quietly.

> **This document was rewritten on 2026-09-18.** The previous version
> (now at [`archive/RESTRICTIONS.md`](./archive/RESTRICTIONS.md), kept for
> the record) banned `stylo` and mandated `lol_html`. Both were superseded on
> **2026-09-08** when the plan changed from "write an engine" to "fork
> Chapbook and strip it", and neither change was ever written back into the
> rules. See [What changed and why](#what-changed-and-why) below.

---

## 1. No browser engine, no WebKit, no WebView

The engine renders text itself. It must never instantiate, depend on, or
delegate to a browser engine.

**True today.** No crate depends on `webkit2gtk` or any web engine. The only
occurrence of the string "webkit" in the engine is `-webkit-hyphens` at
`chapbook-layout/src/fragmentation.rs:349` — a CSS property *name* being
matched, not the engine.

**Checked by:** `Cargo.toml`. A `webkit2gtk` dependency would not compile
against the rest of the workspace without being added deliberately.

---

## 2. No JavaScript, no scripting runtime

Layout, pagination, hit-testing, selection and word lookup are all Rust. No
JS engine, no injected script, no message handlers.

**True today.** Nothing in the engine evaluates script. It is actively
asserted: `crates/chapbook-layout/tests/cfi.rs:65` and
`tests/locator_text.rs:70` both feed a document containing `<script>` and
assert the script content is **absent from the locator text**. That is the
right shape — the rule is enforced by a test, not by a comment.

**Checked by:** those two tests.

---

## 3. `stylo` is the CSS engine, and it is allowed

**This reverses the original rule.** The first plan forbade `stylo` on the
grounds that it "introduces massive compilation overhead and C++ toolchain
requirements". The second half of that was simply wrong: `stylo` is Rust. It
is heavy to *compile* and light to *run*, and the alternative — hand-rolling
a cascade — was not a realistic amount of work.

**True today.** Five stylo crates are pinned at `=0.20.0` in the workspace
root (`stylo`, `stylo_traits`, `stylo_atoms`, `stylo_static_prefs`,
`stylo_dom`), alongside `selectors` `=0.40.0` and `cssparser` `=0.37.0`.
Twelve files under `crates/chapbook-layout/src/` are built on them; the
`dom`, `cascade` and box-tree stages all move together, which is why
`chapbook-layout/src/lib.rs` calls itself "the whole stylo-facing half of
chapbook".

**The one real constraint this creates.** `crates/chapbook-layout` declares
`unsafe impl Send for Node` and `unsafe impl Sync for Node`
(`src/dom/tree.rs:130-131`). Those are only sound because stylo runs
**sequentially** here — `Servo` is constructed with `None` for the
traversal-ownership wrapper, so no parallel style traversal ever happens.
`crates/chapbook-layout/tests/sequential.rs` exists solely to fail the build
if that ever changes. **If you enable parallel stylo traversal, re-argue that
`unsafe impl` first.**

---

## 4. `lol_html` is not used, and there is no CSS stripper

**This reverses the original rule.** The plan was to strip publisher CSS with
Cloudflare's `lol_html` streaming rewriter so Kalam's theme was always
authoritative. Forking Chapbook replaced that with a real cascade, and a
cascade makes stripping both unnecessary and wrong — you cannot strip your way
to correct CSS inheritance.

**True today.** `lol_html` is not a dependency of any crate in the workspace.
Parsing is `html5ever` / `xml5ever` / `markup5ever` `0.39` into an arena DOM;
the cascade is stylo; the `ComputedValues` → `cosmic-text` mapping is
`chapbook-layout/src/style_to_attrs.rs`.

### What theme authority actually is — read this before claiming parity

The old rule said publisher CSS "MUST be stripped" so the host theme wins.
**That is not what the engine does, and the difference is visible.** Sheets
are appended at different cascade origins
(`chapbook-layout/src/cascade/engine.rs:92-109`):

| Sheet | Origin | Who wins |
|---|---|---|
| `UA_CSS` | `UserAgent` | baseline |
| `settings_css` — base font size, line height | `UserAgent` | **the publisher wins** wherever it specifies these |
| `theme_css` — colours | `User` | Sepia recolors *defaults* only, so the publisher still wins where it specifies. **Dark forces** colours with `!important` |
| the shell's own skin sheet | `User`, appended last | wins among equal user-origin sheets |

So: in Dark mode the theme genuinely overrides the publisher's colours.
In Light and Sepia it does not, and a publisher that sets `font-size` or
`line-height` beats the reader's font-size setting. That is a deliberate
Readium/Calibre-shaped trade-off (readability forced in night mode, design
respected otherwise) — not a bug, but it is **not** "the theme is
authoritative", and anyone promising that is promising something the engine
does not do.

---

## 5. No persistence — the host owns every record

The engine writes nothing to disk and remembers nothing between sessions.
Reading position and highlights are values the host hands in and takes back.

**True today.** No engine crate depends on `rusqlite` or any database.
Upstream's `chapbook-library` crate was deleted for exactly this reason
(`docs/kalam/PLAN.md` §7 step 4); its replacement is
`chapbook-reader/src/host_position.rs` and `host_highlights.rs`, where
highlights are painted under the host's own id and never stored.

**Checked by:** `Cargo.toml` — no database dependency in any engine crate.

---

## 6. No networking

The engine consumes local bytes. It makes no HTTP requests and parses no
websites.

**True today.** No engine crate depends on `ureq`, `reqwest` or `hyper`.
Upstream's `opds` and `ureq` features were removed with the crates behind
them; `crates/chapbook-reader/Cargo.toml` lists those four names under
`[lints.rust] unexpected_cfgs` purely so the untouched `#[cfg(feature = …)]`
sites do not warn under `-D warnings`.

---

## 7. No comics, no PDF — with a caveat worth stating precisely

The engine is for reflowable text. CBZ paging and PDF rasterization belong to
the host, in native GTK components (`src/comics.rs`, `src/pdf.rs`).

**True in effect, but the types are still there.** `chapbook_core::BookKind`
still has three variants — `Epub`, `Comic`, `Pdf`
(`crates/chapbook-core/src/book.rs:19`). They are **compiled out**, not
deleted: the `cbz`, `pdf` and `opds` features are gone, and the internal
`_comic` and `_image-book` features stay declared but are never enabled, so
the code behind their `#[cfg]`s does not build. The files are left
byte-identical to upstream on purpose, so cherry-picking upstream fixes stays
cheap.

**That reason no longer applies** — Kalam stopped tracking upstream on
2026-09-19 (`UPSTREAM.md`), so there are no cherry-picks left to stay cheap
for. The variants could now be deleted like any other dead code. They have not
been, because that is a code change and belongs under the dead-code pass
(roadmap 1.16), not in a documentation decision.

**So: do not "clean this up" as a drive-by.** If the variants go, they go as
part of 1.16, deliberately, with the `#[cfg]` sites that reference them.

---

## 8. The engine is a fork; edit inherited files rarely

Kalam-specific code goes in **new** files and new crates
(`host_position.rs`, `host_highlights.rs`, `crates/kalam-reader`,
`tools/kalam-reader-demo`). A commit that edits an inherited file gets a
`kalam:` prefix and a row in [`UPSTREAM.md`](./UPSTREAM.md).

**Why:** the value of the fork is that upstream bug fixes still apply. Every
edit to an inherited file is a future merge conflict.

---

## What changed and why

Recorded because the last change of this size was never written down, and the
rules were still quoting the abandoned plan eight days later.

| | Original plan (to 2026-09-08) | Now |
|---|---|---|
| Approach | write an engine from scratch | fork [ophymx/chapbook](https://github.com/ophymx/chapbook) and strip it |
| HTML handling | `lol_html` streaming rewriter | `html5ever` arena DOM |
| CSS | strip publisher CSS entirely | real stylo cascade, host sheets at user origin |
| `stylo` | banned | required |
| Theme authority | absolute | Dark forces colours; otherwise the publisher wins where it specifies |
| Layout, rasterizer | `cosmic-text` + `tiny-skia` | unchanged — `cosmic-text` + `tiny-skia` |
| Position anchoring | text excerpts | `LayeredLocator` — the same idea, better |
| No WebKit / no JS / no DB / no network / no PDF-comics | required | **unchanged, all still true** |

The full account of the abandoned design is in
[`archive/README-ARCHIVE.md`](./archive/README-ARCHIVE.md).

---

## Keeping this document honest

Four of the rules above are checkable from the manifest and two are checkable
from tests that already exist. The rest are conventions. If you add a rule
here, say how it is checked; if you cannot, it belongs in
[`PLAN.md`](./PLAN.md) as an intention instead.

Two tests already guard this document's claims:

- `crates/chapbook-layout/tests/sequential.rs` — fails if stylo ever runs in
  parallel, which is what makes rule 3's `unsafe impl` sound.
- `crates/chapbook-core/tests/fixture_discipline.rs` — fails if a
  default-running test depends on the downloaded corpus.

The same pattern should be used for anything added here: a rule with a test
holds, a rule with a paragraph does not. That is precisely how rules 3 and 4
came to be wrong for eight days.
