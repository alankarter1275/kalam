//! Library-wide full-text content indexing and deep search using SQLite FTS5.
//!
//! Indexes book content across EPUB and PDF for instant library-wide search.
//! Queries return matching books with snippet excerpts, match counters,
//! chapter headers, and direct reader navigation.

use std::collections::HashMap;
use std::path::Path;
use anyhow::Result;
use chapbook_core::Publication;
use rusqlite::params;
use crate::db::Catalog;
use crate::models::{Book, BookFormat};

/// A single matched passage inside a chapter or page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentMatch {
    pub chapter_index: usize,
    pub chapter_title: String,
    pub snippet: String,
}

/// A book matching the content search query, containing all matched snippets.
#[derive(Debug, Clone)]
pub struct BookContentSearchResult {
    pub book: Book,
    pub matches: Vec<ContentMatch>,
    pub total_matches: usize,
}

/// Status of the library-wide content search index.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContentIndexStatus {
    pub total_books: usize,
    pub indexed_books: usize,
    pub total_chapters: usize,
    pub total_words: usize,
}

/// Sanitize and format a user query string for SQLite FTS5 `MATCH`.
///
/// Handles:
/// - Explicit quoted phrases (e.g. `"Mr. Darcy"` -> `"Mr. Darcy"`)
/// - Multi-word queries with prefix search on the last word (e.g. `darcy eliz` -> `"darcy" eliz*`)
/// - Single word prefix search (e.g. `darcy` -> `darcy*`)
/// - Punctuation and special characters that could otherwise break FTS5 syntax
pub fn sanitize_fts5_query(query: &str) -> String {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    // If enclosed in double quotes, treat as an exact phrase query
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() > 2 {
        let inner: String = trimmed[1..trimmed.len() - 1]
            .chars()
            .filter(|c| *c != '"')
            .collect();
        let inner_trimmed = inner.trim();
        if !inner_trimmed.is_empty() {
            return format!("\"{inner_trimmed}\"");
        }
    }

    // Split query by non-alphanumeric characters into clean words
    let words: Vec<String> = trimmed
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_string())
        .collect();

    if words.is_empty() {
        return String::new();
    }

    if words.len() == 1 {
        format!("{}*", words[0])
    } else {
        let mut parts = Vec::new();
        for w in &words[..words.len() - 1] {
            parts.push(format!("\"{w}\""));
        }
        if let Some(last) = words.last() {
            parts.push(format!("{last}*"));
        }
        parts.join(" ")
    }
}

/// Replace marker tokens with warm golden amber Pango markup spans.
///
/// HTML special characters (`<`, `>`, `&`, `"`) in the original text are
/// safely escaped with `glib::markup_escape_text` before markers are replaced.
pub fn highlight_match_markers(snippet_with_markers: &str) -> String {
    let escaped = glib::markup_escape_text(snippet_with_markers);
    escaped
        .replace(
            "[[[MATCH]]]",
            "<span weight=\"bold\" foreground=\"#d97706\" background=\"#f4d35e\" background_alpha=\"30%\">",
        )
        .replace("[[[/MATCH]]]", "</span>")
}

/// Extract chapters and text from an EPUB file.
fn extract_epub_content(path: &Path) -> Result<Vec<(usize, String, String)>> {
    let book = chapbook_epub::Book::open(path)?;
    let mut chapter_titles: HashMap<usize, String> = HashMap::new();

    fn collect_toc(entries: &[chapbook_core::TocEntry], map: &mut HashMap<usize, String>) {
        for e in entries {
            if let Some(idx) = e.spine_index {
                map.entry(idx).or_insert_with(|| e.label.clone());
            }
            collect_toc(&e.children, map);
        }
    }
    collect_toc(book.toc(), &mut chapter_titles);

    let spine = book.spine();
    let mut sections = Vec::new();

    for (idx, item) in spine.iter().enumerate() {
        if let Ok(bytes) = book.unit_bytes(idx) {
            let text = if let Ok(doc) = chapbook_layout::dom::parse_xhtml(&bytes, &item.href) {
                chapbook_layout::dom::locator_text(&doc)
            } else {
                crate::epub::strip_html(&String::from_utf8_lossy(&bytes))
            };
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                let title = chapter_titles
                    .get(&idx)
                    .cloned()
                    .unwrap_or_else(|| format!("Section {}", idx + 1));
                sections.push((idx, title, trimmed.to_string()));
            }
        }
    }
    Ok(sections)
}

/// Extract pages and text from a PDF file.
fn extract_pdf_content(path: &Path) -> Result<Vec<(usize, String, String)>> {
    let doc = crate::pdf::PdfDocument::open(path)?;
    let page_count = doc.page_count();
    let mut sections = Vec::new();

    for page in 0..page_count {
        let text = doc.extract_raw_text(page).unwrap_or_default();
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            let title = format!("Page {}", page + 1);
            sections.push((page, title, trimmed.to_string()));
        }
    }
    Ok(sections)
}

/// Index a single book into the SQLite FTS5 search index.
pub fn index_book(catalog: &Catalog, book_id: i64) -> Result<()> {
    let Some(book) = catalog.get_book(book_id)? else {
        return Ok(());
    };
    if !book.file_path.exists() {
        return Ok(());
    }

    // Only text-based books (EPUB and PDF) contain searchable written text.
    // Comics (CBZ/CBR) are compressed archives of images without text streams.
    if !matches!(book.format, BookFormat::Epub | BookFormat::Pdf) {
        return Ok(());
    }

    let sections = match book.format {
        BookFormat::Epub => extract_epub_content(&book.file_path).unwrap_or_default(),
        BookFormat::Pdf => extract_pdf_content(&book.file_path).unwrap_or_default(),
        _ => Vec::new(),
    };

    let total_chapters = sections.len();
    let mut total_words = 0usize;
    for (_, _, text) in &sections {
        total_words += text.split_whitespace().count();
    }

    let conn = catalog.conn();
    conn.execute(
        "DELETE FROM book_content_fts WHERE book_id = ?1",
        params![book_id],
    )?;
    conn.execute(
        "DELETE FROM book_search_index_status WHERE book_id = ?1",
        params![book_id],
    )?;

    if !sections.is_empty() {
        let mut stmt = conn.prepare_cached(
            "INSERT INTO book_content_fts (book_id, chapter_index, chapter_title, content)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for (idx, title, text) in sections {
            stmt.execute(params![book_id, idx as i64, title, text])?;
        }
    }

    let now = crate::db::chrono_like_now();
    conn.execute(
        "INSERT INTO book_search_index_status (book_id, indexed_at, total_chapters, total_words)
         VALUES (?1, ?2, ?3, ?4)",
        params![book_id, now, total_chapters as i64, total_words as i64],
    )?;

    Ok(())
}

/// Index all text books (EPUB & PDF) currently in the catalog that are not yet in the index.
pub fn index_all_unindexed(catalog: &Catalog) -> Result<usize> {
    let conn = catalog.conn();
    let mut stmt = conn.prepare(
        "SELECT id FROM books WHERE UPPER(format) IN ('EPUB', 'PDF') AND id NOT IN (SELECT book_id FROM book_search_index_status) ORDER BY id ASC",
    )?;
    let ids: Vec<i64> = stmt
        .query_map([], |r| r.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    drop(stmt);
    drop(conn);

    let mut count = 0;
    for id in ids {
        if index_book(catalog, id).is_ok() {
            count += 1;
        }
    }
    Ok(count)
}

/// Rebuild the full-text search index for all text books (EPUB & PDF) from scratch.
pub fn reindex_all(catalog: &Catalog) -> Result<usize> {
    let conn = catalog.conn();
    conn.execute("DELETE FROM book_content_fts", [])?;
    conn.execute("DELETE FROM book_search_index_status", [])?;
    let mut stmt = conn.prepare("SELECT id FROM books WHERE UPPER(format) IN ('EPUB', 'PDF') ORDER BY id ASC")?;
    let ids: Vec<i64> = stmt
        .query_map([], |r| r.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    drop(stmt);
    drop(conn);

    let mut count = 0;
    for id in ids {
        if index_book(catalog, id).is_ok() {
            count += 1;
        }
    }
    Ok(count)
}

/// Get the current status of the content search index for text books (EPUB & PDF).
pub fn get_index_status(catalog: &Catalog) -> Result<ContentIndexStatus> {
    let conn = catalog.conn();
    let total_books: i64 = conn.query_row(
        "SELECT COUNT(*) FROM books WHERE UPPER(format) IN ('EPUB', 'PDF')",
        [],
        |r| r.get(0),
    )?;
    let indexed_books: i64 = conn.query_row(
        "SELECT COUNT(*) FROM book_search_index_status WHERE book_id IN (SELECT id FROM books WHERE UPPER(format) IN ('EPUB', 'PDF'))",
        [],
        |r| r.get(0),
    )?;
    let (total_chapters, total_words): (i64, i64) = conn.query_row(
        "SELECT COALESCE(SUM(total_chapters), 0), COALESCE(SUM(total_words), 0) FROM book_search_index_status WHERE book_id IN (SELECT id FROM books WHERE UPPER(format) IN ('EPUB', 'PDF'))",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;

    Ok(ContentIndexStatus {
        total_books: total_books.max(0) as usize,
        indexed_books: indexed_books.max(0) as usize,
        total_chapters: total_chapters.max(0) as usize,
        total_words: total_words.max(0) as usize,
    })
}

struct RawMatch {
    book_id: i64,
    chapter_index: usize,
    chapter_title: String,
    snippet: String,
}

/// Execute a library-wide full-text content search.
///
/// Returns matching books with snippets and match counts, ordered by match rank.
pub fn search_content(catalog: &Catalog, query: &str) -> Result<Vec<BookContentSearchResult>> {
    let fts_query = sanitize_fts5_query(query);
    if fts_query.is_empty() {
        return Ok(Vec::new());
    }

    let conn = catalog.conn();
    let mut stmt = conn.prepare_cached(
        "SELECT fts.book_id, fts.chapter_index, fts.chapter_title,
                snippet(book_content_fts, 3, '[[[MATCH]]]', '[[[/MATCH]]]', '…', 16) AS snip
         FROM book_content_fts fts
         WHERE book_content_fts MATCH ?1
         ORDER BY rank
         LIMIT 300",
    )?;

    let raw_rows = stmt.query_map(params![fts_query], |r| {
        Ok(RawMatch {
            book_id: r.get(0)?,
            chapter_index: r.get::<_, i64>(1)? as usize,
            chapter_title: r.get(2)?,
            snippet: r.get(3)?,
        })
    })?;

    let mut matches_by_book: Vec<(i64, Vec<ContentMatch>)> = Vec::new();
    let mut book_index_map: HashMap<i64, usize> = HashMap::new();

    for item in raw_rows.flatten() {
        let highlighted = highlight_match_markers(&item.snippet);
        let m = ContentMatch {
            chapter_index: item.chapter_index,
            chapter_title: item.chapter_title,
            snippet: highlighted,
        };

        if let Some(&idx) = book_index_map.get(&item.book_id) {
            matches_by_book[idx].1.push(m);
        } else {
            let idx = matches_by_book.len();
            book_index_map.insert(item.book_id, idx);
            matches_by_book.push((item.book_id, vec![m]));
        }
    }

    drop(stmt);
    drop(conn);

    let mut results = Vec::new();
    for (book_id, matches) in matches_by_book {
        if let Ok(Some(book)) = catalog.get_book(book_id) {
            let total_matches = matches.len();
            results.push(BookContentSearchResult {
                book,
                matches,
                total_matches,
            });
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_fts5_query() {
        assert_eq!(sanitize_fts5_query("   "), "");
        assert_eq!(sanitize_fts5_query("darcy"), "darcy*");
        assert_eq!(sanitize_fts5_query("Elizabeth Bennet"), "\"Elizabeth\" Bennet*");
        assert_eq!(sanitize_fts5_query("\"Mr. Darcy\""), "\"Mr. Darcy\"");
        assert_eq!(sanitize_fts5_query("science-fiction novel"), "\"science\" \"fiction\" novel*");
    }

    #[test]
    fn test_highlight_match_markers() {
        let input = "He saw [[[MATCH]]]Darcy[[[/MATCH]]] with <Elizabeth> & Jane.";
        let out = highlight_match_markers(input);
        assert!(out.contains("&lt;Elizabeth&gt;"));
        assert!(out.contains("&amp; Jane."));
        assert!(out.contains("<span weight=\"bold\" foreground=\"#d97706\" background=\"#f4d35e\" background_alpha=\"30%\">Darcy</span>"));
    }

    #[test]
    fn test_content_fts_roundtrip() {
        let cat = Catalog::open_in_memory().unwrap();
        let conn = cat.conn();

        // Insert test book into books table
        conn.execute(
            "INSERT INTO books (uuid, title, sort_title, authors, series, description, format, file_name, file_hash, added_at, progress)
             VALUES ('u-1', 'Pride and Prejudice', 'pride and prejudice', 'Jane Austen', NULL, '', 'epub', 'book.epub', 'h1', '2026-01-01', 0)",
            [],
        ).unwrap();
        let book_id = conn.last_insert_rowid();

        // Insert a comic book into books table to verify it is excluded from text content indexing
        conn.execute(
            "INSERT INTO books (uuid, title, sort_title, authors, series, description, format, file_name, file_hash, added_at, progress)
             VALUES ('u-comic', 'Naruto Ch 1', 'naruto ch 1', 'Kishimoto', NULL, '', 'cbz', 'naruto.cbz', 'h2', '2026-01-01', 0)",
            [],
        ).unwrap();

        // Insert content into FTS table directly for test
        conn.execute(
            "INSERT INTO book_content_fts (book_id, chapter_index, chapter_title, content)
             VALUES (?1, 0, 'Chapter 1', 'It is a truth universally acknowledged that Mr. Darcy is proud.')",
            params![book_id],
        ).unwrap();
        conn.execute(
            "INSERT INTO book_content_fts (book_id, chapter_index, chapter_title, content)
             VALUES (?1, 1, 'Chapter 2', 'Elizabeth Bennet walked across the garden thinking about Darcy.')",
            params![book_id],
        ).unwrap();

        conn.execute(
            "INSERT INTO book_search_index_status (book_id, indexed_at, total_chapters, total_words)
             VALUES (?1, '2026-01-01', 2, 22)",
            params![book_id],
        ).unwrap();

        drop(conn);

        let status = get_index_status(&cat).unwrap();
        // total_books should be 1 (only EPUB/PDF text books, excluding the comic)
        assert_eq!(status.total_books, 1);
        assert_eq!(status.indexed_books, 1);
        assert_eq!(status.total_chapters, 2);

        let res = search_content(&cat, "Darcy").unwrap();
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].book.title, "Pride and Prejudice");
        assert_eq!(res[0].total_matches, 2);
        assert_eq!(res[0].matches.len(), 2);
    }
}
