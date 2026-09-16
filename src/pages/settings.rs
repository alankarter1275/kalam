use crate::db::Catalog;
use crate::dict;
use crate::paths::{catalog_db, data_dir, dictionaries_dir, library_dir};
use crate::service::LibraryService;
use gtk::prelude::*;
use relm4::prelude::*;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Appearance,
    Storage,
    Dictionaries,
    BookFiles,
    Metadata,
    Notifications,
}

impl SettingsTab {
    pub fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Storage => "Storage & Backup",
            Self::Dictionaries => "Dictionaries",
            Self::BookFiles => "Book Files",
            Self::Metadata => "Metadata Sources",
            Self::Notifications => "Notifications",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Appearance => "applications-graphics-symbolic",
            Self::Storage => "drive-harddisk-symbolic",
            Self::Dictionaries => "book-open-symbolic",
            Self::BookFiles => "text-x-generic-symbolic",
            Self::Metadata => "system-search-symbolic",
            Self::Notifications => "preferences-system-notifications-symbolic",
        }
    }

    pub fn subtitle(self) -> &'static str {
        match self {
            Self::Appearance => "Every theme is dark. Changes apply immediately.",
            Self::Storage => {
                "Manage where Kalam stores your catalog database, library EPUB files, backups, and reader cache."
            }
            Self::Dictionaries => {
                "Data locations and offline dictionary packs for lookup in the reader."
            }
            Self::BookFiles => {
                "Manage EPUB file metadata writeback and original backup files."
            }
            Self::Metadata => {
                "Used by Edit metadata. Results from every enabled source are merged and badged with their origin."
            }
            Self::Notifications => {
                "History log of recent activity, alerts, and toasts."
            }
        }
    }
}

/// The settings nav rail, grouped exactly like the reference design.
const NAV_GROUPS: &[(&str, &[SettingsTab])] = &[
    ("Appearance", &[SettingsTab::Appearance]),
    (
        "Library",
        &[
            SettingsTab::Storage,
            SettingsTab::Dictionaries,
            SettingsTab::BookFiles,
        ],
    ),
    ("Sources", &[SettingsTab::Metadata]),
    ("App", &[SettingsTab::Notifications]),
];

/// Theme families for the picker: (base id, display name, description,
/// label prefix used to derive each variant's short name).
const THEME_FAMILIES: &[(&str, &str, &str, &str)] = &[
    (
        "onedark",
        "One Dark",
        "Warm-tinted greys, soft blue accent. The default.",
        "One Dark",
    ),
    (
        "tokyonight",
        "Tokyo Night",
        "Deep indigo, high-chroma violet and blue accents.",
        "Tokyo Night",
    ),
    (
        "everforest",
        "Everforest",
        "Low-saturation greens and warm greys. Easy on the eyes.",
        "Everforest",
    ),
    (
        "catppuccin",
        "Catppuccin Mocha",
        "Soft pastels on a near-black lavender base.",
        "Catppuccin",
    ),
    (
        "gruvbox",
        "Gruvbox",
        "Warm retro browns and ochres — a classic.",
        "Gruvbox",
    ),
    (
        "ayu",
        "Ayu",
        "Muted slate with a distinctive amber accent.",
        "Ayu",
    ),
    (
        "nord",
        "Nord",
        "Cool arctic blue-greys. No darker variant — already deep.",
        "Nord",
    ),
];

const THEME_BUTTON_PREFIX: &str = "theme-btn-";
const THEME_BADGE_PREFIX: &str = "theme-badge-";
const THEME_CHECK_PREFIX: &str = "theme-check-";

#[derive(Debug)]
pub enum SettingsMsg {
    SelectTab(SettingsTab),
    ClearNotifications,
    ImportDict,
    DeleteDict(i64),
    /// Reorder the merged-store priority (Phase 9): id, delta (−1 up / +1 down).
    MoveDict(i64, i64),
    Refresh,
}

pub struct SettingsPageModel {
    catalog: Arc<Catalog>,
    #[allow(dead_code)]
    service: LibraryService,
    dicts: Vec<crate::db::Dictionary>,
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

            // Left settings navigation rail, grouped.
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                add_css_class: "kalam-settings-nav",
                set_spacing: 2,
                set_hexpand: false,
                set_vexpand: true,

                #[name = "nav_list"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 2,
                },

                gtk::Box {
                    set_vexpand: true,
                },
            },

            // Right category content area.
            #[name = "scroller"]
            gtk::ScrolledWindow {
                set_hexpand: true,
                set_vexpand: true,
                set_hscrollbar_policy: gtk::PolicyType::Never,
                set_vscrollbar_policy: gtk::PolicyType::Automatic,

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    add_css_class: "kalam-settings-content",
                    set_spacing: 16,
                    set_hexpand: true,

                    #[name = "tab_title"]
                    gtk::Label {
                        add_css_class: "kalam-page-title",
                        set_halign: gtk::Align::Start,
                    },

                    #[name = "tab_subtitle"]
                    gtk::Label {
                        add_css_class: "kalam-page-sub",
                        set_halign: gtk::Align::Start,
                        set_wrap: true,
                    },

                    // 1. Appearance
                    #[name = "appearance_box"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 16,
                        #[watch]
                        set_visible: model.active_tab == SettingsTab::Appearance,

                        #[name = "theme_grid"]
                        gtk::Grid {
                            set_column_homogeneous: true,
                            set_row_spacing: 12,
                            set_column_spacing: 12,
                            set_hexpand: true,
                        },
                    },

                    // 2. Storage & Backup
                    #[name = "storage_box"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 16,
                        #[watch]
                        set_visible: model.active_tab == SettingsTab::Storage,

                        #[name = "paths_host"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 16,
                        },

                        #[name = "backup_row"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 16,
                        },

                        #[name = "export_row"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 16,
                        },
                    },

                    // 3. Dictionaries
                    #[name = "dicts_box"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 16,
                        #[watch]
                        set_visible: model.active_tab == SettingsTab::Dictionaries,

                        #[name = "dict_list"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 16,
                        },
                    },

                    // 4. Book Files
                    #[name = "book_files_box"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 16,
                        #[watch]
                        set_visible: model.active_tab == SettingsTab::BookFiles,

                        #[name = "file_write_row"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 16,
                        },
                    },

                    // 5. Metadata Sources
                    #[name = "metadata_box"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 16,
                        #[watch]
                        set_visible: model.active_tab == SettingsTab::Metadata,

                        #[name = "source_list"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 16,
                        },
                    },

                    // 6. Notifications
                    #[name = "notifications_box"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 16,
                        #[watch]
                        set_visible: model.active_tab == SettingsTab::Notifications,

                        #[name = "notify_list"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 16,
                        },
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
        // A failed read here rendered as "no dictionaries installed", which
        // is exactly what a user would see after a successful uninstall.
        let dicts = match catalog.list_dictionaries() {
            Ok(rows) => rows,
            Err(err) => {
                crate::notify::error("Could not list your dictionaries", &err.to_string());
                Vec::new()
            }
        };
        let active_tab = SettingsTab::Appearance;
        let service = LibraryService::new(catalog.clone());
        let model = SettingsPageModel {
            catalog,
            service,
            dicts,
            active_tab,
        };
        let widgets = view_output!();

        widgets.tab_title.set_label(active_tab.label());
        widgets.tab_subtitle.set_label(active_tab.subtitle());

        for (group, tabs) in NAV_GROUPS.iter() {
            let group_label = gtk::Label::new(Some(*group));
            group_label.add_css_class("kalam-settings-group");
            group_label.set_halign(gtk::Align::Start);
            widgets.nav_list.append(&group_label);
            for tab in tabs.iter() {
                let btn = make_tab_button(*tab, *tab == active_tab);
                let tab_copy = *tab;
                let s = sender.clone();
                btn.connect_clicked(move |_| s.input(SettingsMsg::SelectTab(tab_copy)));
                btn.set_widget_name(&format!("settings-tab-{:?}", tab_copy));
                widgets.nav_list.append(&btn);
            }
        }

        rebuild_dicts(&widgets.dict_list, &model.dicts, &sender);
        build_sources(&widgets.source_list, &model.catalog);
        build_file_write(&widgets.file_write_row, &model.catalog);
        build_paths(&widgets.paths_host, &model.catalog);
        build_backup(&widgets.backup_row, &model.catalog);
        build_export(&widgets.export_row, &model.catalog);
        build_notifications(&widgets.notify_list, &sender);
        build_theme_picker(&widgets.theme_grid, &model.catalog);
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
                widgets.tab_subtitle.set_label(tab.subtitle());
                update_tab_styles(&widgets.nav_list, tab);
                if tab == SettingsTab::Notifications {
                    build_notifications(&widgets.notify_list, &sender);
                }
                widgets.scroller.vadjustment().set_value(0.0);
            }
            SettingsMsg::ClearNotifications => {
                crate::notify::clear_history();
                build_notifications(&widgets.notify_list, &sender);
            }
            SettingsMsg::Refresh => {
                self.refresh();
                rebuild_dicts(&widgets.dict_list, &self.dicts, &sender);
                build_notifications(&widgets.notify_list, &sender);
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
                                let done_sender = sender_clone.clone();
                                crate::tasks::spawn(
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
                                        done_sender.input(SettingsMsg::Refresh);
                                    },
                                );
                            }
                        }
                    },
                );
            }
            SettingsMsg::DeleteDict(id) => {
                let name = self
                    .dicts
                    .iter()
                    .find(|d| d.id == id)
                    .map(|d| d.name.clone())
                    .unwrap_or_default();
                crate::notify::outcome_info(
                    self.catalog.delete_dictionary(id),
                    "Dictionary removed",
                    &name,
                    "Could not remove the dictionary",
                );
                self.refresh();
                rebuild_dicts(&widgets.dict_list, &self.dicts, &sender);
            }
            SettingsMsg::MoveDict(id, delta) => {
                if let Err(err) = self.catalog.move_dictionary_priority(id, delta) {
                    crate::notify::error("Could not reorder dictionaries", &err.to_string());
                }
                self.refresh();
                rebuild_dicts(&widgets.dict_list, &self.dicts, &sender);
            }
        }
        self.update_view(widgets, sender);
    }
}

impl SettingsPageModel {
    fn refresh(&mut self) {
        match self.catalog.list_dictionaries() {
            Ok(rows) => self.dicts = rows,
            // Keep the current list rather than blanking it.
            Err(err) => crate::notify::error("Could not list your dictionaries", &err.to_string()),
        }
    }
}

fn make_tab_button(tab: SettingsTab, active: bool) -> gtk::Button {
    let box_content = gtk::Box::new(gtk::Orientation::Horizontal, 10);

    let icon = crate::icons::symbolic_with_classes(tab.icon(), 16, &["kalam-settings-tab-icon"]);
    icon.set_halign(gtk::Align::Center);
    box_content.append(&icon);

    let label = gtk::Label::new(Some(tab.label()));
    label.set_halign(gtk::Align::Start);
    box_content.append(&label);

    let btn = gtk::Button::new();
    btn.set_child(Some(&box_content));
    btn.add_css_class("kalam-settings-tab");
    if active {
        btn.add_css_class("active");
    }
    btn.set_focus_on_click(false);
    btn
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

/// Attach a generated CSS class carrying literal per-theme colours.
///
/// The provider is registered on the display rather than on a widget-local
/// StyleContext so it survives theme switches without using the deprecated
/// widget style-context API.
fn add_styled_class(widget: &impl IsA<gtk::Widget>, class: &str, decls: &str) {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(&format!(".{class} {{ {decls} }}"));
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
    widget.add_css_class(class);
}

/// Theme families in a 2-column grid; each family is a raised block holding
/// one card per variant.
fn build_theme_picker(host: &gtk::Grid, catalog: &Arc<Catalog>) {
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
    let active = crate::theme::current(catalog).id;
    let mut slot = 0;
    for (family_key, name, desc, prefix) in THEME_FAMILIES.iter() {
        let family = themes_for_family(family_key);
        if family.is_empty() {
            continue;
        }
        let block = theme_family_block(&family, name, desc, prefix, active, host, catalog);
        host.attach(&block, slot % 2, slot / 2, 1, 1);
        slot += 1;
    }
}

/// Group theme ids into the families shown in Settings.
///
/// Most families are `base` + `base-darker`, but Ayu's shipped ids are
/// `ayumirage` and `ayu-darker`, so this cannot rely on a simple `base-`
/// prefix match.
fn themes_for_family(family_key: &str) -> Vec<&'static crate::theme::Theme> {
    crate::theme::ALL
        .iter()
        .filter(|theme| theme_family_key(theme) == family_key)
        .collect()
}

fn theme_family_key(theme: &crate::theme::Theme) -> &'static str {
    if theme.id.starts_with("onedark") {
        "onedark"
    } else if theme.id.starts_with("tokyonight") {
        "tokyonight"
    } else if theme.id.starts_with("everforest") {
        "everforest"
    } else if theme.id.starts_with("catppuccin") {
        "catppuccin"
    } else if theme.id.starts_with("gruvbox") {
        "gruvbox"
    } else if theme.id.starts_with("ayu") {
        "ayu"
    } else if theme.id == "nord" {
        "nord"
    } else {
        theme.id
    }
}

fn active_theme_badge(theme: &crate::theme::Theme) -> &'static str {
    if theme.id == crate::theme::DEFAULT.id {
        "default"
    } else {
        "current"
    }
}

fn visit_widget_tree(widget: &gtk::Widget, visit: &mut impl FnMut(&gtk::Widget)) {
    visit(widget);
    let mut child = widget.first_child();
    while let Some(node) = child {
        let next = node.next_sibling();
        visit_widget_tree(&node, visit);
        child = next;
    }
}

/// Update the picker in place after a theme change.
///
/// Rebuilding the entire grid after every click caused the Appearance tab to
/// visibly blink. The preview cards are static samples, so only the selected
/// state needs to change.
fn update_theme_picker_state(host: &gtk::Grid, active: &crate::theme::Theme) {
    let root: gtk::Widget = host.clone().upcast();
    visit_widget_tree(&root, &mut |widget| {
        if let Ok(btn) = widget.clone().downcast::<gtk::Button>() {
            let name = btn.widget_name();
            let Some(theme_id) = name.strip_prefix(THEME_BUTTON_PREFIX) else {
                return;
            };
            let is_active = theme_id == active.id;
            if let Some(card) = btn.child().and_then(|w| w.downcast::<gtk::Box>().ok()) {
                if is_active {
                    card.add_css_class("active");
                } else {
                    card.remove_css_class("active");
                }
            }
            return;
        }

        let name = widget.widget_name();
        if let Some(theme_id) = name.strip_prefix(THEME_BADGE_PREFIX) {
            if let Ok(label) = widget.clone().downcast::<gtk::Label>() {
                if theme_id == active.id {
                    label.set_label(active_theme_badge(active));
                    label.remove_css_class("kalam-theme-variant-off");
                } else {
                    label.set_label("");
                    label.add_css_class("kalam-theme-variant-off");
                }
            }
        } else if let Some(theme_id) = name.strip_prefix(THEME_CHECK_PREFIX) {
            widget.set_opacity(if theme_id == active.id { 1.0 } else { 0.0 });
        }
    });
}

fn theme_family_block(
    family: &[&crate::theme::Theme],
    name: &str,
    desc: &str,
    prefix: &str,
    active: &str,
    host: &gtk::Grid,
    catalog: &Arc<Catalog>,
) -> gtk::Box {
    let block = gtk::Box::new(gtk::Orientation::Vertical, 6);
    block.add_css_class("kalam-theme-family-card");

    let name_label = gtk::Label::new(Some(name));
    name_label.add_css_class("kalam-theme-family-name");
    name_label.set_halign(gtk::Align::Start);
    block.append(&name_label);

    let desc_label = gtk::Label::new(Some(desc));
    desc_label.add_css_class("kalam-theme-family-desc");
    desc_label.set_wrap(true);
    desc_label.set_xalign(0.0);
    desc_label.set_halign(gtk::Align::Start);
    block.append(&desc_label);

    let variants = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    variants.set_homogeneous(true);
    for theme in family {
        variants.append(&theme_variant_button(theme, prefix, active, host, catalog));
    }
    block.append(&variants);
    block
}

/// One clickable theme card: variant name, mini UI preview, swatch strip,
/// and a ✓ seal on the active theme.
fn theme_variant_button(
    theme: &crate::theme::Theme,
    prefix: &str,
    active: &str,
    host: &gtk::Grid,
    catalog: &Arc<Catalog>,
) -> gtk::Button {
    let is_active = theme.id == active;
    let raw = theme
        .label
        .strip_prefix(prefix)
        .unwrap_or(theme.label)
        .trim()
        .trim_matches(|c| c == '(' || c == ')' || c == ' ');
    let variant: &str = if raw.is_empty() { "Normal" } else { raw };

    let card = gtk::Box::new(gtk::Orientation::Vertical, 6);
    card.add_css_class("kalam-theme-card");
    if is_active {
        card.add_css_class("active");
    }
    add_styled_class(
        &card,
        &format!("kalam-tc-bg-{}", theme.id),
        &format!(
            "background-color: {}; border-color: {};",
            theme.bg, theme.border
        ),
    );

    // Name row: variant name, current/default tag, spacer, ✓ seal.
    let name_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let name_label = gtk::Label::new(Some(variant));
    name_label.add_css_class("kalam-theme-name");
    add_styled_class(
        &name_label,
        &format!("kalam-tc-name-{}", theme.id),
        &format!("color: {};", theme.text),
    );
    name_label.set_halign(gtk::Align::Start);
    name_row.append(&name_label);
    let tag = gtk::Label::new(Some(if is_active {
        active_theme_badge(theme)
    } else {
        ""
    }));
    tag.set_widget_name(&format!("{THEME_BADGE_PREFIX}{}", theme.id));
    tag.add_css_class("kalam-theme-variant");
    if !is_active {
        tag.add_css_class("kalam-theme-variant-off");
    }
    add_styled_class(
        &tag,
        &format!("kalam-tc-variant-{}", theme.id),
        &format!("color: {};", theme.text_dim),
    );
    tag.set_width_chars(7);
    tag.set_max_width_chars(7);
    tag.set_xalign(0.0);
    tag.set_valign(gtk::Align::Center);
    name_row.append(&tag);
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    name_row.append(&spacer);
    let check =
        crate::icons::symbolic_with_classes("object-select-symbolic", 12, &["kalam-theme-check"]);
    check.set_widget_name(&format!("{THEME_CHECK_PREFIX}{}", theme.id));
    check.set_opacity(if is_active { 1.0 } else { 0.0 });
    check.set_valign(gtk::Align::Center);
    add_styled_class(
        &check,
        &format!("kalam-tc-chk-{}", theme.id),
        &format!("background-color: {}; color: {};", theme.accent, theme.bg),
    );
    name_row.append(&check);
    card.append(&name_row);

    // Mini preview: sidebar rail with two dots + main bars and a card.
    let preview = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    preview.add_css_class("kalam-theme-preview");
    preview.set_overflow(gtk::Overflow::Hidden);
    preview.set_height_request(52);

    let rail = gtk::Box::new(gtk::Orientation::Vertical, 4);
    rail.set_size_request(18, -1);
    add_styled_class(
        &rail,
        &format!("kalam-tc-rail-{}", theme.id),
        &format!("background-color: {};", theme.sidebar),
    );
    let dot_accent = gtk::Box::new(gtk::Orientation::Vertical, 0);
    dot_accent.set_size_request(6, 6);
    dot_accent.set_margin_top(6);
    dot_accent.set_halign(gtk::Align::Center);
    add_styled_class(
        &dot_accent,
        &format!("kalam-tc-dot-a-{}", theme.id),
        &format!("background-color: {}; border-radius: 999px;", theme.accent),
    );
    rail.append(&dot_accent);
    let dot_border = gtk::Box::new(gtk::Orientation::Vertical, 0);
    dot_border.set_size_request(6, 6);
    dot_border.set_halign(gtk::Align::Center);
    add_styled_class(
        &dot_border,
        &format!("kalam-tc-dot-b-{}", theme.id),
        &format!("background-color: {}; border-radius: 999px;", theme.border),
    );
    rail.append(&dot_border);
    preview.append(&rail);

    let main = gtk::Box::new(gtk::Orientation::Vertical, 4);
    main.add_css_class("kalam-theme-preview-main");
    main.set_hexpand(true);
    let bar1 = gtk::Box::new(gtk::Orientation::Vertical, 0);
    bar1.set_size_request(55, 4);
    bar1.set_halign(gtk::Align::Start);
    add_styled_class(
        &bar1,
        &format!("kalam-tc-bar1-{}", theme.id),
        &format!("background-color: {}; border-radius: 999px;", theme.border),
    );
    main.append(&bar1);
    let mini = gtk::Box::new(gtk::Orientation::Vertical, 0);
    mini.set_hexpand(true);
    mini.set_size_request(-1, 18);
    add_styled_class(
        &mini,
        &format!("kalam-tc-card-{}", theme.id),
        &format!(
            "background-color: {}; border: 1px solid {}; border-radius: 4px;",
            theme.surface, theme.border
        ),
    );
    main.append(&mini);
    let bar2 = gtk::Box::new(gtk::Orientation::Vertical, 0);
    bar2.set_size_request(35, 4);
    bar2.set_halign(gtk::Align::Start);
    add_styled_class(
        &bar2,
        &format!("kalam-tc-bar2-{}", theme.id),
        &format!("background-color: {}; border-radius: 999px;", theme.border),
    );
    main.append(&bar2);
    preview.append(&main);
    card.append(&preview);

    // Swatch strip: surface2 / surface / border / accent / text.
    let swatches = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    for (slot, colour) in [
        theme.surface_2,
        theme.surface,
        theme.border,
        theme.accent,
        theme.text,
    ]
    .iter()
    .enumerate()
    {
        let cell = gtk::Box::new(gtk::Orientation::Vertical, 0);
        cell.set_hexpand(true);
        cell.set_size_request(-1, 5);
        add_styled_class(
            &cell,
            &format!("kalam-tc-s{slot}-{}", theme.id),
            &format!("background-color: {colour}; border-radius: 999px;"),
        );
        swatches.append(&cell);
    }
    card.append(&swatches);

    let btn = gtk::Button::new();
    btn.set_child(Some(&card));
    btn.set_widget_name(&format!("{THEME_BUTTON_PREFIX}{}", theme.id));
    btn.add_css_class("kalam-theme-btn");
    btn.set_tooltip_text(Some(theme.label));

    let host = host.clone();
    let catalog = catalog.clone();
    let chosen = *theme;
    btn.connect_clicked(move |_| {
        crate::theme::save_and_apply(&catalog, &chosen);
        update_theme_picker_state(&host, &chosen);
        crate::notify::success("Theme changed", chosen.label);
    });
    btn
}

/// Data locations: four label/desc rows with mono path boxes.
fn build_paths(host: &gtk::Box, catalog: &Arc<Catalog>) {
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
    build_libraries(host);
    build_recovery(host, catalog);

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
fn build_libraries(host: &gtk::Box) {
    let reg = crate::libraries::load_registry();
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
fn build_recovery(host: &gtk::Box, catalog: &Arc<Catalog>) {
    let survey = crate::sidecar::survey();
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
                            crate::epub_write::human_size(size),
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
    let cache_badge = chip_label(&crate::epub_write::human_size(size), "kalam-chip-neutral");
    right.append(&cache_badge);
    let clear = gtk::Button::with_label("Clear cache");
    clear.add_css_class("kalam-btn-danger");
    clear.set_sensitive(size > 0);
    {
        let cache_badge = cache_badge.clone();
        clear.connect_clicked(move |btn| {
            let (files, freed) = crate::paths::clear_reader_cache();
            cache_badge.set_label(&format!("Freed {}", crate::epub_write::human_size(freed)));
            crate::notify::info(
                "Reader cache cleared",
                &format!(
                    "Removed {files} file{} · freed {}",
                    if files == 1 { "" } else { "s" },
                    crate::epub_write::human_size(freed)
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

/// Export saved quotes to Markdown, from Settings.
fn build_export(host: &gtk::Box, catalog: &Arc<Catalog>) {
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
    let body = section_card(host, "document-save-symbolic", "Export", None);

    let export_btn = gtk::Button::with_label("Export quotes");
    export_btn.add_css_class("kalam-btn-outlined");
    export_btn.set_valign(gtk::Align::Center);
    {
        let catalog = catalog.clone();
        export_btn.connect_clicked(move |_| {
            match crate::pages::saved_quotes::export_all_quotes_markdown(&catalog) {
                Ok((count, path)) => crate::notify::success(
                    &format!(
                        "{count} quote{} exported",
                        if count == 1 { "" } else { "s" }
                    ),
                    &path.display().to_string(),
                ),
                Err(err) => crate::notify::error("Could not export quotes", &err),
            }
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

/// Toggle for writing metadata back into the EPUB itself, and deleting backups.
fn build_file_write(host: &gtk::Box, catalog: &Arc<Catalog>) {
    use crate::epub_write::{set_write_enabled, write_enabled};

    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
    let body = section_card(host, "text-x-generic-symbolic", "EPUB writeback", None);

    {
        let catalog = catalog.clone();
        let sw = toggle_switch(write_enabled(&catalog), move |on| {
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
    let backups = crate::epub_write::list_backups();
    let total: u64 = backups.iter().map(|(_, size)| size).sum();
    let summary_text = if backups.is_empty() {
        "No originals kept".to_string()
    } else {
        format!(
            "{} original{} · {}",
            backups.len(),
            if backups.len() == 1 { "" } else { "s" },
            crate::epub_write::human_size(total)
        )
    };
    let summary = chip_label(&summary_text, "kalam-chip-neutral");
    right.append(&summary);
    let clean = gtk::Button::with_label("Delete backups");
    clean.add_css_class("kalam-btn-danger");
    clean.set_sensitive(!backups.is_empty());
    {
        let summary = summary.clone();
        clean.connect_clicked(move |btn| {
            let (count, freed) = crate::epub_write::delete_backups();
            summary.set_label(&format!(
                "Deleted {count} backup{}, freed {}.",
                if count == 1 { "" } else { "s" },
                crate::epub_write::human_size(freed)
            ));
            crate::notify::info(
                "Original backups deleted",
                &format!(
                    "Removed {count} backup{} · freed {}",
                    if count == 1 { "" } else { "s" },
                    crate::epub_write::human_size(freed)
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
fn build_sources(host: &gtk::Box, catalog: &Arc<Catalog>) {
    use crate::metadata::{set_source_enabled, source_enabled, SourceId};

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
        let sw = toggle_switch(source_enabled(&catalog, SourceId::OpenLibrary), move |on| {
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
        let sw = toggle_switch(source_enabled(&catalog, SourceId::GoogleBooks), move |on| {
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
        entry.set_text(&catalog.get_pref("meta.googlebooks.key").unwrap_or_default());
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
        country.set_text(
            &catalog
                .get_pref("meta.googlebooks.country")
                .unwrap_or_else(crate::metadata::google_books::detect_country),
        );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ayu_family_collects_both_variants() {
        let ids: Vec<&str> = themes_for_family("ayu").into_iter().map(|t| t.id).collect();
        assert_eq!(ids, vec!["ayumirage", "ayu-darker"]);
    }

    #[test]
    fn selected_badge_marks_only_the_default_theme_as_default() {
        assert_eq!(active_theme_badge(&crate::theme::DEFAULT), "default");
        assert_eq!(active_theme_badge(&crate::theme::TOKYONIGHT), "current");
    }
}
