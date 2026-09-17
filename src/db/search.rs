//! Library search query parser and SQL query generator.
//!
//! Supports filter prefixes:
//! - `tag:name` or `tag:"tag name"`
//! - `author:name` or `author:"author name"`
//! - `status:unread|reading|finished`
//! - `rating:>3` | `rating:>=4` | `rating:<3` | `rating:<=2` | `rating:5`

use super::{escape_like, BOOK_COLUMNS};

#[derive(Debug, Clone, PartialEq)]
pub enum SearchFilter {
    Tag(String),
    Author(String),
    Status(StatusFilter),
    Rating(RatingOp, u8),
    Text(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusFilter {
    Unread,
    Reading,
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RatingOp {
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
    Equal,
}

/// Tokenize search query string, keeping quoted phrases intact.
pub fn tokenize_search_query(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for ch in query.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
            }
            c if c.is_whitespace() && !in_quotes => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            c => {
                current.push(c);
            }
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Parse search query into structured search filters.
pub fn parse_search_query(query: &str) -> Vec<SearchFilter> {
    let raw_tokens = tokenize_search_query(query);
    let mut filters = Vec::new();

    for token in raw_tokens {
        let token_trim = token.trim();
        if token_trim.is_empty() {
            continue;
        }

        if let Some((prefix, val)) = token_trim.split_once(':') {
            let prefix_lower = prefix.to_lowercase();
            match prefix_lower.as_str() {
                "tag" => {
                    let val = val.trim();
                    if !val.is_empty() {
                        filters.push(SearchFilter::Tag(val.to_string()));
                        continue;
                    }
                }
                "author" => {
                    let val = val.trim();
                    if !val.is_empty() {
                        filters.push(SearchFilter::Author(val.to_string()));
                        continue;
                    }
                }
                "status" => {
                    let val = val.trim().to_lowercase();
                    match val.as_str() {
                        "unread" => {
                            filters.push(SearchFilter::Status(StatusFilter::Unread));
                            continue;
                        }
                        "reading" => {
                            filters.push(SearchFilter::Status(StatusFilter::Reading));
                            continue;
                        }
                        "finished" | "read" => {
                            filters.push(SearchFilter::Status(StatusFilter::Finished));
                            continue;
                        }
                        _ => {}
                    }
                }
                "rating" => {
                    let val = val.trim();
                    let (op, rest) = if let Some(r) = val.strip_prefix(">=") {
                        (RatingOp::GreaterThanOrEqual, r)
                    } else if let Some(r) = val.strip_prefix('>') {
                        (RatingOp::GreaterThan, r)
                    } else if let Some(r) = val.strip_prefix("<=") {
                        (RatingOp::LessThanOrEqual, r)
                    } else if let Some(r) = val.strip_prefix('<') {
                        (RatingOp::LessThan, r)
                    } else if let Some(r) = val.strip_prefix('=') {
                        (RatingOp::Equal, r)
                    } else {
                        (RatingOp::Equal, val)
                    };

                    if let Ok(num) = rest.trim().parse::<f32>() {
                        let half_stars = if num <= 5.0 && num > 0.0 {
                            (num * 2.0).round() as u8
                        } else {
                            num.round().clamp(0.0, 10.0) as u8
                        };
                        filters.push(SearchFilter::Rating(op, half_stars));
                        continue;
                    }
                }
                _ => {}
            }
        }

        filters.push(SearchFilter::Text(token_trim.to_string()));
    }

    filters
}

/// Build SQLite SQL string and parameter list from parsed search filters.
pub fn build_search_sql(
    filters: &[SearchFilter],
    sort_order_sql: &str,
) -> (String, Vec<rusqlite::types::Value>) {
    if filters.is_empty() {
        return (
            format!("SELECT {BOOK_COLUMNS} FROM books ORDER BY {sort_order_sql}"),
            Vec::new(),
        );
    }

    let mut where_clauses = Vec::new();
    let mut sql_params: Vec<rusqlite::types::Value> = Vec::new();
    let mut param_idx = 1;

    for filter in filters {
        match filter {
            SearchFilter::Tag(tag) => {
                where_clauses.push(format!(
                    "books.id IN (SELECT book_id FROM book_tags JOIN tags ON tags.id = book_tags.tag_id WHERE tags.name LIKE ?{param_idx} ESCAPE '\\')"
                ));
                sql_params.push(format!("%{}%", escape_like(tag)).into());
                param_idx += 1;
            }
            SearchFilter::Author(author) => {
                where_clauses.push(format!(
                    "books.authors LIKE ?{param_idx} ESCAPE '\\'"
                ));
                sql_params.push(format!("%{}%", escape_like(author)).into());
                param_idx += 1;
            }
            SearchFilter::Status(StatusFilter::Unread) => {
                where_clauses.push(
                    "((books.finished_at IS NULL OR books.finished_at = '') AND books.progress = 0)".to_string(),
                );
            }
            SearchFilter::Status(StatusFilter::Reading) => {
                where_clauses.push(
                    "((books.finished_at IS NULL OR books.finished_at = '') AND books.progress > 0 AND books.progress < 100)".to_string(),
                );
            }
            SearchFilter::Status(StatusFilter::Finished) => {
                where_clauses.push(
                    "((books.finished_at IS NOT NULL AND books.finished_at <> '') OR books.progress >= 100)".to_string(),
                );
            }
            SearchFilter::Rating(op, val) => {
                let op_str = match op {
                    RatingOp::GreaterThan => ">",
                    RatingOp::GreaterThanOrEqual => ">=",
                    RatingOp::LessThan => "<",
                    RatingOp::LessThanOrEqual => "<=",
                    RatingOp::Equal => "=",
                };
                where_clauses.push(format!("books.rating {op_str} ?{param_idx}"));
                sql_params.push((*val as i64).into());
                param_idx += 1;
            }
            SearchFilter::Text(text) => {
                where_clauses.push(format!(
                    "(books.title LIKE ?{param_idx} ESCAPE '\\' OR books.authors LIKE ?{param_idx} ESCAPE '\\' OR IFNULL(books.series,'') LIKE ?{param_idx} ESCAPE '\\')"
                ));
                sql_params.push(format!("%{}%", escape_like(text)).into());
                param_idx += 1;
            }
        }
    }

    let sql = format!(
        "SELECT {BOOK_COLUMNS} FROM books WHERE {} ORDER BY {sort_order_sql}",
        where_clauses.join(" AND ")
    );

    (sql, sql_params)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_search_query() {
        let query = r#"tag:fantasy author:"Brandon Sanderson" status:unread rating:>3 Dune"#;
        let tokens = tokenize_search_query(query);
        assert_eq!(
            tokens,
            vec![
                "tag:fantasy",
                "author:Brandon Sanderson",
                "status:unread",
                "rating:>3",
                "Dune"
            ]
        );
    }

    #[test]
    fn test_parse_search_query() {
        let query = r#"tag:fantasy author:"Brandon Sanderson" status:unread rating:>3 Dune"#;
        let filters = parse_search_query(query);
        assert_eq!(
            filters,
            vec![
                SearchFilter::Tag("fantasy".to_string()),
                SearchFilter::Author("Brandon Sanderson".to_string()),
                SearchFilter::Status(StatusFilter::Unread),
                SearchFilter::Rating(RatingOp::GreaterThan, 6),
                SearchFilter::Text("Dune".to_string()),
            ]
        );
    }

    #[test]
    fn test_build_search_sql() {
        let query = "tag:fantasy status:finished rating:>=4";
        let filters = parse_search_query(query);
        let (sql, params) = build_search_sql(&filters, "sort_title ASC");
        assert!(sql.contains("book_tags"));
        assert!(sql.contains("finished_at"));
        assert!(sql.contains("books.rating >= ?"));
        assert_eq!(params.len(), 2); // tag param + rating param (status needs no param)
    }

    #[test]
    fn test_parse_search_query_edge_cases() {
        let query = r#"Tag:"epic fantasy" Author:"Frank Herbert" Status:Reading Rating:<=4.5 "Dune: Messiah""#;
        let filters = parse_search_query(query);
        assert_eq!(
            filters,
            vec![
                SearchFilter::Tag("epic fantasy".to_string()),
                SearchFilter::Author("Frank Herbert".to_string()),
                SearchFilter::Status(StatusFilter::Reading),
                SearchFilter::Rating(RatingOp::LessThanOrEqual, 9),
                SearchFilter::Text("Dune: Messiah".to_string()),
            ]
        );
    }
}
