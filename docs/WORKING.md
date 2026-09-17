# Repository Working Rules & Developer Invariants

This document outlines the strict developer invariants and repository guidelines for **Kalam**. All developers and automated coding agents modifying this codebase must adhere to these rules without exception.

---

## 1. Core Invariants

### 1. Zero `unwrap()` in Production Code
- **Rule**: `unwrap()` and `expect()` are forbidden in non-test production code.
- **Rationale**: An unhandled panic in production crashes the application, resulting in bad user experience and potential data corruption.
- **Enforcement**: Always handle errors explicitly using `?`, pattern matching (`match`, `if let`), `unwrap_or()`, or `unwrap_or_default()`. If an invariant is truly unreachable, use error logging or safe fallbacks.

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
