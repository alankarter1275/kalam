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

---

## Entry 4 — 2026-10-09 — the engineering state, surveyed from every doc

**The owner's ask, verbatim:**

> "well, we will get to it later. forget it. for now, let's talk about the
> real engineering, ok? what do you know about it right now? what is in the
> docs?"

(Western comics site pick deferred; the 🔶 from Entry 3 stays open.)

### Where the design lives — nine places, all read today

1. **`ROADMAP.md` Part 2 stub** — the gate rule (*nothing in Part 1 may
   depend on online*) and four threads: WASM substrate, the source roster,
   the downloads-hub debt (must move to `src/tasks.rs`; `src/downloads.rs`
   is bare `thread::spawn` + six `lock().unwrap()`s), Literotica account
   sync (raised, never decided).
2. **`docs/conversation.md` §22** — the WASM ruling (owner's, 2026-09-18,
   superseding Lua *and* a "pure Rust + TOML" pivot): buys crash isolation
   + sandboxing; costs the wasmtime dep tree on a 4 GB RAM machine. Three
   conditions: feature off by default; no host before a second plugin; TOML
   alongside — *tiered: wasm for sources that need logic, TOML for sources
   that need a selector*.
3. **`ARCH.md`'s Source-seam section** — the Lua-era seam design,
   banner-marked set aside, pointing to §22.
4. **`docs/archive/roadmap-phases-p0-p12.md` P6+P7** — the richest doc.
   P6+P7 deliberately combined: the queue is infrastructure, the client is
   the feature, designed together so the queue answers to real needs.
5. **`docs/archive/source-seam.md`** — the original seam design: the
   dividing line for plugin surfaces is *does this break when a website
   changes its HTML*; personal app, not an ecosystem; no marketplace, no
   versioning; acceptance test: *a fresh chat can add a source from the
   documented API alone.*
6. **`docs/archive/p7-plan.md`** — the AO3 finding (below).
7. **`docs/archive/p7-scope-correction.md`** — the browse-not-download
   correction (below).
8. **`docs/archive/p7-login-and-reading.md`** + **`p7-storage.md`** +
   **`fichub-and-ffn.md`** — login posture (last, cookie-only, never
   passwords); storage (folder-per-book is already the Calibre shape;
   highlights survive fic updates without storage changes); FicHub's rules
   of use.
9. **`docs/archive/plugins-mod.rs`** — the archived WASM host attempt
   (imported wasmtime that was never in the manifest; kept for its design).

### The settled engineering positions, as the docs record them

1. **The `Source` trait must be rewritten before anything is built.**
   The scope correction (2026-09-04) is explicit: the current four verbs
   (search → details → chapters → content) are a *downloader's* shape —
   "I already know which work I want" — and cannot express wandering.
   The rewrite is browse-first: categories/fandoms, site sort orders,
   author pages, and **capability declaration** — a source says "I do
   search but not author pages" and the UI adapts rather than breaks.
   Retrofitting browse onto a download-shaped API means changing every
   source and every screen, so it happens first.
2. **One trait for fiction and manga, not FictionSource + MangaSource.**
   They differ only in the final step — text vs image URLs, a two-variant
   `Content` enum. Search, pagination, chapter lists, rate limits, the
   download queue and the follow scheduler are identical and must not be
   written twice.
3. **Per-chapter fetching is required, not a nicety.** A 2,000-chapter
   Royal Road serial cannot be fetched to read one chapter. **AO3 is the
   unusual case, not the template** — `download.archiveofourown.org/
   downloads/<id>/fic.epub` is a real EPUB built by AO3 with Calibre and
   listed in their own FAQ, so scraping AO3 is only ever for *finding*
   things, never for parsing text.
4. **Source order proves something different each time:** AO3 (their EPUB
   endpoint — search, filters, following) → Royal Road (first source where
   *we* build the EPUB; the biggest untested piece, tested gently) →
   Literotica (messy structure, no clean chapters) → FFN (a third-party
   bridge).
5. **One shared EPUB assembler** — new code; `epub_write.rs` edits EPUBs
   and cannot create one, but `zip` is already compiled with write
   support.
6. **FFN = FicHub for download + WebKit for browsing.** FicHub has exactly
   two endpoints (`/api/v0/epub`, `/api/v0/meta`), both requiring a fic
   URL you already have — it solves downloading, nothing of browsing.
   FicHub's rules are conditions of use: identify ourselves in the
   user-agent with contact info, never concurrent requests, honour
   429/Retry-After, no bulk export; the dependency must be visible in the
   UI. Browsing FFN means Cloudflare, which means the browser engine.
7. **Three-tier reading model (user-decided):** just reading → nothing
   kept; **Save** → temp area, not a library book; **Download** → real
   book. Open: what exactly separates Save from Download (the answer
   decides whether saved fics need their own screen).
8. **Downloads machinery is real, not vapour:** a job queue with progress
   and cancellation (`src/downloads.rs`, made async in 1.3), a downloads
   page, and a live route — with no sources feeding it. The Part 2 stub's
   requirement stands: migrate to `tasks.rs` when it starts.
9. **Login: in scope but last; never store passwords.** Session cookie
   obtained through a real site page in a browser window we never read
   the password from; stored in config, never in a library folder;
   log-out actually deletes it; read-only always (a leaked session cannot
   be used through kalam to damage an account — no writing actions at
   all).
10. **Remote identity:** downloaded books carry `source` + `remote_id`;
    storage/`catalog.db` shape already supports keeping highlights
    across fic updates (p7-storage). The missing piece is a *temporary*
    identity for a non-library fic plus a promote action — the reader
    itself already unpacks into `cache/reader/<uuid>/` and does not care
    where bytes came from.

### New engineering facts found today (not in the old docs)

- **WebKitGTK is no longer in the binary.** The engine swap removed it
  (Cargo.toml has no webkit entry). The FFN plan (WebKit for browsing) and
  the login design (a real browser window) were written when WebKit shipped
  with the reader. Bringing it back is now a *re-add the dependency*
  decision with real weight — or these designs need a different answer.
- **The trait in-tree is the pre-correction shape.** `src/sources/traits.rs`
  implements exactly the download-shaped verbs the scope correction
  struck down; the browse model was never written. Its only live consumer
  is the comics `RemoteProvider` (streaming images per chapter, via
  `get_chapter_content` + `fetch_image`) — the one working remote path in
  the app, and the thing a trait rewrite must not break.
- **Webnovel was dropped on 2026-09-04** — "almost everything worth
  reading is behind their coin paywall, so a downloader gets a handful of
  free chapters and stops. Bypassing a paywall is out of scope." The owner
  re-added it in Entry 2. **Collision — needs the owner's ruling** 🔶:
  keep it with an honest free-chapters-only scope, or let the old
  reasoning stand?

### The open engineering questions, in dependency order (revised map)

1. **The trait rewrite** — browse verbs + capability declaration; must
   keep RemoteProvider whole. (Everything else hangs off this.)
2. **Substrate order** — all docs agree AO3 lands *native Rust* first to
   prove the interface; the wasm host waits for a second plugin. What the
   first source proves decides the tier split.
3. **Remote identity + the three tiers** — temp identity, Save vs
   Download boundary, promote action.
4. **WebKit's return or replacement** — affects FFN browsing and any
   account flow.
5. **The downloads migration** — `downloads.rs` → `tasks.rs` before the
   hub lights up.
6. **Webnovel's scope ruling** — the Entry 2 collision.

---

## Entry 5 — 2026-10-09 — Webnovel removed; a standing instruction on language

**The owner's ruling, verbatim:**

> "alright, remove WebNovel. also, whtever else you said, I don't understand
> a thing. please use simple language. very simple language, and if possible
> explain it nicely so I can understand"

**✅ Webnovel is removed from the roster.** The 2026-09-04 reasoning stands
after all: most of what is worth reading there sits behind their coin
paywall, and bypassing a paywall is out of scope. It never reached
ROADMAP's stub list, so no roadmap edit is needed. The roster is now:
AO3, MangaDex, Royal Road, Literotica, FFN **+ one or two western comics
sources, pick deferred** (🔶 still open, Entry 3).

**✅ Standing instruction for this conversation (and beyond): very simple
language.** The Entry 4 briefing was too dense — terms like "trait verbs,"
"substrate," "capability declaration" were never unpacked. Every future
entry's chat summary must be plain words, short sentences, and everyday
analogies first, technical terms only when immediately explained. The
full-technical version still lives in this file (the record must stay
precise), but the person must never need it to follow along.

---

## Entry 6 — 2026-10-09 — the Tachiyomi question: how it works, and whether WASM is right

**The owner's ask, verbatim:**

> "the plugs will be build later. right now I want to talk about how it will
> work, and if wasm is the correct answer for this or not. how to make it
> work like how tachiyomi works...."

### How Tachiyomi actually works (architecture, from its source model)

Tachiyomi (community-continued as Mihon since the 2024 shutdown of
tachiyomi.org) is an Android manga reader whose sources are **extensions
delivered as APKs** — small installable packages, each containing one
source's Kotlin code, loaded at runtime without touching the main app. Four
load-bearing parts:

1. **A fixed interface.** Every source implements the same verbs
   (popular / latest / search → details → chapters → pages). The app talks
   only to this interface; it never knows which site it is talking to.
2. **Capability flags.** A source declares what it can do (`supportsLatest`,
   etc.); screens adapt. A source without "latest" just doesn't show that
   tab.
3. **Host-owned I/O.** Extensions do not do raw networking. They build
   requests and parse responses; the shared base class (`HttpSource`)
   provides the client, the per-source rate limit, headers, cookies. One
   shared identity, one place for politeness, one place for caching later.
   This is the single best idea in the system.
4. **Separate distribution.** A repo index (JSON) lists extension APKs with
   versions; the app can update a source without an app update. This part
   exists because Tachiyomi serves *millions of users who cannot compile
   the app*. It is the part kalam explicitly does not need
   (`source-seam.md` §0: personal app, no ecosystem, no marketplace, no
   versioning story — one user who is the developer).

The loading mechanism itself (DexClassLoader) is nothing special — it is
just what Android hands you. On desktop Linux the equivalent (loading
`.so` libraries at runtime) is crash-prone and unsafe; where hot-loading
is wanted, WASM beats it.

### The clarifying split: API-shaped vs scraper-shaped

The roster splits by *how a site breaks*:

- **API-shaped** — MangaDex (official API), AO3 (EPUB endpoint + search
  pages), FFN (FicHub's documented API). These speak stable, documented
  shapes; they essentially never break on markup changes. They are
  ordinary Rust code and want **no plugin substrate at all**.
- **Scraper-shaped** — manga aggregators, Literotica, Royal Road's chapter
  pages. These break when a site redesigns its HTML. Only these care about
  the fix loop.

**Consequence: none of the first four sources needs WASM.** AO3, MangaDex
and FFN are API-shaped; Royal Road is *app logic* (the shared EPUB binder)
that belongs in the binary regardless. The substrate question only bites on
the long tail — which is exactly why §22's "no host before a second
plugin" condition was right.

### The substrate verdict

- **WASM** (a way to run small programs inside a big one, sandboxed,
  without rebuilding the big one — the tech behind Figma/Photoshop plugin
  systems): buys crash isolation, sandboxing, no-app-rebuild fixes. Costs:
  the `wasmtime` dependency tree (heavy for a 4 GB RAM machine), a wasm
  toolchain step per fix, harder debugging, and real host-API design work
  (plugins must do HTTP *through* host functions or the sandbox is either
  crippled or leaky).
- **TOML selector packs** (`mangaball.toml` shape): the seconds fix-loop
  for the *most common* breakage (a changed CSS selector), zero compiling,
  hand-editable. Ceiling: only fits fetch-URL-extract-with-selectors
  sites; no logic. The engine itself is code we must write.
- **Native Rust in the app**: fastest, type-checked, zero new deps; the
  cost is the rebuild loop (fat LTO, `codegen-units = 1`) when a site
  changes — precisely what the archive flagged in `source-seam.md`'s scope
  note.
- (Lua — the original ARCH.md plan — remains the lighter middle option:
  no compile at all, real logic, but no crash isolation; superseded by the
  owner's §22 decision and not revisited unless they ask.)

**Position taken in this entry (affirms §22, sharpens it):** copy
Tachiyomi's *architecture*, not its *distribution*. The three tiers stand:

1. API sources → native Rust, always;
2. simple scraper sites → TOML selector packs;
3. complex scraper sites → WASM, **built only when a real second-plugin
   case exists**, behind the off-by-default feature.

And two Tachiyomi ideas must go into the socket rewrite *now*, because
they are substrate-independent: **capability flags** (browse shelves?
author pages? login? per-chapter? text or images?) and the **one network
gate** — sources never fetch directly; they ask the gate ("fetch this URL,
these headers"), and the gate owns the user-agent (identifying kalam, with
contact info as FicHub requires), per-source rate limits, retry/backoff,
and the future cache. One gate is also what keeps a broken plug from
hammering a site.

### The end-to-end flow, as it will work

Tap Browse → pick a source → the app asks that source's plug for shelves
(what it declared it can do) → you wander/search → tap a work → details
(chapters, description, cover) → tap a chapter → the gate fetches, the
plug parses (or the EPUB endpoint hands over a file), the reader unpacks
into `cache/reader/<uuid>/` exactly as it does today → three tiers apply
(nothing kept / Save / Download-to-library). Downloads go through the
existing queue (after its `tasks.rs` migration). Follows check for new
chapters on a schedule through the same gate.

---

## Entry 7 — 2026-10-09 — the owner proposes per-site sockets

**The owner's proposal, verbatim:**

> "actually, I was thinking making website specific sockets. like AO3's
> interface is completely different from Literotica's. their search system,
> tag system (AO3's tag system is very powerful), and other things are
> completely different. so they all need separate approach! each hand
> crafted. what do you think?"

### What's right in the proposal, and what it costs

**Right:** every source needs a *hand-crafted approach*. AO3's tag system
(fandoms, relationships, characters, ratings, exclusions, wrangled tags,
its dozen sort orders) genuinely is a different world from Literotica's
category tree. Nobody — not Tachiyomi, not us — disputes this. The
per-site craft lands in the **plug**, and it is unavoidable.

**The cost of taking it one step further — per-site *sockets*** (i.e. a
bespoke interface + bespoke app-side handling per site):

1. The Browse screen, search screen, details screen and reader entry all
   become per-site code forests: "if AO3 draw this; if Literotica draw
   that." Every new source edits the app itself.
2. The downloads queue, the follow/update scheduler and the one-gate
   fetcher all ask sources uniform questions ("any new chapters?", "fetch
   this") — per-site sockets fork every one of them.
3. The punchline: **the plugs' hand-crafted code would still have to be
   hand-crafted.** Per-site sockets duplicate the app-side work and remove
   none of the plug-side work.

### The reconciliation: one wall, data-rich; hand-crafted plugs

The standard answer (Tachiyomi's, and already half-present in our unused
`traits.rs` as `get_filter_definitions` → Text/Checkbox/Select/Sort):

- **Search forms as data, not code.** Each plug hands the app a *list of
  controls* — and the shared search screen draws whatever it is given,
  natively. AO3's plug hands over a rich form (text fields for
  characters/ships, dropdowns for rating and sort, checkboxes for
  completion, word-count ranges); Literotica's hands over one category
  dropdown. Hand-crafted where it matters, shared where it matters.
- **Trail links as data** (the P7c "follow a trail" idea): anything
  tappable in a result or detail page — a tag, an author, a series, a
  fandom — is a generic link that produces a new browse query. AO3's plugs
  produce many (every tag, every wrangled relationship), Literotica's few.
- **Capability labels** decide which parts of the app even appear for a
  source.
- **The socket grows, it does not fork:** when a site proves the
  vocabulary too small, the vocabulary gains a concept every source may
  use. The acknowledged danger of one-socket designs is
  lowest-common-denominator flattening — the defense is richness
  (data-driven filters + trail links), and the escape hatch is a bespoke
  screen behind a special capability, allowed but discouraged, decided
  only when a real case demands it.

**Position posed to the owner (🔶 open, awaiting their reply):** one
shared, deliberately rich socket; per-site hand-crafted plugs that fill it
with each site's full personality. Their instinct is already honored in
the plan — tier-1 sources (AO3, MangaDex, FFN-via-FicHub, Royal Road's
binder) are exactly hand-crafted native code.

---

## Entry 8 — 2026-10-09 — DECISION: one rich wall, per-site wall as fallback

**The owner's decision, verbatim:**

> "well, we will try it your way first, if it works good. if not....well,
> per-site wall is always the fallback, and it will always work"

**✅ Decision (closes Entry 7's open question):** one shared, deliberately
rich socket — search forms as data, tappable trail-links, capability
labels, one network gate — filled by hand-crafted per-site plugs.
**Per-site walls are the recorded fallback, not a rejected idea**: if the
rich wall cannot express a site without flattening it, forking that site's
wall is the sanctioned answer. The owner's framing is sound — the per-site
wall is the option with no shared abstraction, so it cannot be blocked by
the abstraction's limits; its cost is duplicating the app-side screens,
queue hooks and gate handling per site.

**The fallback has an early, concrete trigger.** AO3 is first in the build
order *and* the richest site — the wall's hardest test is also its first
one. If AO3's tag/search system cannot be carried through the shared
vocabulary without losing what makes it AO3, that is the moment the
fallback fires, not after several sources are built on a wall that proves
too small.

**Halfway point before a full fork:** a bespoke screen behind a special
capability label (the escape hatch from Entry 7) — one site getting one
special screen is cheaper than that site abandoning the wall entirely.

### State of the Part 2 question map after this entry

1. Roster + ambition — settled except the western comics pick (🔶 open,
   deferred by the owner).
2. P7 rulings re-confirmation — presented across Entries 4–6 (read-only,
   no passwords/login-last, three tiers, per-chapter) without objection;
   explicit owner confirmation not yet asked for directly.
3. Substrate — settled: tiered (native / TOML / WASM-when-proven), WASM
   deferred per §22's conditions; this conversation added the API-vs-
   scraper split as the reason the first four sources need no substrate.
4. Socket shape — **direction settled (this entry)**; the vocabulary
   itself (the actual questions, capabilities, filter-item kinds, trail
   links) is the next design topic, before any code.
5. Remote identity, Save-vs-Download boundary, downloads migration,
   follows scheduler — still to be designed.

---

## Entry 9 — 2026-10-09 — unpacking "the vocabulary"

**The owner's question, verbatim:**

> "what do you mean? every plug will answer what? I don't understand"

Fair — Entry 8's closing line ("the exact list of questions every plug
will answer") compressed the whole idea into one sentence and lost the
owner. Re-explained in chat the same day, plainly:

**The "vocabulary" = a fixed menu of requests the app can make to any
plug.** The app is a customer who always orders from the same menu; each
plug is a different cook who prepares every dish their own way. The menu,
sketched in the chat walkthrough:

1. *Who are you?* — name, icon, text or comics, what you can do
   (the labels).
2. *What shelves do you have?* — AO3 answers fandoms/popular/tags;
   Literotica answers its category tree.
3. *What's on this shelf?* — a page of items (title, author, cover,
   blurb) plus "is there a next page?"
4. *Tell me about this one* — full description, tags, chapter list,
   author.
5. *Search* — two-step: "what's your search form?" (the plug hands over
   its fields as data), then "here are the filled-in fields, give me
   results."
6. *Everything with this tag / by this author* — trail links.
7. *Give me chapter 12* — text or images.
8. *Any new chapters since X?* — the background follow checker.

Plugs may answer "I don't serve that" to anything except the core (the
app hides the missing parts). Writing this list down before code is
cheap; changing it after the screens exist is expensive — that is why it
is the next design step, and nothing more mysterious than that.

---

## Entry 10 — 2026-10-09 — a partial "yup"

**The owner's reply, verbatim:**

> "yup, kind of..."

Understood partially, accepted as enough for now — the right call, not a
failure: the fixed-question-menu idea settles with use, not with more
explaining. No new decisions. Deliberately did **not** pile on another
explanation; the next concrete step (writing the menu against AO3,
question by question, in plain words) is where anything still fuzzy will
surface naturally. Teed up, awaiting the owner's word.

---

## Entry 11 — 2026-10-09 — REQUIREMENT CHANGE: plugins must be true plugins

**The owner's requirement, verbatim:**

> "you know, I want the plugins to be like...well, plugins. If a plugin
> breaks mid use, I should be able to change it and then reload it without
> recompiling the whole app.....understand? what else should be a standard
> behavior of a plugin? I don't know these things...."

### What this changes

This **updates the Entry 6/8 substrate verdict.** The tiered answer stood
on "the first four sources are native Rust; the plugin machinery waits for
a proven need." The owner's requirement makes reloadability a core
property, not a deferred one: *fixing a source must never require
rebuilding kalam.* Consequences:

- **Everything becomes a plugin.** A plugin = a folder with a manifest
  (id, name, version, capability labels) plus either a `.wasm` file
  (logic-bearing sources) or a `.toml` file (pure-data sources — the
  mangaball shape). Native Rust is demoted from a *destination* to
  *temporary scaffolding* during the wall-proving phase.
- **The app core freezes.** Once the wall + gate + loader exist, kalam
  itself should never need a rebuild because a website changed. All churn
  lives in plugin files.
- **§22's three conditions, revisited.** (1) wasmtime behind an
  off-by-default Cargo feature — *stays* (CI and plugin-less builds stay
  light; the owner's build turns it on). (2) "no host before a second
  plugin" — *overtaken by this requirement*: the host moves onto the
  critical path once the wall is proven, because every source is a
  consumer from day one. (3) TOML alongside — *stays*, reframed: a plugin
  may be pure data. The wasmtime cost on a 4 GB machine is now a price
  the owner has consciously chosen to pay; it is manageable, not optional.
- **Honest fix-loop numbers:** a TOML plugin fix = edit text, reload, done
  (zero compiling). A wasm plugin fix = edit Rust, compile *only that
  small crate* to a `.wasm` target (seconds to low minutes — one tiny
  crate, not the fat-LTO whole-app build), drop the file in, reload.
  Neither touches kalam's 20-minute-class full build.

### Standard plugin behaviors (the owner's question, answered)

The norm across plugin systems (Tachiyomi/Mihon extensions, OBS, Figma,
browser extensions), translated to kalam:

1. **Install by dropping a folder in a directory; remove by deleting it.**
   No app changes either way.
2. **Update = replace the file, then reload.** The app never needs
   rebuilding for a source fix. Plugin version numbers make old vs new
   visible.
3. **Enable / disable without deleting** — a source can be switched off
   and keep its settings.
4. **Crash containment** — a broken plugin shows an error for that source
   and nothing else. It must never take the reader down. (Wasm's core
   value; TOML parse errors are contained trivially.)
5. **Per-plugin settings** — a small standard options page each source can
   add controls to, stored by the app (never inside the plugin file).
6. **Everything through the gate** — plugins get no direct internet or
   filesystem access; they ask the host. One user-agent, one rate
   limiter, one cache, one cookie jar. Safety and consistency in one
   place.
7. **An error log per plugin** — a "why is this source broken" console
   in-app; the single most important debugging surface when the plugin is
   a sandboxed binary.
8. **The app must work with zero plugins** — kalam stays today's complete
   offline reader; sources are additions, not limbs.
9. **Mid-use reload, honestly defined:** already-open chapters stay open
   (content is already unpacked in the reader cache); in-flight fetches
   finish or fail cleanly; the swap happens between fetches, not during
   one. A "reload plugin" action in settings plus change-detection on the
   plugin folder.

### Revised build path

1. Design + build the wall and gate (unchanged — still first).
2. AO3 built *natively as scaffolding* — fastest way to prove the wall
   end-to-end against the richest site (the Entry 8 fallback trigger
   also fires earliest this way).
3. While AO3 proves the wall: the minimal wasm host, the plugin folder +
   manifest format, the per-plugin settings page, the error console.
4. Lift AO3 out of the binary into the first true plugin. From then on:
   the core is frozen; Royal Road, Literotica, MangaDex and everything
   else arrive as plugins directly.

**🔶 Open, put to the owner in chat:** confirm that *everything becomes a
plugin* (AO3 and MangaDex included) is what they meant — stable API sites
break rarely, so they *could* stay native; the recommendation is
everything-is-a-plugin for one uniform system.
