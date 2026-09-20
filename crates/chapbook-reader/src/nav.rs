//! Moving through the book: page and unit turns, jumps and the back
//! stack, links, applied reader settings, and the input-action seam.

use chapbook_core::{
    Action, ActionOutcome, Locator, Point, ReadingDirection, ReadingSettings, TocEntry,
};
use chapbook_paint::FrameIntent;

use crate::{Session, SettingsScope, FONT_STEP_PX};

impl Session {
    // ---- Navigation ----

    /// Turn forward one page, crossing into the next unit at the end of
    /// this one. Returns whether the position moved, which is the answer a
    /// shell's loop wants and the one it cannot reliably derive from
    /// `page()` — see [`Position`].
    pub fn next_page(&mut self) -> bool {
        let before = self.position();
        let count = self.page_count();
        if self.page + 1 < count {
            self.page += 1;
            self.mark(FrameIntent::PageTurn);
        } else if self.spine + 1 < self.spine_len() {
            self.spine += 1;
            self.page = 0;
            self.mark(FrameIntent::UnitChange);
        }
        self.selection = None;
        self.position() != before
    }

    /// Turn back one page. Returns whether the position moved.
    pub fn prev_page(&mut self) -> bool {
        let before = self.position();
        if self.page > 0 {
            self.page -= 1;
            self.mark(FrameIntent::PageTurn);
        } else if self.spine > 0 {
            self.spine -= 1;
            let spine = self.spine;
            self.page = self
                .layout_unit(spine)
                .map_or(0, |l| l.pages.len())
                .saturating_sub(1);
            self.mark(FrameIntent::UnitChange);
        }
        self.selection = None;
        self.position() != before
    }

    /// Skip to the start of the next unit. Returns whether it moved.
    pub fn next_unit(&mut self) -> bool {
        let before = self.position();
        if self.spine + 1 < self.spine_len() {
            self.spine += 1;
            self.page = 0;
            self.selection = None;
            self.mark(FrameIntent::UnitChange);
        }
        self.position() != before
    }

    /// Skip to the start of the previous unit. Returns whether it moved.
    pub fn prev_unit(&mut self) -> bool {
        let before = self.position();
        if self.spine > 0 {
            self.spine -= 1;
            self.page = 0;
            self.selection = None;
            self.mark(FrameIntent::UnitChange);
        }
        self.position() != before
    }

    // ---- Settings ----

    /// Apply settings and remember them. Covers every field, including the
    /// three no shell could reach before: line height, justification, and
    /// whether publisher styles apply.
    ///
    /// kalam: they hold for the life of the session. Upstream persisted
    /// them per scope through its library; the host keeps its own
    /// preferences now and applies them on every open, so `scope` changes
    /// nothing here (see [`SettingsScope`]).
    pub fn set_settings(&mut self, settings: ReadingSettings, scope: SettingsScope) {
        let changed = self.settings != settings;
        self.settings = settings;
        if changed {
            self.relayout_keeping_position();
        }
        self.persist_settings(scope);
    }

    /// Step the base font size, keeping the reader's place. A convenience
    /// over [`Session::set_settings`]; persists globally.
    pub fn adjust_font(&mut self, delta: f32) {
        let mut settings = self.settings.clone();
        settings.base_font_px = (settings.base_font_px + delta).clamp(10.0, 40.0);
        self.set_settings(settings, SettingsScope::Global);
    }

    /// Choose the typeface the reader sees, or `None` for the publisher's.
    ///
    /// A convenience over [`Session::set_settings`], and the companion to
    /// [`Session::font_families`], which is the list a picker offers. The
    /// name is matched by the cascade against the session's own font
    /// database; one nothing answers to is not an error, it just falls
    /// through to the next family the way an unknown family in a
    /// publisher's stylesheet does.
    ///
    /// This beats the publisher's own `font-family` — nearly every EPUB
    /// sets one, so a choice that lost to it would not be a choice.
    /// Monospace is left alone, so code listings stay legible.
    pub fn set_font_family(&mut self, family: Option<String>, scope: SettingsScope) {
        let settings = crate::chapbook_core::ReadingSettings {
            font_family: family.filter(|name| !name.trim().is_empty()),
            ..self.settings.clone()
        };
        self.set_settings(settings, scope);
    }

    /// Cycle light → sepia → dark. A convenience over
    /// [`Session::set_settings`]; persists globally.
    pub fn cycle_theme(&mut self) {
        let mut settings = self.settings.clone();
        settings.theme = settings.theme.cycle();
        self.set_settings(settings, SettingsScope::Global);
    }

    fn persist_settings(&mut self, _scope: SettingsScope) {
        // kalam: settings apply for the life of the session; the host
        // persists its own preferences. The scope is kept for the call
        // sites' sake (see `SettingsScope`).
    }

    // ---- Input ----

    /// Which edge this book reads from, for [`TapZones`].
    ///
    /// Ask the session rather than reaching for [`TapZones::default`],
    /// which is `Ltr` and has no way to know better. The book declares
    /// this — EPUB's `page-progression-direction` — so a shell that
    /// picks for itself picks `Ltr` on every platform it ships to, and
    /// an RTL book paged the wrong way is unreadable rather than merely
    /// unfamiliar.
    ///
    /// ```no_run
    /// # use chapbook_core::TapZones;
    /// # fn f(session: &chapbook_reader::Session) {
    /// let zones = TapZones::new(session.reading_direction());
    /// # }
    /// ```
    ///
    /// [`TapZones`]: chapbook_core::TapZones
    /// [`TapZones::default`]: chapbook_core::TapZones::default
    pub fn reading_direction(&self) -> ReadingDirection {
        self.book.publication().reading_direction()
    }

    /// Apply a reader intent from [`chapbook_core::input`].
    ///
    /// This is the second half of the input seam: shells translate native
    /// events into an [`Action`] and the engine applies it, so a tap zone
    /// or a key binding is written once instead of once per platform.
    ///
    /// The [`ActionOutcome`] answers two questions rather than one,
    /// because a shell needs both and can derive neither from the other.
    /// See its own documentation for what guessing costs.
    pub fn apply(&mut self, action: Action) -> ActionOutcome {
        let moved = |did: bool| {
            if did {
                ActionOutcome::Changed
            } else {
                ActionOutcome::Unchanged
            }
        };
        match action {
            Action::NextPage => moved(self.next_page()),
            Action::PrevPage => moved(self.prev_page()),
            Action::NextUnit => moved(self.next_unit()),
            Action::PrevUnit => moved(self.prev_unit()),
            // The bottom of the back stack is where the platform's own
            // Back takes over, on both mobile targets. Declining it here
            // is what lets a shell forward the gesture unconditionally
            // instead of shadowing the history to know when not to.
            Action::Back => {
                if self.back() {
                    ActionOutcome::Changed
                } else {
                    ActionOutcome::Unhandled
                }
            }
            // `adjust_font` clamps, so at either stop this correctly says
            // nothing moved rather than asking for a redraw of the same
            // page at the same size. The key is consumed either way.
            Action::FontUp | Action::FontDown => {
                let before = self.settings.base_font_px;
                let step = if action == Action::FontUp {
                    FONT_STEP_PX
                } else {
                    -FONT_STEP_PX
                };
                self.adjust_font(step);
                moved(self.settings.base_font_px != before)
            }
            Action::CycleTheme => {
                self.cycle_theme();
                ActionOutcome::Changed
            }
            // A reader's chrome belongs to the shell: there is nothing
            // here to toggle, and the shell that bound this wants the
            // event back.
            Action::ToggleMenu => ActionOutcome::Unhandled,
            // `Action` is `#[non_exhaustive]`. An intent this engine has
            // no verb for is one the shell may still want to act on.
            _ => ActionOutcome::Unhandled,
        }
    }

    fn relayout_keeping_position(&mut self) {
        let locator = self.current_offset();
        self.drop_metrics_dependent();
        // Glyph masks are keyed by size; a relayout that changed the font
        // scale would otherwise leave the old sizes' masks resident.
        self.renderer = chapbook_render_tinyskia::Renderer::new();
        let spine = self.spine;
        if let Some(layout) = self.layout_unit(spine) {
            self.page = layout.page_of(locator);
        }
        self.mark(FrameIntent::Relayout);
    }

    // ---- Navigation ----

    /// The book's table of contents, for a shell that offers one.
    pub fn toc(&self) -> &[TocEntry] {
        self.book.publication().toc()
    }

    /// Where the reader is now, as a locator.
    pub fn locator(&self) -> Locator {
        Locator::new(self.spine, self.current_offset())
    }

    /// Jump to a locator, remembering where we came from. `false` if the
    /// spine index doesn't exist.
    pub fn goto(&mut self, target: Locator) -> bool {
        self.jump(target.spine_index, Some(target.char_offset), None)
    }

    /// Jump to an element `id` within a unit — a TOC fragment or a
    /// footnote. `false` if the spine index doesn't exist; a fragment that
    /// turns out not to be in the unit lands at its start.
    pub fn goto_anchor(&mut self, spine_index: usize, fragment: &str) -> bool {
        self.jump(spine_index, None, Some(fragment.to_string()))
    }

    /// Jump to a TOC entry, by spine index where the entry has one and by
    /// href otherwise.
    pub fn goto_toc(&mut self, entry: &TocEntry) -> bool {
        let Some(spine) = entry
            .spine_index
            .or_else(|| entry.href.as_deref().and_then(|h| self.spine_index_of(h)))
        else {
            return false;
        };
        match &entry.fragment {
            Some(fragment) => self.goto_anchor(spine, fragment),
            None => self.goto(Locator::chapter_start(spine)),
        }
    }

    /// The link under a point in panel coordinates, as written in the
    /// document. Only inside the link's own text: pressing the margin
    /// beside a link is not pressing the link.
    pub fn link_at(&mut self, x: f32, y: f32) -> Option<String> {
        let (px, py) = self.content_point(x, y);
        let (spine, page) = (self.spine, self.page);
        let offset = self
            .layout_unit(spine)?
            .pages
            .get(page)?
            .offset_at_exact(Point::new(px, py))?;
        self.unit(spine)?
            .links
            .as_ref()?
            .iter()
            .find(|link| offset >= link.start && offset < link.end)
            .map(|link| link.href.clone())
    }

    /// An image under a point in panel coordinates, as painted on the page.
    pub fn image_at(&mut self, x: f32, y: f32) -> Option<(u32, u32, Vec<u8>)> {
        let (px, py) = self.content_point(x, y);
        let pt = Point::new(px, py);
        let dl = self.page_display_list()?;
        for op in &dl.ops {
            if let chapbook_paint::DisplayOp::Image { resource, dest, .. } = op {
                if dest.contains_point(pt) {
                    if let Some(stored) = self.image_store().get(*resource) {
                        return Some((stored.width, stored.height, stored.rgba.clone()));
                    }
                }
            }
        }
        None
    }

    /// Follow a document-internal link. External links (anything with a
    /// scheme) are a shell decision, not a reading position, so they
    /// return `false` untouched.
    pub fn follow_link(&mut self, href: &str) -> bool {
        if href.contains("://") || href.starts_with("mailto:") {
            return false;
        }
        let (path, fragment) = match href.split_once('#') {
            Some((path, fragment)) => (path, Some(fragment.to_string())),
            None => (href, None),
        };
        // An empty path is a jump within this unit.
        let spine = if path.is_empty() {
            self.spine
        } else {
            let Ok(item) = self.book.publication().spine_item(self.spine) else {
                return false;
            };
            let resolved = chapbook_epub::resolve_href(&item.href.clone(), path);
            match self.spine_index_of(&resolved) {
                Some(spine) => spine,
                None => return false,
            }
        };
        match fragment {
            Some(fragment) => self.goto_anchor(spine, &fragment),
            None => self.goto(Locator::chapter_start(spine)),
        }
    }

    /// The text behind an internal link, without moving the reader — the
    /// payload of a footnote popover.
    ///
    /// `None` for everything a shell should keep handling the old way:
    /// external links, links with no fragment, a path that resolves to no
    /// spine, a non-text unit, or a fragment the unit does not carry. The
    /// unit is read and parsed on each call, so a shell that peeks often
    /// should cache the answer; one footnote tap is one small unit.
    pub fn peek_link(&self, href: &str) -> Option<String> {
        let (path, fragment) = peek_target(href)?;
        let book = self.book.publication();
        // An empty path is a fragment in this unit, as `follow_link` reads it.
        let spine = if path.is_empty() {
            self.spine
        } else {
            let Ok(item) = book.spine_item(self.spine) else {
                return None;
            };
            let resolved = chapbook_epub::resolve_href(&item.href.clone(), path);
            self.spine_index_of(&resolved)?
        };
        let unit_href = book.spine_item(spine).ok()?.href.clone();
        let bytes = book.unit_bytes(spine).ok()?;
        let doc = chapbook_layout::dom::parse_xhtml(&bytes, &unit_href).ok()?;
        let node = doc.element_by_id(fragment)?;
        let text = trim_note(&chapbook_layout::dom::extract_text_at(&doc, node));
        // A TOC entry or a cross-reference in the body should navigate
        // exactly as it always did; only a note is worth answering in
        // place.
        if !looks_like_note(&doc, node, &text) {
            return None;
        }
        (!text.is_empty()).then_some(text)
    }

    /// Return to where the last jump started. `false` with nothing to go
    /// back to.
    pub fn back(&mut self) -> bool {
        let Some(target) = self.back_stack.pop() else {
            return false;
        };
        self.land(target.spine_index, Some(target.char_offset), None);
        true
    }

    pub fn can_go_back(&self) -> bool {
        !self.back_stack.is_empty()
    }

    /// Spine index of a container-root path, tolerating the leading slash
    /// EPUB manifests may or may not carry.
    fn spine_index_of(&self, href: &str) -> Option<usize> {
        let want = href
            .split('#')
            .next()
            .unwrap_or(href)
            .trim_start_matches('/');
        self.book
            .publication()
            .spine()
            .iter()
            .position(|item| item.href.trim_start_matches('/') == want)
    }

    /// A jump: remember where we were, then land.
    fn jump(&mut self, spine: usize, offset: Option<u32>, anchor: Option<String>) -> bool {
        if spine >= self.book.publication().spine().len() {
            return false;
        }
        let from = self.locator();
        self.land(spine, offset, anchor);
        // Cap the trail: a reader chasing footnotes shouldn't grow it
        // without bound.
        self.back_stack.push(from);
        if self.back_stack.len() > 64 {
            self.back_stack.remove(0);
        }
        true
    }

    fn land(&mut self, spine: usize, offset: Option<u32>, anchor: Option<String>) {
        let same_unit = spine == self.spine;
        self.spine = spine;
        self.page = 0;
        self.pending_offset = offset.map(|offset| (spine, offset));
        self.pending_anchor = anchor.map(|anchor| (spine, anchor));
        self.selection = None;
        self.mark(if same_unit {
            FrameIntent::PageTurn
        } else {
            FrameIntent::UnitChange
        });
    }
}

/// Split an href the way [`Session::peek_link`] reads it. Only a link
/// carrying a fragment names an element to show in place, and an external
/// link is the shell's business rather than the book's.
fn peek_target(href: &str) -> Option<(&str, &str)> {
    if href.contains("://") || href.starts_with("mailto:") {
        return None;
    }
    let (path, fragment) = href.split_once('#')?;
    (!fragment.is_empty()).then_some((path, fragment))
}

/// Footnote-shaped, rather than a chapter?
///
/// A note is a list item or an `aside` — the two shapes EPUB footnotes
/// actually come in — or a short paragraph the publisher gave an id to.
/// A section, a heading or a long block is a destination, and a tap on
/// one means "take me there", not "tell me about it".
fn looks_like_note(
    doc: &chapbook_layout::dom::Document,
    node: chapbook_layout::dom::NodeId,
    text: &str,
) -> bool {
    /// A `p`/`div` this short is a note; longer is a destination.
    const SHORT_NOTE_CHARS: usize = 400;
    let chapbook_layout::dom::NodeData::Element(el) = &doc.node(node).data else {
        return false;
    };
    let tag: &str = el.local_name();
    match tag {
        "li" | "aside" | "dd" => true,
        "p" | "div" => text.chars().count() <= SHORT_NOTE_CHARS,
        _ => false,
    }
}

/// A footnote usually ends in a back-link — an arrow, or a "return to
/// text" label — which is chrome rather than content. A note longer than
/// a popover is a preview of itself, not a scroll.
fn trim_note(raw: &str) -> String {
    /// Room for any real footnote; a longer note is a section of its own.
    const MAX_CHARS: usize = 1200;
    let mut text = raw.trim().to_string();
    for tail in ["↩", "↵", "←", "↑", "[back]", "back to text", "return to text"] {
        while let Some(stripped) = text.strip_suffix(tail) {
            text = stripped.trim_end().to_string();
        }
    }
    if text.chars().count() > MAX_CHARS {
        let cut: String = text.chars().take(MAX_CHARS).collect();
        return format!("{cut}…");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::{peek_target, trim_note};

    #[test]
    fn only_fragment_bearing_internal_links_are_peekable() {
        assert_eq!(peek_target("notes.xhtml#fn1"), Some(("notes.xhtml", "fn1")));
        assert_eq!(peek_target("#fn1"), Some(("", "fn1")));
        // No fragment: nothing to show in place.
        assert_eq!(peek_target("chapter2.xhtml"), None);
        assert_eq!(peek_target("notes.xhtml#"), None);
        // External links belong to the shell, not the book.
        assert_eq!(peek_target("https://example.org/#frag"), None);
        assert_eq!(peek_target("mailto:a@b.c#x"), None);
    }

    #[test]
    fn a_note_shows_its_text_without_its_back_link() {
        assert_eq!(
            trim_note("  The author means the moon. ↩  "),
            "The author means the moon."
        );
        assert_eq!(trim_note("See p. 12. [back]"), "See p. 12.");
        assert_eq!(trim_note("Only whitespace ↩"), "Only whitespace");
    }

    #[test]
    fn a_note_is_shown_in_place_but_a_chapter_still_navigates() {
        use chapbook_layout::dom::parse_xhtml;
        let xhtml = concat!(
            "<?xml version=\"1.0\"?>",
            "<html xmlns=\"http://www.w3.org/1999/xhtml\"><body>",
            "<section id=\"ch3\"><h2>Chapter three</h2><p>Body text.</p></section>",
            "<ol class=\"footnotes\">",
            "<li id=\"fn1\">The author means the moon. <a href=\"#r1\">\u{21a9}</a></li>",
            "</ol></body></html>",
        );
        let doc = parse_xhtml(xhtml.as_bytes(), "notes.xhtml").expect("parses");
        let note = doc.element_by_id("fn1").expect("the note");
        let chapter = doc.element_by_id("ch3").expect("the chapter");
        assert!(super::looks_like_note(&doc, note, "The author means the moon."));
        assert!(!super::looks_like_note(&doc, chapter, "Chapter three Body text."));
    }

    #[test]
    fn an_overlong_note_is_cut_on_a_char_boundary() {
        // Multi-byte throughout: a byte slice here would panic.
        let cut = trim_note(&"न".repeat(2000));
        assert!(cut.chars().count() <= 1201);
        assert!(cut.ends_with('…'));
    }
}
