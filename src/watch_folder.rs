//! Auto-import watch folder service.
//!
//! Watches a user-configured directory for new ebooks and comics,
//! ensures downloads/copies have completed (debounce with size stabilization),
//! automatically imports them into the library, and notifies the user.

use crate::app::AppMsg;
use crate::db::Catalog;
use gtk::gio::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const PREF_WATCH_ENABLED: &str = "import.watch_folder_enabled";
pub const PREF_WATCH_PATH: &str = "import.watch_folder_path";

static RELOAD_SENDER: Mutex<Option<relm4::Sender<AppMsg>>> = Mutex::new(None);

/// Check if watch folder auto-import is enabled.
pub fn is_watch_enabled(catalog: &Catalog) -> bool {
    catalog
        .get_pref(PREF_WATCH_ENABLED)
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false)
}

/// Set watch folder auto-import enabled state.
pub fn set_watch_enabled(catalog: &Catalog, enabled: bool) {
    catalog.set_pref(PREF_WATCH_ENABLED, if enabled { "1" } else { "0" });
}

/// Get the configured watch folder path, if any.
pub fn watch_path(catalog: &Catalog) -> Option<PathBuf> {
    catalog
        .get_pref(PREF_WATCH_PATH)
        .filter(|p| !p.trim().is_empty())
        .map(PathBuf::from)
}

/// Set the watch folder path.
pub fn set_watch_path(catalog: &Catalog, path: &Path) {
    catalog.set_pref(PREF_WATCH_PATH, path.to_string_lossy().as_ref());
}

/// Clear the watch folder path.
pub fn clear_watch_path(catalog: &Catalog) {
    catalog.set_pref(PREF_WATCH_PATH, "");
}

/// Request a reload of the watch folder service from anywhere (e.g. Settings).
pub fn request_reload() {
    if let Ok(guard) = RELOAD_SENDER.lock() {
        if let Some(sender) = guard.as_ref() {
            let _ = sender.send(AppMsg::ReloadWatchFolder);
        }
    }
}

/// Check if a path has a supported book file extension.
pub fn is_supported_book_file(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(ext.as_str(), "epub" | "pdf" | "cbz" | "cbr")
}

/// Check if a file is a temporary or hidden download file that should be ignored.
pub fn is_ignored_file(path: &Path) -> bool {
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if name.starts_with('.')
            || name.ends_with(".crdownload")
            || name.ends_with(".part")
            || name.ends_with(".tmp")
            || name.ends_with(".orig")
        {
            return true;
        }
    }
    false
}

/// Recursively collect supported book files in `dir` up to `max_depth`.
pub fn collect_book_files(dir: &Path, max_depth: usize, out: &mut Vec<PathBuf>) {
    if max_depth == 0 {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if is_ignored_file(&p) {
                continue;
            }
            if p.is_dir() {
                collect_book_files(&p, max_depth - 1, out);
            } else if p.is_file() && is_supported_book_file(&p) {
                out.push(p);
            }
        }
    }
}

/// Worker function to import a batch of files and update the catalog and UI.
pub fn import_watch_files(
    catalog: &Arc<Catalog>,
    paths: Vec<PathBuf>,
    app_sender: Option<relm4::Sender<AppMsg>>,
) {
    let mut imported_count = 0;
    let mut last_title = String::new();

    for path in paths {
        match crate::epub::import_epub(catalog, &path) {
            Ok(res) => {
                if !res.duplicate {
                    imported_count += 1;
                    last_title = res.title;
                    log::info!("Watch folder: imported '{}' from {}", last_title, path.display());
                } else {
                    log::debug!("Watch folder: skipped duplicate '{}' ({})", res.title, path.display());
                }
            }
            Err(err) => {
                log::warn!("Watch folder: failed to import {}: {err:#}", path.display());
            }
        }
    }

    if imported_count > 0 {
        let msg = if imported_count == 1 {
            format!("Imported \"{last_title}\"")
        } else {
            format!("Imported {imported_count} new books")
        };
        crate::notify::success("Watch Folder", &msg);

        if let Some(sender) = app_sender {
            let _ = sender.send(AppMsg::RefreshCurrentPage);
        }
    }
}

/// Auto-import watch folder service holding the directory monitor and debounce state.
pub struct WatchFolderService {
    catalog: Arc<Catalog>,
    app_sender: relm4::Sender<AppMsg>,
    monitor: Option<gtk::gio::FileMonitor>,
    current_path: Option<PathBuf>,
    pending: Arc<Mutex<HashMap<PathBuf, (u64, usize)>>>,
    timer_source: Option<gtk::glib::SourceId>,
}

impl WatchFolderService {
    /// Initialize the watch folder service.
    pub fn new(catalog: Arc<Catalog>, app_sender: relm4::Sender<AppMsg>) -> Self {
        if let Ok(mut guard) = RELOAD_SENDER.lock() {
            *guard = Some(app_sender.clone());
        }

        let mut svc = Self {
            catalog,
            app_sender,
            monitor: None,
            current_path: None,
            pending: Arc::new(Mutex::new(HashMap::new())),
            timer_source: None,
        };
        svc.reload();
        svc
    }

    /// Reload the watch folder configuration, rebinding monitors as necessary.
    pub fn reload(&mut self) {
        let enabled = is_watch_enabled(&self.catalog);
        let path_opt = watch_path(&self.catalog);

        let target_path = if enabled {
            path_opt.filter(|p| p.is_dir())
        } else {
            None
        };

        let Some(path) = target_path else {
            if let Some(m) = self.monitor.take() {
                m.cancel();
            }
            if let Some(source) = self.timer_source.take() {
                source.remove();
            }
            if let Ok(mut map) = self.pending.lock() {
                map.clear();
            }
            self.current_path = None;
            return;
        };

        // If already monitoring this exact path, keep running
        if self.current_path.as_deref() == Some(&path) && self.monitor.is_some() {
            return;
        }

        // Cancel previous monitor if path changed
        if let Some(m) = self.monitor.take() {
            m.cancel();
        }
        if let Ok(mut map) = self.pending.lock() {
            map.clear();
        }

        self.current_path = Some(path.clone());

        // Setup FileMonitor on directory
        let file = gtk::gio::File::for_path(&path);
        match file.monitor_directory(gtk::gio::FileMonitorFlags::NONE, gtk::gio::Cancellable::NONE) {
            Ok(monitor) => {
                let pending_arc = self.pending.clone();
                monitor.connect_changed(move |_mon, file, _other, event| {
                    use gtk::gio::FileMonitorEvent;
                    match event {
                        FileMonitorEvent::Created
                        | FileMonitorEvent::Changed
                        | FileMonitorEvent::ChangesDoneHint => {
                            if let Some(p) = file.path() {
                                if !is_ignored_file(&p) && is_supported_book_file(&p) {
                                    let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                                    if let Ok(mut map) = pending_arc.lock() {
                                        map.insert(p, (size, 0));
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                });
                self.monitor = Some(monitor);
            }
            Err(e) => {
                log::warn!("Watch folder: failed to monitor {}: {e}", path.display());
            }
        }

        // Start debounce stabilization timer if not already active
        if self.timer_source.is_none() {
            let pending_arc = self.pending.clone();
            let catalog_arc = self.catalog.clone();
            let sender_clone = self.app_sender.clone();
            let source_id = gtk::glib::timeout_add_local(Duration::from_millis(1500), move || {
                let mut ready = Vec::new();
                if let Ok(mut map) = pending_arc.lock() {
                    let mut to_remove = Vec::new();
                    for (path, (last_size, stable_ticks)) in map.iter_mut() {
                        if !path.exists() {
                            to_remove.push(path.clone());
                            continue;
                        }
                        let current_size = match std::fs::metadata(path) {
                            Ok(m) => m.len(),
                            Err(_) => continue,
                        };
                        if current_size == 0 {
                            continue;
                        }
                        if current_size != *last_size {
                            *last_size = current_size;
                            *stable_ticks = 0;
                        } else {
                            *stable_ticks += 1;
                            if *stable_ticks >= 1 {
                                // Test read access to ensure file lock is released
                                if std::fs::File::open(path).is_ok() {
                                    ready.push(path.clone());
                                    to_remove.push(path.clone());
                                }
                            }
                        }
                    }
                    for r in to_remove {
                        map.remove(&r);
                    }
                }

                if !ready.is_empty() {
                    let cat = catalog_arc.clone();
                    let snd = sender_clone.clone();
                    std::thread::spawn(move || {
                        import_watch_files(&cat, ready, Some(snd));
                    });
                }
                gtk::glib::ControlFlow::Continue
            });
            self.timer_source = Some(source_id);
        }

        // Initial scan of directory in background
        let scan_path = path;
        let cat = self.catalog.clone();
        let snd = self.app_sender.clone();
        std::thread::spawn(move || {
            let mut files = Vec::new();
            collect_book_files(&scan_path, 2, &mut files);
            if !files.is_empty() {
                import_watch_files(&cat, files, Some(snd));
            }
        });
    }
}

impl Drop for WatchFolderService {
    fn drop(&mut self) {
        if let Some(m) = self.monitor.take() {
            m.cancel();
        }
        if let Some(source) = self.timer_source.take() {
            source.remove();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_supported_book_file() {
        assert!(is_supported_book_file(Path::new("book.epub")));
        assert!(is_supported_book_file(Path::new("comic.CBZ")));
        assert!(is_supported_book_file(Path::new("doc.pdf")));
        assert!(is_supported_book_file(Path::new("manga.cbr")));
        assert!(!is_supported_book_file(Path::new("notes.txt")));
        assert!(!is_supported_book_file(Path::new("image.png")));
        assert!(!is_supported_book_file(Path::new("archive.zip")));
    }

    #[test]
    fn test_is_ignored_file() {
        assert!(is_ignored_file(Path::new(".hidden.epub")));
        assert!(is_ignored_file(Path::new(".DS_Store")));
        assert!(is_ignored_file(Path::new("download.epub.crdownload")));
        assert!(is_ignored_file(Path::new("download.pdf.part")));
        assert!(is_ignored_file(Path::new("temp.cbz.tmp")));
        assert!(is_ignored_file(Path::new("book.epub.orig")));
        assert!(!is_ignored_file(Path::new("clean_book.epub")));
        assert!(!is_ignored_file(Path::new("my_manga.cbz")));
    }

    #[test]
    fn test_collect_book_files() {
        let temp_dir = std::env::temp_dir().join(format!("kalam_test_wf_{}", uuid::Uuid::new_v4()));
        let sub_dir = temp_dir.join("sub");
        std::fs::create_dir_all(&sub_dir).unwrap();

        std::fs::write(temp_dir.join("book1.epub"), b"test").unwrap();
        std::fs::write(temp_dir.join(".hidden.epub"), b"test").unwrap();
        std::fs::write(temp_dir.join("download.epub.part"), b"test").unwrap();
        std::fs::write(temp_dir.join("notes.txt"), b"test").unwrap();
        std::fs::write(sub_dir.join("comic.cbz"), b"test").unwrap();

        let mut found = Vec::new();
        collect_book_files(&temp_dir, 2, &mut found);

        let names: Vec<String> = found
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(|s| s.to_string()))
            .collect();

        assert!(names.contains(&"book1.epub".to_string()));
        assert!(names.contains(&"comic.cbz".to_string()));
        assert!(!names.contains(&".hidden.epub".to_string()));
        assert!(!names.contains(&"download.epub.part".to_string()));
        assert!(!names.contains(&"notes.txt".to_string()));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
