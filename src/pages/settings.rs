use crate::db::Catalog;
use crate::dict;
use crate::paths::{catalog_db, data_dir, dictionaries_dir};
use crate::service::{LibraryService, SettingsSnapshot};
use gtk::prelude::*;
use relm4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

/// One Settings pane (decision 144). The variants are called tabs for
/// historical reasons only; the page is the section-41 pane list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Appearance,
    Reading,
    Libraries,
    DataExport,
    BookFiles,
    WatchFolders,
    Dictionaries,
    Metadata,
    Integrations,
    Shortcuts,
    About,
}

impl SettingsTab {
    /// Stable lowercase name, for widget names and the
    /// `KALAM_SETTINGS_PANE` screenshot hook.
    pub fn id(self) -> &'static str {
        match self {
            Self::Appearance => "appearance",
            Self::Reading => "reading",
            Self::Libraries => "libraries",
            Self::DataExport => "data-export",
            Self::BookFiles => "book-files",
            Self::WatchFolders => "watch-folders",
            Self::Dictionaries => "dictionaries",
            Self::Metadata => "metadata",
            Self::Integrations => "integrations",
            Self::Shortcuts => "shortcuts",
            Self::About => "about",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        [
            Self::Appearance,
            Self::Reading,
            Self::Libraries,
            Self::DataExport,
            Self::BookFiles,
            Self::WatchFolders,
            Self::Dictionaries,
            Self::Metadata,
            Self::Integrations,
            Self::Shortcuts,
            Self::About,
        ]
        .into_iter()
        .find(|tab| tab.id() == id)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Reading => "Reading",
            Self::Libraries => "Libraries and storage",
            Self::DataExport => "Data and export",
            Self::BookFiles => "Book files",
            Self::WatchFolders => "Watch folders",
            Self::Dictionaries => "Dictionaries",
            Self::Metadata => "Metadata sources",
            Self::Integrations => "Integrations",
            Self::Shortcuts => "Shortcuts",
            Self::About => "About",
        }
    }

    /// The Meta line under the pane title (design-system section 41).
    /// About carries its own heading block instead and hides the pair.
    pub fn meta_line(self) -> &'static str {
        match self {
            Self::Appearance => {
                "One choice of palette, in light or dark. Changes apply immediately."
            }
            Self::Reading => "Defaults for every book. A book can still keep its own.",
            Self::Libraries => {
                "Every library has its own books and database. Settings and dictionaries are shared."
            }
            Self::DataExport => "Where Kalam keeps your data, and how to take it with you.",
            Self::BookFiles => "What Kalam may write inside your EPUB files.",
            Self::WatchFolders => "Folders Kalam checks for new books.",
            Self::Dictionaries => {
                "The first dictionary that has a word answers. The list is shared by every library."
            }
            Self::Metadata => {
                "Used by Edit metadata. Nothing is fetched when a page opens."
            }
            Self::Integrations => "Downloads, sources and plugins arrive in a later release.",
            Self::Shortcuts => "Every key can be rebound. A key already taken is refused.",
            Self::About => "",
        }
    }
}

/// The pane list, grouped per decision 144.
const PANE_GROUPS: &[(&str, &[SettingsTab])] = &[
    ("Appearance", &[SettingsTab::Appearance]),
    ("Reading", &[SettingsTab::Reading]),
    (
        "Library",
        &[
            SettingsTab::Libraries,
            SettingsTab::DataExport,
            SettingsTab::BookFiles,
            SettingsTab::WatchFolders,
            SettingsTab::Dictionaries,
        ],
    ),
    (
        "Sources",
        &[SettingsTab::Metadata, SettingsTab::Integrations],
    ),
    (
        "App",
        &[
            SettingsTab::Shortcuts,
            SettingsTab::About,
        ],
    ),
];

#[derive(Debug)]
pub enum SettingsMsg {
    SelectTab(SettingsTab),
    ImportDict,
    DeleteDict(i64),
    /// Reorder the merged-store priority after a drag: id, signed distance.
    MoveDict(i64, i64),
    /// Bring back any removed packs shipped in the binary (decision 152).
    RestoreBundledDicts,
    Refresh,
    /// A library-list or backups change needs the disk reads redone:
    /// the FS worker runs again and `FsLoaded` rebuilds the panes
    /// that show them.
    ReloadFs,
    /// The DB snapshot arrived from the worker (7.1 step 2c). Boxed because
    /// it carries the dictionary list; it is sent once at open and again
    /// after a dict import / delete / move.
    Loaded(Box<SettingsSnapshot>),
    /// The disk-flavored reads (sidecar survey, libraries registry, EPUB
    /// backup listing) arrived from the page's own slower worker.
    FsLoaded(Box<SettingsFs>),
}

/// The settings page's disk-flavored reads (7.1 step 2c), gathered by the
/// page's own worker task because they are slow and only three rows want
/// them: the per-book sidecar survey, the libraries registry file, and the
/// EPUB backup listing. The same split `BookPageSnapshot::file_size` made —
/// `LibraryService` stays DB-pure, the FS question rides with the page.
/// One known library's on-disk state, gathered with the registry by
/// the FS worker so the Libraries pane never stats the disk itself.
#[derive(Debug)]
pub struct LibraryState {
    /// The folder is not there (decision 149's missing state).
    pub folder_missing: bool,
    /// Book count read from the folder's own database, when it has one.
    pub books: Option<i64>,
}

#[derive(Debug)]
pub struct SettingsFs {
    pub survey: crate::sidecar::SidecarSurvey,
    pub registry: crate::libraries::LibraryRegistry,
    /// Aligned by index with `registry.libraries`.
    pub library_states: Vec<LibraryState>,
    pub epub_backups: Vec<(std::path::PathBuf, u64)>,
}

pub struct SettingsPageModel {
    catalog: Arc<Catalog>,
    service: LibraryService,
    /// The DB snapshot (2c): None until the worker's first answer, which is
    /// also the signal that panes may build.
    snapshot: Option<SettingsSnapshot>,
    /// The slower disk reads, in their own task. Rows that want them show
    /// "Checking…" until this lands.
    fs: Option<SettingsFs>,
    /// Panes whose widgets are already built. Only the active pane builds
    /// at `Loaded`; the rest build on first visit from the snapshot in
    /// hand — the old `init` built all six tabs at open, which is what
    /// the ~500 ms was.
    built_tabs: Vec<SettingsTab>,
    active_tab: SettingsTab,
}

#[relm4::component(pub)]
impl Component for SettingsPageModel {
    type Init = Arc<Catalog>;
    type Input = SettingsMsg;
    type Output = ();
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_hexpand: true,
            set_vexpand: true,

            // The pane list (design-system section 41): 240 wide, 56 in
            // from the left and top, on plain bg.
            #[name = "nav_list"]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                add_css_class: "kalam-settings-panes",
                set_margin_start: 56,
                set_margin_top: 56,
                set_width_request: 240,
                set_hexpand: false,
                set_vexpand: true,
                set_valign: gtk::Align::Start,
            },

            // The content column: 640 wide, 64 right of the list.
            #[name = "scroller"]
            gtk::ScrolledWindow {
                set_hexpand: true,
                set_vexpand: true,
                set_hscrollbar_policy: gtk::PolicyType::Never,
                set_vscrollbar_policy: gtk::PolicyType::External,

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    add_css_class: "kalam-settings-column",
                    set_margin_start: 64,
                    set_margin_top: 56,
                    set_margin_bottom: 56,
                    set_width_request: 640,
                    set_halign: gtk::Align::Start,
                    set_hexpand: false,

                    #[name = "tab_title"]
                    gtk::Label {
                        add_css_class: "kalam-ui-display",
                        set_halign: gtk::Align::Start,
                        set_wrap: true,
                    },

                    #[name = "tab_meta"]
                    gtk::Label {
                        add_css_class: "kalam-ui-meta",
                        add_css_class: "kalam-settings-Meta",
                        set_halign: gtk::Align::Start,
                        set_wrap: true,
                    },

                    #[name = "appearance_content"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        add_css_class: "kalam-settings-pane",
                        #[watch]
                        set_visible: model.active_tab == SettingsTab::Appearance,
                    },

                    #[name = "reading_content"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        add_css_class: "kalam-settings-pane",
                        #[watch]
                        set_visible: model.active_tab == SettingsTab::Reading,
                    },

                    #[name = "libraries_content"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        add_css_class: "kalam-settings-pane",
                        #[watch]
                        set_visible: model.active_tab == SettingsTab::Libraries,
                    },

                    #[name = "data_content"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        add_css_class: "kalam-settings-pane",
                        #[watch]
                        set_visible: model.active_tab == SettingsTab::DataExport,
                    },

                    #[name = "book_files_content"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        add_css_class: "kalam-settings-pane",
                        #[watch]
                        set_visible: model.active_tab == SettingsTab::BookFiles,
                    },

                    #[name = "watch_content"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        add_css_class: "kalam-settings-pane",
                        #[watch]
                        set_visible: model.active_tab == SettingsTab::WatchFolders,
                    },

                    #[name = "dict_list"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        add_css_class: "kalam-settings-pane",
                        #[watch]
                        set_visible: model.active_tab == SettingsTab::Dictionaries,
                    },

                    #[name = "source_list"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        add_css_class: "kalam-settings-pane",
                        #[watch]
                        set_visible: model.active_tab == SettingsTab::Metadata,
                    },

                    #[name = "integrations_content"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        add_css_class: "kalam-settings-pane",
                        #[watch]
                        set_visible: model.active_tab == SettingsTab::Integrations,
                    },

                    #[name = "shortcuts_content"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        add_css_class: "kalam-settings-pane",
                        #[watch]
                        set_visible: model.active_tab == SettingsTab::Shortcuts,
                    },

                    // About carries its own heading block (decision
                    // 156), so its pane sits flush under the hidden
                    // title pair.
                    #[name = "about_content"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        add_css_class: "kalam-settings-pane",
                        add_css_class: "flush",
                        #[watch]
                        set_visible: model.active_tab == SettingsTab::About,
                    },
                },
            },
        }
    }

    fn init(
        catalog: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // Skeleton only (7.1 step 2c). The old init read ten prefs and
        // tables and built all six tabs' widgets before the page could
        // appear — the measured 465-522 ms. Now every host paints a
        // loading row, the reads run on workers, and only the active
        // pane builds when the snapshot lands.
        //
        // The screenshot loop can preselect a pane:
        // KALAM_SETTINGS_PANE=reading (decision-144 ids).
        let active_tab = std::env::var("KALAM_SETTINGS_PANE")
            .ok()
            .and_then(|id| SettingsTab::from_id(id.trim()))
            .unwrap_or(SettingsTab::Appearance);
        let service = LibraryService::new(catalog.clone());
        let model = SettingsPageModel {
            catalog,
            service,
            snapshot: None,
            fs: None,
            built_tabs: Vec::new(),
            active_tab,
        };
        let widgets = view_output!();

        widgets.tab_title.set_label(active_tab.label());
        widgets.tab_meta.set_label(active_tab.meta_line());
        let chrome = active_tab != SettingsTab::About;
        widgets.tab_title.set_visible(chrome);
        widgets.tab_meta.set_visible(chrome);

        for (group, tabs) in PANE_GROUPS.iter() {
            let group_label = gtk::Label::new(Some(&group.to_uppercase()));
            group_label.add_css_class("kalam-ui-label");
            group_label.add_css_class("kalam-settings-group-label");
            group_label.set_halign(gtk::Align::Start);
            widgets.nav_list.append(&group_label);
            for tab in tabs.iter() {
                let btn = make_pane_item(*tab, *tab == active_tab);
                let tab_copy = *tab;
                let s = sender.clone();
                btn.connect_clicked(move |_| s.input(SettingsMsg::SelectTab(tab_copy)));
                btn.set_widget_name(&format!("settings-tab-{:?}", tab_copy));
                widgets.nav_list.append(&btn);
            }
        }

        // The skeleton's moving parts: one loading row per host, cleared
        // by whichever build runs into it first (2b learned this on Home
        // — without them, unfilled sections paint as if empty).
        for host in [
            &widgets.appearance_content,
            &widgets.reading_content,
            &widgets.libraries_content,
            &widgets.data_content,
            &widgets.book_files_content,
            &widgets.watch_content,
            &widgets.dict_list,
            &widgets.source_list,
            &widgets.integrations_content,
            &widgets.shortcuts_content,
            &widgets.about_content,
        ] {
            host.append(&loading_label());
        }

        // Two workers: the fast DB snapshot fills the page, the
        // disk-flavored extras follow in their own task so they never
        // delay the fill.
        request_snapshot(&model.service, &sender);
        request_fs(&model.catalog, &sender);
        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: Self::Input,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match msg {
            SettingsMsg::SelectTab(tab) => {
                self.active_tab = tab;
                widgets.tab_title.set_label(tab.label());
                widgets.tab_meta.set_label(tab.meta_line());
                let chrome = tab != SettingsTab::About;
                widgets.tab_title.set_visible(chrome);
                widgets.tab_meta.set_visible(chrome);
                update_tab_styles(&widgets.nav_list, tab);
                // Every pane builds once from the snapshot already in
                // the model.
                if !self.built_tabs.contains(&tab) && self.snapshot.is_some() {
                    self.build_tab(tab, widgets, &sender);
                    self.mark_built(tab);
                }
                widgets.scroller.vadjustment().set_value(0.0);
            }
            SettingsMsg::Refresh => {
                // A dictionary import finished: re-read on the worker
                // (the old path re-read `list_dictionaries` inline).
                request_snapshot(&self.service, &sender);
            }
            SettingsMsg::ReloadFs => {
                request_fs(&self.catalog, &sender);
            }
            SettingsMsg::Loaded(snap) => {
                let snap = *snap;
                // A failed read used to render as "no dictionaries
                // installed" — surfaced here now that the read is off-thread.
                if let Some(err) = &snap.dicts_error {
                    crate::notify::error("Could not list your dictionaries", err);
                }
                self.snapshot = Some(snap);
                // (Re)build the active pane: the first arrival fills it,
                // later ones (after a dict import / delete / move) refresh
                // whatever is on screen. The dictionaries pane also
                // refreshes when built but hidden, matching the old
                // Refresh behavior.
                self.build_tab(self.active_tab, widgets, &sender);
                self.mark_built(self.active_tab);
                if self.active_tab != SettingsTab::Dictionaries
                    && self.built_tabs.contains(&SettingsTab::Dictionaries)
                {
                    if let Some(snap) = &self.snapshot {
                        build_dictionaries_pane(&widgets.dict_list, &snap.dicts, &sender);
                    }
                }
            }
            SettingsMsg::FsLoaded(fs) => {
                // The slow disk reads landed. Panes already built get
                // their rows refreshed in place; panes not yet visited
                // will simply build with real data when first opened.
                self.fs = Some(*fs);
                if self.built_tabs.contains(&SettingsTab::Libraries) {
                    self.build_tab(SettingsTab::Libraries, widgets, &sender);
                }
                if self.built_tabs.contains(&SettingsTab::BookFiles) {
                    self.build_tab(SettingsTab::BookFiles, widgets, &sender);
                }
            }
            SettingsMsg::ImportDict => {
                let dialog = gtk::FileDialog::builder()
                    .title("Import dictionary")
                    .build();
                let sender_clone = sender.clone();
                let catalog_clone = self.catalog.clone();
                dialog.open(
                    None::<&gtk::Window>,
                    gtk::gio::Cancellable::NONE,
                    move |res| {
                        if let Ok(file) = res {
                            if let Some(path) = file.path() {
                                let name = path
                                    .file_name()
                                    .map(|n| n.to_string_lossy().into_owned())
                                    .unwrap_or_default();
                                // A0 step 4. This used to run `import_dictionary`
                                // right here, inside the file-chooser callback on
                                // the UI thread: parsing a StarDict or TSV pack of
                                // a few hundred thousand entries froze the whole
                                // window, with the "Importing…" toast painted
                                // *before* the freeze so it looked like a hang.
                                let catalog = catalog_clone.clone();
                                let done_sender = sender_clone.input_sender().clone();
                                crate::tasks::spawn(
                                    "Importing dictionary",
                                    move |reporter| {
                                        // Worker thread: no GTK, no notify. The
                                        // outcome is returned as plain data and
                                        // reported by `on_done` below.
                                        //
                                        // The importer now streams in batches
                                        // and reports the running entry count,
                                        // so this is a real progress bar rather
                                        // than a single "started" ping that sat
                                        // still for the whole import.
                                        reporter.step(0, 0, name.clone());
                                        dict::import_dictionary_with_progress(
                                            &catalog,
                                            &path,
                                            &|written| {
                                                // Total is unknown until the
                                                // stream ends, so report 0 —
                                                // `Update::total` documents
                                                // that as "cannot know yet".
                                                reporter.step(
                                                    written,
                                                    0,
                                                    format!("{name} · {written} entries"),
                                                );
                                            },
                                        )
                                        .map_err(|e| format!("{e:#}"))
                                        // The badge needs to know: the toast
                                        // this raises on failure is gone in a
                                        // few seconds.
                                        .inspect_err(|_| reporter.fail())
                                    },
                                    // A dictionary pack can take a while. The
                                    // worker cannot raise a toast itself, so it
                                    // reports here and this runs on the main
                                    // thread where `notify` is safe.
                                    |update| {
                                        if !update.detail.is_empty() {
                                            crate::notify::activity(
                                                "Importing dictionary…",
                                                &update.detail,
                                            );
                                        }
                                    },
                                    move |result| {
                                        match result {
                                            Ok((name, count)) => crate::notify::success(
                                                "Dictionary imported",
                                                &format!("{name} · {count} entries"),
                                            ),
                                            Err(detail) => crate::notify::error(
                                                "Dictionary import failed",
                                                &detail,
                                            ),
                                        }
                                        let _ = done_sender.send(SettingsMsg::Refresh);
                                    },
                                );
                            }
                        }
                    },
                );
            }
            SettingsMsg::DeleteDict(id) => {
                let name = self
                    .snapshot
                    .as_ref()
                    .and_then(|s| s.dicts.iter().find(|d| d.id == id))
                    .map(|d| d.name.clone())
                    .unwrap_or_default();
                crate::notify::outcome_info(
                    self.catalog.delete_dictionary(id),
                    "Dictionary removed",
                    &name,
                    "Could not remove the dictionary",
                );
                request_snapshot(&self.service, &sender);
            }
            SettingsMsg::MoveDict(id, delta) => {
                if let Err(err) = self.catalog.move_dictionary_priority(id, delta) {
                    crate::notify::error("Could not reorder dictionaries", &err.to_string());
                }
                request_snapshot(&self.service, &sender);
            }
            SettingsMsg::RestoreBundledDicts => {
                let catalog = self.catalog.clone();
                let done_sender = sender.input_sender().clone();
                crate::tasks::spawn(
                    "Restoring dictionary packs",
                    move |reporter| {
                        let result = dict::restore_bundled_dictionaries(&catalog)
                            .map_err(|err| format!("{err:#}"));
                        if result.is_err() {
                            reporter.fail();
                        }
                        result
                    },
                    |_| {},
                    move |result| {
                        match result {
                            Ok(()) => crate::notify::success(
                                "Bundled packs restored",
                                "The dictionaries that ship with Kalam are available again.",
                            ),
                            Err(detail) => {
                                crate::notify::error("Could not restore dictionary packs", &detail)
                            }
                        }
                        let _ = done_sender.send(SettingsMsg::Refresh);
                    },
                );
            }
        }
        self.update_view(widgets, sender);
    }
}

/// One pane-list item (design-system section 41): flat 34 high, the
/// label in text-2, and for the active pane a 3px accent bar at the
/// left with the label bold in text. The bar and weight are CSS on
/// the button's `active` class.
fn make_pane_item(tab: SettingsTab, active: bool) -> gtk::Button {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 0);

    let bar = gtk::Box::new(gtk::Orientation::Vertical, 0);
    bar.add_css_class("kalam-settings-pane-bar");
    content.append(&bar);

    let label = gtk::Label::new(Some(tab.label()));
    label.add_css_class("kalam-settings-pane-item-label");
    label.set_halign(gtk::Align::Start);
    label.set_valign(gtk::Align::Center);
    content.append(&label);

    let btn = gtk::Button::new();
    btn.set_child(Some(&content));
    btn.add_css_class("kalam-settings-pane-item");
    if active {
        btn.add_css_class("active");
    }
    btn
}

/// The skeleton's placeholder row, one per host (7.1 step 2c). Whichever
/// build runs into the host first clears it, so an unfilled section never
/// paints as if it were an empty library.
fn loading_label() -> gtk::Label {
    let label = gtk::Label::new(Some("Loading…"));
    label.add_css_class("kalam-muted");
    label.set_halign(gtk::Align::Start);
    label
}

/// Empty a box of every child. Builders run into cleared hosts so a
/// rebuild never stacks a second copy of a pane under the first.
fn clear_box(host: &gtk::Box) {
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
}

/// Ask the worker for the DB snapshot. Called at init and after a dict
/// import / delete / move — the UI thread never reads the library
/// (7.1 step 2c).
fn request_snapshot(service: &LibraryService, sender: &ComponentSender<SettingsPageModel>) {
    let service = LibraryService::new(service.catalog().clone());
    let s = sender.input_sender().clone();
    crate::tasks::spawn_internal(
        "Reading settings",
        move |_reporter| service.settings(),
        |_| {},
        move |snap| {
            let _ = s.send(SettingsMsg::Loaded(Box::new(snap)));
        },
    );
}

/// Ask the worker for the disk-flavored reads — the per-book sidecar survey
/// (one file read per book), the libraries registry file, and the EPUB
/// backup listing. Slow, only three rows want them, and they must never
/// delay the page fill, so they ride their own task (the same split the
/// book page made for the file-size stat).
fn request_fs(catalog: &Arc<Catalog>, sender: &ComponentSender<SettingsPageModel>) {
    let catalog = catalog.clone();
    let s = sender.input_sender().clone();
    crate::tasks::spawn_internal(
        "Checking storage",
        move |_reporter| {
            let registry = crate::libraries::load_registry();
            // Each known library's folder state and book count ride
            // along: both are disk reads (the count opens that
            // library's database read-only), and the pane's rows are
            // built from them without touching the disk.
            let library_states = registry
                .libraries
                .iter()
                .map(|lib| LibraryState {
                    folder_missing: !lib.path.is_dir(),
                    books: crate::libraries::library_book_count(&lib.path),
                })
                .collect();
            SettingsFs {
                survey: crate::sidecar::survey(&catalog),
                registry,
                library_states,
                epub_backups: crate::epub_metadata::list_backups(),
            }
        },
        |_| {},
        move |fs| {
            let _ = s.send(SettingsMsg::FsLoaded(Box::new(fs)));
        },
    );
}

/// The span/watchdog name for one pane's build. Static per pane because
/// `timing::measure` takes `&'static str`.
fn settings_fill_label(tab: SettingsTab) -> &'static str {
    match tab {
        SettingsTab::Appearance => "settings_fill:appearance",
        SettingsTab::Reading => "settings_fill:reading",
        SettingsTab::Libraries => "settings_fill:libraries",
        SettingsTab::DataExport => "settings_fill:data_export",
        SettingsTab::BookFiles => "settings_fill:book_files",
        SettingsTab::WatchFolders => "settings_fill:watch_folders",
        SettingsTab::Dictionaries => "settings_fill:dictionaries",
        SettingsTab::Metadata => "settings_fill:metadata",
        SettingsTab::Integrations => "settings_fill:integrations",
        SettingsTab::Shortcuts => "settings_fill:shortcuts",
        SettingsTab::About => "settings_fill:about",
    }
}

impl SettingsPageModel {
    /// Build (or rebuild) one pane's widgets from the snapshot already in
    /// the model — no reads happen here. Every call is measured and
    /// announced under its own `settings_fill:<pane>` name so a field run
    /// shows the per-pane split the old single `route_open:settings`
    /// number hid.
    ///
    /// Every caller guarantees a snapshot is in hand: `SelectTab` checks,
    /// `Loaded` just set one, and `FsLoaded` only rebuilds panes that are
    /// already built — which took a snapshot to build. A missing snapshot
    /// would be a broken invariant; the panic ratchet forbids adding an
    /// `expect` for it, so the pane stays on its loading row instead — a
    /// state impossible to miss in QA, and one that cannot corrupt
    /// anything on the way.
    fn build_tab(
        &self,
        tab: SettingsTab,
        widgets: &mut SettingsPageModelWidgets,
        sender: &ComponentSender<SettingsPageModel>,
    ) {
        let Some(snap) = self.snapshot.as_ref() else {
            return;
        };
        let label = settings_fill_label(tab);
        let _t = crate::timing::measure(label);
        let _a = crate::timing::activity(label);
        match tab {
            SettingsTab::Appearance => {
                clear_box(&widgets.appearance_content);
                build_appearance(&widgets.appearance_content, &self.catalog, &snap.theme_id);
            }
            SettingsTab::Reading => {
                clear_box(&widgets.reading_content);
                build_reading(&widgets.reading_content, &self.catalog, snap);
            }
            SettingsTab::Libraries => {
                clear_box(&widgets.libraries_content);
                build_libraries_pane(
                    &widgets.libraries_content,
                    &self.catalog,
                    self.fs.as_ref(),
                    sender,
                );
            }
            SettingsTab::DataExport => {
                clear_box(&widgets.data_content);
                build_data_pane(&widgets.data_content, &self.catalog);
            }
            SettingsTab::BookFiles => {
                clear_box(&widgets.book_files_content);
                build_book_files_pane(
                    &widgets.book_files_content,
                    &self.catalog,
                    snap,
                    self.fs.as_ref().map(|fs| fs.epub_backups.as_slice()),
                    sender,
                );
            }
            SettingsTab::WatchFolders => {
                clear_box(&widgets.watch_content);
                build_watch_sections(
                    &widgets.watch_content,
                    &WatchPaneStore {
                        catalog: self.catalog.clone(),
                    },
                    &snap.watch_rules,
                    &snap.shelves,
                );
            }
            SettingsTab::Dictionaries => {
                build_dictionaries_pane(&widgets.dict_list, &snap.dicts, sender);
            }
            SettingsTab::Metadata => {
                clear_box(&widgets.source_list);
                build_sources_pane(&widgets.source_list, &self.service, snap);
            }
            SettingsTab::Integrations => {
                clear_box(&widgets.integrations_content);
                build_integrations(&widgets.integrations_content);
            }
            SettingsTab::Shortcuts => {
                clear_box(&widgets.shortcuts_content);
                build_shortcuts(&widgets.shortcuts_content);
            }
            SettingsTab::About => {
                clear_box(&widgets.about_content);
                build_about(&widgets.about_content, &self.catalog);
            }
        }
    }

    /// Record that `tab`'s widgets exist, so the next visit is instant.
    /// Kept out of `build_tab` so that method never needs `&mut self`.
    fn mark_built(&mut self, tab: SettingsTab) {
        if !self.built_tabs.contains(&tab) {
            self.built_tabs.push(tab);
        }
    }
}

fn update_tab_styles(container: &gtk::Box, active: SettingsTab) {
    let mut child = container.first_child();
    while let Some(widget) = child {
        if let Ok(btn) = widget.clone().downcast::<gtk::Button>() {
            let name = btn.widget_name();
            if name == format!("settings-tab-{:?}", active) {
                btn.add_css_class("active");
            } else {
                btn.remove_css_class("active");
            }
        }
        child = widget.next_sibling();
    }
}

// ---------------------------------------------------------------------------
// Section-41 building blocks: Label heading + outlined section of
// 48-high hairline rows, label/caption left, control right.
// ---------------------------------------------------------------------------

use crate::appearance::{HeadingFont, InterfaceFont, ReduceMotion, TextSize};
use crate::widgets::dialog::{
    confirmation_dialog, dialog_shell, present_dialog, text_field, ConfirmationKind, DialogSize,
};
use crate::widgets::dropdown::{dropdown, DropdownOption};
use crate::widgets::in_app_dialog::DialogExit;
use crate::widgets::pill_button::{boxed_ghost_button, ghost_button, icon_button, primary_button};
use crate::widgets::segmented::{segmented_control, SegmentedSize};

/// Append one outlined section (design-system sections 14 and 41) to
/// the pane and return the rows box. A section after the first sits
/// 24 below whatever came before it.
fn pane_section_box(host: &gtk::Box, heading: Option<&str>) -> gtk::Box {
    let wrap = gtk::Box::new(gtk::Orientation::Vertical, 0);
    if host.first_child().is_some() {
        wrap.set_margin_top(24);
    }
    if let Some(heading) = heading {
        let label = gtk::Label::new(Some(&heading.to_uppercase()));
        label.add_css_class("kalam-ui-label");
        label.set_halign(gtk::Align::Start);
        label.set_margin_bottom(8);
        wrap.append(&label);
    }
    let rows = gtk::Box::new(gtk::Orientation::Vertical, 0);
    rows.add_css_class("kalam-settings-section");
    wrap.append(&rows);
    host.append(&wrap);
    rows
}

/// A section with its Label-style heading (section 41).
fn pane_section(host: &gtk::Box, heading: &str) -> gtk::Box {
    pane_section_box(host, Some(heading))
}

/// A section with no heading, for panes whose first box follows a
/// heading block of its own (About).
fn pane_section_bare(host: &gtk::Box) -> gtk::Box {
    pane_section_box(host, None)
}

/// A section whose heading is a row of its own rather than a Label:
/// a watch folder's path, which is user text and keeps its own case,
/// with that folder's Remove action at the right (decision 151).
fn pane_section_custom_heading(host: &gtk::Box, heading: &gtk::Box) -> gtk::Box {
    let wrap = gtk::Box::new(gtk::Orientation::Vertical, 0);
    if host.first_child().is_some() {
        wrap.set_margin_top(24);
    }
    heading.set_margin_bottom(8);
    wrap.append(heading);
    let rows = gtk::Box::new(gtk::Orientation::Vertical, 0);
    rows.add_css_class("kalam-settings-section");
    wrap.append(&rows);
    host.append(&wrap);
    rows
}

/// The dashed state card (design-system section 10): a centred
/// Card-title heading and Meta line, with the caller's action(s)
/// appended to the returned card.
fn dashed_state_card(host: &gtk::Box, title: &str, meta: Option<&str>) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.add_css_class("kalam-settings-state-card");

    let heading = gtk::Label::new(Some(title));
    heading.add_css_class("kalam-ui-card-title");
    heading.set_halign(gtk::Align::Center);
    heading.set_xalign(0.5);
    heading.set_wrap(true);
    card.append(&heading);

    if let Some(text) = meta {
        let line = gtk::Label::new(Some(text));
        line.add_css_class("kalam-ui-meta");
        line.set_halign(gtk::Align::Center);
        line.set_xalign(0.5);
        line.set_wrap(true);
        card.append(&line);
    }

    host.append(&card);
    card
}

/// One 48-high row: label over an optional caption at the left,
/// expanding so the caller's control lands at the right.
fn pane_row(rows: &gtk::Box, label: &str, caption: Option<&str>) -> gtk::Box {
    pane_row_inner(rows, label, caption.map(|text| (text, false))).0
}

/// A row whose caption is a warning in the accent — section 44's
/// missing state ("Folder not found").
fn pane_row_alert(rows: &gtk::Box, label: &str, caption: &str) -> gtk::Box {
    pane_row_inner(rows, label, Some((caption, true))).0
}

/// The row builder behind both, returning the caption label as well
/// for rows whose caption changes in place (the reader-cache size).
fn pane_row_inner(
    rows: &gtk::Box,
    label: &str,
    caption: Option<(&str, bool)>,
) -> (gtk::Box, Option<gtk::Label>) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("kalam-settings-row");

    let copy = gtk::Box::new(gtk::Orientation::Vertical, 2);
    copy.set_valign(gtk::Align::Center);
    copy.set_hexpand(true);
    let text = gtk::Label::new(Some(label));
    text.add_css_class("kalam-settings-row-label");
    text.set_halign(gtk::Align::Start);
    text.set_xalign(0.0);
    text.set_wrap(true);
    copy.append(&text);
    let caption_label = caption.map(|(caption, accent)| {
        let cap = gtk::Label::new(Some(caption));
        cap.add_css_class("kalam-ui-caption");
        if accent {
            cap.add_css_class("accent");
        }
        cap.set_halign(gtk::Align::Start);
        cap.set_xalign(0.0);
        cap.set_wrap(true);
        copy.append(&cap);
        cap
    });
    row.append(&copy);
    rows.append(&row);
    (row, caption_label)
}

/// A row with its control appended, vertically centred.
fn pane_row_with(
    rows: &gtk::Box,
    label: &str,
    caption: Option<&str>,
    control: &impl IsA<gtk::Widget>,
) {
    let row = pane_row(rows, label, caption);
    control.set_valign(gtk::Align::Center);
    row.append(control);
}

/// The section-44 chip: 28 high, 11px. Plain for "Not available yet",
/// accent for the marks of honour ("Open", "Speaks first").
fn settings_chip(text: &str, accent: bool) -> gtk::Label {
    let chip = gtk::Label::new(Some(text));
    chip.add_css_class("kalam-settings-chip");
    if accent {
        chip.add_css_class("accent");
    }
    chip.set_valign(gtk::Align::Center);
    chip
}

/// The bare stepper control for a section-41 row: two round buttons
/// around a value label, the shape the reader's own setting rows use.
/// `format` renders the value (line height travels in tenths).
fn stepper_control(
    label: &str,
    min: i32,
    max: i32,
    step: i32,
    initial: i32,
    format: impl Fn(i32) -> String + 'static,
    on_change: impl Fn(i32) + 'static,
) -> gtk::Box {
    let value = Rc::new(Cell::new(initial.clamp(min, max)));
    let format = Rc::new(format);
    let on_change = Rc::new(on_change);
    let step = step.max(1);

    let root = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    root.set_valign(gtk::Align::Center);

    let make_step = |delta: i32, icon: &str, verb: &str| {
        let value = value.clone();
        let on_change = on_change.clone();
        icon_button(
            icon,
            &format!("{verb} {label}"),
            crate::style::size::COMPACT,
            true,
            move || {
                let next = (value.get() + delta).clamp(min, max);
                if next != value.get() {
                    value.set(next);
                    on_change(next);
                }
            },
        )
    };
    let minus = make_step(-step, "list-remove-symbolic", "Decrease");
    let plus = make_step(step, "list-add-symbolic", "Increase");

    let value_label = gtk::Label::new(Some(&format(value.get())));
    value_label.add_css_class("kalam-ui-stepper-value");
    value_label.set_xalign(0.5);
    value_label.set_valign(gtk::Align::Center);

    // icon_button owns the click that moves the value; these second
    // handlers mirror it into the label and the end stops, the same
    // arrangement `setting_row::stepper_row` uses.
    for button in [&minus, &plus] {
        let value = value.clone();
        let format = format.clone();
        let value_label = value_label.clone();
        let minus = minus.clone();
        let plus = plus.clone();
        button.connect_clicked(move |_| {
            let current = value.get();
            value_label.set_label(&format(current));
            minus.set_sensitive(current > min);
            plus.set_sensitive(current < max);
        });
    }
    minus.set_sensitive(value.get() > min);
    plus.set_sensitive(value.get() < max);

    root.append(&minus);
    root.append(&value_label);
    root.append(&plus);
    root
}

// ---------------------------------------------------------------------------
// Appearance: the section-42 palette picker and Text and motion
// (decisions 145 and 146).
// ---------------------------------------------------------------------------

/// One mini-page card's live state, shared between its draw callback,
/// its hover/focus controllers and every card's click handler (which
/// moves the selection as a set).
struct PaletteCardState {
    area: gtk::DrawingArea,
    selected: Rc<Cell<bool>>,
    theme_id: &'static str,
}

/// The paint area's bleed around the 64-high card: room for the 2px
/// ring at 2 offset and for the tick badge standing 7 outside the
/// top-right corner (section 42).
const CARD_BLEED: f64 = 8.0;

/// Parse a theme hex literal (`#rrggbb`) into Cairo components.
/// Anything else is a theme-authoring bug; painting mid-grey keeps
/// the picker usable and the wrong colour impossible to miss.
fn hex_components(hex: &str) -> (f64, f64, f64) {
    let digits = hex.trim_start_matches('#');
    let pair = |index: usize| -> f64 {
        digits
            .get(index..index + 2)
            .and_then(|s| u8::from_str_radix(s, 16).ok())
            .map(|v| v as f64 / 255.0)
            .unwrap_or(0.5)
    };
    (pair(0), pair(2), pair(4))
}

fn hex_source(cr: &gtk::cairo::Context, hex: &str, alpha: f64) {
    let (r, g, b) = hex_components(hex);
    cr.set_source_rgba(r, g, b, alpha);
}

/// One palette's box in the 2 x 2 grid: the name in Label style, then
/// its Light and Dark mini-page cards side by side.
fn palette_box(
    palette: crate::theme::Palette,
    resolved: &crate::theme::Theme,
    catalog: &Arc<Catalog>,
    cards: &Rc<RefCell<Vec<Rc<PaletteCardState>>>>,
) -> gtk::Box {
    let block = gtk::Box::new(gtk::Orientation::Vertical, 0);
    block.add_css_class("kalam-settings-palette-box");

    let name = gtk::Label::new(Some(&palette.label().to_uppercase()));
    name.add_css_class("kalam-ui-label");
    name.set_halign(gtk::Align::Start);
    name.set_margin_bottom(4);
    block.append(&name);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.set_homogeneous(true);
    row.append(&palette_card(
        palette.light(),
        "Light",
        resolved,
        catalog,
        cards,
    ));
    row.append(&palette_card(palette.dark(), "Dark", resolved, catalog, cards));
    block.append(&row);
    block
}

/// One mini-page card (section 42): custom-drawn because it paints
/// another palette's colours — that mode's `bg`, three 4px lines
/// (text at 50% and 30%, accent at 30% width) — with the current
/// theme's accent ring and tick badge marking the standing choice.
fn palette_card(
    theme: crate::theme::Theme,
    mode_name: &str,
    resolved: &crate::theme::Theme,
    catalog: &Arc<Catalog>,
    cards: &Rc<RefCell<Vec<Rc<PaletteCardState>>>>,
) -> gtk::Box {
    let selected = Rc::new(Cell::new(theme.id == resolved.id));
    let hovered = Rc::new(Cell::new(false));
    let focused = Rc::new(Cell::new(false));

    // The ring and badge are painted in the *current* theme's colours;
    // hidden probe labels resolve them off the installed CSS, so a
    // theme switch recolours the picker without a rebuild.
    let outer = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let accent_probe = crate::widgets::paint::color_probe("kalam-ui-token-accent");
    let fill_probe = crate::widgets::paint::color_probe("kalam-ui-token-accent-fill");
    let dim_probe = crate::widgets::paint::color_probe("kalam-ui-token-text-dim");
    outer.append(&accent_probe);
    outer.append(&fill_probe);
    outer.append(&dim_probe);

    let area = gtk::DrawingArea::new();
    area.set_hexpand(true);
    area.set_size_request(-1, 64 + (CARD_BLEED * 2.0) as i32);
    area.set_draw_func({
        let selected = selected.clone();
        let hovered = hovered.clone();
        let focused = focused.clone();
        let accent_probe = accent_probe.clone();
        let fill_probe = fill_probe.clone();
        let dim_probe = dim_probe.clone();
        move |_area, cr, width, height| {
            use crate::widgets::paint;
            let (w, h) = (width as f64, height as f64);
            let card_w = (w - CARD_BLEED * 2.0).max(0.0);
            let card_h = (h - CARD_BLEED * 2.0).max(0.0);

            // The ring: accent when selected or keyboard-focused, a
            // text-2 ring on hover (sections 12, 42, 46).
            let ring = if selected.get() || focused.get() {
                Some(paint::probe_color(&accent_probe))
            } else if hovered.get() {
                Some(paint::probe_color(&dim_probe))
            } else {
                None
            };
            if let Some(colour) = ring {
                paint::set_source(cr, colour, 1.0);
                cr.set_line_width(2.0);
                paint::rounded_rect(
                    cr,
                    CARD_BLEED - 3.0,
                    CARD_BLEED - 3.0,
                    card_w + 6.0,
                    card_h + 6.0,
                    12.0,
                );
                let _ = cr.stroke();
            }

            // The mini page itself, in the other palette's colours.
            hex_source(cr, theme.bg, 1.0);
            paint::rounded_rect(cr, CARD_BLEED, CARD_BLEED, card_w, card_h, 10.0);
            let _ = cr.fill_preserve();
            hex_source(cr, theme.border, 1.0);
            cr.set_line_width(1.0);
            let _ = cr.stroke();

            let inner_x = CARD_BLEED + 10.0;
            let inner_w = (card_w - 20.0).max(0.0);
            for (i, (fraction, hex, alpha)) in [
                (0.68, theme.text, 0.5),
                (0.48, theme.text, 0.3),
                (0.30, theme.accent, 1.0),
            ]
            .into_iter()
            .enumerate()
            {
                hex_source(cr, hex, alpha);
                paint::rounded_rect(
                    cr,
                    inner_x,
                    CARD_BLEED + 12.0 + i as f64 * 12.0,
                    inner_w * fraction,
                    4.0,
                    2.0,
                );
                let _ = cr.fill();
            }

            // The tick badge: 18 across, accent-fill, a white tick,
            // standing 7 outside the top-right corner.
            if selected.get() {
                let cx = CARD_BLEED + card_w - 2.0;
                let cy = CARD_BLEED + 2.0;
                paint::set_source(cr, paint::probe_color(&fill_probe), 1.0);
                cr.arc(cx, cy, 9.0, 0.0, std::f64::consts::TAU);
                let _ = cr.fill();
                cr.set_source_rgb(1.0, 1.0, 1.0);
                cr.set_line_width(1.8);
                cr.set_line_cap(gtk::cairo::LineCap::Round);
                cr.set_line_join(gtk::cairo::LineJoin::Round);
                cr.move_to(cx - 4.0, cy + 0.2);
                cr.line_to(cx - 1.2, cy + 3.2);
                cr.line_to(cx + 4.0, cy - 3.0);
                let _ = cr.stroke();
            }
        }
    });

    let button = gtk::Button::new();
    button.add_css_class("kalam-settings-palette-card");
    button.set_child(Some(&area));
    button.set_tooltip_text(Some(theme.label));
    button.update_property(&[gtk::accessible::Property::Label(theme.label)]);

    let motion = gtk::EventControllerMotion::new();
    {
        let hovered_enter = hovered.clone();
        let area_enter = area.clone();
        motion.connect_enter(move |_, _, _| {
            hovered_enter.set(true);
            area_enter.queue_draw();
        });
        let hovered_leave = hovered.clone();
        let area_leave = area.clone();
        motion.connect_leave(move |_| {
            hovered_leave.set(false);
            area_leave.queue_draw();
        });
    }
    area.add_controller(motion);

    let focus = gtk::EventControllerFocus::new();
    {
        let focused_enter = focused.clone();
        let area_enter = area.clone();
        focus.connect_enter(move |_| {
            focused_enter.set(true);
            area_enter.queue_draw();
        });
        let focused_leave = focused.clone();
        let area_leave = area.clone();
        focus.connect_leave(move |_| {
            focused_leave.set(false);
            area_leave.queue_draw();
        });
    }
    button.add_controller(focus);

    let state = Rc::new(PaletteCardState {
        area: area.clone(),
        selected: selected.clone(),
        theme_id: theme.id,
    });
    cards.borrow_mut().push(state);

    {
        let catalog = catalog.clone();
        let cards = cards.clone();
        button.connect_clicked(move |_| {
            // One choice of palette and mode (decision 145): the card
            // always writes a concrete Light or Dark. A stored System
            // choice is only ever *displayed* here, as the card it
            // currently resolves to.
            let next = crate::theme::ThemeChoice::from(&theme);
            crate::theme::save_and_apply_choice(&catalog, &next);
            for card in cards.borrow().iter() {
                card.selected.set(card.theme_id == theme.id);
                card.area.queue_draw();
            }
            crate::notify::success("Theme changed", &next.label());
        });
    }

    let caption = gtk::Label::new(Some(mode_name));
    caption.add_css_class("kalam-ui-caption");
    caption.set_halign(gtk::Align::Center);

    outer.append(&button);
    outer.append(&caption);
    outer
}

fn build_appearance(host: &gtk::Box, catalog: &Arc<Catalog>, active_theme_id: &str) {
    // Cards mark the *resolved* theme, so a stored System choice
    // still shows which of its palette's two modes is on screen.
    let resolved = crate::theme::choice_by_id(active_theme_id).resolve();

    let grid = gtk::Grid::new();
    grid.set_column_homogeneous(true);
    grid.set_column_spacing(16);
    grid.set_row_spacing(16);
    host.append(&grid);

    let cards: Rc<RefCell<Vec<Rc<PaletteCardState>>>> = Rc::new(RefCell::new(Vec::new()));
    for (i, palette) in crate::theme::Palette::ALL.iter().enumerate() {
        let slot = i as i32;
        let block = palette_box(*palette, &resolved, catalog, &cards);
        grid.attach(&block, slot % 2, slot / 2, 1, 1);
    }

    // Text and motion (decision 146): two dropdowns, two small
    // segmented switches, all writing the global `ui.*` prefs.
    let rows = pane_section(host, "Text and motion");
    let choices = crate::appearance::choices(catalog);

    {
        let catalog = catalog.clone();
        let font = dropdown(
            "Interface font",
            vec![
                DropdownOption::new("DM Sans"),
                DropdownOption::new("System font"),
            ],
            match choices.font {
                InterfaceFont::DmSans => 0,
                InterfaceFont::System => 1,
            },
            None,
            move |index| {
                crate::appearance::set_interface_font(
                    &catalog,
                    if index == 1 {
                        InterfaceFont::System
                    } else {
                        InterfaceFont::DmSans
                    },
                );
            },
            || {},
        );
        pane_row_with(&rows, "Interface font", None, &font.widget());
    }

    {
        let catalog = catalog.clone();
        let heading = dropdown(
            "Heading font",
            vec![
                DropdownOption::new("Playfair Display"),
                DropdownOption::new("Same as interface"),
            ],
            match choices.heading_font {
                HeadingFont::Playfair => 0,
                HeadingFont::Interface => 1,
            },
            None,
            move |index| {
                crate::appearance::set_heading_font(
                    &catalog,
                    if index == 1 {
                        HeadingFont::Interface
                    } else {
                        HeadingFont::Playfair
                    },
                );
            },
            || {},
        );
        pane_row_with(&rows, "Heading font", None, &heading.widget());
    }

    {
        let catalog = catalog.clone();
        let size = segmented_control(
            &["Small", "Default", "Large"],
            match choices.text_size {
                TextSize::Small => 0,
                TextSize::Default => 1,
                TextSize::Large => 2,
            },
            SegmentedSize::Small,
            move |index| {
                crate::appearance::set_text_size(
                    &catalog,
                    match index {
                        0 => TextSize::Small,
                        2 => TextSize::Large,
                        _ => TextSize::Default,
                    },
                );
            },
        );
        pane_row_with(&rows, "Text size", None, &size.widget());
    }

    {
        let catalog = catalog.clone();
        let motion = segmented_control(
            &["System", "On", "Off"],
            match choices.reduce_motion {
                ReduceMotion::System => 0,
                ReduceMotion::On => 1,
                ReduceMotion::Off => 2,
            },
            SegmentedSize::Small,
            move |index| {
                crate::appearance::set_reduce_motion(
                    &catalog,
                    match index {
                        1 => ReduceMotion::On,
                        2 => ReduceMotion::Off,
                        _ => ReduceMotion::System,
                    },
                );
            },
        );
        pane_row_with(&rows, "Reduce motion", None, &motion.widget());
    }
}

// ---------------------------------------------------------------------------
// Reading: the global reading defaults (decision 147). Every control
// writes the same global `reader.*` prefs the reader opens books
// with, and the preview card answers as they move.
// ---------------------------------------------------------------------------

const PREVIEW_TEXT: &str = "The train slowed as it crossed the river, and for a moment the whole \
valley seemed to hold its breath. Mara marked her page with one finger and watched the water \
move below, dark and unhurried, carrying the last of the evening light toward the sea.";

fn build_reading(host: &gtk::Box, catalog: &Arc<Catalog>, snap: &SettingsSnapshot) {
    // The live preview: a chapter heading over sample page text set
    // in the standing choices. Typeface, size, line height and
    // alignment render through Pango; the column width is shown true
    // until the card's own width (600 inside the padding) caps it,
    // the way a narrow window would cap the book.
    let family = Rc::new(RefCell::new(
        snap.reader_font_family
            .clone()
            .unwrap_or_else(|| kalam_reader::BODY_FONT.to_string()),
    ));
    let font_px = Rc::new(Cell::new(snap.reader_font_px as i32));
    let line_tenths = Rc::new(Cell::new(
        (snap.reader_line_height * 10.0).round() as i32,
    ));
    let column_px = Rc::new(Cell::new(snap.reader_column_px as i32));
    let justify = Rc::new(Cell::new(snap.reader_justify));

    let preview = gtk::Box::new(gtk::Orientation::Vertical, 0);
    preview.add_css_class("kalam-settings-preview");
    let column = gtk::Box::new(gtk::Orientation::Vertical, 10);
    column.set_halign(gtk::Align::Center);
    let heading = gtk::Label::new(Some("Chapter One"));
    heading.add_css_class("kalam-ui-section-title");
    heading.set_halign(gtk::Align::Start);
    heading.set_xalign(0.0);
    column.append(&heading);
    let body = gtk::Label::new(None);
    body.set_wrap(true);
    body.set_xalign(0.0);
    column.append(&body);
    preview.append(&column);
    host.append(&preview);

    let refresh: Rc<dyn Fn()> = {
        let body = body.clone();
        let column = column.clone();
        let family = family.clone();
        let font_px = font_px.clone();
        let line_tenths = line_tenths.clone();
        let column_px = column_px.clone();
        let justify = justify.clone();
        Rc::new(move || {
            let line_height = format!("{:.1}", line_tenths.get() as f64 / 10.0);
            body.set_markup(&format!(
                "<span font=\"{} {}\" line_height=\"{line_height}\">{}</span>",
                gtk::glib::markup_escape_text(&family.borrow()),
                font_px.get(),
                gtk::glib::markup_escape_text(PREVIEW_TEXT),
            ));
            body.set_justify(if justify.get() {
                gtk::Justification::Fill
            } else {
                gtk::Justification::Left
            });
            column.set_size_request(column_px.get().min(600), -1);
        })
    };
    refresh();

    // Typography.
    let rows = pane_section(host, "Typography");
    {
        // The picker's list is the snapshot's session-free family
        // list; a stored family missing from it (picked from a book's
        // own @font-face faces in the reader) joins so the standing
        // choice always shows with its tick.
        let mut names = snap.typefaces.clone();
        let current = family.borrow().clone();
        if !names.contains(&current) {
            names.push(current.clone());
            names.sort();
        }
        let selected = names.iter().position(|n| *n == current).unwrap_or(0);
        let options: Vec<DropdownOption> = names
            .iter()
            .map(|name| DropdownOption::in_font(name.clone(), name.clone()))
            .collect();
        let names = Rc::new(names);
        let catalog = catalog.clone();
        let family = family.clone();
        let refresh = refresh.clone();
        let typeface = dropdown(
            "Typeface",
            options,
            selected,
            None,
            move |index| {
                if let Some(name) = names.get(index) {
                    catalog.set_pref("reader.font_family", name);
                    *family.borrow_mut() = name.clone();
                    refresh();
                }
            },
            || {},
        );
        pane_row_with(&rows, "Typeface", None, &typeface.widget());
    }
    {
        let catalog = catalog.clone();
        let font_px = font_px.clone();
        let refresh = refresh.clone();
        let size = stepper_control(
            "Size",
            13,
            24,
            1,
            font_px.get(),
            |v| v.to_string(),
            move |v| {
                catalog.set_pref("reader.font_px", &v.to_string());
                font_px.set(v);
                refresh();
            },
        );
        pane_row_with(&rows, "Size", None, &size);
    }
    {
        let catalog = catalog.clone();
        let line_tenths = line_tenths.clone();
        let refresh = refresh.clone();
        let line = stepper_control(
            "Line height",
            13,
            25,
            1,
            line_tenths.get(),
            |v| format!("{:.1}", v as f64 / 10.0),
            move |v| {
                catalog.set_pref("reader.line_height", &format!("{:.1}", v as f32 / 10.0));
                line_tenths.set(v);
                refresh();
            },
        );
        pane_row_with(&rows, "Line height", None, &line);
    }

    // Layout.
    let rows = pane_section(host, "Layout");
    {
        let catalog = catalog.clone();
        let column_px = column_px.clone();
        let refresh = refresh.clone();
        let column = stepper_control(
            "Column width",
            400,
            860,
            20,
            column_px.get(),
            |v| v.to_string(),
            move |v| {
                catalog.set_pref("reader.column_px", &v.to_string());
                column_px.set(v);
                refresh();
            },
        );
        pane_row_with(&rows, "Column width", None, &column);
    }
    {
        let catalog = catalog.clone();
        let justify = justify.clone();
        let refresh = refresh.clone();
        let alignment = segmented_control(
            &["Left", "Justified"],
            usize::from(justify.get()),
            SegmentedSize::Small,
            move |index| {
                let on = index == 1;
                catalog.set_pref("reader.justify", if on { "1" } else { "0" });
                justify.set(on);
                refresh();
            },
        );
        pane_row_with(&rows, "Alignment", None, &alignment.widget());
    }

    // Behaviour.
    let rows = pane_section(host, "Behaviour");
    {
        let ask = gtk::Switch::new();
        ask.set_active(snap.reader_ask_impression);
        ask.update_property(&[gtk::accessible::Property::Label(
            "Ask for an impression when I finish a book",
        )]);
        let catalog = catalog.clone();
        ask.connect_active_notify(move |sw| {
            catalog.set_pref(
                "reader.ask_impression",
                if sw.is_active() { "1" } else { "0" },
            );
        });
        pane_row_with(
            &rows,
            "Ask for an impression when I finish a book",
            None,
            &ask,
        );
    }
    {
        let scopes = ["library", "series", "book", "off"];
        let selected = scopes
            .iter()
            .position(|s| *s == snap.reader_word_memory_scope)
            .unwrap_or(0);
        let catalog = catalog.clone();
        let memory = segmented_control(
            &["Library", "Series", "Book", "Off"],
            selected,
            SegmentedSize::Small,
            move |index| {
                if let Some(scope) = scopes.get(index) {
                    catalog.set_pref("reader.word_memory_scope", scope);
                }
            },
        );
        pane_row_with(&rows, "Word memory", None, &memory.widget());
    }
}

// ---------------------------------------------------------------------------
// Integrations: designed now, built in Part 2 (decision 154). Every
// control is present and disabled.
// ---------------------------------------------------------------------------

fn build_integrations(host: &gtk::Box) {
    let rows = pane_section(host, "Content sources");
    for name in ["AO3", "Royal Road", "MangaDex"] {
        let row = pane_row(&rows, name, None);
        row.append(&settings_chip("Not available yet", false));
        let sw = gtk::Switch::new();
        sw.set_sensitive(false);
        sw.set_valign(gtk::Align::Center);
        sw.update_property(&[gtk::accessible::Property::Label(name)]);
        row.append(&sw);
    }

    let rows = pane_section(host, "Plugins");
    let _ = pane_row(&rows, "No plugins installed", None);
    let add = boxed_ghost_button("Add plugin", || {});
    add.set_sensitive(false);
    add.set_halign(gtk::Align::Start);
    add.set_margin_top(12);
    host.append(&add);

    let rows = pane_section(host, "Downloads");
    let row = pane_row(&rows, "Download folder", None);
    let change = boxed_ghost_button("Change", || {});
    change.set_sensitive(false);
    change.set_valign(gtk::Align::Center);
    row.append(&change);
    let at_once = stepper_control("Downloads at once", 1, 8, 1, 3, |v| v.to_string(), |_| {});
    at_once.set_sensitive(false);
    pane_row_with(&rows, "Downloads at once", None, &at_once);
}

// ---------------------------------------------------------------------------
// Shortcuts (decision 155). This table is the registry the rebinding
// UI and the app-wide wiring will read; the pane renders it now and
// learns recording when the actions are wired.
// ---------------------------------------------------------------------------

struct ShortcutDef {
    id: &'static str,
    label: &'static str,
    keys: &'static [&'static str],
}

const SHORTCUT_GROUPS: &[(&str, &[ShortcutDef])] = &[
    (
        "Navigation",
        &[
            ShortcutDef {
                id: "nav-home",
                label: "Go to Home",
                keys: &["Alt", "1"],
            },
            ShortcutDef {
                id: "nav-library",
                label: "Go to Library",
                keys: &["Alt", "2"],
            },
            ShortcutDef {
                id: "nav-settings",
                label: "Open Settings",
                keys: &["Ctrl", ","],
            },
            ShortcutDef {
                id: "nav-back",
                label: "Back",
                keys: &["Alt", "←"],
            },
            ShortcutDef {
                id: "nav-notifications",
                label: "Notification history",
                keys: &["Ctrl", "Shift", "N"],
            },
        ],
    ),
    (
        "Library",
        &[
            ShortcutDef {
                id: "library-search",
                label: "Search the library",
                keys: &["Ctrl", "F"],
            },
            ShortcutDef {
                id: "library-import",
                label: "Import EPUB",
                keys: &["Ctrl", "I"],
            },
            ShortcutDef {
                id: "library-import-folder",
                label: "Import from a folder",
                keys: &["Ctrl", "Shift", "I"],
            },
        ],
    ),
    (
        "Window",
        &[
            ShortcutDef {
                id: "window-fullscreen",
                label: "Full screen",
                keys: &["F11"],
            },
            ShortcutDef {
                id: "window-shortcuts",
                label: "Show shortcuts",
                keys: &["Ctrl", "/"],
            },
            ShortcutDef {
                id: "window-quit",
                label: "Quit",
                keys: &["Ctrl", "Q"],
            },
        ],
    ),
    (
        "Everywhere",
        &[
            ShortcutDef {
                id: "app-undo-delete",
                label: "Undo the last delete",
                keys: &["Ctrl", "Z"],
            },
            ShortcutDef {
                id: "app-close-panel",
                label: "Close the open panel",
                keys: &["Esc"],
            },
        ],
    ),
];

fn build_shortcuts(host: &gtk::Box) {
    for (group, defs) in SHORTCUT_GROUPS {
        let rows = pane_section(host, group);
        for def in *defs {
            let row = pane_row(&rows, def.label, None);
            row.add_css_class("key");
            row.set_widget_name(&format!("shortcut-{}", def.id));
            let chips = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            chips.set_valign(gtk::Align::Center);
            for key in def.keys {
                chips.append(&crate::widgets::chip::key_chip(key, false));
            }
            row.append(&chips);
        }
    }
}

// ---------------------------------------------------------------------------
// About (decision 156).
// ---------------------------------------------------------------------------

fn build_about(host: &gtk::Box, catalog: &Arc<Catalog>) {
    let logo = gtk::Image::new();
    logo.set_pixel_size(64);
    logo.set_halign(gtk::Align::Start);
    if let Some(texture) = crate::app::brand_texture(crate::theme::current(catalog).accent) {
        logo.set_paintable(Some(&texture));
    }
    // The mark is rasterized in the accent; keep it in step with the
    // palette the way the rail mark does.
    {
        let logo = logo.clone();
        crate::theme::on_theme_applied(Rc::new(move |theme: &crate::theme::Theme| {
            if let Some(texture) = crate::app::brand_texture(theme.accent) {
                logo.set_paintable(Some(&texture));
            }
        }));
    }
    host.append(&logo);

    let name = gtk::Label::new(Some("Kalam"));
    name.add_css_class("kalam-ui-display");
    name.set_halign(gtk::Align::Start);
    name.set_margin_top(16);
    host.append(&name);

    let version = gtk::Label::new(Some(&format!(
        "Version {}",
        env!("CARGO_PKG_VERSION")
    )));
    version.add_css_class("kalam-ui-meta");
    version.set_halign(gtk::Align::Start);
    version.set_margin_top(8);
    host.append(&version);

    let rows = pane_section_bare(host);
    let licence = gtk::Label::new(Some("See the README in the source"));
    licence.add_css_class("kalam-ui-meta");
    pane_row_with(&rows, "Licence", None, &licence);

    let source = gtk::LinkButton::with_label(
        "https://github.com/alankarter1275/kalam",
        "github.com/alankarter1275/kalam",
    );
    source.add_css_class("kalam-settings-link");
    pane_row_with(&rows, "Source code", None, &source);

    let show = {
        let host = host.clone();
        boxed_ghost_button("Show licences", move || show_licences_dialog(&host))
    };
    pane_row_with(&rows, "Dictionary credits", None, &show);
}

/// The bundled dictionary packs' attribution and licence texts,
/// compiled in so the credits travel with the binary.
const DICTIONARY_NOTICES: &str = concat!(
    include_str!("../../resources/dictionaries/english-wordnet-2025.NOTICE.txt"),
    "\n\n",
    include_str!("../../resources/dictionaries/english-wordnet-2025.LICENSE.md"),
    "\n\n",
    include_str!("../../resources/dictionaries/english-wordnet-2025.WNDB_License.txt"),
    "\n\n————————————\n\n",
    include_str!("../../resources/dictionaries/wordnet-3.0-synonyms.NOTICE.txt"),
    "\n\n————————————\n\n",
    include_str!("../../resources/dictionaries/wordnet-3.0-exc.NOTICE.txt"),
    "\n\n————————————\n\n",
    include_str!("../../resources/dictionaries/english-idioms-2024.NOTICE.txt"),
    "\n\n",
    include_str!("../../resources/dictionaries/english-idioms-2024.LICENSE.md"),
    "\n\n————————————\n\n",
    include_str!("../../resources/dictionaries/cmudict-0.7a.NOTICE.txt"),
);

fn show_licences_dialog(anchor: &gtk::Box) {
    let shell = dialog_shell(
        DialogSize::List,
        Some("The dictionary packs Kalam bundles, and the terms they arrived under."),
    );
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::External);
    scroll.set_vexpand(true);
    let text = gtk::Label::new(Some(DICTIONARY_NOTICES));
    text.add_css_class("kalam-settings-licences");
    text.set_wrap(true);
    text.set_xalign(0.0);
    text.set_halign(gtk::Align::Fill);
    text.set_selectable(true);
    scroll.set_child(Some(&text));
    shell.body().append(&scroll);

    let slot: Rc<RefCell<Option<crate::widgets::in_app_dialog::InAppDialog>>> =
        Rc::new(RefCell::new(None));
    let close = {
        let slot = slot.clone();
        boxed_ghost_button("Close", move || {
            if let Some(dialog) = slot.borrow().clone() {
                dialog.close();
            }
        })
    };
    shell.footer().append(&close);

    match present_dialog(
        anchor,
        "Dictionary credits",
        DialogExit::OwnButtons,
        &shell,
    ) {
        Some(dialog) => *slot.borrow_mut() = Some(dialog),
        None => crate::notify::error(
            "Could not open the dialog",
            "The licences dialog could not be opened.",
        ),
    }
}
// ---------------------------------------------------------------------------
// Libraries and storage, Data and export, Book files (decision 149).
// ---------------------------------------------------------------------------

/// The switches in the new rows: plain `Gtk.Switch`, the initial state
/// set before the handler attaches so it only ever reports changes.
fn row_switch(label: &str, initial: bool, on_toggle: impl Fn(bool) + 'static) -> gtk::Switch {
    let sw = gtk::Switch::new();
    sw.set_active(initial);
    sw.set_valign(gtk::Align::Center);
    sw.set_tooltip_text(Some(label));
    sw.update_property(&[gtk::accessible::Property::Label(label)]);
    sw.connect_active_notify(move |sw| on_toggle(sw.is_active()));
    sw
}

fn build_libraries_pane(
    host: &gtk::Box,
    catalog: &Arc<Catalog>,
    fs: Option<&SettingsFs>,
    sender: &ComponentSender<SettingsPageModel>,
) {
    let rows = pane_section(host, "Libraries");
    match fs {
        None => {
            // The registry file rides the slow worker; until it lands
            // the section says so rather than guessing "no libraries".
            let _ = pane_row(&rows, "Libraries", Some("Reading the library list…"));
        }
        Some(fs) if fs.registry.libraries.is_empty() => {
            let path = crate::paths::legacy_data_dir()
                .to_string_lossy()
                .into_owned();
            let row = pane_row(&rows, "Default library", Some(&path));
            row.set_tooltip_text(Some(&path));
            row.append(&settings_chip("Open", true));
        }
        Some(fs) => {
            for (index, lib) in fs.registry.libraries.iter().enumerate() {
                library_row(&rows, lib, index, fs, sender);
            }
        }
    }

    let add = {
        let s = sender.clone();
        let anchor = rows.clone();
        boxed_ghost_button("Add library…", move || add_library_flow(&anchor, &s))
    };
    add.set_halign(gtk::Align::Start);
    add.set_margin_top(12);
    host.append(&add);

    // Recovery: how much of this library could be rebuilt from its
    // book folders alone. The number is on screen because the promise
    // is only worth something if it can be checked.
    let rows = pane_section(host, "Recovery");
    let (caption, can_write) = match fs.map(|f| &f.survey) {
        None => ("Checking your books' backup coverage…".to_string(), false),
        Some(survey) if survey.books == 0 => ("No books yet.".to_string(), false),
        Some(survey) => {
            let mut text = if survey.missing() == 0 && survey.damaged == 0 {
                format!("All {} books have a backup copy.", survey.books)
            } else {
                format!(
                    "{} of {} books have a backup copy.",
                    survey.recoverable, survey.books
                )
            };
            if survey.damaged > 0 {
                text += &format!(
                    " {} backup copies cannot be read.",
                    survey.damaged
                );
            }
            (text, survey.missing() > 0)
        }
    };
    let write = {
        let catalog = catalog.clone();
        let s = sender.clone();
        boxed_ghost_button("Write missing copies", move || {
            let written = crate::sidecar::backfill_missing(&catalog);
            crate::notify::info(
                "Backup copies written",
                &format!("{written} book folders updated."),
            );
            s.input(SettingsMsg::ReloadFs);
        })
    };
    write.set_sensitive(can_write);
    pane_row_with(
        &rows,
        "Books with a backup copy",
        Some(&caption),
        &write,
    );

    // Backup and cache.
    let rows = pane_section(host, "Backup and cache");
    let backup = {
        let catalog = catalog.clone();
        let anchor = rows.clone();
        boxed_ghost_button("Back up library…", move || {
            let dialog = gtk::FileDialog::builder()
                .title("Save library backup")
                .modal(true)
                .initial_name(format!("kalam-backup-{}.db", today_stamp()))
                .build();
            let window = anchor
                .root()
                .and_then(|r| r.downcast::<gtk::Window>().ok());
            let catalog = catalog.clone();
            dialog.save(window.as_ref(), gtk::gio::Cancellable::NONE, move |res| {
                let Ok(file) = res else { return };
                let Some(path) = file.path() else { return };
                match catalog.backup_to(&path) {
                    Ok(size) => crate::notify::success(
                        "Library backed up",
                        &format!(
                            "{} · {}",
                            crate::epub_metadata::human_size(size),
                            path.display()
                        ),
                    ),
                    Err(err) => {
                        crate::notify::error("Backup failed", &err.to_string());
                    }
                }
            });
        })
    };
    pane_row_with(
        &rows,
        "Back up library",
        Some("A snapshot of the catalog database: metadata, annotations, shelves and reading history."),
        &backup,
    );
    cache_row(&rows);
}

/// One library's row: name over path and book count, the open one
/// carrying the accent chip and the rest their Open/Forget pair — or
/// the missing-folder state with Locate folder in place of Open.
fn library_row(
    rows: &gtk::Box,
    lib: &crate::libraries::LibraryEntry,
    index: usize,
    fs: &SettingsFs,
    sender: &ComponentSender<SettingsPageModel>,
) {
    let state = fs.library_states.get(index);
    let missing = state.is_some_and(|s| s.folder_missing);
    let open_now = fs.registry.active == Some(index);
    let path_text = lib.path.display().to_string();

    if missing {
        let row = pane_row_alert(rows, &lib.name, "Folder not found");
        row.set_tooltip_text(Some(&path_text));
        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        controls.set_valign(gtk::Align::Center);
        controls.append(&locate_library_button(index, &lib.name, rows, sender));
        controls.append(&forget_library_button(index, &lib.name, sender));
        row.append(&controls);
        return;
    }

    let caption = match state.and_then(|s| s.books) {
        Some(n) => format!(
            "{path_text} · {n} book{}",
            if n == 1 { "" } else { "s" }
        ),
        None => path_text.clone(),
    };
    let row = pane_row(rows, &lib.name, Some(&caption));
    row.set_tooltip_text(Some(&path_text));
    if open_now {
        row.append(&settings_chip("Open", true));
        return;
    }
    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    controls.set_valign(gtk::Align::Center);
    controls.append(&open_library_button(index, &lib.name, rows));
    controls.append(&forget_library_button(index, &lib.name, sender));
    row.append(&controls);
}

/// Open another library, behind the restart prompt (decision 149).
/// Switching re-launches rather than swapping underneath a running
/// UI: pages hold open database handles and half-drawn covers, so
/// repointing mid-session would leave them reading one library and
/// writing to another.
fn open_library_button(index: usize, name: &str, anchor: &gtk::Box) -> gtk::Button {
    let name = name.to_string();
    let anchor = anchor.clone();
    boxed_ghost_button("Open", move || {
        let confirm_name = name.clone();
        let _ = confirmation_dialog(
            &anchor,
            &format!("Open {name}?"),
            "Kalam restarts to open this library. Your books are not changed.",
            "Cancel",
            "Restart and open",
            ConfirmationKind::Primary,
            move || {
                let mut reg = crate::libraries::load_registry();
                if !reg.select(index) {
                    // The registry changed under us — another window, or
                    // a hand edit. Saying so beats doing nothing.
                    crate::notify::error(
                        "Could not switch library",
                        "The library list changed. Reopen Settings and try again.",
                    );
                    return;
                }
                if let Err(err) = crate::libraries::save_registry(&reg) {
                    crate::notify::error(
                        "Could not save the library list",
                        &err.to_string(),
                    );
                    return;
                }
                // Only restart once the choice is safely on disk;
                // restarting first would reopen the old library and
                // look like the click did nothing.
                let err = crate::libraries::restart_now();
                crate::notify::error(
                    "Could not restart",
                    &format!(
                        "“{confirm_name}” will open next time you start Kalam. ({err})"
                    ),
                );
            },
        );
    })
}

/// Forget a library: direct, no confirm (decision 150) — it only
/// edits the list, and the toast says the books are untouched so
/// "forget" and "delete my library" are never confusable.
fn forget_library_button(
    index: usize,
    name: &str,
    sender: &ComponentSender<SettingsPageModel>,
) -> gtk::Button {
    let name = name.to_string();
    let s = sender.clone();
    boxed_ghost_button("Forget", move || {
        let mut reg = crate::libraries::load_registry();
        if !reg.forget(index) {
            crate::notify::error(
                "Could not remove that library",
                "The library list changed. Reopen Settings and try again.",
            );
            return;
        }
        match crate::libraries::save_registry(&reg) {
            Ok(()) => {
                crate::notify::info(
                    "Library removed from the list",
                    &format!("“{name}” — the folder and its books were not touched."),
                );
                s.input(SettingsMsg::ReloadFs);
            }
            Err(err) => {
                crate::notify::error("Could not save the library list", &err.to_string())
            }
        }
    })
}

/// Locate folder: point the entry at the library's new home. The
/// picked folder must actually hold a library — repointing at a
/// random folder would trade a clear "not found" for a silent empty
/// library on next launch.
fn locate_library_button(
    index: usize,
    name: &str,
    anchor: &gtk::Box,
    sender: &ComponentSender<SettingsPageModel>,
) -> gtk::Button {
    let name = name.to_string();
    let anchor = anchor.clone();
    let s = sender.clone();
    boxed_ghost_button("Locate folder…", move || {
        let dialog = gtk::FileDialog::builder()
            .title("Locate the library folder")
            .modal(true)
            .build();
        let window = anchor
            .root()
            .and_then(|r| r.downcast::<gtk::Window>().ok());
        let name = name.clone();
        let s = s.clone();
        dialog.select_folder(window.as_ref(), gtk::gio::Cancellable::NONE, move |res| {
            let Ok(folder) = res else { return };
            let Some(path) = folder.path() else { return };
            if !crate::libraries::looks_like_a_library(&path) {
                crate::notify::error(
                    "Not a Kalam library",
                    "That folder does not hold a library. Pick the folder the library moved to.",
                );
                return;
            }
            let mut reg = crate::libraries::load_registry();
            if !reg.relocate(index, &path) {
                crate::notify::error(
                    "Could not update that library",
                    "The library list changed. Reopen Settings and try again.",
                );
                return;
            }
            match crate::libraries::save_registry(&reg) {
                Ok(()) => {
                    crate::notify::success("Library located", &name);
                    s.input(SettingsMsg::ReloadFs);
                }
                Err(err) => {
                    crate::notify::error("Could not save the library list", &err.to_string())
                }
            }
        });
    })
}

/// Add library: pick a folder — an empty one starts a new library,
/// an existing Kalam library folder is reopened — named after the
/// folder, because the user already chose a meaningful name when
/// they picked where to put it.
fn add_library_flow(anchor: &gtk::Box, sender: &ComponentSender<SettingsPageModel>) {
    let dialog = gtk::FileDialog::builder()
        .title("Choose a folder for the library")
        .modal(true)
        .build();
    let window = anchor
        .root()
        .and_then(|r| r.downcast::<gtk::Window>().ok());
    let s = sender.clone();
    dialog.select_folder(window.as_ref(), gtk::gio::Cancellable::NONE, move |res| {
        let Ok(folder) = res else { return };
        let Some(path) = folder.path() else { return };
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Library".to_string());
        // Say which of the two things just happened: "added a library"
        // is ambiguous between "started an empty one" and "found my
        // books", and getting that wrong alarms in one direction.
        let existing = crate::libraries::looks_like_a_library(&path);
        let mut reg = crate::libraries::load_registry();
        reg.add_or_select(&name, &path);
        match crate::libraries::save_registry(&reg) {
            Ok(()) => {
                crate::notify::info(
                    if existing {
                        "Existing library found"
                    } else {
                        "New library created"
                    },
                    &format!("“{name}” — restart Kalam to open it."),
                );
                s.input(SettingsMsg::ReloadFs);
            }
            Err(err) => {
                crate::notify::error("Could not save the library list", &err.to_string());
            }
        }
    });
}

/// The reader-cache row. Its caption is the size, which changes in
/// place when the cache is cleared — hence building through
/// `pane_row_inner` for the caption label.
fn cache_row(rows: &gtk::Box) {
    let size = crate::paths::reader_cache_size();
    let size_text = crate::epub_metadata::human_size(size);
    let (row, caption) = pane_row_inner(
        rows,
        "Reader cache",
        Some((&size_text, false)),
    );

    let clear = boxed_ghost_button("Clear", {
        let caption = caption.clone();
        move || {
            let (files, freed) = crate::paths::clear_reader_cache();
            if let Some(caption) = &caption {
                caption.set_label(&crate::epub_metadata::human_size(
                    crate::paths::reader_cache_size(),
                ));
            }
            crate::notify::info(
                "Reader cache cleared",
                &format!(
                    "Removed {files} file{} · freed {}",
                    if files == 1 { "" } else { "s" },
                    crate::epub_metadata::human_size(freed)
                ),
            );
        }
    });
    // The ghost button's own handler runs first (it connected first);
    // this one then re-reads the size for the end stop.
    clear.connect_clicked(move |btn| {
        btn.set_sensitive(crate::paths::reader_cache_size() > 0);
    });
    clear.set_sensitive(size > 0);
    clear.set_valign(gtk::Align::Center);
    row.append(&clear);
}

fn build_data_pane(host: &gtk::Box, catalog: &Arc<Catalog>) {
    // The four places Kalam keeps things (decision 149): read-only
    // rows, each with a Show that opens the file manager there. The
    // bool is whether the path is itself a folder (Show opens it) or
    // a file (Show opens the folder holding it).
    let rows = pane_section(host, "Locations");
    let locations: [(&str, std::path::PathBuf, bool); 4] = [
        ("Library folder", data_dir(), true),
        ("Catalog database", catalog_db(), false),
        ("Dictionaries", dictionaries_dir(), true),
        (
            "App settings",
            crate::libraries::global_prefs_path(),
            false,
        ),
    ];
    for (label, path, is_dir) in locations {
        let show = {
            let path = path.clone();
            boxed_ghost_button("Show", move || show_location(&path, is_dir))
        };
        pane_row_with(&rows, label, Some(&path.to_string_lossy()), &show);
    }

    let rows = pane_section(host, "Export");
    let export_all = {
        let catalog = catalog.clone();
        boxed_ghost_button("Export…", move || {
            let catalog = catalog.clone();
            crate::tasks::spawn(
                "Exporting reading data",
                {
                    let catalog = catalog.clone();
                    move |_| {
                        let out_path = crate::paths::home_dir()
                            .unwrap_or_else(|| std::path::PathBuf::from("."))
                            .join("Kalam-Export.md");
                        catalog
                            .export_reading_data_markdown(&out_path)
                            .map(|count| (count, out_path))
                            .map_err(|e| e.to_string())
                    }
                },
                |_| {},
                |res| match res {
                    Ok((count, path)) => crate::notify::success(
                        &format!(
                            "{count} item{} exported",
                            if count == 1 { "" } else { "s" }
                        ),
                        &path.display().to_string(),
                    ),
                    Err(err) => {
                        crate::notify::error("Could not export reading data", &err)
                    }
                },
            );
        })
    };
    pane_row_with(
        &rows,
        "Export all data",
        Some("Vocabulary, quotes and highlights to a Markdown file."),
        &export_all,
    );

    let export_quotes = {
        let catalog = catalog.clone();
        boxed_ghost_button("Export…", move || {
            let catalog = catalog.clone();
            crate::tasks::spawn(
                "Exporting quotes",
                {
                    let catalog = catalog.clone();
                    move |_| crate::pages::saved_quotes::export_all_quotes_markdown(&catalog)
                },
                |_| {},
                |res| match res {
                    Ok((count, path)) => crate::notify::success(
                        &format!(
                            "{count} quote{} exported",
                            if count == 1 { "" } else { "s" }
                        ),
                        &path.display().to_string(),
                    ),
                    Err(err) => crate::notify::error("Could not export quotes", &err),
                },
            );
        })
    };
    pane_row_with(
        &rows,
        "Export quotes",
        Some("Saved quotes to a Markdown file."),
        &export_quotes,
    );
}

/// Open the file manager at a data location: the folder itself for
/// folders, the containing folder for files.
fn show_location(path: &std::path::Path, is_dir: bool) {
    let file = gtk::gio::File::for_path(path);
    let launcher = gtk::FileLauncher::new(Some(&file));
    let done = |res: Result<(), gtk::glib::Error>| {
        if let Err(err) = res {
            crate::notify::error("Could not show that location", &err.to_string());
        }
    };
    if is_dir {
        launcher.launch(
            None::<&gtk::Window>,
            None::<&gtk::gio::Cancellable>,
            done,
        );
    } else {
        launcher.open_containing_folder(
            None::<&gtk::Window>,
            None::<&gtk::gio::Cancellable>,
            done,
        );
    }
}

fn build_book_files_pane(
    host: &gtk::Box,
    catalog: &Arc<Catalog>,
    snap: &SettingsSnapshot,
    epub_backups: Option<&[(std::path::PathBuf, u64)]>,
    sender: &ComponentSender<SettingsPageModel>,
) {
    let rows = pane_section_bare(host);

    let clean = row_switch(
        "Clean EPUBs on import",
        snap.clean_on_import,
        {
            let catalog = catalog.clone();
            move |on| crate::epub_sanitizer::set_clean_on_import(&catalog, on)
        },
    );
    pane_row_with(
        &rows,
        "Clean EPUBs on import",
        Some("Removes forced fonts and colours and repairs the markup as books arrive."),
        &clean,
    );

    let write = row_switch(
        "Write edits into the EPUB",
        snap.writeback_enabled,
        {
            let catalog = catalog.clone();
            move |on| crate::epub_metadata::set_write_enabled(&catalog, on)
        },
    );
    pane_row_with(
        &rows,
        "Write edits into the EPUB",
        Some("Edit metadata also updates the book file, so other readers see your changes. The original is kept once."),
        &write,
    );

    let (caption, count, total) = match epub_backups {
        None => ("Checking…".to_string(), 0usize, 0u64),
        Some([]) => ("No backups kept".to_string(), 0, 0),
        Some(backups) => {
            let total: u64 = backups.iter().map(|(_, size)| size).sum();
            (
                format!(
                    "{} file{} · {}",
                    backups.len(),
                    if backups.len() == 1 { "" } else { "s" },
                    crate::epub_metadata::human_size(total)
                ),
                backups.len(),
                total,
            )
        }
    };
    let delete = {
        let s = sender.clone();
        let anchor = rows.clone();
        boxed_ghost_button("Delete backups…", move || {
            let s = s.clone();
            let _ = confirmation_dialog(
                &anchor,
                "Delete original backups?",
                &format!(
                    "{count} file{} ({}) will be deleted. The books in your library are not changed. This cannot be undone.",
                    if count == 1 { "" } else { "s" },
                    crate::epub_metadata::human_size(total)
                ),
                "Cancel",
                "Delete backups",
                ConfirmationKind::Destructive,
                move || {
                    let (count, freed) = crate::epub_metadata::delete_backups();
                    crate::notify::info(
                        "Original backups deleted",
                        &format!(
                            "Removed {count} backup{} · freed {}",
                            if count == 1 { "" } else { "s" },
                            crate::epub_metadata::human_size(freed)
                        ),
                    );
                    s.input(SettingsMsg::ReloadFs);
                },
            );
        })
    };
    delete.set_sensitive(count > 0);
    pane_row_with(&rows, "Original backups", Some(&caption), &delete);
}

// ---------------------------------------------------------------------------
// Watch folders (decision 151).
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum WatchShelfKind {
    Epub,
    Pdf,
    Comics,
}

impl WatchShelfKind {
    fn label(self) -> &'static str {
        match self {
            Self::Epub => "Books (EPUB)",
            Self::Pdf => "Documents (PDF)",
            Self::Comics => "Comics",
        }
    }

    fn shelf_id(self, rule: &crate::watch_folder::WatchFolderRule) -> Option<i64> {
        match self {
            Self::Epub => rule.epub_shelf_id,
            Self::Pdf => rule.pdf_shelf_id,
            Self::Comics => rule.comics_shelf_id,
        }
    }

    fn set_shelf(self, rule: &mut crate::watch_folder::WatchFolderRule, shelf_id: Option<i64>) {
        // Decision 151 replaces the old per-format checkboxes with the
        // routing choice itself: choosing where a format goes also says
        // that format is imported.
        match self {
            Self::Epub => {
                rule.epub_shelf_id = shelf_id;
                rule.import_epub = true;
            }
            Self::Pdf => {
                rule.pdf_shelf_id = shelf_id;
                rule.import_pdf = true;
            }
            Self::Comics => {
                rule.comics_shelf_id = shelf_id;
                rule.import_comics = true;
            }
        }
    }
}

/// The one catalog handle every watch-folder action shares. The pane's
/// widgets outlive the builder that made them, so their callbacks clone
/// this small context instead of threading the handle through each
/// helper separately.
#[derive(Clone)]
struct WatchPaneStore {
    catalog: Arc<Catalog>,
}

impl WatchPaneStore {
    fn update_rule(
        &self,
        rule_id: &str,
        reload: bool,
        change: impl FnOnce(&mut crate::watch_folder::WatchFolderRule),
    ) {
        let mut rules = crate::watch_folder::load_watch_rules(&self.catalog);
        if let Some(rule) = rules.iter_mut().find(|rule| rule.id == rule_id) {
            change(rule);
            crate::watch_folder::save_watch_rules(&self.catalog, &rules);
            if reload {
                crate::watch_folder::request_reload();
            }
        }
    }

    fn refresh(&self, container: &gtk::Box) {
        clear_box(container);
        let rules = crate::watch_folder::load_watch_rules(&self.catalog);
        let shelves = self.catalog.list_shelves().unwrap_or_default();
        build_watch_sections(container, self, &rules, &shelves);
    }
}

fn build_watch_sections(
    container: &gtk::Box,
    store: &WatchPaneStore,
    rules: &[crate::watch_folder::WatchFolderRule],
    shelves: &[crate::db::Shelf],
) {
    if rules.is_empty() {
        let card = dashed_state_card(
            container,
            "No watch folders",
            Some("Kalam can check a folder for new books and add them automatically."),
        );
        let add = {
            let store = store.clone();
            let container = container.clone();
            boxed_ghost_button("Add watch folder…", move || {
                add_watch_folder_flow(&container, &store)
            })
        };
        add.set_halign(gtk::Align::Center);
        card.append(&add);
        return;
    }

    for rule in rules {
        let heading = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let path_text = rule.path.to_string_lossy().into_owned();
        let path = gtk::Label::new(Some(&path_text));
        path.add_css_class("kalam-settings-folder-heading");
        path.set_hexpand(true);
        path.set_halign(gtk::Align::Start);
        path.set_xalign(0.0);
        path.set_ellipsize(gtk::pango::EllipsizeMode::End);
        path.set_tooltip_text(Some(&path_text));
        heading.append(&path);

        let remove = {
            let store = store.clone();
            let container = container.clone();
            let rule_id = rule.id.clone();
            ghost_button("Remove", move || {
                let mut rules = crate::watch_folder::load_watch_rules(&store.catalog);
                rules.retain(|rule| rule.id != rule_id);
                crate::watch_folder::save_watch_rules(&store.catalog, &rules);
                crate::watch_folder::request_reload();
                store.refresh(&container);
            })
        };
        remove.set_valign(gtk::Align::Center);
        heading.append(&remove);

        let rows = pane_section_custom_heading(container, &heading);
        for kind in [
            WatchShelfKind::Epub,
            WatchShelfKind::Pdf,
            WatchShelfKind::Comics,
        ] {
            let picker = watch_shelf_dropdown(container, store, rule, shelves, kind);
            pane_row_with(&rows, kind.label(), None, &picker);
        }

        let subfolders = row_switch("Shelves from subfolders", rule.subfolder_shelves, {
            let store = store.clone();
            let rule_id = rule.id.clone();
            move |on| {
                store.update_rule(&rule_id, false, |rule| {
                    rule.subfolder_shelves = on;
                });
            }
        });
        pane_row_with(&rows, "Shelves from subfolders", None, &subfolders);

        let cleanup = row_switch(
            "Delete the original after import",
            rule.cleanup_original,
            {
                let store = store.clone();
                let rule_id = rule.id.clone();
                move |on| {
                    store.update_rule(&rule_id, false, |rule| {
                        rule.cleanup_original = on;
                    });
                }
            },
        );
        pane_row_with(&rows, "Delete the original after import", None, &cleanup);

        // The board has no pause control, but removing the folder was
        // previously the only replacement for one. Keep that existing
        // behavior as one extra row rather than silently dropping it.
        let enabled = row_switch("Watch this folder", rule.enabled, {
            let store = store.clone();
            let rule_id = rule.id.clone();
            move |on| {
                store.update_rule(&rule_id, true, |rule| {
                    rule.enabled = on;
                });
            }
        });
        pane_row_with(&rows, "Watch this folder", None, &enabled);
    }

    let add = {
        let store = store.clone();
        let container = container.clone();
        boxed_ghost_button("Add watch folder…", move || {
            add_watch_folder_flow(&container, &store)
        })
    };
    add.set_halign(gtk::Align::Start);
    add.set_margin_top(12);
    container.append(&add);
}

fn watch_shelf_dropdown(
    container: &gtk::Box,
    store: &WatchPaneStore,
    rule: &crate::watch_folder::WatchFolderRule,
    shelves: &[crate::db::Shelf],
    kind: WatchShelfKind,
) -> gtk::MenuButton {
    let mut choices = vec![(None, "No shelf".to_string())];
    choices.extend(
        shelves
            .iter()
            .map(|shelf| (Some(shelf.id), shelf.name.clone())),
    );
    let selected = choices
        .iter()
        .position(|(shelf_id, _)| *shelf_id == kind.shelf_id(rule))
        .unwrap_or(0);
    let options = choices
        .iter()
        .map(|(_, name)| DropdownOption::new(name.clone()))
        .collect();
    let label = format!("Shelf for {}", kind.label());

    let on_change = {
        let store = store.clone();
        let rule_id = rule.id.clone();
        let choices = choices.clone();
        move |index: usize| {
            if let Some((shelf_id, _)) = choices.get(index) {
                let shelf_id = *shelf_id;
                store.update_rule(&rule_id, true, |rule| {
                    kind.set_shelf(rule, shelf_id);
                });
            }
        }
    };
    let on_footer = {
        let store = store.clone();
        let container = container.clone();
        let rule_id = rule.id.clone();
        move || {
            new_shelf_dialog(&container, &store, &container, rule_id.clone(), kind);
        }
    };

    dropdown(&label, options, selected, Some("New shelf…"), on_change, on_footer).widget()
}

fn new_shelf_dialog(
    anchor: &gtk::Box,
    store: &WatchPaneStore,
    container: &gtk::Box,
    rule_id: String,
    kind: WatchShelfKind,
) {
    let shell = dialog_shell(
        DialogSize::Form,
        Some(&format!("It will be used for {} from this folder.", kind.label())),
    );
    let field = text_field("Shelf name", "e.g. New arrivals", "");
    shell.body().append(&field.widget());

    let slot: Rc<RefCell<Option<crate::widgets::in_app_dialog::InAppDialog>>> =
        Rc::new(RefCell::new(None));
    let close = {
        let slot = slot.clone();
        Rc::new(move || {
            if let Some(dialog) = slot.borrow().clone() {
                dialog.close();
            }
        })
    };

    let create = {
        let store = store.clone();
        let container = container.clone();
        let field = field.clone();
        let close = close.clone();
        Rc::new(move || {
            let name = field.value().trim().to_string();
            if name.is_empty() {
                field.entry().grab_focus();
                return;
            }
            match store
                .catalog
                .create_shelf(&name, crate::db::ShelfKind::Manual, "", "")
            {
                Ok(shelf_id) => {
                    store.update_rule(&rule_id, true, |rule| {
                        kind.set_shelf(rule, Some(shelf_id));
                    });
                    close();
                    crate::notify::success("Shelf created", &name);
                    store.refresh(&container);
                }
                Err(err) => {
                    crate::notify::error("Could not create that shelf", &err.to_string());
                }
            }
        })
    };

    {
        let create = create.clone();
        field.entry().connect_activate(move |_| create());
    }
    shell.footer().append(&boxed_ghost_button("Cancel", {
        let close = close.clone();
        move || close()
    }));
    shell.footer().append(&primary_button("Create shelf", {
        let create = create.clone();
        move || create()
    }));

    match present_dialog(anchor, "New shelf", DialogExit::UnsavedInput, &shell) {
        Some(dialog) => {
            *slot.borrow_mut() = Some(dialog);
            field.entry().grab_focus();
        }
        None => crate::notify::error(
            "Could not open the dialog",
            "The new shelf dialog could not be opened.",
        ),
    }
}

fn add_watch_folder_flow(anchor: &gtk::Box, store: &WatchPaneStore) {
    let dialog = gtk::FileDialog::builder()
        .title("Select folder to watch")
        .modal(true)
        .build();
    let window = anchor
        .root()
        .and_then(|root| root.downcast::<gtk::Window>().ok());
    let store = store.clone();
    let container = anchor.clone();
    dialog.select_folder(window.as_ref(), gtk::gio::Cancellable::NONE, move |res| {
        let Ok(folder) = res else { return };
        let Some(path) = folder.path() else { return };
        let mut rules = crate::watch_folder::load_watch_rules(&store.catalog);
        if rules.iter().any(|rule| rule.path == path) {
            crate::notify::info(
                "Already watching that folder",
                "This folder is already in your watch list.",
            );
            return;
        }
        rules.push(crate::watch_folder::WatchFolderRule::new(path));
        crate::watch_folder::save_watch_rules(&store.catalog, &rules);
        crate::watch_folder::request_reload();
        store.refresh(&container);
    });
}

// ---------------------------------------------------------------------------
// Dictionaries (decision 152).
// ---------------------------------------------------------------------------

fn set_dictionary_drop_indicator(row: &gtk::Box, y: f64) {
    if y < row.height() as f64 / 2.0 {
        row.add_css_class("drop-before");
        row.remove_css_class("drop-after");
    } else {
        row.remove_css_class("drop-before");
        row.add_css_class("drop-after");
    }
}

fn clear_dictionary_drop_indicator(row: &gtk::Box) {
    row.remove_css_class("drop-before");
    row.remove_css_class("drop-after");
}

fn build_dictionaries_pane(
    host: &gtk::Box,
    dicts: &[crate::db::Dictionary],
    sender: &ComponentSender<SettingsPageModel>,
) {
    use gtk::glib::prelude::{StaticType, ToValue};

    clear_box(host);

    if dicts.is_empty() {
        let card = dashed_state_card(
            host,
            "No dictionaries",
            Some("Import a pack, or bring back the dictionaries that ship with Kalam."),
        );
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.set_halign(gtk::Align::Center);
        let import = {
            let sender = sender.clone();
            primary_button("Import dictionary", move || sender.input(SettingsMsg::ImportDict))
        };
        actions.append(&import);
        let restore = {
            let sender = sender.clone();
            boxed_ghost_button("Restore bundled packs", move || {
                sender.input(SettingsMsg::RestoreBundledDicts)
            })
        };
        actions.append(&restore);
        card.append(&actions);
        return;
    }

    let rows = pane_section_bare(host);
    let order = Rc::new(
        dicts
            .iter()
            .map(|dictionary| dictionary.id)
            .collect::<Vec<_>>(),
    );

    for (index, dictionary) in dicts.iter().enumerate() {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.add_css_class("kalam-settings-row");
        row.add_css_class("dict");

        let handle = gtk::Box::new(gtk::Orientation::Vertical, 0);
        handle.add_css_class("kalam-settings-dict-handle");
        handle.set_size_request(28, 56);
        handle.set_valign(gtk::Align::Fill);
        handle.set_tooltip_text(Some(&format!("Drag to reorder {}", dictionary.name)));
        handle.update_property(&[gtk::accessible::Property::Label(&format!(
            "Drag to reorder {}",
            dictionary.name
        ))]);
        let handle_icon = gtk::Image::from_icon_name("list-drag-handle-symbolic");
        handle_icon.add_css_class("kalam-settings-dict-handle-icon");
        handle_icon.set_pixel_size(16);
        handle_icon.set_halign(gtk::Align::Center);
        handle_icon.set_valign(gtk::Align::Center);
        handle.append(&handle_icon);
        row.append(&handle);

        let provider = gtk::gdk::ContentProvider::for_value(&dictionary.id.to_value());
        let source = gtk::DragSource::new();
        source.set_actions(gtk::gdk::DragAction::MOVE);
        source.set_content(Some(&provider));
        {
            let row_begin = row.clone();
            source.connect_drag_begin(move |source, _drag| {
                row_begin.set_opacity(0.4);
                row_begin.add_css_class("dragging");
                let paintable = gtk::WidgetPaintable::new(Some(&row_begin));
                source.set_icon(Some(&paintable), 14, 28);
            });
        }
        {
            let row_end = row.clone();
            source.connect_drag_end(move |_, _drag, _delete_data| {
                row_end.set_opacity(1.0);
                row_end.remove_css_class("dragging");
            });
        }
        {
            let row_cancel = row.clone();
            source.connect_drag_cancel(move |_, _drag, _reason| {
                row_cancel.set_opacity(1.0);
                row_cancel.remove_css_class("dragging");
                false
            });
        }
        handle.add_controller(source);

        let info = gtk::Box::new(gtk::Orientation::Vertical, 2);
        info.set_hexpand(true);
        info.set_valign(gtk::Align::Center);
        let name = gtk::Label::new(Some(&dictionary.name));
        name.add_css_class("kalam-settings-row-label");
        name.set_halign(gtk::Align::Start);
        name.set_xalign(0.0);
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        info.append(&name);
        let meta = gtk::Label::new(Some(&format!(
            "{} entr{}",
            dictionary.entry_count,
            if dictionary.entry_count == 1 { "y" } else { "ies" }
        )));
        meta.add_css_class("kalam-ui-meta");
        meta.set_halign(gtk::Align::Start);
        meta.set_xalign(0.0);
        info.append(&meta);
        row.append(&info);

        if index == 0 {
            row.append(&settings_chip("Speaks first", true));
        }

        let remove = {
            let sender = sender.clone();
            let anchor = rows.clone();
            let name = dictionary.name.clone();
            let entry_count = dictionary.entry_count;
            let id = dictionary.id;
            ghost_button("Remove", move || {
                let detail = if dict::is_bundled_dictionary(&name) {
                    "Restore bundled packs can bring it back."
                } else {
                    "Kalam will not add it back unless it is imported again."
                };
                let message = format!(
                    "{name} and its {entry_count} entr{} will be removed. {detail}",
                    if entry_count == 1 { "y" } else { "ies" }
                );
                let sender = sender.clone();
                let _ = confirmation_dialog(
                    &anchor,
                    &format!("Remove {name}?"),
                    &message,
                    "Cancel",
                    "Remove dictionary",
                    ConfirmationKind::Destructive,
                    move || sender.input(SettingsMsg::DeleteDict(id)),
                );
            })
        };
        remove.set_valign(gtk::Align::Center);
        row.append(&remove);

        let target = gtk::DropTarget::new(i64::static_type(), gtk::gdk::DragAction::MOVE);
        {
            let row_enter = row.clone();
            target.connect_enter(move |_, _x, y| {
                set_dictionary_drop_indicator(&row_enter, y);
                gtk::gdk::DragAction::MOVE
            });
        }
        {
            let row_motion = row.clone();
            target.connect_motion(move |_, _x, y| {
                set_dictionary_drop_indicator(&row_motion, y);
                gtk::gdk::DragAction::MOVE
            });
        }
        {
            let row_leave = row.clone();
            target.connect_leave(move |_| {
                clear_dictionary_drop_indicator(&row_leave);
            });
        }
        {
            let row_drop = row.clone();
            let sender = sender.clone();
            let order = order.clone();
            target.connect_drop(move |_, value, _x, y| {
                let after = y >= row_drop.height() as f64 / 2.0;
                clear_dictionary_drop_indicator(&row_drop);
                let Ok(source_id) = value.get::<i64>() else {
                    return false;
                };
                let Some(source_index) = order.iter().position(|id| *id == source_id) else {
                    return false;
                };
                let slot = if after { index + 1 } else { index };
                let final_index = if slot > source_index {
                    slot - 1
                } else {
                    slot
                };
                let delta = final_index as i64 - source_index as i64;
                if delta != 0 {
                    sender.input(SettingsMsg::MoveDict(source_id, delta));
                }
                true
            });
        }
        row.add_controller(target);

        rows.append(&row);
    }

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    actions.set_margin_top(12);
    let import = {
        let sender = sender.clone();
        primary_button("Import dictionary", move || sender.input(SettingsMsg::ImportDict))
    };
    actions.append(&import);
    let rescan = {
        let sender = sender.clone();
        boxed_ghost_button("Rescan", move || sender.input(SettingsMsg::Refresh))
    };
    actions.append(&rescan);
    host.append(&actions);
}

// ---------------------------------------------------------------------------
// Metadata sources (decision 153).
// ---------------------------------------------------------------------------

fn set_google_chevron(button: &gtk::Button, expanded: bool) {
    let icon = gtk::Image::from_icon_name(if expanded {
        "pan-up-symbolic"
    } else {
        "pan-down-symbolic"
    });
    icon.set_pixel_size(16);
    button.set_child(Some(&icon));
    let label = if expanded {
        "Hide Google Books settings"
    } else {
        "Show Google Books settings"
    };
    button.set_tooltip_text(Some(label));
    button.update_property(&[gtk::accessible::Property::Label(label)]);
}

fn build_sources_pane(host: &gtk::Box, service: &LibraryService, snap: &SettingsSnapshot) {
    use crate::metadata::{set_source_enabled, SourceId};

    // The pane's callbacks outlive this builder, so they share one
    // handle cloned out of the service here.
    let catalog = service.catalog().clone();

    let rows = pane_section(host, "Books");

    let open_library = row_switch("Open Library", snap.source_open_library, {
        let catalog = catalog.clone();
        move |on| set_source_enabled(&catalog, SourceId::OpenLibrary, on)
    });
    pane_row_with(&rows, "Open Library", Some("No key needed"), &open_library);

    let expanded = Rc::new(Cell::new(snap.source_google_books));
    let revealer = gtk::Revealer::new();
    revealer.set_transition_type(if crate::appearance::reduce_motion() {
        gtk::RevealerTransitionType::Crossfade
    } else {
        gtk::RevealerTransitionType::SlideDown
    });
    revealer.set_transition_duration(180);
    revealer.set_reveal_child(expanded.get());
    let nested = gtk::Box::new(gtk::Orientation::Vertical, 0);
    nested.add_css_class("kalam-settings-nested-rows");
    revealer.set_child(Some(&nested));

    let google_row = pane_row(&rows, "Google Books", None);
    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    controls.set_valign(gtk::Align::Center);

    let chevron = gtk::Button::new();
    chevron.add_css_class("kalam-settings-chevron");
    chevron.set_size_request(36, 36);
    chevron.set_valign(gtk::Align::Center);
    chevron.set_sensitive(snap.source_google_books);
    set_google_chevron(&chevron, expanded.get());

    let google_switch = row_switch("Google Books", snap.source_google_books, {
        let catalog = catalog.clone();
        let expanded_switch = expanded.clone();
        let revealer_switch = revealer.clone();
        let chevron_switch = chevron.clone();
        move |on| {
            set_source_enabled(&catalog, SourceId::GoogleBooks, on);
            expanded_switch.set(on);
            revealer_switch.set_reveal_child(on);
            chevron_switch.set_sensitive(on);
            set_google_chevron(&chevron_switch, on);
        }
    });
    controls.append(&google_switch);
    controls.append(&chevron);
    google_row.append(&controls);
    rows.append(&revealer);

    {
        let expanded_chevron = expanded.clone();
        let revealer_chevron = revealer.clone();
        chevron.connect_clicked(move |button| {
            let next = !expanded_chevron.get();
            expanded_chevron.set(next);
            revealer_chevron.set_reveal_child(next);
            set_google_chevron(button, next);
        });
    }

    // API key. Saving happens when the field is left, not on every
    // keystroke: a partly typed key must never replace a working one.
    let key_row = pane_row(&nested, "API key", None);
    key_row.add_css_class("nested");
    key_row.set_margin_start(20);
    let key_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    key_box.set_valign(gtk::Align::Center);
    key_box.set_size_request(280, -1);
    let key_entry = gtk::Entry::new();
    key_entry.add_css_class("kalam-settings-entry");
    if snap.google_books_key_rejected {
        key_entry.add_css_class("rejected");
    }
    key_entry.set_placeholder_text(Some("Optional API key"));
    key_entry.set_text(&snap.google_books_key);
    key_entry.set_hexpand(true);
    key_entry.update_property(&[gtk::accessible::Property::Label("Google Books API key")]);
    key_box.append(&key_entry);
    let key_error = gtk::Label::new(Some("Google Books rejected this key."));
    key_error.add_css_class("kalam-settings-error-line");
    key_error.set_halign(gtk::Align::Start);
    key_error.set_xalign(0.0);
    key_error.set_visible(snap.google_books_key_rejected);
    key_box.append(&key_error);
    key_row.append(&key_box);

    let saved_key = Rc::new(RefCell::new(snap.google_books_key.trim().to_string()));
    let save_key = {
        let catalog = catalog.clone();
        let entry = key_entry.clone();
        let saved = saved_key.clone();
        let error = key_error.clone();
        Rc::new(move || {
            let value = entry.text().trim().to_string();
            if *saved.borrow() == value {
                return;
            }
            catalog.set_pref("meta.googlebooks.key", &value);
            catalog.set_pref("meta.googlebooks.key_rejected", "0");
            *saved.borrow_mut() = value;
            entry.remove_css_class("rejected");
            error.set_visible(false);
        })
    };
    {
        let save_key = save_key.clone();
        key_entry.connect_activate(move |_| save_key());
    }
    let key_focus = gtk::EventControllerFocus::new();
    {
        let save_key = save_key.clone();
        key_focus.connect_leave(move |_| save_key());
    }
    key_entry.add_controller(key_focus);

    // Country code, saved the same way. An incomplete code is not a
    // setting, so leaving the field with one restores the saved code.
    let country_row = pane_row(&nested, "Country code", Some("Used to rank results"));
    country_row.add_css_class("nested");
    country_row.set_margin_start(20);
    let initial_country = snap.google_books_country.trim().to_uppercase();
    let country_entry = gtk::Entry::new();
    country_entry.add_css_class("kalam-settings-entry");
    country_entry.set_text(&initial_country);
    country_entry.set_max_length(2);
    country_entry.set_width_chars(2);
    country_entry.set_size_request(76, -1);
    country_entry.set_valign(gtk::Align::Center);
    country_entry.update_property(&[gtk::accessible::Property::Label(
        "Google Books country code",
    )]);
    country_row.append(&country_entry);

    let saved_country = Rc::new(RefCell::new(initial_country));
    let save_country = {
        let catalog = catalog.clone();
        let entry = country_entry.clone();
        let saved = saved_country.clone();
        Rc::new(move || {
            let normalized: String = entry
                .text()
                .chars()
                .filter(|c| c.is_ascii_alphabetic())
                .flat_map(char::to_uppercase)
                .take(2)
                .collect();
            if normalized.len() != 2 {
                entry.set_text(&saved.borrow());
                return;
            }
            if entry.text().as_str() != normalized {
                entry.set_text(&normalized);
            }
            if *saved.borrow() == normalized {
                return;
            }
            catalog.set_pref("meta.googlebooks.country", &normalized);
            *saved.borrow_mut() = normalized;
        })
    };
    {
        let save_country = save_country.clone();
        country_entry.connect_activate(move |_| save_country());
    }
    let country_focus = gtk::EventControllerFocus::new();
    {
        let save_country = save_country.clone();
        country_focus.connect_leave(move |_| save_country());
    }
    country_entry.add_controller(country_focus);

    let rows = pane_section(host, "Comics");
    for name in ["MyAnimeList", "Comic Vine"] {
        let row = pane_row(&rows, name, None);
        row.append(&settings_chip("Not available yet", false));
        let sw = gtk::Switch::new();
        sw.set_sensitive(false);
        sw.set_valign(gtk::Align::Center);
        sw.update_property(&[gtk::accessible::Property::Label(name)]);
        row.append(&sw);
    }
}

/// The app's pill switch. `on_toggle` fires only for user changes: the
/// initial state is set before the handler is attached.
pub(crate) fn toggle_switch(initial: bool, on_toggle: impl Fn(bool) + 'static) -> gtk::Switch {
    let sw = gtk::Switch::new();
    sw.set_active(initial);
    sw.set_valign(gtk::Align::Center);
    sw.add_css_class("kalam-switch");
    sw.connect_active_notify(move |sw| on_toggle(sw.is_active()));
    sw
}

/// `2026-07-28`, for backup filenames.
fn today_stamp() -> String {
    gtk::glib::DateTime::now_local()
        .and_then(|d| d.format("%Y-%m-%d"))
        .map(|s| s.to_string())
        .unwrap_or_default()
}
