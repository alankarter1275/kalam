//! Auto-import watch folder service.
//!
//! Watches user-configured directories for new ebooks and comics,
//! ensures downloads/copies have completed (debounce with size stabilization),
//! automatically imports them into the library with format filtering and
//! target shelf routing, and notifies the user.

use crate::app::AppMsg;
use crate::db::Catalog;
use gtk::gio::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const PREF_WATCH_RULES: &str = "import.watch_folder_rules";
pub const PREF_WATCH_ENABLED: &str = "import.watch_folder_enabled";
pub const PREF_WATCH_PATH: &str = "import.watch_folder_path";

static RELOAD_SENDER: Mutex<Option<relm4::Sender<AppMsg>>> = Mutex::new(None);

/// Configuration rule for a watched folder.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WatchFolderRule {
    pub id: String,
    pub path: PathBuf,
    pub enabled: bool,
    pub import_epub: bool,
    pub epub_shelf_id: Option<i64>,
    pub import_pdf: bool,
    pub pdf_shelf_id: Option<i64>,
    pub import_comics: bool,
    pub comics_shelf_id: Option<i64>,
    pub subfolder_shelves: bool,
    pub cleanup_original: bool,
}

impl WatchFolderRule {
    /// Create a new rule with sensible defaults.
    pub fn new(path: PathBuf) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            path,
            enabled: true,
            import_epub: true,
            epub_shelf_id: None,
            import_pdf: true,
            pdf_shelf_id: None,
            import_comics: true,
            comics_shelf_id: None,
            subfolder_shelves: true,
            cleanup_original: false,
        }
    }

    /// Check if this rule permits importing the given file.
    pub fn allows_file(&self, path: &Path) -> bool {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        match ext.as_str() {
            "epub" => self.import_epub,
            "pdf" => self.import_pdf,
            "cbz" | "cbr" => self.import_comics,
            _ => false,
        }
    }

    /// Get target shelf for this format, if specified.
    pub fn target_shelf_for_file(&self, path: &Path) -> Option<i64> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        match ext.as_str() {
            "epub" => self.epub_shelf_id,
            "pdf" => self.pdf_shelf_id,
            "cbz" | "cbr" => self.comics_shelf_id,
            _ => None,
        }
    }
}

/// Load configured watch folder rules from preferences.
pub fn load_watch_rules(catalog: &Catalog) -> Vec<WatchFolderRule> {
    if let Some(json_str) = catalog.get_pref(PREF_WATCH_RULES) {
        if let Ok(rules) = serde_json::from_str::<Vec<WatchFolderRule>>(&json_str) {
            return rules;
        }
    }

    // Backward compatibility with previous single-path preference
    if let Some(path) = watch_path(catalog) {
        let enabled = is_watch_enabled(catalog);
        let default_rule = WatchFolderRule {
            id: uuid::Uuid::new_v4().to_string(),
            path,
            enabled,
            import_epub: true,
            epub_shelf_id: None,
            import_pdf: true,
            pdf_shelf_id: None,
            import_comics: true,
            comics_shelf_id: None,
            subfolder_shelves: true,
            cleanup_original: false,
        };
        let rules = vec![default_rule];
        save_watch_rules(catalog, &rules);
        return rules;
    }

    Vec::new()
}

/// Save watch folder rules to preferences.
pub fn save_watch_rules(catalog: &Catalog, rules: &[WatchFolderRule]) {
    if let Ok(json_str) = serde_json::to_string(rules) {
        catalog.set_pref(PREF_WATCH_RULES, &json_str);
    }
}

/// Check if any watch folder auto-import rule is enabled.
pub fn is_watch_enabled(catalog: &Catalog) -> bool {
    let rules = load_watch_rules(catalog);
    if !rules.is_empty() {
        rules.iter().any(|r| r.enabled)
    } else {
        catalog
            .get_pref(PREF_WATCH_ENABLED)
            .map(|v| v == "1" || v == "true")
            .unwrap_or(false)
    }
}

/// Set watch folder auto-import enabled state on all rules.
pub fn set_watch_enabled(catalog: &Catalog, enabled: bool) {
    let mut rules = load_watch_rules(catalog);
    for r in &mut rules {
        r.enabled = enabled;
    }
    save_watch_rules(catalog, &rules);
    catalog.set_pref(PREF_WATCH_ENABLED, if enabled { "1" } else { "0" });
}

/// Get the first configured watch folder path, if any.
pub fn watch_path(catalog: &Catalog) -> Option<PathBuf> {
    let rules = load_watch_rules(catalog);
    if let Some(first) = rules.first() {
        return Some(first.path.clone());
    }
    catalog
        .get_pref(PREF_WATCH_PATH)
        .filter(|p| !p.trim().is_empty())
        .map(PathBuf::from)
}

/// Set the watch folder path (creates or updates the first rule).
pub fn set_watch_path(catalog: &Catalog, path: &Path) {
    let mut rules = load_watch_rules(catalog);
    if let Some(first) = rules.first_mut() {
        first.path = path.to_path_buf();
        first.enabled = true;
    } else {
        rules.push(WatchFolderRule::new(path.to_path_buf()));
    }
    save_watch_rules(catalog, &rules);
    catalog.set_pref(PREF_WATCH_PATH, path.to_string_lossy().as_ref());
}

/// Clear all watch folder rules.
pub fn clear_watch_path(catalog: &Catalog) {
    save_watch_rules(catalog, &[]);
    catalog.set_pref(PREF_WATCH_PATH, "");
    catalog.set_pref(PREF_WATCH_ENABLED, "0");
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

/// Recursively collect supported book files in `dir` up to `max_depth` matching `rule`.
pub fn collect_book_files_for_rule(
    dir: &Path,
    max_depth: usize,
    rule: &WatchFolderRule,
    out: &mut Vec<PathBuf>,
) {
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
                collect_book_files_for_rule(&p, max_depth - 1, rule, out);
            } else if p.is_file() && rule.allows_file(&p) {
                out.push(p);
            }
        }
    }
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

/// Worker function to import a batch of files and apply folder rules (shelves, cleanups).
pub fn import_watch_files_with_rules(
    catalog: &Arc<Catalog>,
    items: Vec<(PathBuf, String)>,
    app_sender: Option<relm4::Sender<AppMsg>>,
) {
    let rules = load_watch_rules(catalog);
    let rules_by_id: HashMap<String, WatchFolderRule> =
        rules.into_iter().map(|r| (r.id.clone(), r)).collect();

    let mut imported_count = 0;
    let mut last_title = String::new();

    for (path, rule_id) in items {
        let rule_opt = rules_by_id.get(&rule_id);

        match crate::epub::import_epub(catalog, &path) {
            Ok(res) => {
                if !res.duplicate {
                    imported_count += 1;
                    last_title = res.title;
                    log::info!("Watch folder: imported '{}' from {}", last_title, path.display());
                } else {
                    log::debug!("Watch folder: duplicate '{}' ({})", res.title, path.display());
                }

                // Apply target shelf & subfolder rules
                if let Some(rule) = rule_opt {
                    // 1. Format-specific target shelf
                    if let Some(shelf_id) = rule.target_shelf_for_file(&path) {
                        let _ = catalog.add_book_to_shelf(shelf_id, res.book_id);
                    }

                    // 2. Subfolder auto-shelving
                    if rule.subfolder_shelves {
                        if let Ok(rel) = path.strip_prefix(&rule.path) {
                            if let Some(first_comp) = rel.components().next() {
                                if rel.components().count() > 1 {
                                    let subfolder_name = first_comp
                                        .as_os_str()
                                        .to_string_lossy()
                                        .trim()
                                        .to_string();
                                    if !subfolder_name.is_empty() {
                                        let shelf_id = if let Ok(shelves) = catalog.list_shelves() {
                                            shelves
                                                .iter()
                                                .find(|s| s.name.eq_ignore_ascii_case(&subfolder_name))
                                                .map(|s| s.id)
                                        } else {
                                            None
                                        };
                                        let final_id = match shelf_id {
                                            Some(id) => Ok(id),
                                            None => catalog.create_shelf(
                                                &subfolder_name,
                                                crate::db::ShelfKind::Manual,
                                                "",
                                                "",
                                            ),
                                        };
                                        if let Ok(sid) = final_id {
                                            let _ = catalog.add_book_to_shelf(sid, res.book_id);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // 3. Clean up original file after successful import
                    if rule.cleanup_original && path.is_file() {
                        let _ = std::fs::remove_file(&path);
                        log::info!("Watch folder: cleaned up original file {}", path.display());
                    }
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

/// Auto-import watch folder service holding directory monitors and debounce state.
pub struct WatchFolderService {
    catalog: Arc<Catalog>,
    app_sender: relm4::Sender<AppMsg>,
    monitors: Vec<gtk::gio::FileMonitor>,
    pending: Arc<Mutex<HashMap<PathBuf, (u64, usize, String)>>>,
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
            monitors: Vec::new(),
            pending: Arc::new(Mutex::new(HashMap::new())),
            timer_source: None,
        };
        svc.reload();
        svc
    }

    /// Reload watch folder configuration and rebind monitors.
    pub fn reload(&mut self) {
        // Cancel previous monitors
        for m in self.monitors.drain(..) {
            m.cancel();
        }
        if let Some(source) = self.timer_source.take() {
            source.remove();
        }
        if let Ok(mut map) = self.pending.lock() {
            map.clear();
        }

        let rules = load_watch_rules(&self.catalog);
        let enabled_rules: Vec<WatchFolderRule> = rules
            .into_iter()
            .filter(|r| r.enabled && r.path.is_dir())
            .collect();

        if enabled_rules.is_empty() {
            return;
        }

        // Set up FileMonitor for each active watch folder
        for rule in &enabled_rules {
            let file = gtk::gio::File::for_path(&rule.path);
            let flags =
                gtk::gio::FileMonitorFlags::SEND_MOVED | gtk::gio::FileMonitorFlags::WATCH_MOUNTS;
            match file.monitor_directory(flags, gtk::gio::Cancellable::NONE) {
                Ok(monitor) => {
                    let pending_arc = self.pending.clone();
                    let r_clone = rule.clone();
                    monitor.connect_changed(move |_mon, file, _other, event| {
                        use gtk::gio::FileMonitorEvent;
                        match event {
                            FileMonitorEvent::Created
                            | FileMonitorEvent::Changed
                            | FileMonitorEvent::ChangesDoneHint
                            | FileMonitorEvent::MovedIn
                            | FileMonitorEvent::Moved
                            | FileMonitorEvent::Renamed => {
                                if let Some(p) = file.path() {
                                    if !is_ignored_file(&p) && r_clone.allows_file(&p) {
                                        let size =
                                            std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                                        if let Ok(mut map) = pending_arc.lock() {
                                            map.insert(p, (size, 0, r_clone.id.clone()));
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    });
                    self.monitors.push(monitor);
                }
                Err(e) => {
                    log::warn!(
                        "Watch folder: failed to monitor {}: {e}",
                        rule.path.display()
                    );
                }
            }
        }

        // Start debounce stabilization timer
        let pending_arc = self.pending.clone();
        let catalog_arc = self.catalog.clone();
        let sender_clone = self.app_sender.clone();
        let source_id = gtk::glib::timeout_add_local(Duration::from_millis(1500), move || {
            let mut ready = Vec::new();
            if let Ok(mut map) = pending_arc.lock() {
                let mut to_remove = Vec::new();
                for (path, (last_size, stable_ticks, rule_id)) in map.iter_mut() {
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
                                ready.push((path.clone(), rule_id.clone()));
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
                    import_watch_files_with_rules(&cat, ready, Some(snd));
                });
            }
            gtk::glib::ControlFlow::Continue
        });
        self.timer_source = Some(source_id);

        // Initial scan of active directories in background
        let cat = self.catalog.clone();
        let snd = self.app_sender.clone();
        std::thread::spawn(move || {
            let mut initial_items = Vec::new();
            for rule in enabled_rules {
                let mut files = Vec::new();
                collect_book_files_for_rule(&rule.path, 2, &rule, &mut files);
                for f in files {
                    initial_items.push((f, rule.id.clone()));
                }
            }
            if !initial_items.is_empty() {
                import_watch_files_with_rules(&cat, initial_items, Some(snd));
            }
        });
    }
}

impl Drop for WatchFolderService {
    fn drop(&mut self) {
        for m in self.monitors.drain(..) {
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
    fn test_watch_folder_rule_filters() {
        let rule = WatchFolderRule {
            id: "test-rule".into(),
            path: PathBuf::from("/tmp/manga"),
            enabled: true,
            import_epub: false,
            epub_shelf_id: None,
            import_pdf: false,
            pdf_shelf_id: None,
            import_comics: true,
            comics_shelf_id: Some(42),
            subfolder_shelves: true,
            cleanup_original: false,
        };

        assert!(rule.allows_file(Path::new("chapter1.cbz")));
        assert!(rule.allows_file(Path::new("chapter2.cbr")));
        assert!(!rule.allows_file(Path::new("novel.epub")));
        assert!(!rule.allows_file(Path::new("paper.pdf")));
        assert_eq!(rule.target_shelf_for_file(Path::new("chapter1.cbz")), Some(42));
        assert_eq!(rule.target_shelf_for_file(Path::new("novel.epub")), None);
    }

    #[test]
    fn test_watch_folder_rules_serialization() {
        let rule = WatchFolderRule::new(PathBuf::from("/tmp/books"));
        let rules = vec![rule];
        let json = serde_json::to_string(&rules).unwrap();
        let deserialized: Vec<WatchFolderRule> = serde_json::from_str(&json).unwrap();
        assert_eq!(rules, deserialized);
    }

    #[test]
    fn test_collect_book_files_for_rule() {
        let temp_dir =
            std::env::temp_dir().join(format!("kalam_test_wf_rule_{}", uuid::Uuid::new_v4()));
        let sub_dir = temp_dir.join("sub");
        std::fs::create_dir_all(&sub_dir).unwrap();

        std::fs::write(temp_dir.join("book1.epub"), b"test").unwrap();
        std::fs::write(temp_dir.join("paper.pdf"), b"test").unwrap();
        std::fs::write(sub_dir.join("comic.cbz"), b"test").unwrap();

        let comic_only_rule = WatchFolderRule {
            id: "comics-only".into(),
            path: temp_dir.clone(),
            enabled: true,
            import_epub: false,
            epub_shelf_id: None,
            import_pdf: false,
            pdf_shelf_id: None,
            import_comics: true,
            comics_shelf_id: None,
            subfolder_shelves: true,
            cleanup_original: false,
        };

        let mut found = Vec::new();
        collect_book_files_for_rule(&temp_dir, 2, &comic_only_rule, &mut found);

        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].file_name().and_then(|n| n.to_str()),
            Some("comic.cbz")
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
