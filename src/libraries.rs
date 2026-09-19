//! P6.5 — which library we are looking at, and where it lives.
//!
//! Kalam used to keep exactly one library in a fixed place. This module is the
//! piece that lets you choose the folder and keep several. Everything else in
//! the app is unchanged: `paths::data_dir()` asks this module where the active
//! library is, and the ~30 helpers built on `data_dir()` follow automatically.
//!
//! **Why the list cannot live inside a library.** A setting stored in a
//! library cannot be read before you know which library to open — the
//! chicken-and-egg. So the registry is a small file in the *config* directory,
//! outside every library. That is the one genuinely machine-specific piece of
//! state in the app, and correctly so: it describes *this computer*, not the
//! books.
//!
//! **What travels and what does not.** A library folder holds books, covers
//! and its own `catalog.db` — everything about the books. Dictionaries, the
//! theme and app preferences stay global; otherwise you would reinstall
//! dictionaries every time you switched library.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One library the user knows about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryEntry {
    /// What the user calls it. Shown in the switcher.
    pub name: String,
    /// Where it lives. Absolute.
    pub path: PathBuf,
}

/// The registry file: every library, and which one is open.
///
/// Deliberately plain data with no behaviour, so it can be serialised, tested
/// and reasoned about without touching the filesystem.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryRegistry {
    #[serde(default)]
    pub libraries: Vec<LibraryEntry>,
    /// Index into `libraries`. `None` = nothing chosen yet.
    #[serde(default)]
    pub active: Option<usize>,
}

impl LibraryRegistry {
    /// The library currently open, if the registry is coherent.
    ///
    /// Returns `None` rather than panicking when `active` points nowhere — a
    /// hand-edited or truncated file must not take the app down, and the
    /// caller falls back to the legacy location.
    pub fn active(&self) -> Option<&LibraryEntry> {
        self.libraries.get(self.active?)
    }

    /// Add a library, or select it if that path is already known.
    ///
    /// Matching on path, not name: two entries pointing at the same folder
    /// would be two views of one database, and edits through one would appear
    /// to corrupt the other.
    pub fn add_or_select(&mut self, name: &str, path: &Path) -> usize {
        if let Some(i) = self.libraries.iter().position(|l| l.path == path) {
            self.active = Some(i);
            return i;
        }
        self.libraries.push(LibraryEntry {
            name: name.to_string(),
            path: path.to_path_buf(),
        });
        let i = self.libraries.len() - 1;
        self.active = Some(i);
        i
    }

    /// Forget a library. Does **not** touch the files on disk.
    ///
    /// Returns false if the index is out of range. Keeping the books is the
    /// only safe default: "remove from the list" and "delete my library" are
    /// different intentions and must never be the same button.
    pub fn forget(&mut self, index: usize) -> bool {
        if index >= self.libraries.len() {
            return false;
        }
        self.libraries.remove(index);

        // Keep `active` pointing at the same *library*, not the same slot.
        // Removing an earlier entry shifts everything after it down by one,
        // and an off-by-one here silently opens the wrong library.
        self.active = match self.active {
            Some(a) if a == index => None,
            Some(a) if a > index => Some(a - 1),
            other => other,
        };
        true
    }

    /// Select by index. False if out of range, leaving the choice unchanged.
    pub fn select(&mut self, index: usize) -> bool {
        if index >= self.libraries.len() {
            return false;
        }
        self.active = Some(index);
        true
    }
}

// ---------------------------------------------------------------------------
// Reading and writing the registry
// ---------------------------------------------------------------------------

/// `~/.config/kalam` — settings about *this machine*, not about books.
pub fn config_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            crate::paths::home_dir()
                .map(|h| h.join(".config"))
                .unwrap_or_else(|| PathBuf::from("."))
        });
    base.join("kalam")
}

/// Under `cargo test`, pretend there are no global preferences.
///
/// Without this, tests read the *developer's own* `~/.config/kalam/prefs.json`
/// and their results depend on whose machine they run on. That is exactly the
/// failure `docs/pitfalls.md` §19 describes: a test that passes or fails for
/// reasons unrelated to the code. It bit immediately — making
/// `dict_history_enabled` global broke a pre-existing service test, because
/// `log_dict_lookup` consults that pref and the test suddenly depended on a
/// file outside the repository.
///
/// A `cfg!(test)` guard rather than a temp directory: these functions are used
/// all over, and threading a base path through every caller to serve the tests
/// would be worse than the problem. Prefs are covered by
/// `is_global_pref`'s own tests, which are pure and need no filesystem.
fn skip_global_prefs_in_tests() -> bool {
    cfg!(test)
}

/// `~/.config/kalam/libraries.json`
pub fn registry_path() -> PathBuf {
    config_dir().join("libraries.json")
}

/// Load the registry, or an empty one.
///
/// Every failure returns the default rather than an error. A missing file is
/// the normal first-run state, and a corrupt one must not stop the app
/// starting — the fallback is the legacy fixed location, which still has the
/// user's books in it. Losing the *list* is recoverable; refusing to launch is
/// not.
pub fn load_registry() -> LibraryRegistry {
    let path = registry_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return LibraryRegistry::default();
    };
    match serde_json::from_str(&text) {
        Ok(reg) => reg,
        Err(e) => {
            eprintln!("kalam: {} is unreadable ({e}); ignoring it", path.display());
            LibraryRegistry::default()
        }
    }
}

/// Write the registry.
///
/// Written to a temporary file and renamed, so an interrupted write cannot
/// leave a half-written registry behind — the same verify-then-rename rule
/// `epub_metadata.rs` follows. A truncated file here would lose the list of every
/// library the user has.
pub fn save_registry(reg: &LibraryRegistry) -> std::io::Result<()> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    let final_path = registry_path();
    let tmp = final_path.with_extension("json.tmp");

    let json = serde_json::to_string_pretty(reg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, &final_path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg_with(names: &[&str]) -> LibraryRegistry {
        let mut reg = LibraryRegistry::default();
        for n in names {
            reg.add_or_select(n, Path::new(&format!("/books/{n}")));
        }
        reg
    }

    #[test]
    fn a_fresh_install_has_no_libraries_and_no_active_one() {
        // Drives the fallback to the legacy location, so an existing install
        // keeps working with no migration.
        let reg = LibraryRegistry::default();
        assert!(reg.libraries.is_empty());
        assert_eq!(reg.active, None);
        assert_eq!(reg.active(), None);
    }

    #[test]
    fn adding_a_library_selects_it() {
        let mut reg = LibraryRegistry::default();
        reg.add_or_select("Fiction", Path::new("/books/Fiction"));
        assert_eq!(reg.active().map(|l| l.name.as_str()), Some("Fiction"));
    }

    #[test]
    fn the_same_folder_is_never_added_twice() {
        // Two entries for one folder would be two views of one database, and
        // edits through one would look like corruption in the other.
        let mut reg = reg_with(&["Fiction"]);
        let again = reg.add_or_select("A different name", Path::new("/books/Fiction"));
        assert_eq!(reg.libraries.len(), 1, "the folder was added twice");
        assert_eq!(again, 0, "should have selected the existing entry");
        assert_eq!(
            reg.libraries[0].name, "Fiction",
            "re-adding must not rename the existing library"
        );
    }

    #[test]
    fn forgetting_an_earlier_library_keeps_the_right_one_open() {
        // The bug this guards against: `active` is an index, so removing an
        // entry *before* it shifts every later entry down by one. Off by one
        // here means the app silently opens somebody else's library.
        let mut reg = reg_with(&["A", "B", "C"]);
        assert!(reg.select(2)); // C is open
        assert_eq!(reg.active().map(|l| l.name.as_str()), Some("C"));

        assert!(reg.forget(0)); // forget A
        assert_eq!(
            reg.active().map(|l| l.name.as_str()),
            Some("C"),
            "still C, at its new index"
        );
    }

    #[test]
    fn forgetting_a_later_library_does_not_move_the_open_one() {
        let mut reg = reg_with(&["A", "B", "C"]);
        assert!(reg.select(0));
        assert!(reg.forget(2));
        assert_eq!(reg.active().map(|l| l.name.as_str()), Some("A"));
    }

    #[test]
    fn forgetting_the_open_library_leaves_none_open() {
        // Deliberately not "fall back to the first one": the app should ask
        // rather than silently open a library the user did not choose.
        let mut reg = reg_with(&["A", "B"]);
        assert!(reg.select(1));
        assert!(reg.forget(1));
        assert_eq!(reg.active, None);
        assert_eq!(reg.active(), None);
    }

    #[test]
    fn out_of_range_operations_are_refused_not_obeyed() {
        let mut reg = reg_with(&["A"]);
        assert!(!reg.forget(9), "forget past the end must fail");
        assert!(!reg.select(9), "select past the end must fail");
        assert_eq!(reg.libraries.len(), 1);
        assert_eq!(
            reg.active().map(|l| l.name.as_str()),
            Some("A"),
            "a refused operation must not change what is open"
        );
    }

    #[test]
    fn a_nonsense_active_index_is_ignored_rather_than_fatal() {
        // A hand-edited or truncated registry must not take the app down; the
        // caller falls back to the legacy location, where the books still are.
        let reg = LibraryRegistry {
            libraries: vec![LibraryEntry {
                name: "A".into(),
                path: "/books/A".into(),
            }],
            active: Some(7),
        };
        assert_eq!(reg.active(), None);
    }

    #[test]
    fn the_registry_survives_a_round_trip_through_json() {
        // It is the only record of where a user's libraries are.
        let reg = reg_with(&["Fiction", "Research"]);
        let json = serde_json::to_string(&reg).unwrap();
        let back: LibraryRegistry = serde_json::from_str(&json).unwrap();
        assert_eq!(reg, back);
    }

    #[test]
    fn an_older_registry_without_every_field_still_loads() {
        // `#[serde(default)]` on both fields. A registry written by a future
        // version that gained a field must not brick an older build, and vice
        // versa -- the alternative is a user who cannot open their books.
        let back: LibraryRegistry = serde_json::from_str("{}").unwrap();
        assert_eq!(back, LibraryRegistry::default());

        let partial = r#"{"libraries":[{"name":"A","path":"/books/A"}]}"#;
        let back: LibraryRegistry = serde_json::from_str(partial).unwrap();
        assert_eq!(back.libraries.len(), 1);
        assert_eq!(back.active, None, "no active field means nothing is open");
    }
}

/// Does this folder already hold a Kalam library?
///
/// Judged by `catalog.db`, the one file a library cannot exist without. Not by
/// the folder being non-empty: picking `~/Documents` by mistake should not be
/// silently treated as "an existing library", and picking an empty folder is
/// the normal way to start a new one.
pub fn looks_like_a_library(path: &Path) -> bool {
    path.join("catalog.db").is_file()
}

/// On first run, put the existing library into the list.
///
/// Before P6.5 there was exactly one library, in a fixed place, and the app
/// still falls back to it when nothing is selected. That fallback works, but
/// it leaves the user's own books as the one library that does not appear in
/// the list — so "Forget" and "Open" would apply to every library except
/// theirs, and adding a second would make the first seem to vanish.
///
/// Called once at startup. Does nothing if the registry already has entries,
/// or if there is no old library to adopt (a genuinely new install).
///
/// Nothing is moved or copied. The folder stays exactly where it is; only the
/// list learns about it.
pub fn adopt_legacy_library_if_needed() {
    let mut reg = load_registry();
    if !reg.libraries.is_empty() {
        return;
    }
    let legacy = crate::paths::legacy_data_dir();
    if !looks_like_a_library(&legacy) {
        return;
    }
    reg.add_or_select("My library", &legacy);
    if let Err(err) = save_registry(&reg) {
        // Not fatal: without a registry the app falls back to this very
        // folder anyway, so the user still sees their books. They just will
        // not see the library listed until the write succeeds.
        eprintln!("kalam: could not record the existing library: {err}");
    }
}

/// Is the selected library actually there?
///
/// Returns the missing path when a library is selected but its folder has
/// gone — an unplugged drive, an unmounted share, a folder renamed or deleted
/// outside the app.
///
/// **Why this needs checking rather than being left to fail naturally.** It
/// would not fail: `ensure_data_dirs()` calls `create_dir_all`, so the folder
/// is silently recreated, and `Catalog::open` then builds a fresh empty
/// database inside it. The user sees an empty library and reasonably concludes
/// their books are gone, when the drive is merely unplugged. Worse, the
/// recreated folder now *looks* like a real library, so plugging the drive
/// back in does not obviously fix anything.
///
/// A library that has never been opened is not missing — `looks_like_a_library`
/// is false for an empty folder the user just chose, and that is a normal
/// state, not an error.
pub fn missing_active_library() -> Option<PathBuf> {
    let reg = load_registry();
    let active = reg.active()?;
    if active.path.is_dir() {
        return None;
    }
    Some(active.path.clone())
}

/// Restart Kalam so a newly selected library takes effect.
///
/// **Why restart rather than switch in place.** `paths::data_dir()` caches the
/// active library for the life of the process, and deliberately so: pages hold
/// open database handles, in-flight cover decodes and half-built widget trees
/// that all assume one library. Repointing mid-session would leave some of
/// them reading the old library and writing to the new one, which is the kind
/// of bug that corrupts data rather than merely looking wrong. A restart is a
/// second of waiting and cannot be subtly wrong.
///
/// Returns only on failure — on success this process has been replaced.
///
/// `exec` rather than spawn-then-quit: spawning leaves two Kalams alive at
/// once, both with the catalog open, for as long as the old one takes to shut
/// down. `exec` replaces the image, so there is never a second instance and
/// the window manager keeps the same process.
#[cfg(unix)]
pub fn restart_now() -> std::io::Error {
    use std::os::unix::process::CommandExt;

    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => return e,
    };
    // Skip argv[0]; `Command` supplies it.
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    std::process::Command::new(exe).args(args).exec()
}

/// Non-Unix has no `exec`; Kalam is Linux-only, so this is only here to keep
/// the call site honest rather than hidden behind a `cfg` at every use.
#[cfg(not(unix))]
pub fn restart_now() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "restarting is only implemented on Unix",
    )
}

// ---------------------------------------------------------------------------
// Global preferences — the settings that must NOT change when you switch
// ---------------------------------------------------------------------------

/// Is this preference about the *app*, or about *these books*?
///
/// `app_prefs` lives in `catalog.db`, which lives inside a library. That was
/// fine when there was one library and is wrong now: switching would silently
/// reset your theme, your reader font size, your dictionary settings and your
/// API keys, because the new library's database has never heard of them.
///
/// So prefs are split by key. Global ones are mirrored into
/// `~/.config/kalam/prefs.json`; everything else stays in the library where it
/// belongs.
///
/// **Default is per-library**, deliberately. A new pref that should have been
/// global is a mild annoyance the user can fix by setting it again. A new pref
/// that should have been per-library but leaked to global silently applies one
/// library's setting to another, which is data-shaped confusion and much
/// harder to notice.
pub fn is_global_pref(key: &str) -> bool {
    // Every name here was read out of the code rather than guessed. Three of
    // them are not what you would predict -- the app theme is `ui.theme`, the
    // EPUB writeback flag is `epub.write_metadata`, and the bundled-dictionary
    // markers are `bundled_dictionary_*`. Worth stating, because a key that is
    // *nearly* right silently classifies as per-library and the setting
    // quietly stops following the user between libraries.
    if matches!(
        key,
        "ui.theme" | "dict_history_enabled" | "dict_sense_hint" | "epub.write_metadata"
    ) {
        return true;
    }
    // Prefixes, so new reader settings, new bundled dictionaries and new
    // metadata or source providers are global without anyone having to
    // remember to come back here.
    key.starts_with("reader.")
        || key.starts_with("bundled_dictionary_")
        || key.starts_with("meta.")
        || key.starts_with("source.")
}

/// `~/.config/kalam/prefs.json`
pub fn global_prefs_path() -> PathBuf {
    config_dir().join("prefs.json")
}

/// Read every global preference. Missing or corrupt file gives an empty map,
/// for the same reason as the registry: never refuse to start.
pub fn load_global_prefs() -> std::collections::BTreeMap<String, String> {
    if skip_global_prefs_in_tests() {
        return Default::default();
    }
    let Ok(text) = std::fs::read_to_string(global_prefs_path()) else {
        return Default::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

/// Write one global preference, preserving the rest.
///
/// Read-modify-write rather than holding the map in memory: prefs change
/// rarely and by hand, and a cached copy is one more thing that can go stale
/// against a second window.
pub fn set_global_pref(key: &str, value: &str) -> std::io::Result<()> {
    if skip_global_prefs_in_tests() {
        // Never write into a real home directory from a test run.
        return Ok(());
    }
    let mut all = load_global_prefs();
    all.insert(key.to_string(), value.to_string());

    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    let final_path = global_prefs_path();
    let tmp = final_path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(&all)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, &final_path)?;
    Ok(())
}

#[cfg(test)]
mod global_pref_tests {
    use super::*;

    #[test]
    fn app_settings_are_shared_between_libraries() {
        // These describe the app, not the books. If they were per-library,
        // switching would silently reset your theme and reader settings --
        // the user would see it as the app forgetting its configuration.
        // Exactly the keys the code really uses -- checked against
        // theme.rs, epub_metadata.rs, dict.rs, reader.rs and settings.rs, not
        // guessed. Three are counter-intuitive and were wrong on the first
        // attempt: `ui.theme`, `epub.write_metadata`, `bundled_dictionary_*`.
        for key in [
            "ui.theme",
            "reader.theme",
            "reader.font_px",
            "reader.line_height",
            "reader.column_px",
            "dict_history_enabled",
            "dict_sense_hint",
            "epub.write_metadata",
            "meta.googlebooks.key",
            "meta.googlebooks.country",
            "bundled_dictionary_english_wordnet_2025",
            "bundled_dictionary_english_idioms_2024",
            "source.ao3.enabled",
        ] {
            assert!(
                is_global_pref(key),
                "{key} should be shared, not per-library"
            );
        }
    }

    #[test]
    fn things_about_these_books_stay_with_these_books() {
        // A reading goal belongs to a library: 50 books in your fiction
        // library and 12 in your research one is the point of having two.
        for key in [
            "goal.books_per_year",
            "thumbs.backfill_done_count",
            "last_opened_book",
        ] {
            assert!(!is_global_pref(key), "{key} should stay with its library");
        }
    }

    #[test]
    fn an_unrecognised_preference_stays_with_its_library() {
        // The default matters more than it looks. A new pref that should have
        // been global is a mild annoyance -- set it again. A new pref that
        // leaks to global silently applies one library's setting to another,
        // which is much harder to notice and looks like corruption.
        assert!(!is_global_pref("something.invented.tomorrow"));
        assert!(!is_global_pref(""));
    }

    #[test]
    fn the_prefixes_do_not_catch_more_than_they_should() {
        // `reader.` is a prefix rule, so check it cannot swallow a key that
        // merely starts with the same letters.
        assert!(is_global_pref("reader.font_px"));
        assert!(!is_global_pref("readership_count"));
        assert!(!is_global_pref("reader"));
        assert!(is_global_pref("meta.openlibrary.enabled"));
        assert!(!is_global_pref("metadata_dirty"));
        // And the near-miss that started this: `theme` alone is NOT the app
        // theme -- the real key is `ui.theme`.
        assert!(!is_global_pref("theme"));
    }

    #[test]
    fn a_library_folder_that_is_present_is_not_reported_missing() {
        // The check exists because a missing folder does not fail naturally:
        // `create_dir_all` would recreate it and the app would build an empty
        // database inside, so the user's books appear to have vanished when a
        // drive is merely unplugged. Only the "present" half can be tested
        // without touching the real registry -- the missing half needs a
        // registry on disk, which tests deliberately do not write.
        let base = std::env::temp_dir().join(format!("kalam-present-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let entry = LibraryEntry {
            name: "Here".into(),
            path: base.clone(),
        };
        assert!(entry.path.is_dir(), "a present folder must read as present");

        let gone = base.join("not-there");
        let entry = LibraryEntry {
            name: "Gone".into(),
            path: gone,
        };
        assert!(!entry.path.is_dir(), "a missing folder must read as missing");

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_library_is_recognised_by_its_catalog_file() {
        // `catalog.db` is the one file a library cannot exist without.
        // Deliberately not "the folder is non-empty": picking ~/Documents by
        // mistake must not be treated as an existing library, and picking an
        // empty folder is the normal way to start a new one.
        let base = std::env::temp_dir().join(format!("kalam-libtest-{}", std::process::id()));
        let empty = base.join("empty");
        let real = base.join("real");
        let busy = base.join("busy");
        for d in [&empty, &real, &busy] {
            std::fs::create_dir_all(d).unwrap();
        }
        std::fs::write(real.join("catalog.db"), b"x").unwrap();
        std::fs::write(busy.join("notes.txt"), b"x").unwrap();

        assert!(looks_like_a_library(&real));
        assert!(
            !looks_like_a_library(&empty),
            "an empty folder starts a new library"
        );
        assert!(
            !looks_like_a_library(&busy),
            "a folder with unrelated files is not a library"
        );
        assert!(!looks_like_a_library(&base.join("does-not-exist")));

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn tests_never_touch_the_real_config_file() {
        // This is load-bearing. Without it, every test that reads a global
        // pref reads the developer's own ~/.config/kalam/prefs.json, and the
        // suite passes or fails depending on whose machine it runs on
        // (pitfalls §19). It broke a pre-existing service test the moment
        // `dict_history_enabled` became global.
        assert!(
            skip_global_prefs_in_tests(),
            "global prefs must be inert under cargo test"
        );
        assert!(
            load_global_prefs().is_empty(),
            "a test run must see no global prefs, whatever is on this machine"
        );
        // ...and writing must be a no-op rather than editing a real home dir.
        assert!(set_global_pref("ui.theme", "whatever").is_ok());
        assert!(load_global_prefs().is_empty());
    }
}
