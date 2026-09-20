//! Offline dictionary handling — StarDict + Kalam SQLite packs.
//!
//! StarDict format (minimal parser):
//!   .ifo – metadata (wordcount)
//!   .idx – sorted list of [word\0][offset 32-bit BE][size 32-bit BE] (also supports 64-bit BE if sametypesequence has 'g'/'h' – we try both)
//!   .dict / .dict.dz – concatenated definitions
//! For simplicity P3 supports uncompressed .dict; .dict.dz is decompressed via flate2 if present.
//! SQLite pack: a SQLite file with table entries(word TEXT, definition TEXT) or (word, definition) naming variations.

// Module-wide because the StarDict reader exposes a complete parse of the
// format (64-bit offsets, `sametypesequence` variants) of which the app
// currently calls only part, and CI runs `-D warnings`. Narrow this to the
// specific items when the dictionary work next lands: a module-level allow
// also hides anything that becomes dead *later*, which is exactly what it
// should not do.

use crate::db::{
    Catalog, BUNDLED_ANTONYMS_NAME, BUNDLED_IDIOMS_NAME, BUNDLED_SYNONYMS_NAME,
    BUNDLED_WORDNET_NAME,
};
use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

/// Result of a dictionary search.
#[derive(Debug, Clone)]
pub struct DictSearchResult {
    pub word: String,
    pub definition: String,
    pub dict_name: String,
}

const BUNDLED_WORDNET_PREF: &str = "bundled_dictionary_english_wordnet_2025";
#[cfg(feature = "bundled-dictionaries")]
const BUNDLED_WORDNET_TSV_GZ: &[u8] =
    include_bytes!("../resources/dictionaries/english-wordnet-2025.tsv.gz");
#[cfg(not(feature = "bundled-dictionaries"))]
const BUNDLED_WORDNET_TSV_GZ: &[u8] = &[];

const BUNDLED_IDIOMS_PREF: &str = "bundled_dictionary_english_idioms_2024";
#[cfg(feature = "bundled-dictionaries")]
const BUNDLED_IDIOMS_TSV_GZ: &[u8] =
    include_bytes!("../resources/dictionaries/english-idioms-2024.tsv.gz");
#[cfg(not(feature = "bundled-dictionaries"))]
const BUNDLED_IDIOMS_TSV_GZ: &[u8] = &[];

const BUNDLED_SYNONYMS_PREF: &str = "bundled_dictionary_english_synonyms_3_0";
#[cfg(feature = "bundled-dictionaries")]
const BUNDLED_SYNONYMS_TSV_GZ: &[u8] =
    include_bytes!("../resources/dictionaries/english-synonyms-3.0.tsv.gz");
#[cfg(not(feature = "bundled-dictionaries"))]
const BUNDLED_SYNONYMS_TSV_GZ: &[u8] = &[];

const BUNDLED_ANTONYMS_PREF: &str = "bundled_dictionary_english_antonyms_3_0";
#[cfg(feature = "bundled-dictionaries")]
const BUNDLED_ANTONYMS_TSV_GZ: &[u8] =
    include_bytes!("../resources/dictionaries/english-antonyms-3.0.tsv.gz");
#[cfg(not(feature = "bundled-dictionaries"))]
const BUNDLED_ANTONYMS_TSV_GZ: &[u8] = &[];

// Merged-store priorities (lower = consulted first). WordNet speaks for
// shared words by default; imported packs keep the schema default 100.
const PRIORITY_WORDNET: i64 = 10;
const PRIORITY_IDIOMS: i64 = 20;
const PRIORITY_SYNONYMS: i64 = 30;
const PRIORITY_ANTONYMS: i64 = 40;

/// Install the small, redistributable English dictionaries shipped with Kalam.
///
/// Each preference makes its pack a first-run action rather than a migration
/// that re-adds a pack after the user removes it. The compressed sources are
/// kept in the binary so the default dictionaries work without a download.
/// The merged dictionary store is rebuilt only when at least one pack was
/// actually installed this run.
pub fn install_bundled_dictionaries(catalog: &Catalog) -> Result<()> {
    let mut installed = false;
    if install_bundled_tsv(
        catalog,
        BUNDLED_WORDNET_NAME,
        BUNDLED_WORDNET_PREF,
        BUNDLED_WORDNET_TSV_GZ,
        PRIORITY_WORDNET,
    )? {
        installed = true;
    }
    if install_bundled_tsv(
        catalog,
        BUNDLED_IDIOMS_NAME,
        BUNDLED_IDIOMS_PREF,
        BUNDLED_IDIOMS_TSV_GZ,
        PRIORITY_IDIOMS,
    )? {
        installed = true;
    }
    if install_bundled_tsv(
        catalog,
        BUNDLED_SYNONYMS_NAME,
        BUNDLED_SYNONYMS_PREF,
        BUNDLED_SYNONYMS_TSV_GZ,
        PRIORITY_SYNONYMS,
    )? {
        installed = true;
    }
    if install_bundled_tsv(
        catalog,
        BUNDLED_ANTONYMS_NAME,
        BUNDLED_ANTONYMS_PREF,
        BUNDLED_ANTONYMS_TSV_GZ,
        PRIORITY_ANTONYMS,
    )? {
        installed = true;
    }
    if installed {
        catalog.rebuild_combined_dictionary()?;
    }
    Ok(())
}

/// Install one bundled pack. Returns true when the pack was installed (or
/// its priority refreshed) in this run.
fn install_bundled_tsv(
    catalog: &Catalog,
    dictionary_name: &str,
    installed_pref: &str,
    compressed_tsv: &[u8],
    priority: i64,
) -> Result<bool> {
    if compressed_tsv.is_empty() {
        return Ok(false);
    }

    if catalog.get_pref(installed_pref).as_deref() == Some("installed") {
        return Ok(false);
    }

    // This also handles an upgrade from a build that seeded the row before it
    // stored the first-run marker. The priority is (re)applied so packs
    // installed by older builds still join the merged store in the right
    // order.
    if let Some(existing) = catalog
        .list_dictionaries()?
        .iter()
        .find(|dict| dict.name == dictionary_name)
    {
        catalog.set_dictionary_priority(existing.id, priority)?;
        catalog.set_pref(installed_pref, "installed");
        return Ok(false);
    }

    let decoder = flate2::read::GzDecoder::new(compressed_tsv);
    let reader = BufReader::new(decoder);
    let mut entries = Vec::new();
    for line in std::io::BufRead::lines(reader) {
        let line = line?;
        let Some((word, definition)) = line.split_once('\t') else {
            continue;
        };
        let word = word.trim();
        let definition = definition.trim();
        if !word.is_empty() && !definition.is_empty() {
            entries.push((word.to_string(), definition.to_string()));
        }
    }
    if entries.is_empty() {
        return Err(anyhow!(
            "bundled dictionary pack '{dictionary_name}' is empty"
        ));
    }

    let dict_id = catalog.insert_dictionary(dictionary_name, Some("en"), entries.len() as i64)?;
    catalog.set_dictionary_priority(dict_id, priority)?;
    let outcome = (|| {
        catalog.clear_dict_entries(dict_id)?;
        for chunk in entries.chunks(2000) {
            catalog.batch_insert_dict_entries(dict_id, chunk)?;
        }
        Ok(())
    })();
    if let Err(e) = outcome {
        let _ = catalog.delete_dictionary(dict_id);
        return Err(e);
    }
    catalog.set_pref(installed_pref, "installed");
    Ok(true)
}

/// Import a dictionary pack into the catalog.
///
/// Supports:
/// - StarDict triple (.ifo + .idx + .dict[.dz]): provide any one file path, we find siblings.
/// - SQLite file with entries table.
///
/// Returns the dictionary name and entry count.
///
/// Convenience wrapper for callers with no UI to report to (tests, the
/// bundled-pack installer). Import progress is discarded; use
/// [`import_dictionary_with_progress`] to drive a toast.
pub fn import_dictionary(catalog: &Catalog, path: &Path) -> Result<(String, i64)> {
    import_dictionary_with_progress(catalog, path, &|_| {})
}

/// As [`import_dictionary`], reporting entries written so far.
///
/// `progress` is called once per flushed batch (every [`IMPORT_BATCH`]
/// entries) with the running total. It runs on whatever thread is doing the
/// import — a worker — so it must not touch GTK or `notify`; hand the number
/// back through `tasks::Reporter` instead.
pub fn import_dictionary_with_progress(
    catalog: &Catalog,
    path: &Path,
    progress: &dyn Fn(usize),
) -> Result<(String, i64)> {
    // Guard: importing Kalam's own catalog database as a "dictionary pack"
    // would read the app's tables as entries. Compare canonical paths so
    // ~, symlinks and relative paths all resolve.
    if same_file(path, &crate::paths::catalog_db()) {
        return Err(anyhow!(
            "{} is Kalam's own catalog database — pick a dictionary pack (.ifo/.idx/.dict, .db or .tsv) instead",
            path.display()
        ));
    }

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    // Detect StarDict by .ifo/.idx/.dict extension or by sibling presence
    let outcome = if ext == "ifo" || ext == "idx" || ext == "dict" || ext == "dz" {
        import_stardict(catalog, path)
    } else if is_sqlite_file(path)? {
        // Try SQLite detection: file starts with "SQLite format 3\0"
        import_sqlite_pack(catalog, path, progress)
    } else if path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.to_ascii_lowercase().contains("stardict"))
        .unwrap_or(false)
    {
        // Fallback: try stardict detection from base name
        import_stardict(catalog, path)
    } else if ext == "txt" || ext == "tab" || ext == "tsv" {
        // Last try: plain text tab-separated dictionary (word<TAB>definition)
        import_tsv(catalog, path, progress)
    } else {
        let hint = match ext.as_str() {
            "db" | "sqlite" | "sqlite3" => "a SQLite dictionary pack",
            "txt" | "tab" | "tsv" => "a tab-separated dictionary",
            "ifo" | "idx" | "dz" => "a StarDict pack",
            _ => "a StarDict pack, a SQLite dictionary pack, or a tab-separated dictionary",
        };
        return Err(anyhow!(
            "unrecognized dictionary format for {} — expected {hint}",
            path.display()
        ));
    }?;

    // A new dictionary must join the merged store immediately.
    catalog.rebuild_combined_dictionary()?;
    Ok(outcome)
}

/// Whether two paths point at the same file (canonicalize both sides; a
/// missing file fails the comparison rather than matching by accident).
fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => false,
    }
}

fn is_sqlite_file(path: &Path) -> Result<bool> {
    let mut f = File::open(path)?;
    let mut header = [0u8; 16];
    let n = f.read(&mut header).unwrap_or(0);
    if n < 16 {
        return Ok(false);
    }
    Ok(&header[..16] == b"SQLite format 3\0")
}

/// Import SQLite pack: look for table `entries` or `dict` or `words`
fn import_sqlite_pack(
    catalog: &Catalog,
    path: &Path,
    progress: &dyn Fn(usize),
) -> Result<(String, i64)> {
    use rusqlite::Connection;

    // Read-only: an import must never modify the pack file (and never
    // create -wal/-journal sidecars next to a user's read-only data).
    let conn = Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .context("open sqlite dict")?;
    // Find a suitable table
    let mut tables = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
    )?;
    for r in stmt.query_map([], |row| row.get::<_, String>(0))? {
        tables.push(r?);
    }

    let chosen = if tables.contains(&"entries".to_string()) {
        "entries"
    } else if tables.contains(&"dict".to_string()) {
        "dict"
    } else if tables.contains(&"words".to_string()) {
        "words"
    } else if !tables.is_empty() {
        &tables[0]
    } else {
        return Err(anyhow!("sqlite dict has no tables"));
    };

    // Determine column names
    let chosen_ident = quote_ident(chosen);
    let mut columns_stmt = conn.prepare(&format!("PRAGMA table_info({chosen_ident})"))?;
    let mut cols = Vec::new();
    for r in columns_stmt.query_map([], |row| row.get::<_, String>(1))? {
        cols.push(r?);
    }
    // Pick the headword and definition columns by name, falling back to
    // position. Written as `find(...).or_else(...)` rather than the previous
    // `if any(..) { find(..).unwrap() }` chain for two reasons: it searches
    // once instead of twice, and it has no `unwrap()` to audit.
    //
    // The fallbacks used to index `cols[0]` / `cols[1]` directly, which
    // panics when `cols` is empty. That is reachable from a *user-chosen*
    // file: `PRAGMA table_info` returns zero rows for a virtual table whose
    // module is not loaded, and `chosen` can be such a table because the last
    // resort is "whatever `sqlite_master` lists first". A panic here happens
    // on the import worker thread and replaces the careful error message
    // below with a backtrace.
    let pick = |names: &[&str]| -> Option<&String> {
        cols.iter()
            .find(|c| names.iter().any(|n| c.eq_ignore_ascii_case(n)))
    };
    let word_col = pick(&["word", "term"])
        .or_else(|| cols.first())
        .ok_or_else(|| {
            anyhow!(
                "table '{chosen}' in {} has no columns to read a dictionary from",
                path.display()
            )
        })?;
    let def_col = pick(&["definition", "meaning", "def"])
        .or_else(|| cols.get(1))
        .or_else(|| cols.first())
        .ok_or_else(|| {
            anyhow!(
                "table '{chosen}' in {} has no definition column",
                path.display()
            )
        })?;

    let sql = format!(
        "SELECT {}, {} FROM {}",
        quote_ident(word_col),
        quote_ident(def_col),
        chosen_ident
    );
    let dict_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("ImportedDict")
        .to_string();

    // Streamed straight from the source cursor into batched inserts: the pack
    // is never held in memory as a whole. See `EntrySink`.
    let mut stmt = conn.prepare(&sql)?;
    let mut sink = EntrySink::new(catalog, &dict_name, None, progress)?;
    let mut rows = stmt.query([])?;
    loop {
        let row = match rows.next() {
            Ok(Some(row)) => row,
            Ok(None) => break,
            Err(e) => {
                sink.abort();
                return Err(e.into());
            }
        };
        let w: String = match row.get(0) {
            Ok(w) => w,
            Err(e) => {
                sink.abort();
                return Err(e.into());
            }
        };
        let d: String = match row.get(1) {
            Ok(d) => d,
            Err(e) => {
                sink.abort();
                return Err(e.into());
            }
        };
        let (w, d) = (w.trim(), d.trim());
        if !w.is_empty() && !d.is_empty() {
            sink.push(w.to_string(), d.to_string())?;
        }
    }
    let count = sink
        .finish()
        .map_err(|e| anyhow!("sqlite dict {}: {e}", path.display()))?;
    Ok((dict_name, count))
}

/// Double-quote a SQLite identifier, doubling any embedded quotes so a
/// crafted pack table/column name can never break out of the identifier.
fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn import_tsv(catalog: &Catalog, path: &Path, progress: &dyn Fn(usize)) -> Result<(String, i64)> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let dict_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("TSVDict")
        .to_string();

    // Streamed line by line — a TSV pack is the largest thing a user is
    // likely to import, and it used to be fully materialised before the
    // first row was written. See `EntrySink`.
    let mut sink = EntrySink::new(catalog, &dict_name, None, progress)?;
    for line in std::io::BufRead::lines(reader) {
        let line = match line {
            Ok(line) => line,
            Err(e) => {
                sink.abort();
                return Err(e.into());
            }
        };
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let split = line
            .find('\t')
            .map(|tab| (&line[..tab], line[tab + 1..].trim()))
            .or_else(|| {
                line.find("  ")
                    .map(|sep| (&line[..sep], line[sep..].trim()))
            });
        if let Some((w, d)) = split {
            let w = w.trim();
            if !w.is_empty() && !d.is_empty() {
                sink.push(w.to_string(), d.to_string())?;
            }
        }
    }
    let count = sink.finish().map_err(|_| anyhow!("TSV dict empty"))?;
    Ok((dict_name, count))
}

/// How many entries to hold in memory before flushing to SQLite.
///
/// Matches the batch size `install_bundled_dictionaries` already uses. The
/// number is a memory bound, not a throughput knob: 2,000 entries is a few
/// hundred KB, where a whole WordNet-class pack held at once is a few hundred
/// **MB** on a machine with 4 GB.
const IMPORT_BATCH: usize = 2_000;

/// Streams dictionary entries into the catalog in bounded batches.
///
/// # Why this exists
///
/// The SQLite and TSV importers used to build a `Vec<(String, String)>` of
/// the *entire* pack and hand it over at the end. A 500k-entry pack is
/// therefore several hundred MB resident before a single row is written —
/// on the 4 GB laptop this app targets, next to a WebKit process. Worse, the
/// user saw one "Importing…" toast and then nothing at all, because there was
/// no progress to report until the whole file had been parsed.
///
/// This flushes every [`IMPORT_BATCH`] entries and reports as it goes, so
/// peak memory is a constant and the toast actually moves.
///
/// # Failure behaviour
///
/// Same contract as before: if anything fails part-way, the dictionary's meta
/// row is removed. A half-imported dictionary would otherwise occupy its name
/// (blocking a re-import) and report a wrong entry count. Call [`finish`] to
/// commit; dropping without it leaves the rows but is only reachable on an
/// error path that has already deleted the dictionary.
struct EntrySink<'a> {
    catalog: &'a Catalog,
    dict_id: i64,
    buffer: Vec<(String, String)>,
    total: i64,
    progress: &'a dyn Fn(usize),
}

impl<'a> EntrySink<'a> {
    /// Register the dictionary and prepare to stream into it.
    fn new(
        catalog: &'a Catalog,
        name: &str,
        lang: Option<&str>,
        progress: &'a dyn Fn(usize),
    ) -> Result<Self> {
        // The real count is not known until the stream ends, so record 0 and
        // correct it in `finish`.
        let dict_id = catalog
            .insert_dictionary(name, lang, 0)
            .map_err(|e| anyhow!("insert dict meta: {e}"))?;
        // Clear first so re-importing an existing dictionary replaces its
        // entries instead of duplicating them.
        if let Err(e) = catalog.clear_dict_entries(dict_id) {
            let _ = catalog.delete_dictionary(dict_id);
            return Err(anyhow!("clear dict: {e}"));
        }
        Ok(Self {
            catalog,
            dict_id,
            buffer: Vec::with_capacity(IMPORT_BATCH),
            total: 0,
            progress,
        })
    }

    fn push(&mut self, word: String, definition: String) -> Result<()> {
        self.buffer.push((word, definition));
        if self.buffer.len() >= IMPORT_BATCH {
            self.flush()?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        if let Err(e) = self
            .catalog
            .batch_insert_dict_entries(self.dict_id, &self.buffer)
        {
            self.abort();
            return Err(anyhow!("batch insert: {e}"));
        }
        self.total += self.buffer.len() as i64;
        self.buffer.clear();
        (self.progress)(self.total as usize);
        Ok(())
    }

    /// Remove the dictionary row after a failed import.
    fn abort(&self) {
        let _ = self.catalog.delete_dictionary(self.dict_id);
    }

    /// Flush the tail and write the true entry count. Returns the count.
    fn finish(mut self) -> Result<i64> {
        self.flush()?;
        if self.total == 0 {
            self.abort();
            return Err(anyhow!("dictionary pack has no usable entries"));
        }
        if let Err(e) = self
            .catalog
            .set_dictionary_entry_count(self.dict_id, self.total)
        {
            self.abort();
            return Err(anyhow!("set entry count: {e}"));
        }
        Ok(self.total)
    }
}

/// Register a dictionary and bulk-insert its entries, removing the meta row
/// again if the insert fails — a half-imported dictionary would otherwise
/// occupy its name (blocking a re-import) and report a wrong entry count.
///
/// Kept for callers that genuinely have the whole set in memory already (the
/// StarDict reader, which must seek around its `.dict` blob anyway). New
/// streaming callers should use [`EntrySink`].
fn insert_imported_entries(
    catalog: &Catalog,
    name: &str,
    lang: Option<&str>,
    entries: &[(String, String)],
) -> Result<i64> {
    let dict_id = catalog
        .insert_dictionary(name, lang, entries.len() as i64)
        .map_err(|e| anyhow!("insert dict meta: {e}"))?;
    let outcome = (|| {
        // Clear first so re-importing an existing dictionary replaces its
        // entries instead of duplicating them.
        catalog
            .clear_dict_entries(dict_id)
            .map_err(|e| anyhow!("clear dict: {e}"))?;
        for chunk in entries.chunks(2000) {
            catalog
                .batch_insert_dict_entries(dict_id, chunk)
                .map_err(|e| anyhow!("batch insert: {e}"))?;
        }
        Ok(())
    })();
    if let Err(e) = outcome {
        let _ = catalog.delete_dictionary(dict_id);
        return Err(e);
    }
    Ok(dict_id)
}

// ---------------------------------------------------------------------------
// StarDict
// ---------------------------------------------------------------------------

fn import_stardict(catalog: &Catalog, any_path: &Path) -> Result<(String, i64)> {
    let base = stardict_base_path(any_path)?;
    let ifo_path = base.with_extension("ifo");
    let idx_path = base.with_extension("idx");
    let dict_path = base.with_extension("dict");
    let dict_dz_path = PathBuf::from(format!("{}.dict.dz", base.display()));

    let ifo_path = if ifo_path.exists() {
        ifo_path
    } else if any_path.extension().map(|e| e == "ifo").unwrap_or(false) {
        any_path.to_path_buf()
    } else if Path::new(&format!("{}.ifo", base.display())).exists() {
        PathBuf::from(format!("{}.ifo", base.display()))
    } else {
        find_sibling_with_ext(any_path, "ifo")
            .ok_or_else(|| anyhow!("StarDict .ifo not found for {}", any_path.display()))?
    };

    let idx_path = if idx_path.exists() {
        idx_path
    } else {
        find_sibling_with_ext(&ifo_path, "idx").ok_or_else(|| anyhow!("StarDict .idx not found"))?
    };

    let dict_path_opt = if dict_path.exists() {
        Some(dict_path)
    } else if dict_dz_path.exists() {
        Some(dict_dz_path)
    } else if let Some(p) = find_sibling_with_ext(&ifo_path, "dict") {
        Some(p)
    } else if let Some(p) = find_sibling_with_ext(&ifo_path, "dz") {
        Some(p)
    } else {
        find_file_ending_with(
            ifo_path.parent().unwrap_or_else(|| Path::new(".")),
            ".dict.dz",
        )
    };

    let dict_path = dict_path_opt.ok_or_else(|| anyhow!("StarDict .dict/.dict.dz not found"))?;

    let meta = parse_ifo(&ifo_path)?;
    let dict_name = meta.get("bookname").cloned().unwrap_or_else(|| {
        base.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("StarDict")
            .to_string()
    });

    let entries_meta = parse_idx(&idx_path, &meta)?;
    let dict_bytes = read_dict_file(&dict_path)?;

    let mut entries = Vec::with_capacity(entries_meta.len());
    for em in entries_meta {
        if em.offset as usize + em.size as usize <= dict_bytes.len() {
            let slice = &dict_bytes[em.offset as usize..em.offset as usize + em.size as usize];
            let def = String::from_utf8_lossy(slice)
                .trim_end_matches('\0')
                .trim()
                .to_string();
            if !def.is_empty() {
                entries.push((em.word, def));
            }
        }
    }

    if entries.is_empty() {
        return Err(anyhow!("StarDict produced no entries"));
    }

    insert_imported_entries(catalog, &dict_name, None, &entries)?;
    Ok((dict_name, entries.len() as i64))
}

fn stardict_base_path(any_path: &Path) -> Result<PathBuf> {
    let s = any_path.to_string_lossy();
    let mut base = s.to_string();
    for ext in &[".ifo", ".idx", ".dict.dz", ".dict", ".dz"] {
        if base.to_ascii_lowercase().ends_with(ext) {
            base = base[..base.len() - ext.len()].to_string();
            break;
        }
    }
    Ok(PathBuf::from(base))
}

fn find_sibling_with_ext(base: &Path, ext: &str) -> Option<PathBuf> {
    let parent = base.parent()?;
    let stem = base.file_stem()?.to_str()?;
    let candidate = parent.join(format!("{stem}.{ext}"));
    if candidate.exists() {
        return Some(candidate);
    }
    if let Ok(dir) = std::fs::read_dir(parent) {
        for entry in dir.flatten() {
            let p = entry.path();
            let ext_match = p
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case(ext))
                .unwrap_or(false);
            let name_match = p
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.contains(stem))
                .unwrap_or(false);
            if ext_match && name_match {
                return Some(p);
            }
            if ext == "dz" && p.to_string_lossy().ends_with(".dict.dz") {
                return Some(p);
            }
            if ext == "dict"
                && (p.to_string_lossy().ends_with(".dict")
                    || p.to_string_lossy().ends_with(".dict.dz"))
            {
                return Some(p);
            }
        }
    }
    None
}

fn find_file_ending_with(dir: &Path, suffix: &str) -> Option<PathBuf> {
    let rd = std::fs::read_dir(dir).ok()?;
    for e in rd.flatten() {
        let p = e.path();
        if p.to_string_lossy().ends_with(suffix) {
            return Some(p);
        }
    }
    None
}

fn parse_ifo(path: &Path) -> Result<HashMap<String, String>> {
    let content = std::fs::read_to_string(path).context("read .ifo")?;
    let mut map = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with("StarDict's dict") {
            continue;
        }
        if let Some(eq) = line.find('=') {
            let key = line[..eq].trim().to_string();
            let value = line[eq + 1..].trim().to_string();
            map.insert(key, value);
        }
    }
    Ok(map)
}

#[derive(Debug)]
struct IdxEntryMeta {
    word: String,
    offset: u64,
    size: u64,
}

fn parse_idx(path: &Path, _ifo_meta: &HashMap<String, String>) -> Result<Vec<IdxEntryMeta>> {
    let data = std::fs::read(path).context("read .idx")?;
    let mut out = Vec::new();
    let mut i = 0usize;
    let mut try_64 = false;
    if let Some(bits) = _ifo_meta.get("idxoffsetbits") {
        if bits.trim() == "64" {
            try_64 = true;
        }
    }
    if try_64 {
        while i < data.len() {
            let start = i;
            while i < data.len() && data[i] != 0 {
                i += 1;
            }
            if i >= data.len() {
                break;
            }
            let word_bytes = &data[start..i];
            let word = String::from_utf8_lossy(word_bytes).to_string();
            i += 1;
            if i + 16 > data.len() {
                break;
            }
            let offset = u64::from_be_bytes([
                data[i],
                data[i + 1],
                data[i + 2],
                data[i + 3],
                data[i + 4],
                data[i + 5],
                data[i + 6],
                data[i + 7],
            ]);
            let size = u64::from_be_bytes([
                data[i + 8],
                data[i + 9],
                data[i + 10],
                data[i + 11],
                data[i + 12],
                data[i + 13],
                data[i + 14],
                data[i + 15],
            ]);
            i += 16;
            out.push(IdxEntryMeta { word, offset, size });
        }
    } else {
        while i < data.len() {
            let start = i;
            while i < data.len() && data[i] != 0 {
                i += 1;
            }
            if i >= data.len() {
                break;
            }
            let word_bytes = &data[start..i];
            let word = String::from_utf8_lossy(word_bytes).to_string();
            i += 1;
            if i + 8 > data.len() {
                break;
            }
            let offset =
                u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]) as u64;
            let size =
                u32::from_be_bytes([data[i + 4], data[i + 5], data[i + 6], data[i + 7]]) as u64;
            i += 8;
            out.push(IdxEntryMeta { word, offset, size });
        }
    }
    Ok(out)
}

fn read_dict_file(path: &Path) -> Result<Vec<u8>> {
    let s = path.to_string_lossy().to_ascii_lowercase();
    if s.ends_with(".dz") || s.ends_with(".gz") {
        let file = File::open(path).context("open dict.dz")?;
        let mut gz = flate2::read::GzDecoder::new(file);
        let mut buf = Vec::new();
        gz.read_to_end(&mut buf).context("decompress dict.dz")?;
        Ok(buf)
    } else {
        std::fs::read(path).context("read .dict")
    }
}

// ---------------------------------------------------------------------------
// Utility for cleaning definition HTML to plain-ish text for GTK display
// ---------------------------------------------------------------------------

pub fn strip_dict_html(input: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in input.chars() {
        if in_tag {
            if ch == '>' {
                in_tag = false;
            }
            continue;
        }
        if ch == '<' {
            in_tag = true;
            continue;
        }
        out.push(ch);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Catalog;
    use std::io::Write;

    /// Unique scratch dir per test run; removed by drop.
    struct ScratchDir(PathBuf);
    impl ScratchDir {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("kalam-dict-test-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&p).unwrap();
            ScratchDir(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
        fn join(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }
    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn write_file(path: &Path, content: &str) {
        let mut f = File::create(path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn quote_ident_escapes_embedded_quotes() {
        assert_eq!(quote_ident("entries"), "\"entries\"");
        assert_eq!(quote_ident("my dict"), "\"my dict\"");
        assert_eq!(
            quote_ident("word\"; DROP TABLE x; --"),
            "\"word\"\"; DROP TABLE x; --\""
        );
    }

    #[test]
    fn same_file_resolves_symlinks_and_relative_paths() {
        let scratch = ScratchDir::new();
        let target = scratch.join("target.txt");
        write_file(&target, "x");
        let link = scratch.join("link.txt");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        assert!(same_file(&target, &target));
        assert!(same_file(&target, &link));
        assert!(same_file(&target, &scratch.join("./target.txt")));
        assert!(!same_file(&target, &scratch.join("other.txt")));
        assert!(!same_file(&target, &scratch.join("missing.txt")));
    }

    #[test]
    fn sqlite_pack_imports_entries_and_leaves_file_read_only() {
        let scratch = ScratchDir::new();
        let pack = scratch.join("mydict.db");
        {
            let conn = rusqlite::Connection::open(&pack).unwrap();
            conn.execute_batch(
                "CREATE TABLE entries (word TEXT, definition TEXT);
                 INSERT INTO entries VALUES ('serendipity', 'a happy accident');
                 INSERT INTO entries VALUES ('ephemeral', 'short-lived');",
            )
            .unwrap();
        }
        let before = std::fs::read(&pack).unwrap();

        let cat = Catalog::open_in_memory().unwrap();
        let (name, count) = import_dictionary(&cat, &pack).unwrap();
        assert_eq!(name, "mydict");
        assert_eq!(count, 2);

        let dicts = cat.list_dictionaries().unwrap();
        assert_eq!(dicts.len(), 1);
        assert_eq!(dicts[0].name, "mydict");
        assert_eq!(dicts[0].entry_count, 2);

        let entry = cat.lookup_entry("serendipity").unwrap();
        assert_eq!(entry.word, "serendipity");
        assert_eq!(entry.senses.len(), 1);
        assert_eq!(entry.senses[0].def, "a happy accident");

        // The pack must be untouched: same bytes, no wal/journal sidecars.
        assert_eq!(std::fs::read(&pack).unwrap(), before);
        let sidecars: Vec<_> = std::fs::read_dir(scratch.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with("-wal") || n.ends_with("-journal") || n.ends_with("-shm"))
            .collect();
        assert!(sidecars.is_empty(), "pack sidecars created: {sidecars:?}");
    }

    #[test]
    fn sqlite_pack_with_odd_identifiers_imports_safely() {
        let scratch = ScratchDir::new();
        let pack = scratch.join("weird.db");
        {
            let conn = rusqlite::Connection::open(&pack).unwrap();
            conn.execute_batch(
                "CREATE TABLE \"my dict\" (\"word\", \"definition\");
                 INSERT INTO \"my dict\" VALUES ('kw1', 'def one');",
            )
            .unwrap();
        }
        let cat = Catalog::open_in_memory().unwrap();
        let (_, count) = import_dictionary(&cat, &pack).unwrap();
        assert_eq!(count, 1);
        let entry = cat.lookup_entry("kw1").unwrap();
        assert_eq!(entry.senses[0].def, "def one");
    }

    #[test]
    fn tsv_import_accepts_tab_and_two_space_separators() {
        let scratch = ScratchDir::new();
        let tsv = scratch.join("words.tsv");
        write_file(
            &tsv,
            "# comment line\napple\tA fruit.\nbee  A flying insect.\n\nstray no separator\n",
        );
        let cat = Catalog::open_in_memory().unwrap();
        let (name, count) = import_dictionary(&cat, &tsv).unwrap();
        assert_eq!(name, "words");
        assert_eq!(count, 2);
        assert_eq!(cat.lookup_entry("apple").unwrap().senses[0].def, "A fruit.");
        assert_eq!(
            cat.lookup_entry("bee").unwrap().senses[0].def,
            "A flying insect."
        );
        assert!(cat.lookup_entry("stray").unwrap().senses.is_empty());
    }

    #[test]
    fn same_file_rejects_distinct_files_and_directory_is_not_a_pack() {
        // The self-import guard compares canonical paths; verify the
        // comparison itself and that a directory is not accepted as a pack.
        let scratch = ScratchDir::new();
        let db = scratch.join("catalog.db");
        {
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.execute_batch("CREATE TABLE x (y TEXT);").unwrap();
        }
        let copy = scratch.join("catalog-copy.db");
        std::fs::copy(&db, &copy).unwrap();
        assert!(same_file(&db, &db));
        assert!(!same_file(&db, &copy));

        // A directory is never a dictionary pack either.
        let cat = Catalog::open_in_memory().unwrap();
        let err = import_dictionary(&cat, scratch.path()).unwrap_err();
        assert!(err.to_string().contains("unrecognized dictionary format"));
    }

    #[test]
    fn unrecognized_format_error_names_the_expected_formats() {
        let scratch = ScratchDir::new();
        let f = scratch.join("data.xyz");
        write_file(&f, "hello");
        let cat = Catalog::open_in_memory().unwrap();
        let err = import_dictionary(&cat, &f).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("StarDict"), "msg: {msg}");
        assert!(msg.contains("SQLite"), "msg: {msg}");
        assert!(msg.contains("tab-separated"), "msg: {msg}");
    }

    #[test]
    fn reimport_replaces_entries_instead_of_duplicating() {
        let scratch = ScratchDir::new();
        let tsv = scratch.join("words.tsv");
        write_file(&tsv, "alpha\tfirst version\n");
        let cat = Catalog::open_in_memory().unwrap();
        import_dictionary(&cat, &tsv).unwrap();
        assert_eq!(cat.dict_entry_count().unwrap(), 1);

        write_file(&tsv, "alpha\tsecond version\nbeta\tadded later\n");
        let (_, count) = import_dictionary(&cat, &tsv).unwrap();
        assert_eq!(count, 2);
        assert_eq!(cat.dict_entry_count().unwrap(), 2);
        let entry = cat.lookup_entry("alpha").unwrap();
        assert_eq!(entry.senses[0].def, "second version");
    }
}
