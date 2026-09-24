//! Prefs queries.
//!
//! Split out of a 3,400-line `db.rs` purely to make it navigable; these are
//! the same methods on the same `Catalog`, moved verbatim.

use super::*;

impl Catalog {
    // -----------------------------------------------------------------------
    // P4.1: preferences
    // -----------------------------------------------------------------------

    /// Read a preference.
    ///
    /// P6.5: some preferences describe the *app* rather than *these books* —
    /// your theme, reader font size, dictionary settings, API keys. Those live
    /// in `~/.config/kalam/prefs.json` and are shared by every library;
    /// otherwise switching library would silently reset them, because the new
    /// library's database has never heard of them. See
    /// `libraries::is_global_pref`.
    ///
    /// Global reads fall back to the library database when the config file has
    /// no entry, which is what carries an existing install across: values
    /// written before this split still live in `catalog.db`, and get promoted
    /// on the next write.
    pub fn get_pref(&self, key: &str) -> Option<String> {
        if crate::libraries::is_global_pref(key) {
            if let Some(v) = crate::libraries::load_global_prefs().get(key) {
                return Some(v.clone());
            }
        }
        let conn = self.conn.lock().ok()?;
        conn.query_row(
            "SELECT value FROM app_prefs WHERE key = ?1",
            params![key],
            |r| r.get(0),
        )
        .optional()
        .ok()
        .flatten()
    }

    /// Write a preference, to the config file or the library as appropriate.
    ///
    /// A global pref is written to *both*: the config file is the one that is
    /// read, and the library copy keeps an older Kalam working against the
    /// same library. Cheap insurance against a downgrade turning a user's
    /// settings into defaults.
    pub fn set_pref(&self, key: &str, value: &str) {
        if crate::libraries::is_global_pref(key) {
            if let Err(err) = crate::libraries::set_global_pref(key, value) {
                // Not fatal: the library copy below still holds the value, so
                // the setting works until Kalam is pointed at another library.
                eprintln!("kalam: could not save global preference {key}: {err}");
            }
        }
        if let Ok(conn) = self.conn.lock() {
            let _ = conn.execute(
                "INSERT INTO app_prefs (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            );
        }
    }

    /// Convenience for numeric prefs; falls back when unset or unparsable.
    pub fn get_pref_i64(&self, key: &str, default: i64) -> i64 {
        self.get_pref(key)
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }

    /// Convenience for writing numeric prefs.
    pub fn set_pref_i64(&self, key: &str, value: i64) {
        self.set_pref(key, &value.to_string());
    }
}
