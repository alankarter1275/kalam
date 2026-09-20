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

/// `~/.local/share/kalam/library/<uuid>`
pub fn book_dir(uuid: &str) -> PathBuf {
    library_dir().join(uuid)
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
}
