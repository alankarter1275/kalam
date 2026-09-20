use crate::sources::{RemoteBookCard, SearchPage, SourceManager, Source};
use gtk::prelude::*;
use relm4::prelude::*;
use std::sync::Arc;

#[derive(Debug)]
pub enum BrowseOut {
    OpenRemoteBook { source_id: String, remote_id: String },
}

#[derive(Debug)]
pub enum BrowseMsg {
    Search(String),
    SourceSelected(String),
    FilterChanged(String, String),
    LoadMore,
    SearchSuccess(SearchPage),
    SearchFailed(String),
    OpenBook(String), // remote_id
    CoverLoaded { remote_id: String, bytes: Vec<u8> },
}

pub struct BrowseInit {
    pub manager: Arc<SourceManager>,
    pub source_id: String,
    pub initial_query: Option<String>,
}

pub struct BrowseModel {
    #[allow(dead_code)] // browse state for the Wasm sources UI
    manager: Arc<SourceManager>,
    #[allow(dead_code)] // as above
    source_id: String,
    active_source: Option<Arc<dyn Source>>,
    query: String,
    filter_values: std::collections::HashMap<String, String>,
    page: u32,
    has_more: bool,
    results: Vec<RemoteBookCard>,
    status: String,
    is_loading: bool,
}

impl BrowseModel {
    pub fn new(init: BrowseInit) -> Self {
        let active = init.manager.get(&init.source_id);
        let q = init.initial_query.unwrap_or_default();
        let mut filter_values = std::collections::HashMap::new();
        if let Some(ref source) = active {
            for def in source.get_filter_definitions() {
                filter_values.insert(def.id, def.default_value);
            }
        }

        Self {
            manager: init.manager,
            source_id: init.source_id.clone(),
            active_source: active,
            query: q,
            filter_values,
            page: 1,
            has_more: false,
            results: Vec::new(),
            status: "Loading…".to_string(),
            is_loading: false,
        }
    }

    fn trigger_search(&mut self, sender: &ComponentSender<Self>) {
        if let Some(source) = self.active_source.clone() {
            self.is_loading = true;
            self.status = "Searching…".to_string();
            if self.page == 1 {
                self.results.clear();
            }

            let filters = self.filter_values.clone();

            let s = sender.input_sender().clone();
            let page_num = self.page;
            let q = self.query.clone();
            crate::tasks::spawn(
                "Searching",
                move |_| source.search(&q, page_num, &filters),
                |_| {},
                move |res| match res {
                    Ok(page) => { let _ = s.send(BrowseMsg::SearchSuccess(page)); }
                    Err(e)   => { let _ = s.send(BrowseMsg::SearchFailed(e.to_string())); }
                },
            );
        } else {
            self.status = "No sources available.".to_string();
        }
    }
}

#[relm4::component(pub)]
impl Component for BrowseModel {
    type Init = BrowseInit;
    type Input = BrowseMsg;
    type Output = BrowseOut;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 16,
            add_css_class: "kalam-page-container",

            // ── Header ───────────────────────────────────────────────────────────
            // ── Header ───────────────────────────────────────────────────────────
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 12,
                add_css_class: "kalam-page-header",

                gtk::Label {
                    set_label: "Browse Sources",
                    add_css_class: "kalam-title-large",
                    set_halign: gtk::Align::Start,
                    set_hexpand: true,
                },
                gtk::Label {
                    set_label: "Source:",
                    add_css_class: "kalam-subtitle-muted",
                    set_valign: gtk::Align::Center,
                },
                #[name = "source_combo"]
                gtk::DropDown::from_strings(&["Archive of Our Own", "WeebCentral", "RoyalRoad", "MangaDex"]),
            },

            // ── Search bar ───────────────────────────────────────────────────────
            gtk::SearchEntry {
                set_placeholder_text: Some("Search Title or Author…"),
                set_hexpand: true,
                connect_activate[sender] => move |entry| {
                    // Only fire on Enter — avoids hammering API on every keystroke.
                    sender.input(BrowseMsg::Search(entry.text().to_string()));
                },
            },

            // ── Filters & Sort bar ────────────────────────────────────────────────
            #[name = "filter_box"]
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 12,
            },

            // ── Status line (shows while loading / empty / error) ────────────────
            gtk::Label {
                #[watch]
                set_label: &model.status,
                #[watch]
                set_visible: model.is_loading || model.results.is_empty(),
                add_css_class: "kalam-subtitle-muted",
            },

            // ── Results grid ─────────────────────────────────────────────────────
            gtk::ScrolledWindow {
                set_hexpand: true,
                set_vexpand: true,
                #[watch]
                set_visible: !model.results.is_empty(),

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 16,
                    
                    #[name = "grid_box"]
                    gtk::FlowBox {
                        set_selection_mode: gtk::SelectionMode::None,
                        set_valign: gtk::Align::Start,
                        set_homogeneous: true,
                        set_max_children_per_line: 6,
                        set_min_children_per_line: 2,
                        set_row_spacing: 12,
                        set_column_spacing: 12,
                    },
                    
                    gtk::Button {
                        set_label: "Load More",
                        add_css_class: "suggested-action",
                        set_halign: gtk::Align::Center,
                        #[watch]
                        set_visible: model.has_more && !model.is_loading,
                        connect_clicked => BrowseMsg::LoadMore,
                    }
                }
            }
        }
    }

    fn init(
        init_data: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = BrowseModel::new(init_data);
        let widgets = view_output!();

        let sources = model.manager.all();
        let source_ids: Vec<String> = sources.iter().map(|s| s.id().to_string()).collect();
        let source_names: Vec<String> = sources.iter().map(|s| s.name().to_string()).collect();

        let str_refs: Vec<&str> = source_names.iter().map(|s| s.as_str()).collect();
        widgets.source_combo.set_model(Some(&gtk::StringList::new(&str_refs)));

        let selected_idx = source_ids.iter().position(|id| id == &model.source_id).unwrap_or(0);
        widgets.source_combo.set_selected(selected_idx as u32);

        let s_source = sender.clone();
        widgets.source_combo.connect_selected_notify(move |combo| {
            let idx = combo.selected() as usize;
            if let Some(id) = source_ids.get(idx) {
                s_source.input(BrowseMsg::SourceSelected(id.clone()));
            }
        });

        if let Some(ref source) = model.active_source {
            render_dynamic_filters(&widgets.filter_box, source, sender.clone());
        }

        // Trigger initial search to display trending content immediately
        sender.input(BrowseMsg::Search(model.query.clone()));

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
            // ── Search triggered ──────────────────────────────────────────────────
            BrowseMsg::Search(q) => {
                self.query = q;
                self.page = 1;
                self.trigger_search(&sender);
            }

            BrowseMsg::FilterChanged(key, val) => {
                self.filter_values.insert(key, val);
                self.page = 1;
                self.trigger_search(&sender);
            }

            BrowseMsg::SourceSelected(source_id) => {
                self.source_id = source_id.clone();
                self.active_source = self.manager.get(&source_id);
                self.filter_values.clear();
                if let Some(ref source) = self.active_source {
                    for def in source.get_filter_definitions() {
                        self.filter_values.insert(def.id, def.default_value);
                    }
                    render_dynamic_filters(&widgets.filter_box, source, sender.clone());
                }
                self.page = 1;
                self.trigger_search(&sender);
            }

            BrowseMsg::LoadMore => {
                self.page += 1;
                self.trigger_search(&sender);
            }

            // ── Results arrived ───────────────────────────────────────────────────
            BrowseMsg::SearchSuccess(page) => {
                self.is_loading = false;
                self.has_more = page.has_more;
                
                if self.page == 1 {
                    self.results = page.results.clone();
                    // Clear previous grid children
                    while let Some(child) = widgets.grid_box.first_child() {
                        widgets.grid_box.remove(&child);
                    }
                } else {
                    self.results.extend(page.results.clone());
                }

                self.status = if self.results.is_empty() {
                    "No results found.".to_string()
                } else {
                    format!("{} results", self.results.len())
                };

                // Append only the NEW results if paginating
                for res in page.results {
                    // ── Card: 160×240 cover + title + author + button ──────────────
                    let card = gtk::Box::new(gtk::Orientation::Vertical, 6);
                    card.set_size_request(160, -1);
                    card.add_css_class("kalam-book-card");

                    // Cover image (placeholder until async fetch completes)
                    let pic = gtk::Picture::new();
                    pic.set_content_fit(gtk::ContentFit::Cover);
                    pic.set_size_request(160, 220);
                    card.append(&pic);

                    // Kick off cover fetch in background
                    if let (Some(url), Some(source)) =
                        (&res.cover_url, self.active_source.clone())
                    {
                        let url = url.clone();
                        let remote_id = res.remote_id.clone();
                        let s = sender.input_sender().clone();
                        crate::tasks::spawn(
                            "Loading cover",
                            move |_| source.fetch_image(&url),
                            |_| {},
                            move |res| {
                                if let Ok(bytes) = res {
                                    let _ = s.send(BrowseMsg::CoverLoaded { remote_id, bytes });
                                }
                            },
                        );
                    }

                    // Title
                    let title_lbl = gtk::Label::new(Some(&res.title));
                    title_lbl.set_wrap(true);
                    title_lbl.set_max_width_chars(20);
                    title_lbl.set_lines(2);
                    title_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
                    title_lbl.add_css_class("kalam-body-emphasis");
                    card.append(&title_lbl);

                    // Author
                    if !res.author.is_empty() && res.author != "Unknown" {
                        let author_lbl = gtk::Label::new(Some(&res.author));
                        author_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
                        author_lbl.add_css_class("kalam-subtitle-muted");
                        card.append(&author_lbl);
                    }

                    // "View" button
                    let btn = gtk::Button::with_label("View Details");
                    btn.add_css_class("kalam-btn-tonal");
                    let rid = res.remote_id.clone();
                    let s = sender.clone();
                    btn.connect_clicked(move |_| s.input(BrowseMsg::OpenBook(rid.clone())));
                    card.append(&btn);

                    widgets.grid_box.append(&card);
                }
            }

            // ── Cover image arrived — find the matching Picture and set it ────────
            // NOTE: Because the grid children are FlowBoxChild wrappers, we walk them.
            BrowseMsg::CoverLoaded { remote_id, bytes } => {
                // Match by position: find the result index for this remote_id
                if let Some(idx) = self.results.iter().position(|r| r.remote_id == remote_id) {
                    // nth FlowBoxChild → its child Box → first child is the Picture
                    if let Some(flow_child) = widgets.grid_box.child_at_index(idx as i32) {
                        if let Some(card) = flow_child.child() {
                            let card_box = card.downcast::<gtk::Box>().unwrap();
                            if let Some(pic_widget) = card_box.first_child() {
                                if let Ok(pic) = pic_widget.downcast::<gtk::Picture>() {
                                    if let Ok(tex) = gtk::gdk::Texture::from_bytes(
                                        &gtk::glib::Bytes::from(&bytes),
                                    ) {
                                        pic.set_paintable(Some(&tex));
                                    }
                                }
                            }
                        }
                    }
                }
            }

            BrowseMsg::SearchFailed(err) => {
                self.is_loading = false;
                self.status = format!("Search failed: {}", err);
            }

            BrowseMsg::OpenBook(remote_id) => {
                if let Some(src) = &self.active_source {
                    let _ = sender.output(BrowseOut::OpenRemoteBook {
                        source_id: src.id().to_string(),
                        remote_id,
                    });
                }
            }
        }

        self.update_view(widgets, sender);
    }
}


fn render_dynamic_filters(
    container: &gtk::Box,
    source: &Arc<dyn Source>,
    sender: ComponentSender<BrowseModel>,
) {
    use crate::sources::FilterType;

    let defs = source.get_filter_definitions();
    if defs.is_empty() {
        container.set_visible(false);
        return;
    }
    container.set_visible(true);

    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    for def in defs {
        let box_item = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let label = gtk::Label::new(Some(&format!("{}:", def.name)));
        label.add_css_class("kalam-subtitle-muted");
        box_item.append(&label);

        let filter_id = def.id.clone();
        match def.filter_type {
            FilterType::Text { placeholder } => {
                let entry = gtk::Entry::new();
                entry.set_placeholder_text(Some(&placeholder));
                let s = sender.clone();
                let fid = filter_id.clone();
                entry.connect_activate(move |e| {
                    s.input(BrowseMsg::FilterChanged(fid.clone(), e.text().to_string()));
                });
                box_item.append(&entry);
            }
            FilterType::Select { options } | FilterType::Sort { options } => {
                let labels: Vec<String> = options.iter().map(|(l, _)| l.clone()).collect();
                let str_refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
                let drop = gtk::DropDown::from_strings(&str_refs);
                let s = sender.clone();
                let fid = filter_id.clone();
                drop.connect_selected_notify(move |d| {
                    let idx = d.selected() as usize;
                    if let Some((_, val)) = options.get(idx) {
                        s.input(BrowseMsg::FilterChanged(fid.clone(), val.clone()));
                    }
                });
                box_item.append(&drop);
            }
            FilterType::Checkbox => {
                let chk = gtk::CheckButton::new();
                let s = sender.clone();
                let fid = filter_id.clone();
                chk.connect_toggled(move |c| {
                    let val = if c.is_active() { "T".to_string() } else { "".to_string() };
                    s.input(BrowseMsg::FilterChanged(fid.clone(), val));
                });
                box_item.append(&chk);
            }
        }
        container.append(&box_item);
    }
}
