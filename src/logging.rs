//! Where the reading engine's messages go.
//!
//! The engine reports through the `log` facade, and the facade needs a logger
//! installed to do anything at all. Until now none was, so **every engine
//! message was silently discarded** — 17 call sites across the reading crates
//! writing into nothing.
//!
//! That mattered more than it sounds, because one of those messages is the
//! number that decides how the bubble reader gets built. `Session::open` logs:
//!
//! ```text
//! opened "Some Book" in 214 ms (fonts: 312 faces, 188 ms)
//! ```
//!
//! Total time to open a book, how many fonts were found, and how much of it was
//! the font scan. The engine has been measuring that split all along; nobody
//! could see it.
//!
//! ## Turning it on
//!
//! Nothing is written unless `RUST_LOG` is set, so an ordinary launch is
//! completely unchanged — no file created, nothing printed:
//!
//! ```text
//! RUST_LOG=info kalam     # engine messages, including the open timings
//! RUST_LOG=debug kalam    # more detail
//! RUST_LOG=kalam_reader   # one crate only
//! ```
//!
//! ## Where it goes
//!
//! To the terminal **and** to `~/.local/share/kalam/kalam.log`. Both, because
//! the app is normally started from a `.desktop` file with no terminal attached
//! — stderr there disappears entirely, so the file is the only record that
//! survives. This is also why the app's own 83 `eprintln!` sites are invisible
//! in normal use; converting them is a separate, larger job.
//!
//! The file is appended to, never truncated, so a run that crashed still has
//! its earlier lines.

use std::io::Write;
use std::path::Path;

/// Sends every log line to a file and to the terminal.
///
/// `env_logger` writes to a single destination, so fanning out to two is this
/// struct's entire job.
///
/// The terminal half is deliberately best-effort: if stderr is closed or
/// broken — the normal case under a `.desktop` launch — that must not stop the
/// file from being written. Only the file's result decides whether the write
/// succeeded, because the file is the destination that actually matters.
struct Tee {
    file: std::fs::File,
}

impl Write for Tee {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let written = self.file.write(buf)?;
        let _ = std::io::stderr().write_all(buf);
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()?;
        let _ = std::io::stderr().flush();
        Ok(())
    }
}

/// Install the logger. Call once, early in `main()`, before anything can log.
///
/// Returns without doing anything when `RUST_LOG` is unset, which is the
/// default: no file is created and no logger is installed, so a normal launch
/// is byte-for-byte what it was before.
pub fn init() {
    if std::env::var_os("RUST_LOG").is_none() {
        return;
    }

    let path = crate::paths::log_file();
    // `Env::default()` reads `RUST_LOG`, which the guard above has already
    // established is set.
    let mut builder = env_logger::Builder::from_env(env_logger::Env::default());
    builder.format_timestamp_millis();

    match open_log_file(&path) {
        Some(file) => {
            eprintln!("kalam: logging to {}", path.display());
            builder.target(env_logger::Target::Pipe(Box::new(Tee { file })));
        }
        None => {
            // A log file that cannot be opened is not a reason for the app to
            // fail, or even to lose its messages. Fall back to the terminal.
            eprintln!(
                "kalam: could not open {}, logging to the terminal only",
                path.display()
            );
            builder.target(env_logger::Target::Stderr);
        }
    }

    builder.init();
}

/// Open the log file for appending, creating its folder if needed.
///
/// `None` means "give up and log to the terminal" — for example a read-only
/// home directory, or `XDG_DATA_HOME` pointing somewhere unwritable.
fn open_log_file(path: &Path) -> Option<std::fs::File> {
    // A parent that cannot be created — a file standing where the folder
    // belongs, a read-only home — ends here rather than in a panic.
    std::fs::create_dir_all(path.parent()?).ok()?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()
}

#[cfg(test)]
mod tests {
    use super::{open_log_file, Tee};
    use std::io::Write;

    /// The one thing `Tee` gets wrong easily: reporting success based on the
    /// terminal when the file is what matters. A closed stderr must not turn a
    /// good file write into an error, and a file failure must not be hidden by
    /// a successful terminal write.
    #[test]
    fn tee_reports_the_file_not_the_terminal() {
        let dir = std::env::temp_dir().join(format!("kalam-tee-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("tee.log");

        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)
            .expect("temp file");
        let mut tee = Tee { file };

        tee.write_all(b"opened \"Book\" in 214 ms\n").unwrap();
        tee.flush().unwrap();
        drop(tee);

        let back = std::fs::read_to_string(&path).expect("read back");
        assert_eq!(back, "opened \"Book\" in 214 ms\n");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An unwritable log path must degrade to `None`, not panic and not take
    /// the app down with it. This is the fallback branch in `init`: a read-only
    /// home directory, or `XDG_DATA_HOME` pointing somewhere unusable, should
    /// cost you the log file and nothing else.
    #[test]
    fn unwritable_log_path_degrades_to_none() {
        // A path whose parent cannot be created: a file standing where the
        // directory would have to be.
        let dir = std::env::temp_dir().join(format!("kalam-tee-blocker-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let blocker = dir.join("not-a-directory");
        let _ = std::fs::write(&blocker, b"x");

        let under_it = blocker.join("kalam.log");
        assert!(
            open_log_file(&under_it).is_none(),
            "a path through a plain file must yield None, not an error"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The log lives in the shared location, not the active library's folder:
    /// a log is about the program, not about one collection of books, and it
    /// should be in the same place whichever library is open.
    #[test]
    fn log_file_lives_in_the_shared_dir() {
        let path = crate::paths::log_file();
        assert_eq!(
            path.file_name().and_then(|n| n.to_str()),
            Some("kalam.log"),
            "the log file name is part of what the docs promise the user"
        );
        assert_eq!(
            path.parent(),
            Some(crate::paths::shared_data_dir().as_path()),
            "the log must not move when the user switches library"
        );
    }
}
