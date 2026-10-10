# Repository Working Rules & Developer Invariants

This document outlines the strict developer invariants and repository guidelines for **Kalam**. All developers and automated coding agents modifying this codebase must adhere to these rules without exception.

> **There are two files named `WORKING.md` in this repository.** This one
> (`docs/WORKING.md`) is the short list of invariants and guardrails. The
> other, [`docs/kalam/WORKING.md`](./kalam/WORKING.md), is the much longer
> agent-facing guide: hard rules, the build and CI loop, a repository map,
> design decisions not to undo, and how a working round goes. They do not
> overlap in content, but the names collide — check which one you have open
> before editing.

---

## 1. Core Invariants

### 1. Zero `unwrap()` in Production Code
- **Rule**: `unwrap()` and `expect()` are forbidden in non-test production code.
- **Rationale**: An unhandled panic in production crashes the application, resulting in bad user experience and potential data corruption.
- **Enforcement**: Always handle errors explicitly using `?`, pattern matching (`match`, `if let`), `unwrap_or()`, or `unwrap_or_default()`. If an invariant is truly unreachable, use error logging or safe fallbacks.
- **Status, honestly**: this rule is currently broken in **12** places, so it is enforced as a *ratchet* rather than an absolute — `tests/guardrails.rs` fails if the count rises above 12, and the constant is lowered as they are fixed. A test asserting zero would have failed the day it was written and then been deleted; a ratchet at the real number does the useful job.

### 2. Sidecar Backup Synchronization on Database Updates
- **Rule**: Every database modification that alters book annotations, bookmarks, metadata, or reading progress must synchronize the corresponding sidecar backup files (`crate::sidecar::refresh_for_book`).
- **Rationale**: Sidecar files ensure portable, non-destructive persistence of user annotations alongside book files even if the main database is rebuilt or moved.
- **Enforcement**: Call `crate::sidecar::refresh_for_book(catalog, book_id)` immediately after any database insert, update, or deletion operation affecting annotations or reading state.

### 3. No Root Scratch Files
- **Rule**: Do not create or leave temporary scratch files, debug logs, or intermediate artifacts in the repository root or worktree root.
- **Rationale**: Keeps the working tree clean, prevents accidental commits of junk files, and maintains standard repository hygiene.
- **Enforcement**: Place documentation under `docs/`, tests under `tests/`, and temporary build outputs under `target/`.

### 4. Plain English Documentation & Communication
- **Rule**: Code comments, commit messages, and documentation files must be written in clear, concise, plain English.
- **Rationale**: Keeps codebase context easily accessible to developers of all skill levels and ensures smooth handoffs between engineering agents.

---

## 2. Reading Engine Architecture

- **Separation of Concerns**: `crates/kalam-reader` acts as the embedded reader widget and owns rendering, layout, and font shaping without disk I/O.
- **Persistence Owner**: Kalam's SQLite catalog (`~/.local/share/kalam/catalog.db`) owns database persistence for bookmarks, annotations, saved words, and progress.

---

## 3. Guardrails and ratchets (2026-09-19)

`crates/kalam-reader` inherits strict boundaries from upstream, which is a large part of why that code stayed clean. `src/` had no equivalent — and this document's own rules were prose, which constrains nobody.

**The proof is in the engine's own archived `RESTRICTIONS.md`.** It banned `stylo` while the workspace pinned five stylo crates and built the whole cascade on them, and mandated `lol_html` as the CSS sanitizer when no such dependency existed anywhere. Two of its four rules were false and nothing noticed, because nothing checked them.

So the rules below are **tests**, in `tests/guardrails.rs`:

| Rule | Ratchet |
|---|---|
| No `unwrap()`/`expect()` in production code (§1 above) | 12 |
| Pages ask `LibraryService`, they do not hold `Arc<Catalog>` | 58 occurrences in `src/pages/` |
| Colours live in `theme.rs`, not `resources/style.css` | 184 hex literals |

A **ratchet** counts today's violations and asserts the count never rises. Fix some, lower the constant. That is what makes a rule practical when most of them are already broken in a few places — a test asserting the ideal would fail on day one and get deleted.

### Anti-bloat rules

These stay prose, because nothing can test for "unnecessary". The list is kept short enough to actually be read:

- **No new abstraction before there is a second caller.** One caller means write it inline; extract when the second appears.
- **No new dependency without saying what it replaces or enables.**
- **No new environment-variable switch without removing one.** There are currently **five** `KALAM_*` switches plus `RUST_LOG`; that is the ceiling. Note this already slipped by one since the count was first written down, which is the argument for writing the ceiling down at all.
- **No new page without a route and a way back out of it.**
- **Part 2 stays out of Part 1.** `docs/offline-roadmap.md` is the boundary; anything that touches the network belongs on the other side of it.
