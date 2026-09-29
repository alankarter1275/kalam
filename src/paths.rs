//! XDG paths for Kalam data and config.

use std::fs;
use std::path::{Path, PathBuf};

/// Where the **active library** lives — the root every other path hangs off.
///
/// P6.5: this used to be a fixed location. It now asks
/// `crate::libraries` which library is open and returns that folder, falling
/// back to the fixed location when no library has been chosen (first run, or
/// an unreadable registry). Because ~30 helpers below are built on this one
/// function, making libraries switchable was a change here rather than a
/// sweep through the app.
///
/// The result is cached for the life of the process. Switching library
/// re-launches rather than swapping underneath a running UI: pages hold open
/// database handles and half-drawn covers, and repointing this mid-session
/// would leave them reading from one library and writing to another.
pub fn data_dir() -> PathBuf {
    static ACTIVE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    ACTIVE
        .get_or_init(|| {
            crate::libraries::load_registry()
                .active()
                .map(|lib| lib.path.clone())
                .unwrap_or_else(legacy_data_dir)
        })
        .clone()
}

/// The pre-P6.5 fixed location, `~/.local/share/kalam`.
///
/// Still the default when no library has been chosen, so an existing install
/// keeps working untouched and an upgrade is a no-op until the user asks for
/// something else.
pub fn legacy_data_dir() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            home_dir()
                .map(|h| h.join(".local/share"))
                .unwrap_or_else(|| {
                    // Neither `$HOME` nor the password database produced a
                    // home. Starting up in the current directory still beats
                    // refusing to run, but this used to happen silently —
                    // which is how a library ends up somewhere its owner
                    // cannot find. Say where it went.
                    log::error!(
                        "no home directory found; storing the library in the current directory ({})",
                        std::env::current_dir()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|_| "unknown".into())
                    );
                    PathBuf::from(".")
                })
        });
    base.join("kalam")
}

/// `~/.local/share/kalam/catalog.db`
pub fn catalog_db() -> PathBuf {
    data_dir().join("catalog.db")
}

/// `~/.local/share/kalam/library`
pub fn library_dir() -> PathBuf {
    data_dir().join("library")
}

/// How many bytes of a book folder name may come from the author and title.
///
/// Linux allows 255 bytes per name. The allowance below (author + title +
/// separator + the short id suffix) stays far under that even when every
/// character is three bytes of Bengali or Japanese, so a long translated
/// title can never produce a folder the file system rejects.
const FOLDER_PREFIX_MAX_BYTES: usize = 120;

/// Build the human-readable folder name for a book.
///
/// Owner decision, 2026-09-29: a folder named only by a uuid tells you
/// nothing when you browse your library with a file manager. Folders are
/// now named `Author - Title <short-id>` (or `Title <short-id>` when there
/// is no author). The uuid stays the book's key inside the database — the
/// name is a label, and the app never parses it to find anything.
///
/// The short id is the first 8 characters of the uuid. It is kept at the
/// end because author + title alone can collide (two editions, two books
/// with the same name); eight hex characters make a collision practically
/// impossible, and on the rare clash the folder gets the full uuid instead
/// (see [`book_folder_name_full`]).
pub fn book_folder_name(authors: &str, title: &str, uuid: &str) -> String {
    folder_name_with_suffix(authors, title, uuid_suffix(uuid))
}

/// Same as [`book_folder_name`], but ending in the full uuid. Used when the
/// short form would collide with a folder that already exists.
pub fn book_folder_name_full(authors: &str, title: &str, uuid: &str) -> String {
    folder_name_with_suffix(authors, title, uuid)
}

fn folder_name_with_suffix(authors: &str, title: &str, suffix: &str) -> String {
    // "Unknown" is what the importer writes for PDFs and comics that carry
    // no author. Treating it as no-author keeps those folders clean
    // ("Nausicaä 3f2ab91c", not "Unknown - Nausicaä 3f2ab91c") — the owner
    // picked exactly that shape for authorless books.
    //
    // The byte cap is 60 because Bengali and Japanese names are three bytes
    // per character: 48 bytes cut "রবীন্দ্রনাথ ঠাকুর" (Rabindranath Tagore,
    // 49 bytes) mid-name — found by a CI test, not by review.
    let authors = sanitize_folder_text(authors, 60);
    let has_author = !authors.is_empty() && !authors.eq_ignore_ascii_case("unknown");

    let title = sanitize_folder_text(title, 64);
    let title = if title.is_empty() { "Untitled" } else { title.as_str() };

    let prefix = if has_author {
        format!("{authors} - {title}")
    } else {
        title.to_string()
    };
    // Leave room for " {suffix}" whatever the character widths, then cut on
    // a character boundary so the name never ends mid-letter.
    let room = FOLDER_PREFIX_MAX_BYTES.saturating_sub(suffix.len() + 1);
    let prefix = truncate_bytes(&prefix, room);
    format!("{prefix} {suffix}")
}

/// The short id used at the end of a folder name: the first 8 characters of
/// the uuid, or the whole uuid when it is shorter.
fn uuid_suffix(uuid: &str) -> &str {
    uuid.get(0..8).unwrap_or(uuid)
}

/// Make a piece of metadata safe to use inside one folder name.
///
/// Turns whitespace runs, `/` (the one character Linux forbids) and
/// invisible control characters into single spaces — a separator most
/// likely stood where they were, and dropping it would glue two words
/// together ("A/B" becoming "AB"). Drops leading dots (a leading dot would
/// hide the folder in most file managers) and trailing dots and spaces,
/// and caps the length in bytes. An empty result is returned as-is;
/// callers decide their own fallback.
fn sanitize_folder_text(raw: &str, max_bytes: usize) -> String {
    let mut out = String::with_capacity(raw.len().min(max_bytes + 4));
    let mut last_was_space = true; // also eats leading spaces
    for ch in raw.chars() {
        // Whitespace first, deliberately: a tab *is* a control character,
        // and checking control first would swallow it whole — turning
        // "A\tB" into "AB" instead of "A B".
        if ch.is_whitespace() {
            if !last_was_space {
                out.push(' ');
                last_was_space = true;
            }
            continue;
        }
        if ch.is_control() || ch == '/' {
            // Same reasoning as above, one step further: the character
            // goes, the word break it stood for stays.
            if !last_was_space {
                out.push(' ');
                last_was_space = true;
            }
            continue;
        }
        out.push(ch);
        last_was_space = false;
    }
    let trimmed = out.trim_matches(['.', ' ']).to_string();
    truncate_bytes(&trimmed, max_bytes)
}

/// Shorten to `max_bytes` without splitting a character. Standard library
/// byte slicing panics mid-character, and a panic here would take the app
/// down over a long book title.
fn truncate_bytes(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text.get(0..end).unwrap_or_default().to_string()
}

/// What the folder scan found: folder names by short id (8 hex characters)
/// and by full uuid. Two maps because a folder name ends in one or the
/// other, never both.
#[derive(Default)]
struct FolderIndex {
    by_short_id: std::collections::HashMap<String, String>,
    by_full_uuid: std::collections::HashMap<String, String>,
}

/// Folder names discovered so far, shared by every thread. Built once by
/// the first `book_dir` call that needs it, then kept up to date by
/// [`note_folder`] as books are imported and folders renamed.
static FOLDER_INDEX: std::sync::OnceLock<std::sync::Mutex<Option<FolderIndex>>> =
    std::sync::OnceLock::new();

/// Where one book's files live.
///
/// Used to be `library/<uuid>` and nothing else. Now folders carry readable
/// names (`library/Hayao Miyazaki - Nausicaä 3f2ab91c/`), so this resolves
/// the uuid to the real folder name through an in-memory index — one scan
/// of the library directory per app run, no disk access per lookup. That
/// matters because the library grid asks for every book's file path at
/// once, and a stat call per book is measurable on a spinning disk.
///
/// Resolution order:
///
/// 1. The in-memory index (covers readable names, old uuid names alike).
/// 2. Fall back to `library/<uuid>` — the shape every folder had before
///    this change, and still what a not-yet-imported uuid resolves to.
///    Callers get the same "missing file" behaviour they always had.
pub fn book_dir(uuid: &str) -> PathBuf {
    let lib = library_dir();
    match resolve_library_folder(uuid) {
        Some(name) => lib.join(name),
        None => lib.join(uuid),
    }
}

/// Tell the resolver about a folder it should know: a freshly imported
/// book, or a folder that was just renamed. Cheap and safe to skip — the
/// next full scan (next app run) finds the folder anyway; this only keeps
/// the current session from needing that.
///
/// Unlike the scan, this knows the uuid the folder belongs to, so it
/// records the pair directly — no parsing, no ambiguity.
pub fn note_folder(uuid: &str, folder_name: &str) {
    let lock = FOLDER_INDEX.get_or_init(|| std::sync::Mutex::new(None));
    let mut guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(index) = guard.as_mut() {
        index.by_full_uuid.insert(uuid.to_string(), folder_name.to_string());
    }
}

fn resolve_library_folder(uuid: &str) -> Option<String> {
    let lock = FOLDER_INDEX.get_or_init(|| std::sync::Mutex::new(None));
    let mut guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if guard.is_none() {
        *guard = Some(scan_library_folders(&library_dir()));
    }
    let index = guard.as_ref()?;
    if let Some(name) = index.by_full_uuid.get(uuid) {
        return Some(name.clone());
    }
    index.by_short_id.get(uuid_suffix(uuid)).cloned()
}

/// Read the library directory once and index every folder that ends in an
/// id we can resolve: either 8 hex characters (the normal short id) or a
/// full uuid (the collision-avoiding form). Folders that end in neither —
/// a user's own folders, anything foreign — are ignored, not an error.
fn scan_library_folders(lib: &Path) -> FolderIndex {
    let mut index = FolderIndex::default();
    let Ok(entries) = fs::read_dir(lib) else {
        return index;
    };
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let Ok(name) = entry.file_name().into_string() else {
            continue; // not valid UTF-8: cannot be one of ours
        };
        index_insert(&mut index, &name);
    }
    index
}

/// Add one folder name to the index if it ends in a resolvable id. When two
/// different folders claim the same id, the id becomes ambiguous and is
/// dropped: guessing between two books is worse than an honest miss.
fn index_insert(index: &mut FolderIndex, folder_name: &str) {
    let Some(id) = folder_name.rsplit(' ').next() else {
        return;
    };
    let looks_like_full_uuid = id.len() == 36
        && id.matches('-').count() == 4
        && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
    let looks_like_short_id = id.len() == 8 && id.chars().all(|c| c.is_ascii_hexdigit());

    let (map, key) = if looks_like_full_uuid {
        (&mut index.by_full_uuid, id)
    } else if looks_like_short_id {
        (&mut index.by_short_id, id)
    } else {
        return;
    };
    match map.get(key) {
        Some(existing) if existing == folder_name => {} // already known
        Some(_) => {
            // Two folders, one id. Drop it rather than pick a winner.
            log::debug!("ambiguous folder id {id:?}: {folder_name:?} vs another folder");
            map.remove(key);
        }
        None => {
            map.insert(key.to_string(), folder_name.to_string());
        }
    }
}

/// Extracted EPUB cache: `cache/reader/<uuid>`, the unzipped copy that
/// `epub_book::OpenBook` reads spine titles and chapter bodies out of.
///
/// Not the reader page's own storage — the engine widget opens the `.epub`
/// itself. This is what the book, library and remote-detail pages use to list
/// chapters without unzipping the same file again.
pub fn reader_cache_dir(uuid: &str) -> PathBuf {
    data_dir().join("cache").join("reader").join(uuid)
}

/// `~/.local/share/kalam/covers` — covers kept for remembered metadata.
///
/// Separate from `library/<uuid>/` because that directory is deleted with the
/// book; these must outlive it so a re-import can restore the chosen cover.
pub fn override_covers_dir() -> PathBuf {
    data_dir().join("covers")
}

/// Cached author photos.
///
/// Per-library, and that is a judgement call rather than an obvious one: these
/// are fetched from the internet and would be identical in every library, so
/// sharing them would save a little disk and a few refetches. They stay with
/// the library because they are a *cache of this library's authors* — deleting
/// a library should take its cached photos with it, and a shared folder would
/// accumulate photos for authors nobody owns any more with nothing to prune
/// it. Same reasoning for `series_covers_dir`. Cheap to revisit: both are
/// caches, so moving them loses nothing but a refetch.
///
pub fn authors_dir() -> PathBuf {
    data_dir().join("authors")
}

/// `~/.local/share/kalam/series-covers` — covers for remote series works,
/// fetched with the series cache and named by their Open Library cover id.
pub fn series_covers_dir() -> PathBuf {
    data_dir().join("series-covers")
}

/// Dictionaries — **shared by every library**, not stored inside one.
///
/// P6.5: this used to hang off `data_dir()`, which is now the *active
/// library*. Left that way, switching library would look for dictionaries in
/// the new folder, find none, and reinstall the bundled packs — about 4 MB
/// compressed and a few seconds of work — once per library, for ever. A
/// dictionary is a property of the installation, not of a shelf of books.
///
/// Lives under the shared root so an existing install keeps the packs it
/// already has: the path is unchanged for anyone who has never switched
/// library.
pub fn dictionaries_dir() -> PathBuf {
    shared_data_dir().join("dictionaries")
}

/// `~/.local/share/kalam/fonts` — typefaces the reader loads alongside the
/// bundled ones, with no system-wide install (roadmap 2.8).
///
/// Shared rather than per-library for the reason [`dictionaries_dir`] gives:
/// a typeface belongs to the installation, not to a shelf of books, and a
/// reader who switches library should not have to copy their fonts.
pub fn fonts_dir() -> PathBuf {
    shared_data_dir().join("fonts")
}

/// The root for things every library shares.
///
/// Always the classic location, never the active library. Kept separate from
/// `legacy_data_dir()` by name even though they resolve alike today, because
/// they answer different questions — "where does shared state live" versus
/// "where did the single library used to live" — and a future change to one
/// should not silently move the other.
pub fn shared_data_dir() -> PathBuf {
    legacy_data_dir()
}

/// `~/.local/share/kalam/kalam.log`
///
/// In the shared location rather than the active library's folder: a log is
/// about the program, not about one collection of books, so it stays in the
/// same place whichever library is open. Only written when `RUST_LOG` is set —
/// see `crate::logging`.
pub fn log_file() -> PathBuf {
    shared_data_dir().join("kalam.log")
}

/// `~/.local/share/kalam/cache/thumbs` — persistent cover thumbnails (A0 step 3).
///
/// Unlike the in-memory `COVER_CACHE` (which dies at relaunch), these stay on
/// disk so a relaunched library grid decodes a tiny 256×408 PNG instead of the
/// full cover on every open.
pub fn thumbs_dir() -> PathBuf {
    data_dir().join("cache").join("thumbs")
}

/// Thumbnail path for a book's uuid.
pub fn thumbnail_path(uuid: &str) -> PathBuf {
    thumbs_dir().join(format!("{uuid}.png"))
}

/// Derive the thumbnail path for a *library* cover path.
///
/// Covers live at `library/<uuid>/cover.ext`, so the parent directory name is
/// the uuid. Returns `None` for any path that is not a library cover (e.g. a
/// stashed override or a remote series cover) — those simply decode full.
pub fn thumbnail_for_cover(cover: &Path) -> Option<PathBuf> {
    let uuid = cover.parent()?.file_name()?.to_str()?;
    Some(thumbnail_path(uuid))
}

/// The user's home directory.
///
/// Roadmap 1.7. This used to read `$HOME` and nothing else, and every one of
/// its dozen-odd callers fell back to `"."` — so with `HOME` unset (cron, a
/// systemd unit, `sudo` without `-E`, a minimal container) Kalam created its
/// entire library in whatever directory it happened to be started from,
/// silently. Reading the password database is what every other tool on Linux
/// does, and it means "no home directory" now describes a machine that
/// genuinely has none rather than one where an environment variable was
/// missing.
///
/// Split into [`resolve_home`] so the precedence is testable without mutating
/// the process environment, which is not safe while other tests are running.
pub(crate) fn home_dir() -> Option<PathBuf> {
    resolve_home(std::env::var_os("HOME").as_deref())
}

/// `$HOME` when it is set and non-empty, otherwise the password database.
fn resolve_home(env_home: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    // An empty `$HOME` counts as unset. `PathBuf::from("")` joins to a
    // relative path, which is the original bug wearing a different hat.
    if let Some(h) = env_home {
        if !h.is_empty() {
            return Some(PathBuf::from(h));
        }
    }
    passwd_home_dir()
}

/// The home directory from the password database, via `getpwuid_r`.
///
/// This is the lookup `$HOME` is supposed to mirror, so going to the source is
/// what makes the function work when the variable is missing. The `_r` form
/// is not optional pedantry: plain `getpwuid` fills a static buffer, and
/// `home_dir` is reachable from worker threads now that pages query on
/// background tasks — concurrent calls would race over that buffer.
fn passwd_home_dir() -> Option<PathBuf> {
    // SAFETY: `getuid` takes no arguments and cannot fail.
    let uid = unsafe { libc::getuid() };

    // Scratch space libc writes into: `pwd` receives the struct, `buf` the
    // strings its fields point at. `MaybeUninit` rather than `zeroed` because
    // a failed lookup leaves `pwd` untouched and reading it would be a lie.
    let mut buf = vec![0u8; 4096];
    let mut pwd = std::mem::MaybeUninit::<libc::passwd>::uninit();
    let mut result: *mut libc::passwd = std::ptr::null_mut();

    // SAFETY: `pwd.as_mut_ptr()` is a valid writable `passwd` and `buf` a
    // valid writable buffer of exactly `buf.len()` bytes; both outlive the
    // call, which writes only within them. `result` is set to either a pointer
    // into `pwd` or null. Nothing escapes the function — `pw_dir` is copied
    // into an owned `PathBuf` below, before `buf` and `pwd` are dropped.
    let rc = unsafe {
        libc::getpwuid_r(
            uid,
            pwd.as_mut_ptr(),
            buf.as_mut_ptr().cast::<libc::c_char>(),
            buf.len(),
            &mut result,
        )
    };

    // `rc != 0` is a lookup error; a null `result` means no entry for this
    // uid. Either way `pwd` may be uninitialised, so it must not be read.
    if rc != 0 || result.is_null() {
        return None;
    }

    // SAFETY: a non-null `result` means libc initialised `pwd` and pointed
    // `result` at it, and both are still alive here. `pw_dir` is a
    // NUL-terminated C string owned by libc — copied, never freed.
    let dir = unsafe { (*result).pw_dir };
    if dir.is_null() {
        return None;
    }
    // SAFETY: `dir` is a valid NUL-terminated C string per `passwd`'s contract.
    //
    // Taken as raw bytes rather than `to_string_lossy`: a Unix path is bytes
    // and not necessarily UTF-8, and a lossy conversion would swap an unusual
    // byte for U+FFFD and hand back a path that does not exist.
    use std::os::unix::ffi::OsStrExt;
    let bytes = unsafe { std::ffi::CStr::from_ptr(dir) }.to_bytes();
    let path = PathBuf::from(std::ffi::OsStr::from_bytes(bytes));
    if path.as_os_str().is_empty() {
        None
    } else {
        Some(path)
    }
}

pub fn ensure_data_dirs() -> std::io::Result<()> {
    // Per-library: these describe *these books* and live inside the library.
    fs::create_dir_all(data_dir())?;
    fs::create_dir_all(library_dir())?;
    fs::create_dir_all(data_dir().join("cache").join("reader"))?;
    fs::create_dir_all(thumbs_dir())?;
    fs::create_dir_all(override_covers_dir())?;
    fs::create_dir_all(authors_dir())?;
    fs::create_dir_all(series_covers_dir())?;

    // Shared: installed once, used by every library. See `dictionaries_dir`.
    fs::create_dir_all(dictionaries_dir())?;
    // Created empty and scanned on every open; a reader who never drops a
    // font in pays one empty directory read.
    fs::create_dir_all(fonts_dir())?;
    Ok(())
}

/// `~/.local/share/kalam/cache/reader`
pub fn reader_cache_root() -> PathBuf {
    data_dir().join("cache").join("reader")
}

/// Total bytes held by extracted-EPUB caches.
pub fn reader_cache_size() -> u64 {
    dir_size(&reader_cache_root())
}

/// Delete every extracted book cache. They are rebuilt on next open.
pub fn clear_reader_cache() -> (usize, u64) {
    let root = reader_cache_root();
    let freed = dir_size(&root);
    let mut removed = 0;
    if let Ok(entries) = fs::read_dir(&root) {
        for entry in entries.flatten() {
            if fs::remove_dir_all(entry.path()).is_ok() {
                removed += 1;
            }
        }
    }
    (removed, freed)
}

/// Drop caches for books no longer in the library, and any older than
/// `max_age_days`. Called at startup so the cache cannot grow forever.
pub fn prune_reader_cache(live_uuids: &[String], max_age_days: u64) -> u64 {
    use std::time::{Duration, SystemTime};

    let root = reader_cache_root();
    let Ok(entries) = fs::read_dir(&root) else {
        return 0;
    };
    let max_age = Duration::from_secs(max_age_days * 24 * 60 * 60);
    let now = SystemTime::now();
    let mut freed = 0;

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        // A cache for a deleted book is dead weight regardless of age.
        let orphaned = !live_uuids.iter().any(|u| u == name);
        let stale = entry
            .metadata()
            .and_then(|m| m.accessed().or_else(|_| m.modified()))
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age > max_age);

        if orphaned || stale {
            let size = dir_size(&path);
            if fs::remove_dir_all(&path).is_ok() {
                freed += size;
            }
        }
    }
    freed
}

fn dir_size(path: &std::path::Path) -> u64 {
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => dir_size(&e.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    /// A scratch library directory, unique per test so parallel tests
    /// cannot see each other's folders.
    ///
    /// Placed above the first `#[test]`, like every helper in this repo:
    /// the guardrail that counts production `.unwrap()`s suppresses the
    /// whole `mod tests` only down to the first nested test item, so a
    /// helper with an `.expect()` that sits between test functions would
    /// be miscounted as production code.
    fn scratch_library(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "kalam-paths-test-{tag}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    // These test `resolve_home` rather than `home_dir` on purpose: the
    // precedence is the part with logic in it, and driving it through
    // `std::env::set_var` would race every other test in the binary, since
    // the environment is process-wide and tests run in parallel.

    #[test]
    fn resolve_home_prefers_the_environment_value() {
        let got = resolve_home(Some(OsStr::new("/srv/books")));
        assert_eq!(got.as_deref(), Some(Path::new("/srv/books")));
    }

    #[test]
    fn resolve_home_does_not_second_guess_a_relative_home() {
        // A relative `$HOME` is almost certainly a mistake, but overriding it
        // would mean disagreeing with whatever set it. Trust the environment;
        // the fallback exists for a *missing* value, not a surprising one.
        let got = resolve_home(Some(OsStr::new("relative/home")));
        assert_eq!(got.as_deref(), Some(Path::new("relative/home")));
    }

    #[test]
    fn resolve_home_treats_an_empty_home_as_unset() {
        // The case the fix is for: `PathBuf::from("")` joins to a relative
        // path, so returning it would reproduce the original bug in a
        // different shape. Empty must fall through to the password database.
        let got = resolve_home(Some(OsStr::new("")));
        // `as_ref` so the assertion borrows rather than consumes `got`, which
        // the failure message below still needs.
        assert!(
            got.as_ref().is_some_and(|p| !p.as_os_str().is_empty()),
            "an empty $HOME should fall through to getpwuid_r, got {got:?}"
        );
    }

    #[test]
    fn resolve_home_falls_back_to_the_password_database() {
        // The uid running the tests has a passwd entry on the platforms Kalam
        // targets. This asserts the fallback works at all. It deliberately
        // does not assert the value, or that it is absolute: both depend on
        // the machine, and a test that fails only on someone else's laptop
        // costs more than it checks.
        assert!(
            resolve_home(None).is_some(),
            "getpwuid_r returned no home for the current uid"
        );
    }

    #[test]
    fn folder_scan_resolves_short_ids_and_legacy_uuids() {
        let lib = scratch_library("scan");
        let readable = "Hayao Miyazaki - Nausicaä 3f2ab91c";
        let legacy = "9c8b7a65-4321-4321-8765-ba9876543210";
        std::fs::create_dir_all(lib.join(readable)).expect("readable");
        std::fs::create_dir_all(lib.join(legacy)).expect("legacy");

        let index = scan_library_folders(&lib);
        // The readable folder is found by the short id at its end.
        assert_eq!(
            index.by_short_id.get("3f2ab91c").map(String::as_str),
            Some(readable)
        );
        // The old bare-uuid folder is found by its uuid.
        assert_eq!(
            index.by_full_uuid.get(legacy).map(String::as_str),
            Some(legacy)
        );

        let _ = std::fs::remove_dir_all(&lib);
    }

    #[test]
    fn folder_scan_ignores_folders_without_an_id() {
        let lib = scratch_library("foreign");
        std::fs::create_dir_all(lib.join("My Reading Notes")).expect("foreign");
        std::fs::create_dir_all(lib.join("backup 2024")).expect("dated");
        // A file, not a folder: skipped without being opened.
        std::fs::write(lib.join("readme.txt"), b"x").expect("file");

        let index = scan_library_folders(&lib);
        assert!(index.by_short_id.is_empty());
        assert!(index.by_full_uuid.is_empty());

        let _ = std::fs::remove_dir_all(&lib);
    }

    #[test]
    fn folder_scan_drops_ambiguous_ids_rather_than_guessing() {
        let lib = scratch_library("ambiguous");
        std::fs::create_dir_all(lib.join("Book One 3f2ab91c")).expect("one");
        std::fs::create_dir_all(lib.join("Book Two 3f2ab91c")).expect("two");

        let index = scan_library_folders(&lib);
        // Two folders claim "3f2ab91c". Rather than silently picking one
        // — which could hand one book another book's files — the id is
        // dropped and the caller falls back to the plain uuid path.
        assert!(!index.by_short_id.contains_key("3f2ab91c"));

        let _ = std::fs::remove_dir_all(&lib);
    }

    #[test]
    fn note_folder_and_invalidations_keep_the_index_honest() {
        // index_insert is the whole logic behind note_folder; testing it
        // directly keeps this test independent of the process-wide cache.
        let mut index = FolderIndex::default();
        index_insert(&mut index, "Author - Title 11112222");
        assert_eq!(
            index.by_short_id.get("11112222").map(String::as_str),
            Some("Author - Title 11112222")
        );
        // Noting the same folder twice is harmless.
        index_insert(&mut index, "Author - Title 11112222");
        assert_eq!(index.by_short_id.len(), 1);
        // A conflicting folder with the same id makes it ambiguous.
        index_insert(&mut index, "Other Book 11112222");
        assert!(!index.by_short_id.contains_key("11112222"));
        // The full-uuid form is indexed separately and never conflicts
        // with the short form.
        index_insert(&mut index, "Author - Title 99998888-7777-4666-8555-444433332211");
        assert!(index
            .by_full_uuid
            .contains_key("99998888-7777-4666-8555-444433332211"));
    }

    #[test]
    fn sanitize_truncates_on_a_character_boundary() {
        // Bengali characters are three bytes each. Cutting at a byte limit
        // mid-character would panic on byte slicing, so the helper walks
        // back to a boundary instead.
        let long = "ঘ".repeat(60); // 180 bytes
        let cut = sanitize_folder_text(&long, 50);
        assert!(cut.len() <= 50, "cut is {} bytes", cut.len());
        assert!(!cut.is_empty());
        // Every remaining character is whole: re-encoding never panics.
        assert_eq!(cut.chars().count() * 3, cut.len());

        // Slashes, tabs, control characters and leading dots are cleaned.
        let messy = sanitize_folder_text("  ../A\tB\u{1}/C  .. ", 100);
        assert_eq!(messy, "A B C");
    }

    #[test]
    fn uuid_short_suffix_handles_short_uuids_without_panicking() {
        assert_eq!(uuid_suffix("3f2ab91c-77d0-4c2e-9a10-52f1b3d4e5f6"), "3f2ab91c");
        // Shorter than eight characters (test fixtures use these): the
        // whole thing is the suffix, and nothing panics.
        assert_eq!(uuid_suffix("abc-123"), "abc-123");
        assert_eq!(uuid_suffix(""), "");
    }
}
