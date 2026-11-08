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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_unreachable_sources_keep_their_own_explanation() {
        // Downloads, RemoteBrowse and Fanfiction are hidden from the sidebar
        // on purpose, but their copy stays, so putting one back is a
        // one-line change in `NavItem::ALL` and nothing else.
        let downloads = placeholder_copy(NavItem::Downloads);
        assert!(downloads.starts_with("Unified download queue"));
        assert!(downloads.contains("\n\n"));

        let comics = placeholder_copy(NavItem::Comics);
        assert!(comics.starts_with("Comics hub"));

        let ao3 = placeholder_copy(NavItem::RemoteBrowse);
        assert!(ao3.starts_with("Archive of Our Own"));

        let fanfic = placeholder_copy(NavItem::Fanfiction);
        assert!(fanfic.starts_with("Other fanfiction sources"));
    }

    #[test]
    fn shelves_and_settings_point_at_the_real_page() {
        let shelves = placeholder_copy(NavItem::Shelves);
        assert!(shelves.contains("return in P4"));

        let settings = placeholder_copy(NavItem::Settings);
        assert!(settings.contains("Use the Settings page for paths"));
    }

    #[test]
    fn every_other_page_falls_back_to_its_sidebar_label() {
        let home = placeholder_copy(NavItem::Home);
        assert_eq!(home, "Home — placeholder page.");

        let library = placeholder_copy(NavItem::Library);
        assert_eq!(library, "Library — placeholder page.");

        let authors = placeholder_copy(NavItem::Authors);
        assert_eq!(authors, "Authors — placeholder page.");

        let editor = placeholder_copy(NavItem::Editor);
        assert_eq!(editor, "Editor — placeholder page.");
    }

    #[test]
    fn the_browse_page_does_not_use_its_sidebar_label() {
        // The sidebar calls this page "Browse". The placeholder spells the
        // source out instead, so the fallback arm and this arm disagree
        // about the same page's name. Pinned as it is.
        let ao3 = placeholder_copy(NavItem::RemoteBrowse);
        assert!(!ao3.contains("placeholder page."));
        assert_eq!(NavItem::RemoteBrowse.label(), "Browse");
    }
}
