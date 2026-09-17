# Author Page Database & Caching Optimization

## Overview
This document records the architectural changes and performance optimizations made to author page database lookups, offline caching, and OpenLibrary network request handling.

## Key Changes
1. **Indexed SQLite Author Lookup (`Catalog::books_for_author`)**:
   - Replaced full-library loading and Rust in-memory filtering with candidate UNION queries targeting author display name, sort name, and delimited multi-author lists.
   - Utilizes `idx_books_authors` index (`books(authors COLLATE NOCASE)`) so SQLite does fast indexed lookups instead of scanning the full table.
   - Preserves exact author normalization filtering on the candidate set.

2. **Offline Author Caching & Local Storage**:
   - `AuthorProfile` records and aliases are stored in `author_profiles` and `author_aliases` tables in `catalog.db`.
   - Photos and work covers are cached on disk in `authors_dir()`.
   - On page navigation, cached profiles load immediately from SQLite and disk without issuing network calls.
   - On network errors (e.g. offline mode), `fetch_and_cache_author` falls back cleanly to the existing cached SQLite profile.

3. **Rate Limiting & Network Budgeting (`enrich_author_work`)**:
   - Network requests during work enrichment are capped with a strict budget of 5 remote work detail/rating calls per fetch.
   - Remote series names check local `series_cache` first.
   - Re-fetches re-use previously cached `AuthorWork` attributes (series name, key, ratings, covers) from SQLite so already-enriched works require 0 HTTP requests.
   - When OpenLibrary returns HTTP 429 (Rate Limited), all further remote work detail, rating, and cover requests are aborted immediately, preventing UI freezes.

## Verification
- `cargo check`: Passed cleanly with zero compilation errors.
- `cargo test`: All 345 unit tests passed, including `books_for_author_queries_database_indexed` and `books_for_author_uses_minimal_queries`.
