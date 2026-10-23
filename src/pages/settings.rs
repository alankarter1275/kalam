use crate::db::Catalog;
use crate::dict;
use crate::paths::{catalog_db, data_dir, dictionaries_dir, library_dir};
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
    /// Temporary home for the notification log: decision 148 moves it
    /// to the rail bell, and until the bell lands the pane keeps the
    /// history reachable so the surface never loses it mid-phase.
    Notifications,
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
            Self::Notifications => "notifications",
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
            Self::Notifications,
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
            Self::Notifications => "Notifications",
        }
    }

    /// The Meta line under the pane title (design-system section 41).
    /// About carries its own heading block instead and hides the pair.
    pub fn Meta(self) -> &'static str {
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
            Self::Notifications => "History log of recent activity, alerts, and toasts.",
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
            SettingsTab::Notifications,
        ],
    ),
];

#[derive(Debug)]
pub enum SettingsMsg {
    SelectTab(SettingsTab),
    ClearNotifications,
    ImportDict,
    DeleteDict(i64),
    /// Reorder the merged-store priority (Phase 9): id, delta (−1 up / +1 down).
    MoveDict(i64, i64),
    Refresh,
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
#[derive(Debug)]
pub struct SettingsFs {
    pub survey: crate::sidecar::SidecarSurvey,
    pub registry: crate::libraries::LibraryRegistry,
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

                    #[name = "tab_Meta"]
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

                    #[name = "notify_list"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        add_css_class: "kalam-settings-pane",
                        #[watch]
                        set_visible: model.active_tab == SettingsTab::Notifications,
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
        widgets.tab_Meta.set_label(active_tab.Meta());
        let chrome = active_tab != SettingsTab::About;
        widgets.tab_title.set_visible(chrome);
        widgets.tab_Meta.set_visible(chrome);

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
            &widgets.notify_list,
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
                widgets.tab_Meta.set_label(tab.Meta());
                let chrome = tab != SettingsTab::About;
                widgets.tab_title.set_visible(chrome);
                widgets.tab_Meta.set_visible(chrome);
                update_tab_styles(&widgets.nav_list, tab);
                // The notifications log is in-memory session state, so it
                // rebuilds on every visit (its old behavior); every other
                // pane builds once from the snapshot already in the model.
                if tab == SettingsTab::Notifications {
                    build_notifications(&widgets.notify_list, &sender);
                } else if !self.built_tabs.contains(&tab) && self.snapshot.is_some() {
                    self.build_tab(tab, widgets, &sender);
                    self.mark_built(tab);
                }
                widgets.scroller.vadjustment().set_value(0.0);
            }
            SettingsMsg::ClearNotifications => {
                crate::notify::clear_history();
                build_notifications(&widgets.notify_list, &sender);
            }
            SettingsMsg::Refresh => {
                // A dictionary import finished: re-read on the worker (the
                // old path re-read `list_dictionaries` inline) and refresh
                // the activity log while we are here, exactly as before.
                request_snapshot(&self.service, &sender);
                build_notifications(&widgets.notify_list, &sender);
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
                        rebuild_dicts(&widgets.dict_list, &snap.dicts, &sender);
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
        move |_reporter| SettingsFs {
            survey: crate::sidecar::survey(&catalog),
            registry: crate::libraries::load_registry(),
            epub_backups: crate::epub_metadata::list_backups(),
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
        SettingsTab::Notifications => "settings_fill:notifications",
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
                // Until this pane is rebuilt in the section-41 style it
                // hosts the previous Storage builders: data locations,
                // libraries, sidecar recovery and backup.
                clear_box(&widgets.libraries_content);
                let paths = legacy_subhost(&widgets.libraries_content);
                build_paths(
                    &paths,
                    &self.catalog,
                    self.fs.as_ref().map(|fs| &fs.registry),
                    self.fs.as_ref().map(|fs| &fs.survey),
                );
                let backup = legacy_subhost(&widgets.libraries_content);
                build_backup(&backup, &self.catalog);
            }
            SettingsTab::DataExport => {
                clear_box(&widgets.data_content);
                let export = legacy_subhost(&widgets.data_content);
                build_export(&export, &self.catalog);
            }
            SettingsTab::BookFiles => {
                clear_box(&widgets.book_files_content);
                build_file_write(
                    &widgets.book_files_content,
                    &self.catalog,
                    snap,
                    self.fs.as_ref().map(|fs| fs.epub_backups.as_slice()),
                );
            }
            SettingsTab::WatchFolders => {
                clear_box(&widgets.watch_content);
                render_watch_cards_with(
                    &widgets.watch_content,
                    &self.catalog,
                    &snap.watch_rules,
                    &snap.shelves,
                );
            }
            SettingsTab::Dictionaries => {
                rebuild_dicts(&widgets.dict_list, &snap.dicts, sender);
            }
            SettingsTab::Metadata => {
                clear_box(&widgets.source_list);
                build_sources(&widgets.source_list, &self.catalog, snap);
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
            SettingsTab::Notifications => {
                build_notifications(&widgets.notify_list, sender);
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

/// A fresh vertical host for one of the legacy builders that still
/// owns its own heading cards, spaced the way the old content column
/// spaced them. Goes away pane by pane as the builders are rebuilt
/// in the section-41 row style.
fn legacy_subhost(host: &gtk::Box) -> gtk::Box {
    let sub = gtk::Box::new(gtk::Orientation::Vertical, 16);
    if host.first_child().is_some() {
        sub.set_margin_top(16);
    }
    host.append(&sub);
    sub
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
use crate::widgets::dialog::{dialog_shell, present_dialog, DialogSize};
use crate::widgets::dropdown::{dropdown, DropdownOption};
use crate::widgets::in_app_dialog::DialogExit;
use crate::widgets::pill_button::{boxed_ghost_button, icon_button};
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

/// One 48-high row: label over an optional caption at the left,
/// expanding so the caller's control lands at the right.
fn pane_row(rows: &gtk::Box, label: &str, caption: Option<&str>) -> gtk::Box {
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
    if let Some(caption) = caption {
        let cap = gtk::Label::new(Some(caption));
        cap.add_css_class("kalam-ui-caption");
        cap.set_halign(gtk::Align::Start);
        cap.set_xalign(0.0);
        cap.set_wrap(true);
        copy.append(&cap);
    }
    row.append(&copy);
    rows.append(&row);
    row
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
        let hovered = hovered.clone();
        let area = area.clone();
        motion.connect_enter(move |_, _, _| {
            hovered.set(true);
            area.queue_draw();
        });
        let hovered = hovered.clone();
        let area = area.clone();
        motion.connect_leave(move |_| {
            hovered.set(false);
            area.queue_draw();
        });
    }
    area.add_controller(motion);

    let focus = gtk::EventControllerFocus::new();
    {
        let focused = focused.clone();
        let area = area.clone();
        focus.connect_enter(move |_| {
            focused.set(true);
            area.queue_draw();
        });
        let focused = focused.clone();
        let area = area.clone();
        focus.connect_leave(move |_| {
            focused.set(false);
            area.queue_draw();
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
            body.set_markup(&format!(
                "<span font=\"{} {}\" line_height=\"{}\">{}</span>",
                gtk::glib::markup_escape_text(&family.borrow()),
                font_px.get(),
                format!("{:.1}", line_tenths.get() as f64 / 10.0),
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
/// One card with an accent icon, a title, an optional description, a hairline
/// divider, and a body host the caller fills with rows. Appended to `host`.
fn section_card(host: &gtk::Box, icon_name: &str, title: &str, desc: Option<&str>) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    card.add_css_class("kalam-section-card");

    let head = gtk::Box::new(gtk::Orientation::Vertical, 3);
    head.add_css_class("kalam-section-head");
    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let icon = crate::icons::symbolic_with_classes(icon_name, 16, &["kalam-section-icon"]);
    title_row.append(&icon);
    let title_label = gtk::Label::new(Some(title));
    title_label.add_css_class("kalam-section-title");
    title_label.set_halign(gtk::Align::Start);
    title_row.append(&title_label);
    head.append(&title_row);
    if let Some(text) = desc {
        let desc_label = gtk::Label::new(Some(text));
        desc_label.add_css_class("kalam-section-desc");
        desc_label.set_wrap(true);
        desc_label.set_xalign(0.0);
        desc_label.set_halign(gtk::Align::Start);
        head.append(&desc_label);
    }
    card.append(&head);

    let divider = gtk::Separator::new(gtk::Orientation::Horizontal);
    divider.add_css_class("kalam-section-divider");
    card.append(&divider);

    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    body.add_css_class("kalam-section-body");
    card.append(&body);

    host.append(&card);
    body
}

/// Label + description on the left, one control on the right. Hairlines
/// between rows come from CSS, so rows simply stack.
fn setting_row(body: &gtk::Box, label: &str, desc: &str, right: &impl IsA<gtk::Widget>) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    row.add_css_class("kalam-setting-row");

    let left = gtk::Box::new(gtk::Orientation::Vertical, 3);
    left.set_hexpand(true);
    let label_widget = gtk::Label::new(Some(label));
    label_widget.add_css_class("kalam-setting-label");
    label_widget.set_halign(gtk::Align::Start);
    label_widget.set_xalign(0.0);
    left.append(&label_widget);
    let desc_widget = gtk::Label::new(Some(desc));
    desc_widget.add_css_class("kalam-setting-desc");
    desc_widget.set_wrap(true);
    desc_widget.set_xalign(0.0);
    desc_widget.set_halign(gtk::Align::Start);
    left.append(&desc_widget);
    row.append(&left);

    let control = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    control.set_valign(gtk::Align::Center);
    control.append(right);
    row.append(&control);

    body.append(&row);
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

/// A pill chip label: `kalam-chip` plus one colour class.
fn chip_label(text: &str, class: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("kalam-chip");
    label.add_css_class(class);
    label.set_valign(gtk::Align::Center);
    label
}

/// Data locations: four label/desc rows with mono path boxes.
fn build_paths(
    host: &gtk::Box,
    catalog: &Arc<Catalog>,
    registry: Option<&crate::libraries::LibraryRegistry>,
    survey: Option<&crate::sidecar::SidecarSurvey>,
) {
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
    build_libraries(host, registry);
    build_recovery(host, catalog, survey);

    let body = section_card(
        host,
        "folder-symbolic",
        "Data locations",
        Some("These paths are set at first run. Moving data requires copying the files manually."),
    );
    let items = [
        (
            "Data Directory",
            "Base folder for Kalam application data",
            data_dir().to_string_lossy().into_owned(),
        ),
        (
            "Catalog Database",
            "SQLite database storing library books, shelves, tags, and reading progress",
            catalog_db().to_string_lossy().into_owned(),
        ),
        (
            "Library Files",
            "Directory where EPUB books and extracted covers are stored",
            library_dir().to_string_lossy().into_owned(),
        ),
        (
            "Dictionaries Directory",
            "Location for offline StarDict, SQLite, and TSV dictionary packs",
            dictionaries_dir().to_string_lossy().into_owned(),
        ),
    ];
    for (title, sub, path) in items {
        setting_row(&body, title, sub, &path_box(&path));
    }
}

/// P6.5 — the libraries card: which library is open, and the ones you know.
///
/// Deliberately a plain list plus "Add", not a full manager. Switching
/// re-launches rather than swapping underneath a running UI: pages hold open
/// database handles and half-drawn covers, so repointing mid-session would
/// leave them reading one library and writing to another.
fn build_libraries(host: &gtk::Box, reg: Option<&crate::libraries::LibraryRegistry>) {
    // The registry file is read by the page's slow worker (2c); until it
    // lands this card says so rather than guessing "no libraries".
    let Some(reg) = reg else {
        let body = section_card(
            host,
            "library-symbolic",
            "Libraries",
            Some("A library is a folder holding its own books, covers and database."),
        );
        setting_row(&body, "Libraries", "Reading the library list…", &loading_label());
        return;
    };
    let body = section_card(
        host,
        "library-symbolic",
        "Libraries",
        Some(concat!(
            "A library is a folder holding its own books, covers and database. ",
            "Books in one library do not appear in another. Copy the folder to ",
            "another machine and it opens there."
        )),
    );

    if reg.libraries.is_empty() {
        setting_row(
            &body,
            "Default library",
            "No library has been chosen, so Kalam is using its original folder.",
            &path_box(&crate::paths::legacy_data_dir().to_string_lossy()),
        );
    } else {
        let active = reg.active;
        for (i, lib) in reg.libraries.iter().enumerate() {
            let open_now = active == Some(i);

            // Path, then Open / Forget. Grouped in one row so a long path
            // cannot push the buttons off the edge.
            let controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            controls.set_valign(gtk::Align::Center);
            controls.append(&path_box(&lib.path.to_string_lossy()));

            if !open_now {
                let open = gtk::Button::with_label("Open");
                open.add_css_class("kalam-btn-outlined");
                open.set_valign(gtk::Align::Center);
                let name = lib.name.clone();
                open.connect_clicked(move |btn| {
                    // Ask first. Switching restarts the app, which closes
                    // whatever the user is reading -- doing that from a single
                    // unlabelled click would be rude.
                    let window = btn.root().and_then(|r| r.downcast::<gtk::Window>().ok());
                    let dialog = gtk::AlertDialog::builder()
                        .modal(true)
                        .message(format!("Open “{name}”?"))
                        .detail(concat!(
                            "Kalam will restart to open the other library. ",
                            "Anything you are reading will be closed; your ",
                            "place is saved."
                        ))
                        .buttons(vec![
                            "Cancel".to_string(),
                            "Restart and open".to_string(),
                        ])
                        .cancel_button(0)
                        .default_button(1)
                        .build();

                    let name = name.clone();
                    dialog.choose(
                        window.as_ref(),
                        gtk::gio::Cancellable::NONE,
                        move |answer| {
                            // Escape and the Cancel button both land here as
                            // an error or index 0. `glib::Error` is not
                            // comparable, so match rather than `!= Ok(1)`.
                            if !matches!(answer, Ok(1)) {
                                return;
                            }
                            let mut reg = crate::libraries::load_registry();
                            if !reg.select(i) {
                                // The registry changed under us -- another
                                // window, or a hand edit. Saying so beats
                                // silently doing nothing.
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
                            // restarting first would reopen the old library
                            // and look like the click did nothing.
                            let err = crate::libraries::restart_now();
                            crate::notify::error(
                                "Could not restart",
                                &format!("“{name}” will open next time you start Kalam. ({err})"),
                            );
                        },
                    );
                });
                controls.append(&open);
            }

            let forget = gtk::Button::with_label("Forget");
            forget.add_css_class("kalam-btn-outlined");
            forget.set_valign(gtk::Align::Center);
            let fname = lib.name.clone();
            forget.connect_clicked(move |_| {
                let mut reg = crate::libraries::load_registry();
                if !reg.forget(i) {
                    crate::notify::error(
                        "Could not remove that library",
                        "The library list changed. Reopen Settings and try again.",
                    );
                    return;
                }
                match crate::libraries::save_registry(&reg) {
                    // Say plainly that the books are untouched. "Forget" and
                    // "delete my library" must never be confusable.
                    Ok(()) => crate::notify::info(
                        "Library removed from the list",
                        &format!("“{fname}” — the folder and its books were not touched."),
                    ),
                    Err(err) => {
                        crate::notify::error("Could not save the library list", &err.to_string())
                    }
                }
            });
            controls.append(&forget);

            setting_row(
                &body,
                &lib.name,
                if open_now {
                    "Open now"
                } else {
                    "Opens on next launch when selected"
                },
                &controls,
            );
        }
    }

    // Adding a library is the one action wired up so far. Switching and
    // forgetting need the relaunch flow and a confirmation, which come next.
    let add = gtk::Button::with_label("Add library…");
    add.add_css_class("kalam-btn-outlined");
    add.set_valign(gtk::Align::Center);
    add.connect_clicked(move |btn| {
        // `FileDialog`, matching the rest of this file -- the older
        // `FileChooserNative` needs the dialog kept alive by hand.
        let dialog = gtk::FileDialog::builder()
            .title("Choose a folder for the library")
            .modal(true)
            .build();
        let window = btn.root().and_then(|r| r.downcast::<gtk::Window>().ok());
        dialog.select_folder(window.as_ref(), gtk::gio::Cancellable::NONE, move |res| {
            let Ok(folder) = res else { return };
            let Some(path) = folder.path() else { return };

            // Named after the folder: the user already chose a meaningful name
            // when they picked where to put it, and asking twice for the same
            // information is a step nobody wants.
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Library".to_string());

            // Say which of the two things just happened. "Added a library" is
            // ambiguous between "started an empty one" and "found my books",
            // and getting that wrong is alarming in one direction and
            // confusing in the other.
            let existing = crate::libraries::looks_like_a_library(&path);

            let mut reg = crate::libraries::load_registry();
            reg.add_or_select(&name, &path);
            match crate::libraries::save_registry(&reg) {
                Ok(()) => crate::notify::info(
                    if existing {
                        "Existing library found"
                    } else {
                        "New library created"
                    },
                    &format!("“{name}” — restart Kalam to open it."),
                ),
                Err(err) => {
                    crate::notify::error("Could not save the library list", &err.to_string());
                }
            }
        });
    });
    setting_row(
        &body,
        "Add a library",
        concat!(
            "Pick a folder. An empty folder starts a new library; ",
            "an existing Kalam library folder is reopened."
        ),
        &add,
    );
}

/// How much of this library could be rebuilt from its folders alone.
///
/// The point of `kalam.json` is that losing `catalog.db` should not lose your
/// tags, highlights and reading positions. That promise is only worth
/// something if it is checkable — the backups are written on code paths that
/// could quietly stop running, and nobody would notice until the day they
/// mattered. So the number is on screen.
fn build_recovery(
    host: &gtk::Box,
    catalog: &Arc<Catalog>,
    survey: Option<&crate::sidecar::SidecarSurvey>,
) {
    // The survey reads one sidecar file per book — the heaviest read on the
    // whole page, which is exactly why it runs on the page's slow worker
    // (2c). Until it lands the row says so instead of claiming "No books".
    let Some(survey) = survey else {
        let body = section_card(
            host,
            "document-save-symbolic",
            "Recovery",
            Some(concat!(
                "Each book folder keeps a kalam.json copy of its details, tags, ",
                "highlights and reading position. If the catalog database is ever ",
                "lost, this is what a rebuild would use."
            )),
        );
        setting_row(
            &body,
            "Books with a backup copy",
            "Checking your books' backup coverage…",
            &loading_label(),
        );
        return;
    };
    let body = section_card(
        host,
        "document-save-symbolic",
        "Recovery",
        Some(concat!(
            "Each book folder keeps a kalam.json copy of its details, tags, ",
            "highlights and reading position. If the catalog database is ever ",
            "lost, this is what a rebuild would use."
        )),
    );

    let missing = survey.missing();
    let summary = if survey.books == 0 {
        "No books yet.".to_string()
    } else if missing == 0 && survey.damaged == 0 {
        format!("All {} books have a backup copy.", survey.books)
    } else {
        let mut parts = vec![format!("{} of {} covered", survey.recoverable, survey.books)];
        if missing > 0 {
            parts.push(format!("{missing} missing"));
        }
        if survey.damaged > 0 {
            parts.push(format!("{} unreadable", survey.damaged));
        }
        parts.join(" · ")
    };

    let backfill = gtk::Button::with_label("Write missing copies");
    backfill.add_css_class("kalam-btn-outlined");
    backfill.set_valign(gtk::Align::Center);
    backfill.set_sensitive(missing > 0);
    {
        let catalog = catalog.clone();
        backfill.connect_clicked(move |btn| {
            let written = crate::sidecar::backfill_missing(&catalog);
            btn.set_sensitive(false);
            crate::notify::info(
                "Backup copies written",
                &format!("{written} book folders updated."),
            );
        });
    }
    setting_row(&body, "Books with a backup copy", &summary, &backfill);
}

/// A mono path box; long paths ellipsize but stay selectable and have the
/// full path as a tooltip.
fn path_box(path: &str) -> gtk::Box {
    let wrap = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    wrap.add_css_class("kalam-settings-path-box");
    let label = gtk::Label::new(Some(path));
    label.set_selectable(true);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label.set_max_width_chars(34);
    label.set_tooltip_text(Some(path));
    wrap.append(&label);
    wrap
}

/// Back up the catalog, and clear the reader cache.
fn build_backup(host: &gtk::Box, catalog: &Arc<Catalog>) {
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
    let body = section_card(host, "folder-download-symbolic", "Backup & cache", None);

    let backup_btn = gtk::Button::with_label("Back up library…");
    backup_btn.add_css_class("kalam-btn-outlined");
    backup_btn.set_valign(gtk::Align::Center);
    {
        let catalog = catalog.clone();
        backup_btn.connect_clicked(move |btn| {
            let dialog = gtk::FileDialog::builder()
                .title("Save library backup")
                .modal(true)
                .initial_name(format!("kalam-backup-{}.db", today_stamp()))
                .build();
            let window = btn.root().and_then(|r| r.downcast::<gtk::Window>().ok());
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
        });
    }
    setting_row(
        &body,
        "Back up library database",
        "Creates a snapshot of catalog.db containing all metadata, annotations, shelves, ratings, and reading history.",
        &backup_btn,
    );

    let right = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    right.set_valign(gtk::Align::Center);
    let size = crate::paths::reader_cache_size();
    let cache_badge = chip_label(&crate::epub_metadata::human_size(size), "kalam-chip-neutral");
    right.append(&cache_badge);
    let clear = gtk::Button::with_label("Clear cache");
    clear.add_css_class("kalam-btn-danger");
    clear.set_sensitive(size > 0);
    {
        let cache_badge = cache_badge.clone();
        clear.connect_clicked(move |btn| {
            let (files, freed) = crate::paths::clear_reader_cache();
            cache_badge.set_label(&format!("Freed {}", crate::epub_metadata::human_size(freed)));
            crate::notify::info(
                "Reader cache cleared",
                &format!(
                    "Removed {files} file{} · freed {}",
                    if files == 1 { "" } else { "s" },
                    crate::epub_metadata::human_size(freed)
                ),
            );
            btn.set_sensitive(false);
        });
    }
    right.append(&clear);
    setting_row(
        &body,
        "Extracted EPUB cache",
        "Unzipped copies of your books, kept so chapter lists and book details do not re-open the same EPUB every time. Safe to clear; they are rebuilt when a page needs them.",
        &right,
    );
}

/// Export saved reading data and quotes to Markdown, from Settings.
fn build_export(host: &gtk::Box, catalog: &Arc<Catalog>) {
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
    let body = section_card(host, "document-save-symbolic", "Export", None);

    let export_all_btn = gtk::Button::with_label("Export All Data");
    export_all_btn.add_css_class("kalam-btn-outlined");
    export_all_btn.set_valign(gtk::Align::Center);
    {
        let catalog = catalog.clone();
        export_all_btn.connect_clicked(move |_| {
            // Same rule as the quotes export above.
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
                        &format!("{count} item{} exported", if count == 1 { "" } else { "s" }),
                        &path.display().to_string(),
                    ),
                    Err(err) => crate::notify::error("Could not export reading data", &err),
                },
            );
        });
    }
    setting_row(
        &body,
        "Export All Data to Markdown",
        "Saves vocabulary, quotes, and highlights to ~/Kalam-Export.md",
        &export_all_btn,
    );

    let export_btn = gtk::Button::with_label("Export quotes");
    export_btn.add_css_class("kalam-btn-outlined");
    export_btn.set_valign(gtk::Align::Center);
    {
        let catalog = catalog.clone();
        export_btn.connect_clicked(move |_| {
            // The export reads the catalog and writes a file: on a task,
            // per ARCH.md. `catalog` is cloned per click because the
            // handler can fire more than once.
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
        });
    }
    setting_row(
        &body,
        "Export quotes to Markdown",
        "Saves all saved quotes to ~/Quotes.md",
        &export_btn,
    );
}

/// Installed dictionary packs: icon, name, mono meta, Remove — plus the
/// filled Import button in the card footer.
fn rebuild_dicts(
    host: &gtk::Box,
    dicts: &[crate::db::Dictionary],
    sender: &ComponentSender<SettingsPageModel>,
) {
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
    let body = section_card(
        host,
        "book-open-symbolic",
        "Installed packs",
        Some("StarDict (.ifo/.idx/.dict), SQLite (.db), and TSV formats are supported."),
    );

    if dicts.is_empty() {
        let empty = gtk::Label::new(Some(
            "No dictionaries yet. Import StarDict, SQLite or TSV packs for offline lookup (D key in the reader).",
        ));
        empty.add_css_class("kalam-placeholder");
        empty.set_wrap(true);
        empty.set_xalign(0.0);
        body.append(&empty);
    } else {
        let last = dicts.len() - 1;
        for (index, d) in dicts.iter().enumerate() {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            row.add_css_class("kalam-setting-row");

            let icon = gtk::Box::new(gtk::Orientation::Vertical, 0);
            icon.add_css_class("kalam-dict-icon");
            icon.set_size_request(32, 32);
            icon.set_halign(gtk::Align::Center);
            icon.set_valign(gtk::Align::Center);
            icon.append(&crate::icons::symbolic_with_classes(
                "book-open-symbolic",
                16,
                &["kalam-dict-icon-glyph"],
            ));
            row.append(&icon);

            let info = gtk::Box::new(gtk::Orientation::Vertical, 2);
            info.set_hexpand(true);
            let name_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let name_label = gtk::Label::new(Some(&d.name));
            name_label.add_css_class("kalam-setting-label");
            name_label.set_halign(gtk::Align::Start);
            name_label.set_xalign(0.0);
            name_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            name_row.append(&name_label);
            // Phase 9: the top of the list is what the merged store consults
            // first — say so, or the ordering is unexplained.
            if index == 0 {
                name_row.append(&chip_label("speaks first", "kalam-chip-info"));
            }
            info.append(&name_row);
            let meta = match &d.lang {
                Some(lang) => format!("{lang} · {} entries", d.entry_count),
                None => format!("{} entries", d.entry_count),
            };
            let meta_label = gtk::Label::new(Some(&meta));
            meta_label.add_css_class("kalam-dict-meta");
            meta_label.set_halign(gtk::Align::Start);
            meta_label.set_xalign(0.0);
            info.append(&meta_label);
            row.append(&info);

            let id = d.id;
            let up = gtk::Button::new();
            up.set_child(Some(&crate::icons::symbolic_with_classes(
                "go-up-symbolic",
                16,
                &["kalam-inline-icon"],
            )));
            up.add_css_class("kalam-mini-btn");
            up.set_valign(gtk::Align::Center);
            up.set_sensitive(index > 0);
            up.set_tooltip_text(Some("Speak before the dictionaries above"));
            {
                let s = sender.clone();
                up.connect_clicked(move |_| s.input(SettingsMsg::MoveDict(id, -1)));
            }
            row.append(&up);

            let down = gtk::Button::new();
            down.set_child(Some(&crate::icons::symbolic_with_classes(
                "go-down-symbolic",
                16,
                &["kalam-inline-icon"],
            )));
            down.add_css_class("kalam-mini-btn");
            down.set_valign(gtk::Align::Center);
            down.set_sensitive(index < last);
            down.set_tooltip_text(Some("Speak after the dictionaries below"));
            {
                let s = sender.clone();
                down.connect_clicked(move |_| s.input(SettingsMsg::MoveDict(id, 1)));
            }
            row.append(&down);

            let remove = gtk::Button::with_label("Remove");
            remove.add_css_class("kalam-btn-danger");
            remove.add_css_class("kalam-btn-sm");
            remove.set_valign(gtk::Align::Center);
            let s = sender.clone();
            remove.connect_clicked(move |_| s.input(SettingsMsg::DeleteDict(id)));
            row.append(&remove);

            body.append(&row);
        }
    }

    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    footer.add_css_class("kalam-card-footer");
    let import_btn = gtk::Button::new();
    import_btn.set_child(Some(&crate::icons::labelled(
        "list-add-symbolic",
        16,
        "Import dictionary",
        6,
    )));
    import_btn.add_css_class("kalam-btn-filled");
    {
        let s = sender.clone();
        import_btn.connect_clicked(move |_| s.input(SettingsMsg::ImportDict));
    }
    footer.append(&import_btn);
    let refresh_btn = gtk::Button::new();
    refresh_btn.set_child(Some(&crate::icons::symbolic_with_classes(
        "view-refresh-symbolic",
        16,
        &["kalam-inline-icon"],
    )));
    refresh_btn.add_css_class("kalam-btn-ghost");
    refresh_btn.set_tooltip_text(Some("Rescan dictionary packs"));
    {
        let s = sender.clone();
        refresh_btn.connect_clicked(move |_| s.input(SettingsMsg::Refresh));
    }
    footer.append(&refresh_btn);
    body.append(&footer);
}

fn build_shelf_dropdown(
    shelves: &[(Option<i64>, String)],
    current_shelf_id: Option<i64>,
    on_change: impl Fn(Option<i64>) + 'static,
) -> gtk::DropDown {
    let titles: Vec<&str> = shelves.iter().map(|(_, name)| name.as_str()).collect();
    let drop = gtk::DropDown::from_strings(&titles);
    drop.set_valign(gtk::Align::Center);

    let init_idx = shelves
        .iter()
        .position(|(id, _)| *id == current_shelf_id)
        .unwrap_or(0);
    drop.set_selected(init_idx as u32);

    let shelves_map = shelves.to_vec();
    drop.connect_selected_notify(move |combo| {
        let idx = combo.selected() as usize;
        if let Some((shelf_id, _)) = shelves_map.get(idx) {
            on_change(*shelf_id);
        }
    });

    drop
}

/// Re-render from live data — the click paths (add / remove a watch folder)
/// still read inline, as they always did; this wrapper keeps them working.
fn render_watch_cards(container: &gtk::Box, catalog: &std::sync::Arc<crate::db::Catalog>) {
    let rules = crate::watch_folder::load_watch_rules(catalog);
    let shelves = catalog.list_shelves().unwrap_or_default();
    render_watch_cards_with(container, catalog, &rules, &shelves);
}

/// Build the watch-folder cards from data the caller already holds: the
/// page's snapshot on first build (7.1 step 2c — no read happens on the UI
/// thread there), or a fresh click-time read through the wrapper above.
fn render_watch_cards_with(
    container: &gtk::Box,
    catalog: &std::sync::Arc<crate::db::Catalog>,
    rules: &[crate::watch_folder::WatchFolderRule],
    shelves: &[crate::db::Shelf],
) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    let mut shelf_choices: Vec<(Option<i64>, String)> =
        vec![(None, "Library Default (No shelf)".to_string())];
    for s in shelves {
        shelf_choices.push((Some(s.id), s.name.clone()));
    }

    if rules.is_empty() {
        let placeholder = gtk::Box::new(gtk::Orientation::Vertical, 6);
        placeholder.set_margin_top(12);
        placeholder.set_margin_bottom(12);

        let empty = gtk::Label::new(Some(
            "No watch folders configured yet.\nClick “+ Add Watch Folder” to monitor folders and route books to shelves.",
        ));
        empty.add_css_class("kalam-placeholder");
        empty.set_wrap(true);
        empty.set_justify(gtk::Justification::Center);
        placeholder.append(&empty);
        container.append(&placeholder);
        return;
    }

    for rule in rules {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
        card.add_css_class("kalam-watch-card");

        // Header: Path + Switch + Remove
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        header.add_css_class("kalam-watch-card-header");

        let folder_icon = gtk::Image::from_icon_name("folder-symbolic");
        header.append(&folder_icon);

        let path_display = rule.path.to_string_lossy().into_owned();
        let path_chip = chip_label(&path_display, "kalam-chip-neutral");
        path_chip.set_tooltip_text(Some(&path_display));
        header.append(&path_chip);

        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        header.append(&spacer);

        let sw = toggle_switch(rule.enabled, {
            let cat = catalog.clone();
            let rule_id = rule.id.clone();
            move |active| {
                let mut all_rules = crate::watch_folder::load_watch_rules(&cat);
                if let Some(r) = all_rules.iter_mut().find(|r| r.id == rule_id) {
                    r.enabled = active;
                    crate::watch_folder::save_watch_rules(&cat, &all_rules);
                    crate::watch_folder::request_reload();
                }
            }
        });
        sw.set_tooltip_text(Some("Enable or pause monitoring for this folder"));
        header.append(&sw);

        let remove_btn = gtk::Button::from_icon_name("user-trash-symbolic");
        remove_btn.add_css_class("flat");
        remove_btn.add_css_class("danger");
        remove_btn.set_tooltip_text(Some("Remove this watch folder"));
        {
            let cat = catalog.clone();
            let cont = container.clone();
            let rule_id = rule.id.clone();
            remove_btn.connect_clicked(move |_| {
                let mut all_rules = crate::watch_folder::load_watch_rules(&cat);
                all_rules.retain(|r| r.id != rule_id);
                crate::watch_folder::save_watch_rules(&cat, &all_rules);
                crate::watch_folder::request_reload();
                render_watch_cards(&cont, &cat);
            });
        }
        header.append(&remove_btn);
        card.append(&header);

        // Format row 1: EPUB
        let row_epub = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row_epub.add_css_class("kalam-watch-format-row");
        row_epub.set_valign(gtk::Align::Center);

        let epub_check = gtk::CheckButton::with_label("Novels & Ebooks (EPUB)");
        epub_check.set_active(rule.import_epub);
        epub_check.add_css_class("kalam-watch-format-label");
        row_epub.append(&epub_check);

        let arrow_epub = gtk::Label::new(Some("Put into:"));
        arrow_epub.add_css_class("kalam-setting-desc");
        row_epub.append(&arrow_epub);

        let epub_drop = build_shelf_dropdown(
            &shelf_choices,
            rule.epub_shelf_id,
            {
                let cat = catalog.clone();
                let rule_id = rule.id.clone();
                move |sid| {
                    let mut all_rules = crate::watch_folder::load_watch_rules(&cat);
                    if let Some(r) = all_rules.iter_mut().find(|r| r.id == rule_id) {
                        r.epub_shelf_id = sid;
                        crate::watch_folder::save_watch_rules(&cat, &all_rules);
                    }
                }
            },
        );
        epub_drop.set_sensitive(rule.import_epub);
        row_epub.append(&epub_drop);

        {
            let cat = catalog.clone();
            let rule_id = rule.id.clone();
            let drop_ref = epub_drop.clone();
            epub_check.connect_toggled(move |chk| {
                let active = chk.is_active();
                drop_ref.set_sensitive(active);
                let mut all_rules = crate::watch_folder::load_watch_rules(&cat);
                if let Some(r) = all_rules.iter_mut().find(|r| r.id == rule_id) {
                    r.import_epub = active;
                    crate::watch_folder::save_watch_rules(&cat, &all_rules);
                    crate::watch_folder::request_reload();
                }
            });
        }
        card.append(&row_epub);

        // Format row 2: PDF
        let row_pdf = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row_pdf.add_css_class("kalam-watch-format-row");
        row_pdf.set_valign(gtk::Align::Center);

        let pdf_check = gtk::CheckButton::with_label("Documents & Papers (PDF)");
        pdf_check.set_active(rule.import_pdf);
        pdf_check.add_css_class("kalam-watch-format-label");
        row_pdf.append(&pdf_check);

        let arrow_pdf = gtk::Label::new(Some("Put into:"));
        arrow_pdf.add_css_class("kalam-setting-desc");
        row_pdf.append(&arrow_pdf);

        let pdf_drop = build_shelf_dropdown(
            &shelf_choices,
            rule.pdf_shelf_id,
            {
                let cat = catalog.clone();
                let rule_id = rule.id.clone();
                move |sid| {
                    let mut all_rules = crate::watch_folder::load_watch_rules(&cat);
                    if let Some(r) = all_rules.iter_mut().find(|r| r.id == rule_id) {
                        r.pdf_shelf_id = sid;
                        crate::watch_folder::save_watch_rules(&cat, &all_rules);
                    }
                }
            },
        );
        pdf_drop.set_sensitive(rule.import_pdf);
        row_pdf.append(&pdf_drop);

        {
            let cat = catalog.clone();
            let rule_id = rule.id.clone();
            let drop_ref = pdf_drop.clone();
            pdf_check.connect_toggled(move |chk| {
                let active = chk.is_active();
                drop_ref.set_sensitive(active);
                let mut all_rules = crate::watch_folder::load_watch_rules(&cat);
                if let Some(r) = all_rules.iter_mut().find(|r| r.id == rule_id) {
                    r.import_pdf = active;
                    crate::watch_folder::save_watch_rules(&cat, &all_rules);
                    crate::watch_folder::request_reload();
                }
            });
        }
        card.append(&row_pdf);

        // Format row 3: Comics (CBZ / CBR)
        let row_comics = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row_comics.add_css_class("kalam-watch-format-row");
        row_comics.set_valign(gtk::Align::Center);

        let comics_check = gtk::CheckButton::with_label("Comics & Manga (CBZ/CBR)");
        comics_check.set_active(rule.import_comics);
        comics_check.add_css_class("kalam-watch-format-label");
        row_comics.append(&comics_check);

        let arrow_comics = gtk::Label::new(Some("Put into:"));
        arrow_comics.add_css_class("kalam-setting-desc");
        row_comics.append(&arrow_comics);

        let comics_drop = build_shelf_dropdown(
            &shelf_choices,
            rule.comics_shelf_id,
            {
                let cat = catalog.clone();
                let rule_id = rule.id.clone();
                move |sid| {
                    let mut all_rules = crate::watch_folder::load_watch_rules(&cat);
                    if let Some(r) = all_rules.iter_mut().find(|r| r.id == rule_id) {
                        r.comics_shelf_id = sid;
                        crate::watch_folder::save_watch_rules(&cat, &all_rules);
                    }
                }
            },
        );
        comics_drop.set_sensitive(rule.import_comics);
        row_comics.append(&comics_drop);

        {
            let cat = catalog.clone();
            let rule_id = rule.id.clone();
            let drop_ref = comics_drop.clone();
            comics_check.connect_toggled(move |chk| {
                let active = chk.is_active();
                drop_ref.set_sensitive(active);
                let mut all_rules = crate::watch_folder::load_watch_rules(&cat);
                if let Some(r) = all_rules.iter_mut().find(|r| r.id == rule_id) {
                    r.import_comics = active;
                    crate::watch_folder::save_watch_rules(&cat, &all_rules);
                    crate::watch_folder::request_reload();
                }
            });
        }
        card.append(&row_comics);

        // Options: Subfolder shelves + Clean up
        let options_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
        options_box.set_margin_top(4);

        let subfolder_check = gtk::CheckButton::with_label(
            "Automatically create shelves from subfolders (e.g. Manga/One Piece ➔ \"One Piece\")",
        );
        subfolder_check.set_active(rule.subfolder_shelves);
        {
            let cat = catalog.clone();
            let rule_id = rule.id.clone();
            subfolder_check.connect_toggled(move |chk| {
                let mut all_rules = crate::watch_folder::load_watch_rules(&cat);
                if let Some(r) = all_rules.iter_mut().find(|r| r.id == rule_id) {
                    r.subfolder_shelves = chk.is_active();
                    crate::watch_folder::save_watch_rules(&cat, &all_rules);
                }
            });
        }
        options_box.append(&subfolder_check);

        let cleanup_check = gtk::CheckButton::with_label(
            "Clean up: delete original file from folder after safe import",
        );
        cleanup_check.set_active(rule.cleanup_original);
        {
            let cat = catalog.clone();
            let rule_id = rule.id.clone();
            cleanup_check.connect_toggled(move |chk| {
                let mut all_rules = crate::watch_folder::load_watch_rules(&cat);
                if let Some(r) = all_rules.iter_mut().find(|r| r.id == rule_id) {
                    r.cleanup_original = chk.is_active();
                    crate::watch_folder::save_watch_rules(&cat, &all_rules);
                }
            });
        }
        options_box.append(&cleanup_check);

        card.append(&options_box);
        container.append(&card);
    }
}

/// Toggle for writing metadata back into the EPUB itself, and deleting backups.
fn build_file_write(
    host: &gtk::Box,
    catalog: &Arc<Catalog>,
    snap: &SettingsSnapshot,
    epub_backups: Option<&[(std::path::PathBuf, u64)]>,
) {
    use crate::epub_metadata::set_write_enabled;

    while let Some(child) = host.first_child() {
        host.remove(&child);
    }

    // 1. EPUB Sanitizer / Polish Engine. Every initial state below comes
    // from the snapshot (7.1 step 2c); the catalog stays for the
    // click-time writes.
    let polish_body = section_card(
        host,
        "edit-clear-symbolic",
        "EPUB polish & sanitizer",
        Some("Clean, repair, and normalize EPUB files when importing into Kalam."),
    );
    {
        let catalog = catalog.clone();
        let sw = toggle_switch(snap.clean_on_import, move |on| {
            crate::epub_sanitizer::set_clean_on_import(&catalog, on);
        });
        setting_row(
            &polish_body,
            "Sanitize and polish EPUBs on import",
            "On: automatically removes toxic styles (tiny fonts, forced black/white colors, fixed margins), repairs malformed XML tags, and regenerates missing Table of Contents (toc.ncx) so books look consistent and readable. Off: EPUBs are stored untouched as imported.",
            &sw,
        );
    }

    // 2. Auto-Import Watch Folders (Multi-folder routing)
    let watch_body = section_card(
        host,
        "folder-download-symbolic",
        "Auto-import watch folders",
        Some("Automatically import and organize new books and comics added to folders on your computer."),
    );
    {
        let top_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        top_row.set_valign(gtk::Align::Center);

        let add_btn = gtk::Button::with_label("+ Add Watch Folder");
        add_btn.add_css_class("kalam-btn-subtle");

        let cards_container = gtk::Box::new(gtk::Orientation::Vertical, 10);

        let catalog_add = catalog.clone();
        let cc_add = cards_container.clone();
        add_btn.connect_clicked(move |btn| {
            let root = btn.root();
            let window = root.and_then(|r| r.downcast::<gtk::Window>().ok());
            let dialog = gtk::FileDialog::builder()
                .title("Select Folder to Watch")
                .modal(true)
                .build();

            let cat = catalog_add.clone();
            let cc = cc_add.clone();
            dialog.select_folder(window.as_ref(), gtk::gio::Cancellable::NONE, move |res| {
                let Ok(folder) = res else { return };
                let Some(path) = folder.path() else { return };
                let mut rules = crate::watch_folder::load_watch_rules(&cat);
                if rules.iter().any(|r| r.path == path) {
                    crate::notify::info("Watch Folder", "This folder is already in your watch list.");
                    return;
                }
                rules.push(crate::watch_folder::WatchFolderRule::new(path));
                crate::watch_folder::save_watch_rules(&cat, &rules);
                crate::watch_folder::request_reload();
                render_watch_cards(&cc, &cat);
            });
        });
        top_row.append(&add_btn);

        watch_body.append(&top_row);

        render_watch_cards_with(
            &cards_container,
            catalog,
            &snap.watch_rules,
            &snap.shelves,
        );
        watch_body.append(&cards_container);
    }

    // 3. EPUB writeback & backups
    let body = section_card(host, "text-x-generic-symbolic", "EPUB writeback", None);

    {
        let catalog = catalog.clone();
        let sw = toggle_switch(snap.writeback_enabled, move |on| {
            set_write_enabled(&catalog, on);
        });
        setting_row(
            &body,
            "Also write metadata into the EPUB file",
            "On: saving in Edit metadata also updates the book file, so Calibre and other readers see your changes. The untouched original is kept once as <name>.epub.orig, and the new file is only swapped in after it is verified. Off: edits stay inside Kalam and your files are never modified.",
            &sw,
        );
    }

    let right = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    right.set_valign(gtk::Align::Center);
    // The backup listing is a directory walk, so it rides the page's slow
    // worker (2c); until it lands the chip says so and delete stays off.
    let summary_text = match epub_backups {
        None => "Checking…".to_string(),
        Some(backups) => {
            let total: u64 = backups.iter().map(|(_, size)| size).sum();
            if backups.is_empty() {
                "No originals kept".to_string()
            } else {
                format!(
                    "{} original{} · {}",
                    backups.len(),
                    if backups.len() == 1 { "" } else { "s" },
                    crate::epub_metadata::human_size(total)
                )
            }
        }
    };
    let summary = chip_label(&summary_text, "kalam-chip-neutral");
    right.append(&summary);
    let clean = gtk::Button::with_label("Delete backups");
    clean.add_css_class("kalam-btn-danger");
    clean.set_sensitive(epub_backups.is_some_and(|b| !b.is_empty()));
    {
        let summary = summary.clone();
        clean.connect_clicked(move |btn| {
            let (count, freed) = crate::epub_metadata::delete_backups();
            summary.set_label(&format!(
                "Deleted {count} backup{}, freed {}.",
                if count == 1 { "" } else { "s" },
                crate::epub_metadata::human_size(freed)
            ));
            crate::notify::info(
                "Original backups deleted",
                &format!(
                    "Removed {count} backup{} · freed {}",
                    if count == 1 { "" } else { "s" },
                    crate::epub_metadata::human_size(freed)
                ),
            );
            btn.set_sensitive(false);
        });
    }
    right.append(&clean);
    setting_row(
        &body,
        "Original backups",
        "Kept once per book when writeback is on. Deleting is permanent: you lose the ability to undo metadata written into those files.",
        &right,
    );
}

/// `2026-07-28`, for backup filenames.
fn today_stamp() -> String {
    gtk::glib::DateTime::now_local()
        .and_then(|d| d.format("%Y-%m-%d"))
        .map(|s| s.to_string())
        .unwrap_or_default()
}

/// Explains why a country code is needed at all.
fn country_hint_label() -> gtk::Label {
    let label = gtk::Label::new(Some(
        "Two-letter code. Google only serves results for countries it has rights in.",
    ));
    label.add_css_class("kalam-muted");
    label.set_wrap(true);
    label.set_xalign(0.0);
    label
}

/// One section card per metadata provider, each with a real switch and its
/// own extra rows.
fn build_sources(host: &gtk::Box, catalog: &Arc<Catalog>, snap: &SettingsSnapshot) {
    use crate::metadata::{set_source_enabled, SourceId};

    while let Some(child) = host.first_child() {
        host.remove(&child);
    }

    let ol_body = section_card(
        host,
        "system-search-symbolic",
        "Open Library",
        Some("Internet Archive. No key needed. Strong on older and public-domain titles."),
    );
    {
        let catalog = catalog.clone();
        let sw = toggle_switch(snap.source_open_library, move |on| {
            set_source_enabled(&catalog, SourceId::OpenLibrary, on);
        });
        setting_row(
            &ol_body,
            "Enable Open Library lookup",
            "Adds a search panel inside the metadata editor.",
            &sw,
        );
    }

    let gb_body = section_card(
        host,
        "system-search-symbolic",
        "Google Books",
        Some(
            "Broad coverage, good for recent and non-English books. Works without a key, but anonymous requests share a global quota and can be rate limited.",
        ),
    );
    {
        let catalog = catalog.clone();
        let sw = toggle_switch(snap.source_google_books, move |on| {
            set_source_enabled(&catalog, SourceId::GoogleBooks, on);
        });
        setting_row(
            &gb_body,
            "Enable Google Books lookup",
            "Adds results from Google Books to the metadata editor.",
            &sw,
        );
    }

    // API key row.
    {
        let right = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        right.set_valign(gtk::Align::Center);
        let entry = gtk::Entry::new();
        entry.set_placeholder_text(Some("Optional API key — lifts the shared rate limit"));
        entry.set_text(&snap.google_books_key);
        entry.set_hexpand(true);
        entry.add_css_class("kalam-setting-entry");
        right.append(&entry);
        let save = gtk::Button::with_label("Save key");
        save.add_css_class("kalam-btn-outlined");
        save.add_css_class("kalam-btn-sm");
        {
            let catalog = catalog.clone();
            let entry = entry.clone();
            save.connect_clicked(move |_| {
                let key = entry.text().trim().to_string();
                catalog.set_pref("meta.googlebooks.key", &key);
                if key.is_empty() {
                    crate::notify::info("Google Books key cleared", "Using the shared quota");
                } else {
                    crate::notify::success("Google Books key saved", "Your own quota is in use");
                }
            });
        }
        right.append(&save);
        setting_row(
            &gb_body,
            "API key",
            "Free from console.cloud.google.com — create a project, enable the Books API, then make an API key. No card required.",
            &right,
        );
    }

    // Country row.
    {
        let right = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        right.set_valign(gtk::Align::Center);
        let country = gtk::Entry::new();
        country.set_max_length(2);
        country.set_width_chars(4);
        country.set_placeholder_text(Some("IN"));
        country.set_text(&snap.google_books_country);
        country.add_css_class("kalam-setting-entry");
        right.append(&country);
        let save = gtk::Button::with_label("Save");
        save.add_css_class("kalam-btn-outlined");
        save.add_css_class("kalam-btn-sm");
        {
            let catalog = catalog.clone();
            let country = country.clone();
            save.connect_clicked(move |_| {
                let code = country.text().trim().to_uppercase();
                catalog.set_pref("meta.googlebooks.country", &code);
                crate::notify::success("Country saved", &code);
            });
        }
        right.append(&save);
        let hint = country_hint_label();
        right.append(&hint);
        setting_row(
            &gb_body,
            "Country",
            "Two-letter code. Google only serves results for countries it has rights in.",
            &right,
        );
    }
}

/// Recent notifications, so a toast that faded can still be read.
fn build_notifications(host: &gtk::Box, sender: &ComponentSender<SettingsPageModel>) {
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
    let body = section_card(
        host,
        "preferences-system-notifications-symbolic",
        "Activity log",
        Some("The last 25 events this session. Toasts fade; this keeps the record."),
    );

    let entries = crate::notify::history();
    if entries.is_empty() {
        let empty = gtk::Label::new(Some("Nothing yet this session."));
        empty.add_css_class("kalam-muted");
        empty.set_halign(gtk::Align::Start);
        body.append(&empty);
    } else {
        for entry in entries.iter().take(25) {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            row.add_css_class("kalam-setting-row");

            let badge_class = match entry.kind {
                crate::notify::Kind::Success => "kalam-chip-success",
                crate::notify::Kind::Error => "kalam-chip-danger",
                crate::notify::Kind::Info => "kalam-chip-info",
                crate::notify::Kind::Progress => "kalam-chip-neutral",
            };
            row.append(&chip_label(entry.kind.label(), badge_class));

            let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
            text.set_hexpand(true);
            let title = gtk::Label::new(Some(&entry.title));
            title.add_css_class("kalam-setting-label");
            title.set_halign(gtk::Align::Start);
            title.set_xalign(0.0);
            title.set_ellipsize(gtk::pango::EllipsizeMode::End);
            text.append(&title);
            if !entry.detail.trim().is_empty() {
                let detail = gtk::Label::new(Some(entry.detail.trim()));
                detail.add_css_class("kalam-setting-desc");
                detail.set_halign(gtk::Align::Start);
                detail.set_xalign(0.0);
                detail.set_ellipsize(gtk::pango::EllipsizeMode::End);
                detail.set_tooltip_text(Some(&entry.detail));
                text.append(&detail);
            }
            row.append(&text);

            let at = gtk::Label::new(Some(&entry.at));
            at.add_css_class("kalam-muted");
            at.set_valign(gtk::Align::Center);
            row.append(&at);

            body.append(&row);
        }
    }

    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    footer.set_halign(gtk::Align::End);
    footer.add_css_class("kalam-card-footer");
    let clear = gtk::Button::with_label("Clear history");
    clear.add_css_class("kalam-btn-ghost");
    clear.add_css_class("kalam-btn-sm");
    {
        let s = sender.clone();
        clear.connect_clicked(move |_| s.input(SettingsMsg::ClearNotifications));
    }
    footer.append(&clear);
    body.append(&footer);
}
