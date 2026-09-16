//! Dictionaries queries.
//!
//! Split out of a 3,400-line `db.rs` purely to make it navigable; these are
//! the same methods on the same `Catalog`, moved verbatim.

use super::*;
use std::collections::HashMap;
use std::sync::OnceLock;
use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

/// Princeton WordNet 3.0 morphological exception lists (noun/verb/adj/adv),
/// gzipped. Surface form → lemmas, one entry per line. See
/// `resources/dictionaries/wordnet-3.0-exc.NOTICE.txt` for source, licence
/// and checksums.
const WORDNET_NOUN_EXC: &[u8] =
    include_bytes!("../../resources/dictionaries/wordnet-3.0-noun.exc.gz");
const WORDNET_VERB_EXC: &[u8] =
    include_bytes!("../../resources/dictionaries/wordnet-3.0-verb.exc.gz");
const WORDNET_ADJ_EXC: &[u8] =
    include_bytes!("../../resources/dictionaries/wordnet-3.0-adj.exc.gz");
const WORDNET_ADV_EXC: &[u8] =
    include_bytes!("../../resources/dictionaries/wordnet-3.0-adv.exc.gz");

/// WordNet exception lists, parsed once into surface form (lowercase) →
/// lemma candidates. Consulted before the suffix-rule fallback so irregulars
/// like `went → go`, `mice → mouse` and `better → good` resolve to a real
/// headword instead of a dead end. Loaded lazily: a lookup that never needs
/// lemmatization never pays the parse.
fn wordnet_exceptions() -> &'static HashMap<String, Vec<String>> {
    static EXC: OnceLock<HashMap<String, Vec<String>>> = OnceLock::new();
    EXC.get_or_init(|| {
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        for bytes in [
            WORDNET_NOUN_EXC,
            WORDNET_VERB_EXC,
            WORDNET_ADJ_EXC,
            WORDNET_ADV_EXC,
        ] {
            let decoder = flate2::read::GzDecoder::new(bytes);
            let reader = std::io::BufReader::new(decoder);
            for line in std::io::BufRead::lines(reader).map_while(|line| line.ok()) {
                let mut parts = line.split_whitespace();
                let Some(surface) = parts.next() else {
                    continue;
                };
                let lemmas: Vec<String> = parts.map(str::to_string).collect();
                if !lemmas.is_empty() {
                    map.entry(surface.to_string()).or_default().extend(lemmas);
                }
            }
        }
        map
    })
}

/// Fold a headword into its canonical dictionary key.
///
/// Lowercases, strips diacritics (NFD + drop combining marks), collapses
/// whitespace and trims surrounding non-alphanumerics per token, so `Run`,
/// `run` and `rún` all fold to `run` and every lookup hits the same `key`
/// column through `idx_dict_entries_key` instead of a case-insensitive
/// scan over `word`.
pub fn fold_key(word: &str) -> String {
    normalize_dictionary_term(word)
        .to_lowercase()
        .nfd()
        .filter(|ch| !is_combining_mark(*ch))
        .collect()
}

/// Bundled pack names — single source of truth for the auxiliary chip
/// queries (synonyms / antonyms / idioms) used by the popup.
pub const BUNDLED_WORDNET_NAME: &str = "English WordNet 2025";
pub const BUNDLED_IDIOMS_NAME: &str = "English Idioms and Expressions";
pub const BUNDLED_SYNONYMS_NAME: &str = "English Synonyms (WordNet 3.0)";
pub const BUNDLED_ANTONYMS_NAME: &str = "English Antonyms (WordNet 3.0)";

/// One sense in the redesigned popup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sense {
    /// Display number (1-based, sequential across the whole entry).
    pub number: usize,
    pub pos: Option<String>,
    pub def: String,
    pub example: Option<String>,
}

/// Everything the popup shows for one lookup: the winning entry's senses,
/// the word-level POS list, the auxiliary chips and idiom cards, and
/// did-you-mean / phrase-breakdown suggestions when there is no headword.
#[derive(Debug, Clone, Default)]
pub struct EntryData {
    pub word: String,
    pub senses: Vec<Sense>,
    pub pos: Vec<String>,
    pub synonyms: Vec<String>,
    pub antonyms: Vec<String>,
    pub idioms: Vec<(String, String)>,
    pub suggestions: Vec<String>,
}

/// Result of a phrase lookup: either the phrase (or a contained phrase
/// headword) matched, or the phrase was broken down per token.
#[derive(Debug, Clone)]
pub enum PhraseLookup {
    /// The full phrase — or the longest contained multi-word headword —
    /// matched. Entries are the matched headwords.
    Phrase(Vec<DictEntry>),
    /// No phrase headword matched; per-token results. Tokens with no hits
    /// are omitted.
    Breakdown(Vec<(String, Vec<DictEntry>)>),
    /// Nothing matched anywhere.
    Empty,
}

impl Catalog {
    // -----------------------------------------------------------------------
    // P3: Dictionaries
    // -----------------------------------------------------------------------

    pub fn list_dictionaries(&self) -> Result<Vec<Dictionary>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT id, name, lang, entry_count, added_at, priority
             FROM dictionaries ORDER BY priority ASC, name ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Dictionary {
                id: r.get(0)?,
                name: r.get(1)?,
                lang: r.get(2)?,
                entry_count: r.get(3)?,
                added_at: r.get(4)?,
                priority: r.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn insert_dictionary(
        &self,
        name: &str,
        lang: Option<&str>,
        entry_count: i64,
    ) -> Result<i64> {
        let conn = self.conn();
        let now = chrono_like_now();
        conn.execute(
            "INSERT INTO dictionaries (name, lang, entry_count, added_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(name) DO UPDATE SET lang=excluded.lang, entry_count=excluded.entry_count",
            params![name, lang, entry_count, now],
        )?;
        let id = conn.query_row(
            "SELECT id FROM dictionaries WHERE name = ?1",
            params![name],
            |r| r.get(0),
        )?;
        Ok(id)
    }

    pub fn delete_dictionary(&self, id: i64) -> Result<()> {
        let conn = self.conn();
        conn.execute("DELETE FROM dictionaries WHERE id = ?1", params![id])?;
        drop(conn);
        // A removed dictionary must stop speaking for its words immediately.
        self.rebuild_combined_dictionary()?;
        Ok(())
    }

    /// Priority for the merged store: lower numbers are consulted first, so
    /// the dictionary that speaks for a shared word is the one with the
    /// lowest priority (ties break by lowest id). Bundled packs get explicit
    /// priorities at install time; imported packs keep the default 100.
    pub fn set_dictionary_priority(&self, id: i64, priority: i64) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "UPDATE dictionaries SET priority = ?1 WHERE id = ?2",
            params![priority, id],
        )?;
        Ok(())
    }

    /// Move a dictionary one step up (`delta = -1`) or down (`+1`) in the
    /// effective order, then renumber all priorities compactly (0, 1, 2, …)
    /// and rebuild the merged store so the change speaks immediately.
    ///
    /// Order is the only thing that matters to `rebuild_combined_dictionary`
    /// — the compact renumbering overwrites the bundled packs' semantic
    /// 10/20/30/40 values, which is fine because nothing re-derives meaning
    /// from the numbers themselves. Re-importing a pack keeps its (possibly
    /// user-set) priority: `insert_dictionary`'s upsert does not touch it.
    pub fn move_dictionary_priority(&self, id: i64, delta: i64) -> Result<()> {
        let mut dicts = self.list_dictionaries()?;
        let Some(idx) = dicts.iter().position(|d| d.id == id) else {
            return Ok(());
        };
        let target = idx as i64 + delta;
        if target < 0 || target as usize >= dicts.len() {
            return Ok(());
        }
        dicts.swap(idx, target as usize);
        for (pos, d) in dicts.iter().enumerate() {
            self.set_dictionary_priority(d.id, pos as i64)?;
        }
        self.rebuild_combined_dictionary()
    }

    /// Rebuild the merged dictionary store from the raw entries.
    ///
    /// One row per headword key; the dictionary that speaks for a word is
    /// the one with the lowest priority (ties: lowest id), and the senses
    /// are that dictionary's definitions for the word, deduplicated, in
    /// entry order. The reader never merges at lookup time — it reads this
    /// table, so every word appears exactly once.
    ///
    /// Called automatically whenever the dictionary set changes (bundled
    /// install, import, removal) and once at migration for existing
    /// databases.
    pub fn rebuild_combined_dictionary(&self) -> Result<()> {
        struct Row {
            key: String,
            word: String,
            dict_id: i64,
            definition: String,
        }
        let mut conn = self.conn();

        // One pass over every entry, ordered by priority then entry order:
        // the first time a key appears it belongs to the winning dictionary.
        let mut rows = Vec::new();
        {
            let mut stmt = conn.prepare_cached(
                "SELECT e.key, e.word, e.dict_id, e.definition
                 FROM dict_entries e
                 JOIN dictionaries d ON d.id = e.dict_id
                 WHERE e.key IS NOT NULL AND e.key <> ''
                 ORDER BY d.priority ASC, d.id ASC, e.id ASC",
            )?;
            let mut query = stmt.query([])?;
            while let Some(r) = query.next()? {
                rows.push(Row {
                    key: r.get(0)?,
                    word: r.get(1)?,
                    dict_id: r.get(2)?,
                    definition: r.get(3)?,
                });
            }
        }

        // Merge in Rust: first key wins; identical definitions within the
        // winning dictionary collapse into one sense.
        let mut merged: Vec<(String, String, i64, Vec<String>)> = Vec::new();
        let mut position: HashMap<String, usize> = HashMap::new();
        for row in rows {
            if let Some(&pos) = position.get(&row.key) {
                if merged[pos].2 == row.dict_id {
                    let senses = &mut merged[pos].3;
                    if !senses.contains(&row.definition) {
                        senses.push(row.definition);
                    }
                }
            } else {
                position.insert(row.key.clone(), merged.len());
                merged.push((row.key, row.word, row.dict_id, vec![row.definition]));
            }
        }

        let tx = conn.transaction()?;
        tx.execute("DELETE FROM combined_words", [])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO combined_words (key, word, dict_id, senses) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (key, word, dict_id, senses) in merged {
                let json = serde_json::to_string(&senses).unwrap_or_else(|_| "[]".into());
                stmt.execute(params![key, word, dict_id, json])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn set_dictionary_entry_count(&self, id: i64, count: i64) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "UPDATE dictionaries SET entry_count = ?1 WHERE id = ?2",
            params![count, id],
        )?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn insert_dict_entry(&self, dict_id: i64, word: &str, definition: &str) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO dict_entries (dict_id, word, definition, key) VALUES (?1, ?2, ?3, ?4)",
            params![dict_id, word, definition, fold_key(word)],
        )?;
        Ok(())
    }

    pub fn batch_insert_dict_entries(
        &self,
        dict_id: i64,
        entries: &[(String, String)],
    ) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO dict_entries (dict_id, word, definition, key) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (w, d) in entries {
                stmt.execute(params![dict_id, w, d, fold_key(w)])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn clear_dict_entries(&self, dict_id: i64) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM dict_entries WHERE dict_id = ?1",
            params![dict_id],
        )?;
        Ok(())
    }

    /// One-time backfill of the `key` column for rows imported before the v11
    /// migration. Guarded by `key IS NULL` so it runs at most once per
    /// database and is a no-op on fresh installs (nothing imported yet).
    /// Batched in small transactions because existing libraries can hold a
    /// six-figure WordNet pack.
    pub(crate) fn backfill_dict_entry_keys(&self) -> Result<()> {
        let mut conn = self.conn();
        loop {
            let pending: Vec<(i64, String)> = {
                let mut stmt = conn.prepare_cached(
                    "SELECT id, word FROM dict_entries WHERE key IS NULL LIMIT 2000",
                )?;
                let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
                rows.collect::<std::result::Result<Vec<_>, _>>()?
            };
            if pending.is_empty() {
                return Ok(());
            }
            let tx = conn.transaction()?;
            {
                let mut stmt = tx.prepare("UPDATE dict_entries SET key = ?1 WHERE id = ?2")?;
                for (id, word) in &pending {
                    stmt.execute(params![fold_key(word), id])?;
                }
            }
            tx.commit()?;
        }
    }

    pub fn search_dict(&self, word: &str, limit: usize) -> Result<Vec<DictEntry>> {
        let clean = word.trim();
        if clean.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }

        let variants = dictionary_query_variants(clean);
        let lim = limit as i64;
        // Try exact and prefix matches for every normalized form before using
        // the older substring fallback. This lets `running` find `run` even
        // when a definition happens to contain the word "running".
        for variant in &variants {
            let out = self.search_dict_exact_or_prefix(variant, lim)?;
            if !out.is_empty() {
                return Ok(out);
            }
        }

        let normalized = normalize_dictionary_term(clean);
        let fallback = if normalized.is_empty() {
            clean
        } else {
            normalized.as_str()
        };
        self.search_dict_substring(fallback, lim)
    }

    /// Phrase lookup: try the whole phrase as a headword, then the longest
    /// contained multi-word headword (slide a window from longest to
    /// shortest — catches "run out of steam" inside a longer selection),
    /// then per-token single-word results.
    pub fn search_phrase(&self, phrase: &str, limit: usize) -> Result<PhraseLookup> {
        let clean = phrase.trim();
        if clean.is_empty() || limit == 0 {
            return Ok(PhraseLookup::Empty);
        }
        let lim = limit as i64;

        // (a) The whole phrase as a headword: try every normalized variant
        // (exact then prefix). Definition-substring matches are deliberately
        // excluded — a phrase lookup must stay headword-based.
        for variant in dictionary_query_variants(clean) {
            let hits = self.search_dict_exact_or_prefix(&variant, lim)?;
            if !hits.is_empty() {
                return Ok(PhraseLookup::Phrase(hits));
            }
        }

        let tokens: Vec<&str> = clean.split_whitespace().collect();
        if tokens.len() > 1 {
            // (b) Longest contained multi-word headword, exact match only.
            // The full phrase was already tried in (a), so windows start one
            // token shorter.
            for window_len in (2..tokens.len()).rev() {
                for window in tokens.windows(window_len) {
                    let candidate = window.join(" ");
                    let hits = self.search_dict_exact(&candidate, lim)?;
                    if !hits.is_empty() {
                        return Ok(PhraseLookup::Phrase(hits));
                    }
                }
            }
        }

        // (c) Per-token breakdown. `search_dict` handles each token's own
        // lemmas and inflections (went -> go inside a phrase).
        let mut breakdown = Vec::new();
        for token in &tokens {
            let hits = self.search_dict(token, limit)?;
            if !hits.is_empty() {
                breakdown.push((token.to_string(), hits));
            }
        }
        if breakdown.is_empty() {
            Ok(PhraseLookup::Empty)
        } else {
            Ok(PhraseLookup::Breakdown(breakdown))
        }
    }

    /// Exact headword match only (`key = fold_key(clean)`), no prefix
    /// fallback. Used by phrase window matching, where a contained
    /// sub-phrase must be a real headword rather than a prefix. Reads the
    /// merged store, so each word comes back once with all its senses.
    fn search_dict_exact(&self, clean: &str, limit: i64) -> Result<Vec<DictEntry>> {
        let key = fold_key(clean);
        if key.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT rowid, dict_id, word, senses FROM combined_words
             WHERE key = ?1 COLLATE NOCASE
             ORDER BY word ASC LIMIT ?2",
        )?;
        let mut out = Vec::new();
        for r in stmt.query_map(params![key.as_str(), limit], combined_row_to_entry)? {
            out.push(r?);
        }
        Ok(out)
    }

    /// Exact headword hit, then prefix hit, both against the merged store's
    /// `key` column. Exact results always come first; prefix results follow
    /// key order.
    fn search_dict_exact_or_prefix(&self, clean: &str, limit: i64) -> Result<Vec<DictEntry>> {
        let out = self.search_dict_exact(clean, limit)?;
        if !out.is_empty() {
            return Ok(out);
        }

        let key = fold_key(clean);
        if key.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.conn();
        let like = format!("{}%", escape_like(&key));
        let mut out = Vec::new();
        let mut stmt2 = conn.prepare_cached(
            "SELECT rowid, dict_id, word, senses FROM combined_words
             WHERE key LIKE ?1 ESCAPE '\\' COLLATE NOCASE
             ORDER BY key COLLATE NOCASE ASC, word ASC LIMIT ?2",
        )?;
        for r in stmt2.query_map(params![like.as_str(), limit], combined_row_to_entry)? {
            out.push(r?);
        }
        Ok(out)
    }

    fn search_dict_substring(&self, clean: &str, limit: i64) -> Result<Vec<DictEntry>> {
        let conn = self.conn();
        let like = format!("%{}%", escape_like(clean));
        let mut stmt = conn.prepare_cached(
            "SELECT rowid, dict_id, word, senses FROM combined_words
             WHERE key LIKE ?1 ESCAPE '\\' COLLATE NOCASE
                OR senses LIKE ?1 ESCAPE '\\'
             ORDER BY LENGTH(key) ASC, key COLLATE NOCASE ASC LIMIT ?2",
        )?;
        let mut out = Vec::new();
        for r in stmt.query_map(params![like, limit], combined_row_to_entry)? {
            out.push(r?);
        }
        Ok(out)
    }

    #[allow(dead_code)]
    pub fn dict_entry_count(&self) -> Result<i64> {
        let conn = self.conn();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM dict_entries", [], |r| r.get(0))?;
        Ok(n)
    }

    /// The full popup entry for a lookup: senses from the merged store
    /// (parsed into numbered senses with POS), the word-level POS list,
    /// synonym/antonym chips and idiom cards from the auxiliary packs, and
    /// suggestions when there is no headword (phrase breakdown tokens, or
    /// did-you-mean prefix matches).
    pub fn lookup_entry(&self, query: &str) -> Result<EntryData> {
        let clean = query.trim();
        let mut entry = EntryData {
            word: clean.to_string(),
            ..Default::default()
        };
        if clean.is_empty() {
            return Ok(entry);
        }

        // (a) Exact headword via the query variants.
        for variant in dictionary_query_variants(clean) {
            if let Some(raw) = self.combined_raw(&fold_key(&variant))? {
                entry.word = raw.0;
                entry.senses = parse_senses(&raw.1);
                entry.pos = distinct_pos(&entry.senses);
                break;
            }
        }

        // (b) Longest contained multi-word headword (phrase selection).
        if entry.senses.is_empty() {
            let tokens: Vec<&str> = clean.split_whitespace().collect();
            if tokens.len() > 1 {
                'outer: for window_len in (2..tokens.len()).rev() {
                    for window in tokens.windows(window_len) {
                        let candidate = window.join(" ");
                        if let Some(raw) = self.combined_raw(&fold_key(&candidate))? {
                            entry.word = raw.0;
                            entry.senses = parse_senses(&raw.1);
                            entry.pos = distinct_pos(&entry.senses);
                            break 'outer;
                        }
                    }
                }
            }
        }

        // (c) No headword: suggestions.
        if entry.senses.is_empty() {
            let tokens: Vec<&str> = clean.split_whitespace().collect();
            entry.suggestions = if tokens.len() > 1 {
                tokens.iter().map(|t| t.to_string()).collect()
            } else {
                self.did_you_mean(clean, 8)?
            };
            return Ok(entry);
        }

        // The auxiliary packs always enrich the winning entry.
        entry.synonyms = self.pack_list(BUNDLED_SYNONYMS_NAME, &entry.word, "synonyms:")?;
        entry.antonyms = self.pack_list(BUNDLED_ANTONYMS_NAME, &entry.word, "antonyms:")?;
        entry.idioms = self.idioms_for(&entry.word)?;
        Ok(entry)
    }

    /// Raw merged-store row for a key: (word, decoded senses).
    fn combined_raw(&self, key: &str) -> Result<Option<(String, Vec<String>)>> {
        if key.is_empty() {
            return Ok(None);
        }
        let conn = self.conn();
        let row = conn
            .query_row(
                "SELECT word, senses FROM combined_words
                 WHERE key = ?1 COLLATE NOCASE",
                params![key],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            )
            .optional()?;
        let Some((word, json)) = row else {
            return Ok(None);
        };
        let senses: Vec<String> = serde_json::from_str(&json).unwrap_or_default();
        Ok(Some((word, senses)))
    }

    /// The parsed chip list from an auxiliary pack row ("synonyms: a, b, c").
    fn pack_list(&self, pack_name: &str, word: &str, prefix: &str) -> Result<Vec<String>> {
        let key = fold_key(word);
        if key.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.conn();
        let row = conn
            .query_row(
                "SELECT e.definition FROM dict_entries e
                 JOIN dictionaries d ON d.id = e.dict_id
                 WHERE d.name = ?1 AND e.key = ?2 COLLATE NOCASE
                 LIMIT 1",
                params![pack_name, key],
                |r| r.get::<_, String>(0),
            )
            .optional()?;
        let Some(def) = row else {
            return Ok(Vec::new());
        };
        let Some(rest) = def.trim().strip_prefix(prefix) else {
            return Ok(Vec::new());
        };
        Ok(rest
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect())
    }

    /// Idiom cards: phrases from the idioms pack that contain the word as a
    /// token (or equal it), so "melancholy" surfaces "in a melancholy mood".
    fn idioms_for(&self, word: &str) -> Result<Vec<(String, String)>> {
        let key = fold_key(word);
        if key.is_empty() {
            return Ok(Vec::new());
        }
        let any = format!("% {} %", escape_like(word));
        let start = format!("{} %", escape_like(word));
        let end = format!("% {}", escape_like(word));
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT e.word, e.definition FROM dict_entries e
             JOIN dictionaries d ON d.id = e.dict_id
             WHERE d.name = ?1 AND (
                   e.key = ?2 COLLATE NOCASE
                OR e.word LIKE ?3 ESCAPE '\\' COLLATE NOCASE
                OR e.word LIKE ?4 ESCAPE '\\' COLLATE NOCASE
                OR e.word LIKE ?5 ESCAPE '\\' COLLATE NOCASE)
             ORDER BY LENGTH(e.word) ASC LIMIT 8",
        )?;
        let mut out = Vec::new();
        for r in stmt.query_map(
            params![
                BUNDLED_IDIOMS_NAME,
                key,
                any.as_str(),
                start.as_str(),
                end.as_str()
            ],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )? {
            out.push(r?);
        }
        Ok(out)
    }

    /// Prefix matches from the merged store for "did you mean" chips.
    fn did_you_mean(&self, word: &str, limit: usize) -> Result<Vec<String>> {
        let key = fold_key(word);
        if key.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.conn();
        let like = format!("{}%", escape_like(&key));
        let mut stmt = conn.prepare_cached(
            "SELECT word FROM combined_words
             WHERE key LIKE ?1 ESCAPE '\\' COLLATE NOCASE
             ORDER BY LENGTH(key) ASC, key COLLATE NOCASE ASC LIMIT ?2",
        )?;
        let mut out: Vec<String> = Vec::new();
        for r in stmt.query_map(params![like.as_str(), limit as i64], |r| {
            r.get::<_, String>(0)
        })? {
            let w: String = r?;
            let mut seen = false;
            for entry in &out {
                if entry.eq_ignore_ascii_case(w.as_str()) {
                    seen = true;
                    break;
                }
            }
            if !seen {
                out.push(w);
            }
        }
        Ok(out)
    }

    /// Whether the word was already saved for this book (popup bookmark).
    pub fn saved_word_exists(&self, word: &str, book_id: i64) -> Result<bool> {
        let conn = self.conn();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM saved_words
             WHERE word = ?1 COLLATE NOCASE AND book_id = ?2",
            params![word, book_id],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

}

/// A combined_words row -> the entry the reader shows: the word once, its
/// senses numbered in the definition text (a single sense is shown plain,
/// without a number).
fn combined_row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<DictEntry> {
    let senses_json: String = row.get(3)?;
    Ok(DictEntry {
        id: row.get(0)?,
        dict_id: row.get(1)?,
        word: row.get(2)?,
        definition: format_senses(&senses_json),
    })
}

/// Turn a stored senses JSON array into the display definition: numbered
/// lines for multiple senses, the plain text for a single sense.
fn format_senses(senses_json: &str) -> String {
    let senses: Vec<String> = serde_json::from_str(senses_json).unwrap_or_default();
    match senses.len() {
        0 => String::new(),
        1 => senses[0].clone(),
        _ => senses
            .iter()
            .enumerate()
            .map(|(i, s)| format!("{}. {}", i + 1, s))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// Parse the stored sense strings into numbered senses with POS.
///
/// The bundled WordNet pack stores one row per word whose definition is a
/// structured blob: "noun:; 1. def; 2. def; verb:; 1. def". Other packs
/// store one plain definition per row. A plain row becomes a single sense
/// without POS. Senses are renumbered sequentially across the whole entry
/// (the blob's internal per-POS numbers are parse scaffolding only).
fn parse_senses(raw_senses: &[String]) -> Vec<Sense> {
    let mut out = Vec::new();
    for blob in raw_senses {
        let parsed = parse_pos_blob(blob);
        if parsed.is_empty() {
            out.push(Sense {
                number: out.len() + 1,
                pos: None,
                def: blob.clone(),
                example: None,
            });
        } else {
            for mut sense in parsed {
                sense.number = out.len() + 1;
                out.push(sense);
            }
        }
    }
    out
}

/// Split a structured WordNet blob ("noun:; 1. ...; 2. ...; verb:; 1. ...")
/// into senses. Splitting on "; " consumes the semicolon, so a POS marker
/// arrives as its own "noun:" segment; detection requires the segment to be
/// exactly the POS name plus ':', which keeps ordinary gloss words
/// ("control:;", "vehicle:;") from being mistaken for groups. Segments after
/// a numbered sense that do not start with a number are continuations of
/// that sense (the glosses run on with "; "). A trailing quoted phrase is
/// split off as the example line.
fn parse_pos_blob(blob: &str) -> Vec<Sense> {
    const POS_MARKERS: [&str; 4] = ["noun", "adjective", "verb", "adverb"];
    let mut senses: Vec<Sense> = Vec::new();
    let mut current_pos: Option<String> = None;

    for segment in blob.split("; ") {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        let mut pos = current_pos.clone();
        let mut text = segment;
        for marker in POS_MARKERS {
            if let Some(rest) = segment.strip_prefix(marker) {
                // Only an exact "noun:"-style segment is a group marker; a
                // plain definition that merely starts with "noun" (for
                // example "noun: a person, place or thing" from another
                // pack) must stay ordinary text.
                let rest = rest.trim_start();
                if rest == ":" || rest.is_empty() {
                    pos = Some(marker.to_string());
                    text = rest.strip_prefix(':').unwrap_or("").trim();
                    break;
                }
            }
        }
        current_pos = pos.clone();
        if text.is_empty() {
            continue;
        }

        if let Some(rest) = strip_sense_number(text) {
            senses.push(Sense {
                number: 0,
                pos,
                def: rest.to_string(),
                example: None,
            });
        } else if let Some(last) = senses.last_mut() {
            last.def.push_str("; ");
            last.def.push_str(text);
        } else if !text.is_empty() {
            senses.push(Sense {
                number: 0,
                pos,
                def: text.to_string(),
                example: None,
            });
        }
    }

    for sense in &mut senses {
        if let Some((def, example)) = split_example(&sense.def) {
            sense.def = def;
            sense.example = Some(example);
        }
    }
    senses
}

/// Strip a leading "N." sense number. Returns the text after it.
fn strip_sense_number(text: &str) -> Option<&str> {
    let t = text.trim_start();
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 {
        return None;
    }
    let rest = t[digits..].trim_start().strip_prefix('.')?.trim_start();
    if rest.is_empty() {
        None
    } else {
        Some(rest)
    }
}

/// If a definition ends with a quoted phrase ("..." or '...'), split it off
/// as the example line. The closing quote is the last character; the
/// matching opening quote is its previous occurrence, so the example is the
/// text between the pair (nested quotes of the other kind survive).
fn split_example(def: &str) -> Option<(String, String)> {
    let def = def.trim();
    for q in ['"', '\''] {
        if !def.ends_with(q) {
            continue;
        }
        let Some(close) = def.rfind(q) else { continue };
        if close == 0 {
            continue;
        }
        let Some(open) = def[..close].rfind(q) else {
            continue;
        };
        // The opening quote must follow whitespace or opening punctuation —
        // an apostrophe inside a word ("it's") is not an opening quote.
        // Check the character immediately before the quote (untrimmed), so
        // "word \"phrase\"" still splits.
        if let Some(prev) = def[..open].chars().next_back() {
            if !prev.is_whitespace() && !matches!(prev, '(' | '[' | ':' | ';' | ',') {
                continue;
            }
        }
        let example = def[open + q.len_utf8()..close].trim().to_string();
        let head = def[..open]
            .trim()
            .trim_end_matches([';', ':', ','])
            .trim()
            .to_string();
        if !head.is_empty() && !example.is_empty() {
            return Some((head, example));
        }
    }
    None
}

/// Distinct POS values in first-seen order — the header pill ("noun · verb").
fn distinct_pos(senses: &[Sense]) -> Vec<String> {
    let mut out = Vec::new();
    for s in senses {
        if let Some(p) = &s.pos {
            if !out.iter().any(|x| x == p) {
                out.push(p.clone());
            }
        }
    }
    out
}

pub(crate) fn dictionary_query_variants(term: &str) -> Vec<String> {
    let trimmed = term.trim();
    let normalized = normalize_dictionary_term(trimmed);
    let primary = if normalized.is_empty() {
        trimmed.to_string()
    } else {
        normalized.clone()
    };
    let mut variants = Vec::new();
    push_dictionary_variant(&mut variants, trimmed.to_string());
    push_dictionary_variant(&mut variants, primary.clone());

    let mut inflection_source = primary;
    if let Some(base) = dictionary_possessive_base(&inflection_source) {
        push_dictionary_variant(&mut variants, base.to_string());
        inflection_source = base.to_string();
    }

    if inflection_source.split_whitespace().count() == 1
        && inflection_source.chars().all(|ch| ch.is_alphabetic())
    {
        // Irregulars come from the WordNet exception lists first; the
        // suffix rules below are the fallback for forms the lists do not
        // cover. Both go through `push_dictionary_variant` so dedup stays
        // case-insensitive.
        let surface = inflection_source.to_lowercase();
        if let Some(lemmas) = wordnet_exceptions().get(&surface) {
            for lemma in lemmas {
                push_dictionary_variant(&mut variants, lemma.clone());
            }
        } else {
            for variant in simple_inflection_variants(&inflection_source) {
                push_dictionary_variant(&mut variants, variant);
            }
        }
    }
    variants
}

fn normalize_dictionary_term(term: &str) -> String {
    term.split_whitespace()
        .map(|token| token.trim_matches(|ch: char| !ch.is_alphanumeric()))
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn dictionary_possessive_base(term: &str) -> Option<&str> {
    term.strip_suffix("'s")
        .or_else(|| term.strip_suffix("’s"))
        .or_else(|| term.strip_suffix('\''))
        .or_else(|| term.strip_suffix('’'))
}

// ---------------------------------------------------------------------------
// P5.5: Lesk-style likely-sense ranking
// ---------------------------------------------------------------------------

/// Stopwords ignored when scoring gloss overlap against the context sentence.
/// Small fixed list (the brief: "ship a tiny stopword list"); closed-class
/// English words plus a few very frequent verbs. Never include sense/gloss
/// content words — only function words.
const STOPWORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "nor", "if", "so", "yet", "for", "of", "to", "in", "on",
    "at", "by", "with", "from", "up", "down", "into", "out", "over", "under", "as", "than", "that",
    "this", "these", "those", "which", "who", "whom", "whose", "what", "when", "where", "why",
    "how", "is", "are", "was", "were", "be", "been", "being", "am", "do", "does", "did", "have",
    "has", "had", "will", "would", "can", "could", "shall", "should", "may", "might", "must",
    "not", "no", "yes", "i", "you", "he", "she", "it", "we", "they", "me", "him", "her", "us",
    "them", "my", "your", "his", "its", "our", "their", "there", "here", "then", "now", "just",
    "very", "too", "also", "only", "such", "same", "some", "any", "all", "both", "each", "few",
    "more", "most", "other", "another", "one", "two",
];

/// Score a context sentence against a sense's gloss, using Lesk-style set
/// overlap on content words. Returns the number of shared content tokens;
/// a token counts once no matter how often it appears on either side.
fn gloss_overlap(context_tokens: &[String], gloss_tokens: &[String]) -> usize {
    context_tokens
        .iter()
        .filter(|t| gloss_tokens.contains(t))
        .count()
}

/// Which sense index is most likely given the sentence around the lookup.
///
/// Simplified Lesk (the brief's algorithm): lowercase + tokenize the context
/// sentence into a set; for each sense, build a bag of words from its gloss
/// and examples; score by overlap, ignoring stopwords; return the top-scoring
/// sense index — or `None` when every sense scores zero (no evidence, no
/// hint) or when two or more senses tie for the top score (no hint rather
/// than an arbitrary one).
///
/// Two refinements keep the hint honest on real WordNet data:
///
/// - `headword` (the canonical looked-up word) is excluded from the context
///   bag. It is in the sentence by definition and appears in many of the
///   word's own glosses ("put into a bank account"), so counting it would
///   manufacture ties and drown out real evidence such as "deposit".
/// - Tokens are lightly stemmed on both sides so inflected sentence words
///   match the gloss's base forms ("deposits" ≈ "deposit", "running" ≈
///   "run"). Both sides go through the same stemmer, so it can be lossy.
///
/// The hint never hides or misrepresents the entry: a wrong guess costs at
/// most a misplaced highlight.
pub fn likely_sense_index(
    context_sentence: &str,
    headword: &str,
    senses: &[Sense],
) -> Option<usize> {
    if context_sentence.trim().is_empty() {
        return None;
    }
    let context_tokens = tokenize_context(context_sentence);
    let headword_stem = stem_token(&headword.to_ascii_lowercase());
    let context_tokens: Vec<String> = context_tokens
        .into_iter()
        .filter(|t| *t != headword_stem)
        .collect();
    if context_tokens.is_empty() {
        return None;
    }
    let mut best_score = 0usize;
    let mut best_index: Option<usize> = None;
    let mut tied = false;
    for (i, sense) in senses.iter().enumerate() {
        let gloss_tokens = tokenize_context(&sense.def);
        let mut score = gloss_overlap(&context_tokens, &gloss_tokens);
        if let Some(example) = &sense.example {
            let example_tokens = tokenize_context(example);
            score += gloss_overlap(&context_tokens, &example_tokens);
        }
        if score > best_score {
            best_score = score;
            best_index = Some(i);
            tied = false;
        } else if score == best_score && score > 0 {
            tied = true;
        }
    }
    if tied {
        None
    } else {
        best_index
    }
}

/// Lowercase a sentence, split on non-alphanumeric runs, drop stopwords and
/// one-character tokens, and stem each surviving token. Returns owned tokens
/// so case is normalized on both sides of the overlap check.
fn tokenize_context(sentence: &str) -> Vec<String> {
    sentence
        .split(|ch: char| !ch.is_alphanumeric())
        .map(|t| t.to_ascii_lowercase())
        .filter(|t| t.len() > 1)
        .filter(|t| !STOPWORDS.contains(&t.as_str()))
        .map(|t| stem_token(&t))
        .collect()
}

/// Tiny rule-based stemmer for the Lesk overlap: strips common English
/// suffixes so inflected words match base forms. Deliberately lossy — both
/// sides of the comparison go through it, so consistent "wrong" stems still
/// match each other. Suffixes are only stripped when a meaningful stem
/// remains (3+ letters), and the "ing"/"ed"/"ly" rules require a vowel in
/// the remainder so roots like "bring" and "need" survive untouched.
// Several suffix rules share an identical body ("ies"/"ied" -> y, "ed"/"ly"
// -> truncate 2). clippy::if_same_then_else wants them merged, but each arm is
// a distinct linguistic rule that will diverge as the stemmer is refined, and
// collapsing them into one condition makes the rule set unreadable.
#[allow(clippy::if_same_then_else)]
fn stem_token(token: &str) -> String {
    let mut t = token;
    for suffix in ["'s", "’s"] {
        if let Some(base) = t.strip_suffix(suffix) {
            t = base;
            break;
        }
    }
    let mut out = t.to_string();
    let mut stripped = false;
    let n = out.len();
    if n >= 4 {
        if out.ends_with("ies") {
            out.truncate(n - 3);
            out.push('y');
            stripped = true;
        } else if out.ends_with("ied") {
            out.truncate(n - 3);
            out.push('y');
            stripped = true;
        } else if out.ends_with("ying") && n >= 6 {
            out.truncate(n - 3);
            stripped = true;
        } else if out.ends_with("es")
            && n >= 5
            && out[..n - 2].len() >= 3
            && !out[..n - 2].ends_with("us")
        {
            out.truncate(n - 2);
            stripped = true;
        } else if out.ends_with('s')
            && n >= 4
            && out[..n - 1].len() >= 3
            && !out.ends_with("ss")
            && !out.ends_with("us")
            && !out.ends_with("is")
        {
            out.truncate(n - 1);
            stripped = true;
        } else if out.ends_with("ing")
            && n >= 5
            && out[..n - 3].len() >= 3
            && has_vowel(&out[..n - 3])
        {
            out.truncate(n - 3);
            stripped = true;
        } else if out.ends_with("ed")
            && n >= 4
            && out[..n - 2].len() >= 3
            && has_vowel(&out[..n - 2])
        {
            out.truncate(n - 2);
            stripped = true;
        } else if out.ends_with("ly")
            && n >= 5
            && out[..n - 2].len() >= 3
            && has_vowel(&out[..n - 2])
        {
            out.truncate(n - 2);
            stripped = true;
        } else if out.ends_with('e') && n >= 4 && out[..n - 1].len() >= 3 && !out.ends_with("ee") {
            out.truncate(n - 1);
            stripped = true;
        }
    }
    // Suffix stripping can leave a doubled consonant ("running" -> "runn").
    if stripped && out.len() >= 3 && !out.ends_with("ss") {
        let len = out.len();
        let bytes = out.as_bytes();
        if bytes[len - 1] == bytes[len - 2] {
            out.truncate(len - 1);
        }
    }
    out
}

fn has_vowel(s: &str) -> bool {
    s.chars().any(|c| matches!(c, 'a' | 'e' | 'i' | 'o' | 'u'))
}

fn simple_inflection_variants(term: &str) -> Vec<String> {
    let mut variants = Vec::new();
    if let Some(stem) = term.strip_suffix("ies") {
        if stem.len() >= 2 {
            variants.push(format!("{stem}y"));
        }
    }
    if let Some(stem) = term.strip_suffix("ied") {
        if stem.len() >= 2 {
            variants.push(format!("{stem}y"));
        }
    }
    if let Some(stem) = term.strip_suffix("es") {
        if stem.len() >= 2 {
            variants.push(stem.to_string());
        }
    }
    if let Some(stem) = term.strip_suffix('s') {
        if stem.len() >= 3
            && !term.ends_with("ss")
            && !term.ends_with("us")
            && !term.ends_with("is")
        {
            variants.push(stem.to_string());
        }
    }
    if let Some(stem) = term.strip_suffix("ing") {
        if stem.len() >= 3 {
            add_simple_verb_stems(&mut variants, stem);
        }
    }
    if let Some(stem) = term.strip_suffix("ed") {
        if stem.len() >= 3 {
            add_simple_verb_stems(&mut variants, stem);
        }
    }
    variants
}

fn add_simple_verb_stems(variants: &mut Vec<String>, stem: &str) {
    let undoubled = undouble_final_letter(stem);
    variants.push(undoubled.clone());
    if let Some(last) = undoubled.chars().last() {
        if "bcdfghjklmnpqrstvwxyz".contains(last.to_ascii_lowercase()) {
            variants.push(format!("{undoubled}e"));
        }
    }
}

fn undouble_final_letter(term: &str) -> String {
    let mut chars = term.chars().collect::<Vec<_>>();
    if chars.len() >= 2 && chars[chars.len() - 1] == chars[chars.len() - 2] {
        chars.pop();
    }
    chars.into_iter().collect()
}

fn push_dictionary_variant(variants: &mut Vec<String>, candidate: String) {
    if !candidate.is_empty()
        && !variants
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&candidate))
    {
        variants.push(candidate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_key_normalizes_case_whitespace_and_diacritics() {
        assert_eq!(fold_key("Run"), "run");
        assert_eq!(fold_key("RUN"), "run");
        assert_eq!(fold_key("rún"), "run");
        assert_eq!(fold_key("ÉTÉ"), "ete");
        assert_eq!(fold_key("  Hello,   World!!  "), "hello world");
        assert_eq!(fold_key(""), "");
        assert_eq!(fold_key("!!! ..."), "");
    }

    #[test]
    fn inserted_entries_carry_folded_keys() {
        let cat = Catalog::open_in_memory().unwrap();
        let dict_id = cat.insert_dictionary("Test pack", Some("en"), 2).unwrap();
        cat.batch_insert_dict_entries(
            dict_id,
            &[
                ("Rún".to_string(), "an Irish hero".to_string()),
                ("Run".to_string(), "to move fast".to_string()),
            ],
        )
        .unwrap();
        let conn = cat.conn();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM dict_entries WHERE key = 'run'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn search_dict_folds_case_and_diacritics_into_keys() {
        let cat = Catalog::open_in_memory().unwrap();
        let dict_id = cat.insert_dictionary("Test pack", Some("en"), 2).unwrap();
        cat.batch_insert_dict_entries(
            dict_id,
            &[
                ("run".to_string(), "to move fast".to_string()),
                (
                    "Rúnestone".to_string(),
                    "a stone carved with runes".to_string(),
                ),
            ],
        )
        .unwrap();
        cat.rebuild_combined_dictionary().unwrap();

        // Exact hit regardless of case.
        let hits = cat.search_dict("Run", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].word, "run");

        // Diacritics fold away.
        let hits = cat.search_dict("rún", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].word, "run");

        // Prefix fallback through the key column.
        let hits = cat.search_dict("Rune", 10).unwrap();
        assert!(hits.iter().any(|hit| hit.word == "Rúnestone"));
    }

    #[test]
    fn exact_key_lookup_uses_the_key_index() {
        let cat = Catalog::open_in_memory().unwrap();
        let dict_id = cat.insert_dictionary("Test pack", Some("en"), 1).unwrap();
        cat.batch_insert_dict_entries(dict_id, &[("run".to_string(), "fast".to_string())])
            .unwrap();
        let conn = cat.conn();
        let plan: String = conn
            .query_row(
                "EXPLAIN QUERY PLAN
                 SELECT id, dict_id, word, definition FROM dict_entries
                 WHERE key = ?1 COLLATE NOCASE",
                params!["run"],
                |r| r.get(3),
            )
            .unwrap();
        assert!(
            plan.contains("idx_dict_entries_key"),
            "expected the key index in the query plan, got: {plan}"
        );
    }

    #[test]
    fn dictionary_variants_resolve_irregulars_from_wordnet_exceptions() {
        let went = dictionary_query_variants("went");
        assert!(went.iter().any(|variant| variant == "go"));

        let mice = dictionary_query_variants("mice");
        assert!(mice.iter().any(|variant| variant == "mouse"));

        // adj.exc lists two lemmas for `better`; both must surface.
        let better = dictionary_query_variants("better");
        assert!(better.iter().any(|variant| variant == "good"));
        assert!(better.iter().any(|variant| variant == "well"));

        // Possessive base resolves through the noun exceptions.
        let childrens = dictionary_query_variants("children’s");
        assert!(childrens.iter().any(|variant| variant == "child"));

        // Capitalized surface still resolves (case-insensitive map lookup).
        let went_cap = dictionary_query_variants("Went");
        assert!(went_cap.iter().any(|variant| variant == "go"));
    }

    #[test]
    fn dictionary_variants_keep_suffix_rules_as_fallback() {
        // Forms the exception lists do not cover still go through the suffix
        // rules (e.g. walked → walk).
        let walked = dictionary_query_variants("walked");
        assert!(walked.iter().any(|variant| variant == "walk"));

        // The suffix rules must not add junk on top of an irregular hit:
        // `better` resolves to its WordNet lemmas, not a guessed stem.
        let better = dictionary_query_variants("better");
        assert!(!better.iter().any(|variant| variant == "bett"));
    }

    #[test]
    fn search_dict_resolves_irregulars_to_headwords() {
        let cat = Catalog::open_in_memory().unwrap();
        let dict_id = cat.insert_dictionary("Test pack", Some("en"), 4).unwrap();
        cat.batch_insert_dict_entries(
            dict_id,
            &[
                (
                    "go".to_string(),
                    "to move from one place to another".to_string(),
                ),
                ("mouse".to_string(), "a small rodent".to_string()),
                ("good".to_string(), "having desirable qualities".to_string()),
                ("run".to_string(), "to move fast".to_string()),
            ],
        )
        .unwrap();
        cat.rebuild_combined_dictionary().unwrap();

        let hits = cat.search_dict("went", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].word, "go");

        let hits = cat.search_dict("mice", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].word, "mouse");

        let hits = cat.search_dict("better", 10).unwrap();
        assert!(hits.iter().any(|hit| hit.word == "good"));

        // Regular inflection still resolves through the suffix fallback.
        let hits = cat.search_dict("running", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].word, "run");
    }

    #[test]
    fn search_phrase_returns_the_full_phrase_headword() {
        let cat = Catalog::open_in_memory().unwrap();
        let dict_id = cat.insert_dictionary("Idioms", Some("en"), 3).unwrap();
        cat.batch_insert_dict_entries(
            dict_id,
            &[
                (
                    "odd mixture".to_string(),
                    "a strange combination".to_string(),
                ),
                ("odd".to_string(), "strange".to_string()),
                ("mixture".to_string(), "a blend".to_string()),
            ],
        )
        .unwrap();
        cat.rebuild_combined_dictionary().unwrap();

        match cat.search_phrase("odd mixture", 5).unwrap() {
            PhraseLookup::Phrase(hits) => {
                assert_eq!(hits.len(), 1);
                assert_eq!(hits[0].word, "odd mixture");
            }
            other => panic!("expected Phrase, got {other:?}"),
        }
    }

    #[test]
    fn search_phrase_finds_a_contained_phrase_headword() {
        // A longer selection still resolves to the contained headword.
        let cat = Catalog::open_in_memory().unwrap();
        let dict_id = cat.insert_dictionary("Idioms", Some("en"), 5).unwrap();
        cat.batch_insert_dict_entries(
            dict_id,
            &[
                ("run out of steam".to_string(), "lose energy".to_string()),
                ("run".to_string(), "to move fast".to_string()),
                ("out".to_string(), "away".to_string()),
                ("of".to_string(), "belonging to".to_string()),
                ("steam".to_string(), "water vapour".to_string()),
            ],
        )
        .unwrap();
        cat.rebuild_combined_dictionary().unwrap();

        match cat.search_phrase("run out of steam today", 5).unwrap() {
            PhraseLookup::Phrase(hits) => {
                assert_eq!(hits[0].word, "run out of steam");
            }
            other => panic!("expected Phrase, got {other:?}"),
        }
    }

    #[test]
    fn search_phrase_breaks_down_when_no_phrase_headword() {
        let cat = Catalog::open_in_memory().unwrap();
        let dict_id = cat.insert_dictionary("WordNet", Some("en"), 2).unwrap();
        cat.batch_insert_dict_entries(
            dict_id,
            &[
                ("odd".to_string(), "not divisible by two".to_string()),
                ("mixture".to_string(), "a blend".to_string()),
            ],
        )
        .unwrap();
        cat.rebuild_combined_dictionary().unwrap();

        match cat.search_phrase("odd mixture", 5).unwrap() {
            PhraseLookup::Breakdown(parts) => {
                let tokens: Vec<&str> = parts.iter().map(|(t, _)| t.as_str()).collect();
                assert_eq!(tokens, vec!["odd", "mixture"]);
                assert!(parts.iter().all(|(_, hits)| !hits.is_empty()));
            }
            other => panic!("expected Breakdown, got {other:?}"),
        }
    }

    #[test]
    fn search_phrase_omits_tokens_without_hits() {
        let cat = Catalog::open_in_memory().unwrap();
        let dict_id = cat.insert_dictionary("WordNet", Some("en"), 1).unwrap();
        cat.batch_insert_dict_entries(dict_id, &[("odd".to_string(), "strange".to_string())])
            .unwrap();
        cat.rebuild_combined_dictionary().unwrap();

        match cat.search_phrase("odd mixture", 5).unwrap() {
            PhraseLookup::Breakdown(parts) => {
                assert_eq!(parts.len(), 1);
                assert_eq!(parts[0].0, "odd");
            }
            other => panic!("expected Breakdown, got {other:?}"),
        }
    }

    #[test]
    fn search_phrase_handles_irregular_tokens_in_the_breakdown() {
        let cat = Catalog::open_in_memory().unwrap();
        let dict_id = cat.insert_dictionary("WordNet", Some("en"), 1).unwrap();
        cat.batch_insert_dict_entries(dict_id, &[("go".to_string(), "to move".to_string())])
            .unwrap();
        cat.rebuild_combined_dictionary().unwrap();

        match cat.search_phrase("went home", 5).unwrap() {
            PhraseLookup::Breakdown(parts) => {
                assert_eq!(parts.len(), 1);
                assert_eq!(parts[0].0, "went");
                assert_eq!(parts[0].1[0].word, "go");
            }
            other => panic!("expected Breakdown, got {other:?}"),
        }
    }

    #[test]
    fn search_phrase_empty_when_nothing_matches() {
        let cat = Catalog::open_in_memory().unwrap();
        let dict_id = cat.insert_dictionary("WordNet", Some("en"), 1).unwrap();
        cat.batch_insert_dict_entries(dict_id, &[("go".to_string(), "to move".to_string())])
            .unwrap();
        cat.rebuild_combined_dictionary().unwrap();

        assert!(matches!(
            cat.search_phrase("zzzqqq nothing", 5).unwrap(),
            PhraseLookup::Empty
        ));
    }

    // -----------------------------------------------------------------------
    // Merged dictionary store (v12)
    // -----------------------------------------------------------------------

    fn seed_dict(cat: &Catalog, name: &str, priority: i64, entries: &[(&str, &str)]) -> i64 {
        let id = cat
            .insert_dictionary(name, Some("en"), entries.len() as i64)
            .unwrap();
        cat.set_dictionary_priority(id, priority).unwrap();
        cat.batch_insert_dict_entries(
            id,
            &entries
                .iter()
                .map(|(w, d)| (w.to_string(), d.to_string()))
                .collect::<Vec<_>>(),
        )
        .unwrap();
        id
    }

    #[test]
    fn combined_store_shows_only_the_highest_priority_dictionary() {
        let cat = Catalog::open_in_memory().unwrap();
        seed_dict(
            &cat,
            "WordNet",
            10,
            &[
                ("set", "to put something in place"),
                ("set", "a group of things"),
            ],
        );
        seed_dict(
            &cat,
            "My Dictionary",
            100,
            &[
                ("set", "my own meaning one"),
                ("set", "my own meaning two"),
                ("set", "my own meaning three"),
            ],
        );
        cat.rebuild_combined_dictionary().unwrap();

        let hits = cat.search_dict("set", 10).unwrap();
        assert_eq!(hits.len(), 1, "one word must appear once");
        assert_eq!(hits[0].word, "set");
        let def = &hits[0].definition;
        assert!(def.contains("to put something in place"));
        assert!(def.contains("a group of things"));
        assert!(
            !def.contains("my own meaning"),
            "lower-priority dictionary must stay hidden: {def}"
        );
        // Both senses of the winner are listed, numbered.
        assert!(def.contains("1. to put something in place"));
        assert!(def.contains("2. a group of things"));
    }

    #[test]
    fn combined_store_falls_back_to_the_next_dictionary() {
        let cat = Catalog::open_in_memory().unwrap();
        seed_dict(&cat, "WordNet", 10, &[("set", "a group of things")]);
        seed_dict(&cat, "My Dictionary", 100, &[("quokka", "a small wallaby")]);
        cat.rebuild_combined_dictionary().unwrap();

        // WordNet has no quokka; the imported dictionary speaks.
        let hits = cat.search_dict("quokka", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].definition, "a small wallaby");
    }

    #[test]
    fn combined_store_dedupes_identical_senses() {
        let cat = Catalog::open_in_memory().unwrap();
        seed_dict(
            &cat,
            "WordNet",
            10,
            &[
                ("set", "to put something in place"),
                ("set", "to put something in place"),
            ],
        );
        cat.rebuild_combined_dictionary().unwrap();

        let hits = cat.search_dict("set", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].definition, "to put something in place");
    }

    #[test]
    fn combined_store_rebuilds_after_delete() {
        let cat = Catalog::open_in_memory().unwrap();
        seed_dict(&cat, "WordNet", 10, &[("set", "a group of things")]);
        let other = seed_dict(&cat, "My Dictionary", 100, &[("quokka", "a small wallaby")]);
        cat.rebuild_combined_dictionary().unwrap();
        assert_eq!(cat.search_dict("quokka", 10).unwrap().len(), 1);

        // Removing the only dictionary for a word drops the word entirely.
        cat.delete_dictionary(other).unwrap();
        assert!(cat.search_dict("quokka", 10).unwrap().is_empty());
        assert_eq!(cat.search_dict("set", 10).unwrap().len(), 1);
    }

    #[test]
    fn combined_store_priority_ignores_sense_counts() {
        // The agreed rule: priority wins even when the lower-priority
        // dictionary has more senses for the word.
        let cat = Catalog::open_in_memory().unwrap();
        seed_dict(&cat, "WordNet", 10, &[("set", "one")]);
        seed_dict(
            &cat,
            "My Dictionary",
            100,
            &[
                ("set", "one"),
                ("set", "two"),
                ("set", "three"),
                ("set", "four"),
                ("set", "five"),
                ("set", "six"),
            ],
        );
        cat.rebuild_combined_dictionary().unwrap();

        let hits = cat.search_dict("set", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].definition, "one");
    }

    #[test]
    fn dictionary_variants_strip_outer_punctuation() {
        let variants = dictionary_query_variants("\u{201c}word,\u{201d}");
        assert!(variants.iter().any(|variant| variant == "word"));
    }

    #[test]
    fn dictionary_variants_handle_common_inflections() {
        let running = dictionary_query_variants("running");
        assert!(running.iter().any(|variant| variant == "run"));

        let studies = dictionary_query_variants("studies");
        assert!(studies.iter().any(|variant| variant == "study"));
    }

    #[test]
    fn dictionary_variants_handle_possessives() {
        let variants = dictionary_query_variants("children’s");
        assert!(variants.iter().any(|variant| variant == "children"));
    }

    // ── Phase 5: POS-blob and sense parsing ──

    #[test]
    fn parse_pos_blob_splits_pos_groups_and_numbers_senses() {
        let blob = "noun:; 1. a domestic animal kept as a companion; 2. a person; \
                    verb:; 1. to hunt with dogs";
        let senses = parse_pos_blob(blob);
        assert_eq!(senses.len(), 3);
        assert_eq!(senses[0].pos.as_deref(), Some("noun"));
        assert_eq!(senses[0].def, "a domestic animal kept as a companion");
        assert_eq!(senses[1].pos.as_deref(), Some("noun"));
        assert_eq!(senses[1].def, "a person");
        assert_eq!(senses[2].pos.as_deref(), Some("verb"));
        assert_eq!(senses[2].def, "to hunt with dogs");
        assert!(senses.iter().all(|s| s.example.is_none()));
    }

    #[test]
    fn parse_pos_blob_handles_adjective_and_adverb_groups() {
        let blob = "adjective:; 1. quick to notice; adverb:; 1. in a quick manner";
        let senses = parse_pos_blob(blob);
        assert_eq!(senses.len(), 2);
        assert_eq!(senses[0].pos.as_deref(), Some("adjective"));
        assert_eq!(senses[1].pos.as_deref(), Some("adverb"));
    }

    #[test]
    fn parse_pos_blob_ignores_fake_markers_and_keeps_continuations() {
        // "control:;" and "vehicle:;" end with ":;" but are not POS groups —
        // only the four full POS names split groups. Segments without a
        // sense number continue the previous definition.
        let blob = "noun:; 1. authority over something; control:; the power to direct; \
                    verb:; 1. to drive a vehicle:; a motorized conveyance";
        let senses = parse_pos_blob(blob);
        assert_eq!(senses.len(), 2);
        assert_eq!(senses[0].pos.as_deref(), Some("noun"));
        assert_eq!(
            senses[0].def,
            "authority over something; control:; the power to direct"
        );
        assert_eq!(senses[1].pos.as_deref(), Some("verb"));
        assert_eq!(senses[1].def, "to drive a vehicle:; a motorized conveyance");
    }

    #[test]
    fn parse_pos_blob_splits_trailing_quoted_example() {
        let blob = "noun:; 1. a remark that is insincere \"he spoke with indirect discourse\"";
        let senses = parse_pos_blob(blob);
        assert_eq!(senses.len(), 1);
        assert_eq!(senses[0].def, "a remark that is insincere");
        assert_eq!(
            senses[0].example.as_deref(),
            Some("he spoke with indirect discourse")
        );
    }

    #[test]
    fn split_example_ignores_stray_apostrophes() {
        // A trailing quote char that is not a real quote pair (the previous
        // quote is an apostrophe inside a word) must not split the gloss.
        assert_eq!(split_example("a thing it's '"), None);
        assert_eq!(split_example("the boys'"), None);
    }

    #[test]
    fn parse_pos_blob_keeps_mid_gloss_quotes_intact() {
        // The one double-quoted row in the bundled pack quotes inside the
        // gloss, not at the end — the definition must stay whole and must
        // not gain an example line.
        let blob = "noun:; 1. a report of a discourse in which deictic terms are \
                    modified appropriately (e.g., \u{2018}he said \"I am a fool\"\u{2019} \
                    would be modified to \u{2018}he said he is a fool\u{2019})";
        let senses = parse_pos_blob(blob);
        assert_eq!(senses.len(), 1);
        assert_eq!(senses[0].example, None);
        assert!(senses[0].def.contains("I am a fool"));
        assert!(senses[0].def.starts_with("a report of a discourse"));
    }

    #[test]
    fn parse_senses_keeps_plain_rows_as_single_senses() {
        let senses = parse_senses(&["a single plain definition".to_string()]);
        assert_eq!(senses.len(), 1);
        assert_eq!(senses[0].number, 1);
        assert_eq!(senses[0].pos, None);
        assert_eq!(senses[0].def, "a single plain definition");
    }

    #[test]
    fn parse_senses_renumbers_across_rows_and_pos_groups() {
        let senses = parse_senses(&[
            "noun:; 1. first; 2. second".to_string(),
            "plain row".to_string(),
        ]);
        let numbers: Vec<usize> = senses.iter().map(|s| s.number).collect();
        assert_eq!(numbers, vec![1, 2, 3]);
        assert_eq!(senses[1].def, "second");
        assert_eq!(senses[2].def, "plain row");
        assert_eq!(senses[2].pos, None);
    }

    #[test]
    fn distinct_pos_preserves_first_seen_order() {
        let senses = parse_senses(&["verb:; 1. v1; noun:; 1. n1; verb:; 2. v2".to_string()]);
        assert_eq!(distinct_pos(&senses), vec!["verb", "noun"]);
    }

    // -----------------------------------------------------------------------
    // P5.5: Lesk-style likely-sense ranking
    // -----------------------------------------------------------------------

    fn test_sense(def: &str, example: Option<&str>) -> Sense {
        Sense {
            number: 1,
            pos: None,
            def: def.to_string(),
            example: example.map(|s| s.to_string()),
        }
    }

    #[test]
    fn likely_sense_prefers_the_gloss_matching_the_sentence() {
        let senses = vec![
            test_sense("a financial institution that accepts deposits", None),
            test_sense("land alongside a river or lake", None),
        ];
        assert_eq!(
            likely_sense_index("We walked along the river bank at sunset.", "bank", &senses),
            Some(1)
        );
    }

    #[test]
    fn likely_sense_uses_example_overlap_too() {
        // The context shares no word with either gloss, but the first
        // sense's example contains "savings", so the example overlap wins.
        let senses = vec![
            test_sense(
                "a financial institution",
                Some("she keeps her savings at the bank"),
            ),
            test_sense("rising ground bordering a waterway", None),
        ];
        assert_eq!(
            likely_sense_index("She opened a savings account at the bank.", "bank", &senses),
            Some(0)
        );
    }

    #[test]
    fn likely_sense_neutral_context_returns_none() {
        let senses = vec![
            test_sense("a financial institution that accepts deposits", None),
            test_sense("land alongside a river or lake", None),
        ];
        assert_eq!(
            likely_sense_index(
                "The quick brown fox jumps over the lazy dog.",
                "bank",
                &senses
            ),
            None
        );
    }

    #[test]
    fn likely_sense_empty_context_returns_none() {
        let senses = vec![test_sense("a financial institution", None)];
        assert_eq!(likely_sense_index("", "bank", &senses), None);
        assert_eq!(likely_sense_index("   ", "bank", &senses), None);
        assert_eq!(likely_sense_index("the and of or", "bank", &senses), None);
    }

    #[test]
    fn likely_sense_tie_returns_none_not_an_arbitrary_guess() {
        let senses = vec![
            test_sense("financial institution", None),
            test_sense("financial matters", None),
        ];
        assert_eq!(likely_sense_index("financial", "bank", &senses), None);
    }

    #[test]
    fn likely_sense_matching_is_case_insensitive() {
        let senses = vec![
            test_sense("a financial institution", None),
            test_sense("land alongside a river", None),
        ];
        assert_eq!(likely_sense_index("RIVER BANK", "bank", &senses), Some(1));
    }

    #[test]
    fn likely_sense_ignores_the_looked_up_word_itself() {
        // Every gloss contains the headword, and the sentence contains it
        // too — without exclusion that would be a tie; with exclusion there
        // is no evidence, so no arbitrary hint.
        let senses = vec![
            test_sense("a bank that accepts deposits", None),
            test_sense("a bank beside a river", None),
        ];
        assert_eq!(
            likely_sense_index("The bank was crowded this morning.", "bank", &senses),
            None
        );
    }

    #[test]
    fn likely_sense_stems_inflected_context_words() {
        // "deposits" in the gloss and "deposit" in the sentence both stem
        // to "deposit"; the other sense shares no word with the sentence.
        let senses = vec![
            test_sense("an institution that accepts deposits", None),
            test_sense("land alongside a river", None),
        ];
        assert_eq!(
            likely_sense_index("I will deposit the cheque tomorrow.", "bank", &senses),
            Some(0)
        );
    }

    #[test]
    fn tokenize_context_keeps_content_words_only() {
        let tokens = tokenize_context("The bank by the river was muddy.");
        assert!(tokens.contains(&"bank".to_string()));
        assert!(tokens.contains(&"river".to_string()));
        assert!(tokens.contains(&"muddy".to_string()));
        assert!(!tokens.contains(&"the".to_string()));
        assert!(!tokens.contains(&"by".to_string()));
        assert!(!tokens.contains(&"was".to_string()));
    }

    #[test]
    fn stem_token_strips_common_inflections() {
        assert_eq!(stem_token("running"), "run");
        assert_eq!(stem_token("played"), "play");
        assert_eq!(stem_token("scored"), "scor");
        assert_eq!(stem_token("score"), "scor");
        assert_eq!(stem_token("studies"), "study");
        assert_eq!(stem_token("tried"), "try");
        assert_eq!(stem_token("boxes"), "box");
        assert_eq!(stem_token("deposits"), "deposit");
        assert_eq!(stem_token("bank's"), "bank");
    }

    #[test]
    fn stem_token_leaves_roots_alone() {
        // Guards: a root that merely ends in a suffix must survive.
        assert_eq!(stem_token("need"), "need");
        assert_eq!(stem_token("sing"), "sing");
        assert_eq!(stem_token("bring"), "bring");
        assert_eq!(stem_token("gas"), "gas");
        assert_eq!(stem_token("kiss"), "kiss");
        assert_eq!(stem_token("class"), "class");
        assert_eq!(stem_token("bus"), "bus");
        assert_eq!(stem_token("this"), "this");
        assert_eq!(stem_token("use"), "use");
        assert_eq!(stem_token("bank"), "bank");
    }
}

#[test]
fn dictionary_priority_order_and_reorder_change_the_merged_winner() {
    let cat = Catalog::open_in_memory().unwrap();
    let wordnet = cat.insert_dictionary("WordNet", Some("en"), 1).unwrap();
    let idioms = cat.insert_dictionary("Idioms", Some("en"), 1).unwrap();
    let imported = cat.insert_dictionary("Imported", Some("en"), 1).unwrap();
    cat.set_dictionary_priority(wordnet, 10).unwrap();
    cat.set_dictionary_priority(idioms, 20).unwrap();
    // imported keeps the insert default (100).

    // All three define the same headword differently.
    for (dict_id, def) in [
        (wordnet, "WordNet's sense"),
        (idioms, "Idioms' sense"),
        (imported, "Imported's sense"),
    ] {
        cat.batch_insert_dict_entries(dict_id, &[("bank".to_string(), def.to_string())])
            .unwrap();
    }
    cat.rebuild_combined_dictionary().unwrap();

    // list order is effective order (priority ASC).
    let order: Vec<String> = cat
        .list_dictionaries()
        .unwrap()
        .iter()
        .map(|d| d.name.clone())
        .collect();
    assert_eq!(order, vec!["WordNet", "Idioms", "Imported"]);
    assert_eq!(cat.list_dictionaries().unwrap()[0].priority, 10);

    // The lowest priority speaks for shared words.
    let entry = cat.lookup_entry("bank").unwrap();
    assert_eq!(entry.senses[0].def, "WordNet's sense");

    // Move Imported up twice → it speaks first; priorities are compact.
    cat.move_dictionary_priority(imported, -1).unwrap();
    cat.move_dictionary_priority(imported, -1).unwrap();
    let dicts = cat.list_dictionaries().unwrap();
    assert_eq!(dicts[0].name, "Imported");
    assert_eq!(dicts[0].priority, 0);
    assert_eq!(dicts[1].name, "WordNet");
    assert_eq!(dicts[1].priority, 1);
    assert_eq!(dicts[2].name, "Idioms");
    assert_eq!(dicts[2].priority, 2);
    let entry = cat.lookup_entry("bank").unwrap();
    assert_eq!(entry.senses[0].def, "Imported's sense");

    // Moving past the edges is a no-op, and unknown ids are ignored.
    cat.move_dictionary_priority(imported, -1).unwrap();
    cat.move_dictionary_priority(999, -1).unwrap();
    assert_eq!(cat.list_dictionaries().unwrap()[0].name, "Imported");
}
