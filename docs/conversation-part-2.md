# Part 2 (online) — the design conversation, in full

> **What this file is.** The complete record of the Part 2 — online design
> conversation, opened 2026-10-09 at the owner's instruction. Unlike
> `docs/conversation.md` (the distilled, cross-cutting decision log), this
> file keeps **full detail**: the owner's asks recorded verbatim, the
> research with file-and-line evidence behind every claim, the decisions as
> they land, and the open questions in order. Entries are appended in
> conversation order and never rewritten except to mark a decision's final
> status.
>
> Status markers follow the house style: ✅ accepted · 🔶 under discussion ·
> ~~struck~~ rejected.
>
> **Terminology guard.** "Part 2" here always means **ROADMAP's Part 2 —
> online** (networked: sources, plugins, downloads). It is *not* Part 1's
> internal "Phase 2 — Reading quality" (offline items 2.1–2.13, mostly
> shipped: dictionary keyboard, PDF rebuild, bubbles). The old phase plan
> also called the online work "P7" before it was archived — same thing,
> older name.

---

## Entry 1 — 2026-10-09 — the opening

**The owner's ask, verbatim:**

> "now let's also talk about a few more things. for phase 2, ok? the online
> phase. this conversation should be recorded in a separate file, alright?
> and in complete detail. now where should we start?"

Agreed: this file is that record.

### The state of the ground, as researched today (evidence cited)

**What ROADMAP's Part 2 stub already settles** (`ROADMAP.md:2053`, "Part 2 —
online (stub)"; rule: *nothing in Part 1 may depend on it*):

1. **Plugin substrate = WebAssembly**, superseding the older Lua design
   (ARCH.md's Source-seam section, banner-marked as set aside). Three
   standing conditions: (a) `wasmtime` behind a Cargo feature that is **off
   by default**; (b) **do not ship the host before a second plugin exists**;
   (c) **keep a plain TOML selector config alongside**, because most scraper
   breakage is a changed CSS selector, and a selector fix that needs a wasm
   rebuild is not a fast fix loop. Reasoning in `docs/conversation.md` §22.
2. **Online sources: AO3, MangaDex, Royal Road, Literotica, FFN.** The old
   design and its evidence are in
   `docs/archive/roadmap-phases-p0-p12.md`; the `Source` trait has never
   been implemented — "treat it as unbuilt."
3. **Downloads hub.** The pages exist and are unreachable (the nav hides
   them). Standing requirement when this starts: use `src/tasks.rs` —
   `src/downloads.rs` currently uses bare `thread::spawn` and six
   `lock().unwrap()` calls, the cascade-panic pattern that `src/tasks.rs:52`
   exists to prevent.
4. **Literotica account sync** — raised once, never decided.

**The archived P7 design (changelog 2026-09-04) — the richest prior design,
to be re-confirmed or revised in this conversation, not silently inherited.**
Its rulings, in summary:

- Staged as P7a trait+browse+search → P7b **read online** → P7c author
  pages → P7d the other three sources → P7e accounts (if wanted) →
  P7f **download/follow/auto-update**. Note the inversion: downloading,
  which was the entire original plan, is the *last* stage — read-online
  costs little because the reader already unpacks into
  `cache/reader/<uuid>/` and never reads an EPUB directly.
- **Writing actions are out entirely** — no kudos, bookmarking, posting,
  review-reading. Kalam is read-only against every source.
- **Login goes last; passwords are ruled out forever.** If accounts ever
  happen: session-cookie only, obtained by logging in through a real site
  page in a WebKit window kalam never reads the password from, stored in
  config (not in a library folder, so it cannot travel with copied books).
  Standing context: AO3's own guidance warns third-party apps are
  at-your-own-risk; no public API exists.
- **Kalam's own follows already work across all sources without an
  account** — a better feature than site-side lists.
- **Very long serials cannot be bulk-downloaded** (a 2,000-chapter Royal
  Road work fetched to read one chapter is not viable) — so the trait keeps
  a per-chapter content verb AO3 will never use.
- **Temporary reads are not deleted on app close** — a few days or a size
  cap, so returning an hour later does not re-download.
- Parked at the owner's request: multiple books/readers open at once
  (later designed and shipped as bubbles, item 2.13, 2026-09-28).

**The Source seam as it stands in the tree today:**

- `src/sources/traits.rs` (116 lines) — a **Tachiyomi-shaped** `Source`
  trait: `id` / `name` / `base_url`, `get_filter_definitions` (dynamic
  filters: Text, Checkbox, Select, Sort — for UI generation),
  `search(query, page, filters) -> SearchPage`, `get_details`,
  `get_chapters`, `get_chapter_content -> ChapterContent`
  (`Images(Vec<String>)` for manga / `Html(String)` for fiction), and
  `fetch_image` (referer/auth headers, for the ComicsReader's
  ImageProvider). All synchronous; the doc comment says call from
  `crate::tasks::spawn` workers. Nearly every item is
  `#[allow(dead_code)]` with notes like "reserved for the Wasm API."
- `src/sources/mod.rs` (45 lines) — `SourceManager` with an empty `Vec`,
  a `register()` marked "used in Phase 4 when registering dynamic Wasm
  plugins", and a `GLOBAL_SOURCE_MANAGER` OnceLock. Zero `impl Source`
  exists anywhere in the repo (ROADMAP.md:198 confirms).
- `src/sources/scrapers/mangaball.toml` (23 lines) — a sample **plain-TOML
  selector scraper**: `base_url` + `[search]`/`[details]`/`[chapters]`/
  `[pages]` sections of CSS selectors and URL templates. Never executed by
  anything; it is the shape of the "selector fix should not need a wasm
  rebuild" condition above.

**The rest of the scaffolding that already exists, shipped and waiting:**

- Hidden nav entries, all Part 2: `NavItem::Downloads` (queue page works,
  reads "0 downloads tracked" forever), `NavItem::RemoteBrowse` ("Browse"),
  `NavItem::Fanfiction`. `Route::RemoteSearch` already special-cases
  source_id `"royalroad"` → Fanfiction nav (`src/models.rs`).
- Routes and pages that exist for remote content: `RemoteDetail`,
  `RemoteReader`, `RemoteSearch` (used by the comics reader's
  `RemoteProvider` — remote reading already works for comics),
  `src/pages/browse.rs`, `src/pages/downloads.rs`, `src/downloads.rs`
  (`DOWNLOAD_MANAGER`), `src/watch_folder.rs`.
- **A working network pattern is already in-tree**: `ureq = "2"`
  (Cargo.toml:150) powers `src/metadata/google_books.rs` — real HTTP with
  retry/backoff on 429 and a shared agent (`src/metadata/mod.rs:329`).
  That is the house pattern for kalam's outbound HTTP: blocking ureq
  inside `tasks::spawn` workers, never async runtime, never the UI thread.

### Where we start — the question map (proposed this entry, 🔶)

In dependency order, the conversation has five questions:

1. **Roster and ambition** — which sources, and what "online" does:
   browse, search, read-online, follow updates, download. (The old design
   ordered read-online first, download last.)
2. **The 2026-09-04 rulings** — re-confirm or reopen each (read-only
   posture, no passwords ever, login last, temp-read retention, per-chapter
   verb).
3. **Substrate** — how a source is actually implemented: native Rust
   adapter, TOML selector pack, WASM plugin, and in what order. The stub's
   three wasm conditions imply native/TOML first; the trait's shape
   implies the Tachiyomi/wasm world. Where does the first real source
   live?
4. **Remote reading plumbing** — identity for non-library books, the
   reader's temp cache and "keep this" promote, per-chapter fetching for
   long serials.
5. **Downloads hub + follows** — the `downloads.rs` → `tasks.rs` debt,
   update checking, metadata mapping into the library.

Question 1 opens the conversation (posed in chat the same day).

---

## Entry 2 — 2026-10-09 — Question 1 and 2 answered: the roster grows, the staging philosophy falls

**The owner's answers, verbatim:**

> "1. yes a few more. Webnovel. and one or two for the western comics."
>
> "2. kind of.....but it's a feature, alright? nothing like last or first.
> there is an option and it's up to use to do it. though I agree that the
> first work is to read"

### Decisions

**✅ Roster — the stub's five stand, plus two additions.** AO3, MangaDex,
Royal Road, Literotica, FFN (ROADMAP Part 2 stub) **+ Webnovel** **+ one or
two western comics sources** (sites not yet named — open follow-up 🔶).
Seven-ish sources once named. ROADMAP's stub list will be updated when the
roster closes, not per-turn.

Two per-source notes recorded now, researched when each source is designed:

- **Webnovel** (webnovel.com, Qidian International): free + paywalled
  (VIP/locked) chapters; app-centric platform. A logged-out scraper reaches
  free chapters only — locked content needs an account, and the standing
  rulings (no passwords ever; session-cookie login last, if ever) apply
  squarely. The source's honest scope is free content until and unless an
  account path exists.
- **Western comics** differ from manga in *consumption model*, and that
  matters for the trait later: manga streams chapter-by-chapter (the
  MangaDex API model, `ChapterContent::Images`), while western comics are
  usually consumed as **whole issue files** (CBZ/CBR) — which is exactly
  what kalam's existing importer and comics reader already eat. A
  direct-download source (archive-style site) may fit kalam better than a
  page-streaming source, and might want a *file-download* verb the current
  trait does not have. Design question, parked until the sites are named
  and the substrate question (map item 3) is taken.

**✅ Ambition — the P7 staging inversion is rejected as philosophy.**
Read-online and download are **peer features**, both present, both
user-choice — "nothing like last or first. there is an option and it's up
to us to do it." The old P7f framing (download as the *last* stage,
presented as a philosophical inversion) is struck. What survives is
narrower and practical: **build order starts with reading** ("the first
work is to read") — because the reader's cache-unpack design makes
remote-reading cheap, per the archived P7 evidence. Build order is a
sequencing fact, not a product statement.

### Open follow-ups from this entry

- 🔶 Which western comics sites (one or two)? — posed back in chat same day.
- 🔶 Confirm reading of decision 2 (peer features; build reading first) —
  restated in chat same day for a yes/no.

---

## Entry 3 — 2026-10-09 — decision 2 confirmed; the western comics landscape, researched

**The owner's answers, verbatim:**

> "1. I don't know any. can you look up for a few popular websites where one
> can read western comics for free"
>
> "2. yup"

**✅ Decision 2 closed:** all options exist in the app as peer features
(read online, download, follow); "reading first" is build order only.

**🔶 Western comics sources — the researched landscape** (web search,
2026-10-09). The owner asked for popular free sites; the honest finding is
that the landscape splits by legality and by consumption model:

*Unlicensed, current Marvel/DC ("the popular free sites"):*

- **ReadComicOnline.li** — the best-known page-streaming reader; the
  engineering catch is Cloudflare-class bot protection.
- **ReadAllComics.com** — page-streaming, no account, long-running,
  simpler protections historically.
- **ViewComic / ViewComics** — long-running simple streamer, Marvel/DC/
  Image/Vertigo.
- **ZipComic.com** — stream + download; top-traffic competitor as of
  mid-2026.
- **Batcave.biz** — DC-heavy plus golden-age.
- **GetComics.org** — direct-download archive: whole issues as CBZ/CBR/PDF
  behind third-party file hosters (Mega, MediaFire). Fits kalam's importer
  model exactly, but the bytes live on hosters with their own friction.
  Torrents (1337x, TPB) surfaced too — out of scope for a source adapter.

*Legal, free:*

- **Comic Book Plus** — ~17,000 public-domain Golden/Silver Age issues,
  fully legal, online reader. Digital Comic Museum is the same category.
- **GlobalComix** — legit creator-first platform, 100k+ comics, publishers
  include DC, Marvel, Kodansha, Dark Horse; free tier + one subscription
  unlocking everything. **No documented public API** (searched; only an old
  2021 PHP API client on GitHub and third-party Apify scrapers exist), so
  an adapter would target the undocumented internal API — fragile, same
  class of work as scraping despite the legit content.
- Marvel.com / DC Kids free rotations, Hoopla (library card, app/DRM
  bound) — real but not source-adapter material.

**Fit with kalam, as assessed in this entry:**

- A page-streaming site maps 1:1 onto the existing `Source` trait
  (`search → get_details → get_chapters → ChapterContent::Images +
  fetch_image` with referer) and the ComicsReader's RemoteProvider.
- GetComics maps onto the *other* half of the owner's ambition — download
  into the library — feeding the existing CBZ/CBR importer; cost is the
  file-hoster layer.
- Legality recorded plainly: the current-issues sites are unlicensed;
  Comic Book Plus is the clean legal archive but its content is
  1940s–60s vintage.

**Recommendation posed to the owner (same day, in chat):** one streamer
first — ReadAllComics (simplest, no Cloudflare reputation) or
ReadComicOnline (biggest catalog, bot-protection risk); GetComics as the
download-side addition if whole-issue files are wanted; Comic Book Plus if
a legal source matters to them. Awaiting the owner's pick of one or two
(🔶 open).
