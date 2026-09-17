//! Data export engine for Kalam reading data.
//!
//! Sub-Step C of Step 3: Export saved vocabulary words, quotes, and highlight
//! notes to human-readable Markdown files (~/Kalam-Export.md).

use crate::db::Catalog;
use anyhow::Result;
use std::path::{Path, PathBuf};

impl Catalog {
    /// Export all reading data (saved vocabulary, quotes, highlight notes)
    /// to a Markdown document at `output_path`.
    ///
    /// If `output_path` is empty or points to `~/Kalam-Export.md`, defaults to
    /// `~/Kalam-Export.md`.
    /// Returns the total number of items exported (words + quotes + highlights).
    pub fn export_reading_data_markdown(&self, output_path: &Path) -> Result<usize> {
        let expanded_path;
        let path = if output_path.as_os_str().is_empty() || output_path == Path::new("~/Kalam-Export.md") {
            expanded_path = crate::paths::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("Kalam-Export.md");
            &expanded_path
        } else if let Ok(stripped) = output_path.strip_prefix("~/") {
            expanded_path = crate::paths::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(stripped);
            &expanded_path
        } else {
            output_path
        };

        // Gather saved vocabulary words, quotes, and highlights
        let words = self.list_all_saved_words_unlimited()?;
        let (quotes, highlights) = self.list_all_annotations_unlimited()?;

        // Batch load book metadata
        let mut book_ids = Vec::new();
        for w in &words {
            if let Some(id) = w.book_id {
                book_ids.push(id);
            }
        }
        for q in &quotes {
            book_ids.push(q.book_id);
        }
        for h in &highlights {
            book_ids.push(h.book_id);
        }
        book_ids.sort_unstable();
        book_ids.dedup();
        let books = self.books_by_ids(&book_ids).unwrap_or_default();

        let mut md = String::new();
        md.push_str("# Kalam Reading Data Export\n\n");
        md.push_str(&format!("Exported: {}\n\n", crate::db::chrono_like_now()));

        // 1. Saved Vocabulary Words
        md.push_str("## Saved Vocabulary\n\n");
        if words.is_empty() {
            md.push_str("_No saved vocabulary words._\n\n");
        } else {
            for word in &words {
                let ipa = crate::db::pronunciation_for(&word.word);
                let entry = self.lookup_entry(&word.word).ok();
                let pos_str = entry.as_ref().map(|e| e.pos.join(", ")).unwrap_or_default();

                md.push_str(&format!("- **{}**", word.word));
                if let Some(ipa_text) = ipa {
                    if !ipa_text.is_empty() {
                        md.push_str(&format!(" `/{}/`", ipa_text));
                    }
                }
                if !pos_str.is_empty() {
                    md.push_str(&format!(" *({})*", pos_str));
                }
                md.push('\n');

                let def_clean = word.definition.trim();
                if !def_clean.is_empty() {
                    let formatted_def = def_clean.replace("\r\n", "\n").replace('\n', " ");
                    md.push_str(&format!("  - **Definition:** {}\n", formatted_def));
                }
                if let Some(ctx) = &word.context_text {
                    let ctx_clean = ctx.trim();
                    if !ctx_clean.is_empty() {
                        let formatted_ctx = ctx_clean.replace("\r\n", "\n").replace('\n', " ");
                        let source_str = word.book_id.and_then(|id| books.get(&id)).map(|b| {
                            format!(" (*{}* by {})", b.title, b.authors_display())
                        }).unwrap_or_default();
                        md.push_str(&format!("  - **Context:** \"{}\"{}\n", formatted_ctx, source_str));
                    }
                }
            }
            md.push('\n');
        }

        // 2. Saved Quotes
        md.push_str("## Saved Quotes\n\n");
        if quotes.is_empty() {
            md.push_str("_No saved quotes._\n\n");
        } else {
            for q in &quotes {
                let book_info = books.get(&q.book_id);
                let title = book_info.map(|b| b.title.as_str()).unwrap_or("Unknown Book");
                let authors = book_info.map(|b| b.authors_display()).unwrap_or("Unknown");

                let excerpt_clean = q.text_excerpt.trim().replace("\r\n", "\n").replace('\n', " ");
                md.push_str(&format!("- > {}\n", excerpt_clean));
                md.push_str(&format!("  — *{}* by {} ({})\n", title, authors, q.created_at));
                let note_clean = q.note.trim();
                if !note_clean.is_empty() {
                    let formatted_note = note_clean.replace("\r\n", "\n").replace('\n', " ");
                    md.push_str(&format!("  - **Note:** {}\n", formatted_note));
                }
            }
            md.push('\n');
        }

        // 3. Highlight Notes
        md.push_str("## Highlight Notes\n\n");
        if highlights.is_empty() {
            md.push_str("_No highlight notes._\n\n");
        } else {
            for h in &highlights {
                let book_info = books.get(&h.book_id);
                let title = book_info.map(|b| b.title.as_str()).unwrap_or("Unknown Book");
                let authors = book_info.map(|b| b.authors_display()).unwrap_or("Unknown");

                let excerpt_clean = h.text_excerpt.trim().replace("\r\n", "\n").replace('\n', " ");
                md.push_str(&format!("- **\"{}\"**\n", excerpt_clean));
                let note_clean = h.note.trim();
                if !note_clean.is_empty() {
                    let formatted_note = note_clean.replace("\r\n", "\n").replace('\n', " ");
                    md.push_str(&format!("  - **Note:** {}\n", formatted_note));
                }
                md.push_str(&format!("  - **Source:** *{}* by {} ({})\n", title, authors, h.created_at));
            }
            md.push('\n');
        }

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(path, md)?;

        let total_count = words.len() + quotes.len() + highlights.len();
        Ok(total_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::BookFormat;

    fn temp_test_file(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{prefix}_{}.md", uuid::Uuid::new_v4()))
    }

    #[test]
    fn test_export_reading_data_markdown_empty() {
        let cat = Catalog::open_in_memory().expect("in-memory catalog");
        let path = temp_test_file("export_empty");

        let count = cat.export_reading_data_markdown(&path).unwrap();
        assert_eq!(count, 0);

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("# Kalam Reading Data Export"));
        assert!(content.contains("## Saved Vocabulary"));
        assert!(content.contains("## Saved Quotes"));
        assert!(content.contains("## Highlight Notes"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_export_reading_data_markdown_with_data() {
        let cat = Catalog::open_in_memory().expect("in-memory catalog");
        let book_id = cat
            .insert_book(
                "uuid-export-test",
                "Export Test Book",
                "Jane Author",
                None,
                "",
                BookFormat::Epub,
                "book.epub",
                "hash-export-test",
                None,
                &[],
            )
            .expect("insert book");

        // Insert saved word
        cat.insert_saved_word(
            "serendipity",
            "finding valuable things by chance",
            None,
            Some(book_id),
            Some(1),
            Some("It was a happy serendipity."),
        )
        .expect("insert word");

        // Insert quote
        cat.insert_annotation(
            book_id,
            "quote",
            0,
            "",
            0,
            "",
            0,
            "yellow",
            "To be or not to be.",
            "Famous soliloquy",
        )
        .expect("insert quote");

        // Insert highlight
        cat.insert_annotation(
            book_id,
            "highlight",
            1,
            "",
            0,
            "",
            0,
            "green",
            "Important key concept text",
            "My custom note",
        )
        .expect("insert highlight");

        let path = temp_test_file("export_data");

        let count = cat.export_reading_data_markdown(&path).unwrap();
        assert_eq!(count, 3);

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("# Kalam Reading Data Export"));
        assert!(content.contains("serendipity"));
        assert!(content.contains("finding valuable things by chance"));
        assert!(content.contains("It was a happy serendipity."));
        assert!(content.contains("To be or not to be."));
        assert!(content.contains("Export Test Book"));
        assert!(content.contains("Jane Author"));
        assert!(content.contains("Famous soliloquy"));
        assert!(content.contains("Important key concept text"));
        assert!(content.contains("My custom note"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_export_reading_data_markdown_path_expansion() {
        let cat = Catalog::open_in_memory().expect("in-memory catalog");
        let default_target = crate::paths::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Kalam-Export.md");

        // Clean up beforehand if it exists
        let _ = std::fs::remove_file(&default_target);

        let count = cat.export_reading_data_markdown(Path::new("~/Kalam-Export.md")).unwrap();
        assert_eq!(count, 0);
        assert!(default_target.exists());

        let content = std::fs::read_to_string(&default_target).unwrap();
        assert!(content.contains("# Kalam Reading Data Export"));

        let _ = std::fs::remove_file(&default_target);
    }
}
