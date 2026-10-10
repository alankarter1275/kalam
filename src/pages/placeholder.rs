use crate::models::NavItem;
use gtk::prelude::*;
use relm4::prelude::*;

pub struct PlaceholderPageModel {
    #[allow(dead_code)] // remembers which nav item this placeholder stands in for
    item: NavItem,
}

#[relm4::component(pub)]
impl SimpleComponent for PlaceholderPageModel {
    type Init = NavItem;
    type Input = ();
    type Output = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 12,
            set_hexpand: true,

            #[name = "title"]
            gtk::Label {
                add_css_class: "kalam-page-title",
                set_halign: gtk::Align::Start,
            },
            #[name = "body"]
            gtk::Label {
                add_css_class: "kalam-placeholder",
                set_halign: gtk::Align::Start,
                set_wrap: true,
            },
        }
    }

    fn init(
        item: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = PlaceholderPageModel { item };
        let widgets = view_output!();

        widgets.title.set_label(item.label());
        widgets.body.set_label(&placeholder_copy(item));

        ComponentParts { model, widgets }
    }
}

fn placeholder_copy(item: NavItem) -> String {
    match item {
        NavItem::Downloads => {
            "Unified download queue for AO3, fanfiction, comics, and batch imports.\n\n\
             Nothing in the queue yet — this module activates with online sources."
                .into()
        }
        NavItem::Comics => "Comics hub (local CBZ/CBR first, catalogue later).\n\n\
             Suwayomi-like browsing will live here without leaving Kalam."
            .into(),
        NavItem::RemoteBrowse => "Archive of Our Own — search, download EPUB, track updates.\n\n\
             Source adapter lands after the local reader is solid."
            .into(),
        NavItem::Fanfiction => "Other fanfiction sources, same adapter pattern as AO3.\n\n\
             Placeholder until the sources framework exists."
            .into(),
        NavItem::Shelves => {
            "Shelves (smart & manual) return in P4 with real rules over your library.".into()
        }
        NavItem::Settings => "Use the Settings page for paths. Extra options come later.".into(),
        other => format!("{} — placeholder page.", other.label()),
    }
}
