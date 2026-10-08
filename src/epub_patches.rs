//! The EPUB patch matcher (Phase 6, step 2).
//!
//! A text patch is a replacement rule expressed **in source space**: the
//! find string is what the entry's bytes literally contain (DOM text,
//! entity-escaped back into source form by whoever creates the patch), and
//! the context anchors are the bytes immediately around it. Matching is
//! therefore literal — no parsing, no DOM, no entity decoding — which is
//! what makes the write-back promise hold by construction: **everything
//! outside a patch's span is untouched bytes**, and a permanent bake can
//! splice the same spans into the archive entry with the same result.
//!
//! Repeated words are disambiguated by context, the same find-again-by-
//! context idea as the locator quote layer. A patch whose text is gone, or
//! that still matches more than once after context filtering, is *flagged*
//! in the outcome — never silently skipped, never guessed at.
//!
//! Patches apply in creation order, and each one sees the result of the
//! previous one — a later patch can build on an earlier edit, and a patch
//! whose target an earlier edit consumed reports `NotFound`.

use crate::db::PatchRecord;
use std::ops::Range;

/// How many bytes of context are captured on each side of a verified span.
/// Render-time matching is literal, so the anchors only ever have to match
/// the same bytes they were captured from; they earn their keep when the
/// prefix text changes under the patch (an earlier pending patch deleted
/// from the list, most likely) and the find text alone would be ambiguous.
pub const PATCH_CONTEXT_BYTES: usize = 32;

/// What the matcher concluded about one patch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// Exactly one match, after context filtering. The span is in bytes,
    /// into the entry as it stands *at the time this patch was resolved*.
    Found(Range<usize>),
    /// The text is not there (any more). Flagged for review, never skipped
    /// silently.
    NotFound,
    /// Still several candidates after context filtering. Also flagged —
    /// guessing would risk editing the wrong sentence.
    Ambiguous(usize),
}

/// One patch's result against one entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchOutcome {
    pub id: i64,
    pub resolution: Resolution,
}

/// Resolve one find-with-context against the bytes.
///
/// The single matching primitive: used to apply patches (below) and by the
/// edit surfaces to verify, at save time, that a rule they are about to
/// store resolves against the real entry — catching entity-encoded
/// oddities when the fix is typed, not silently at render.
///
/// Empty context anchors match anywhere; a non-empty one must sit
/// immediately beside the find, byte for byte.
pub fn resolve_span(bytes: &[u8], find: &str, before: &str, after: &str) -> Resolution {
    let needle = find.as_bytes();
    // An empty find would "match" everywhere; a rule like that is a bug in
    // its creator, and matching nothing is the safe answer.
    if needle.is_empty() {
        return Resolution::NotFound;
    }
    let before = before.as_bytes();
    let after = after.as_bytes();
    let mut hits: Vec<usize> = Vec::new();
    if needle.len() <= bytes.len() {
        for (i, window) in bytes.windows(needle.len()).enumerate() {
            if window != needle {
                continue;
            }
            let ends_before = i >= before.len() && &bytes[i - before.len()..i] == before;
            let end = i + needle.len();
            let starts_after =
                bytes.len() >= end + after.len() && &bytes[end..end + after.len()] == after;
            if ends_before && starts_after {
                hits.push(i);
            }
        }
    }
    match hits.len() {
        0 => Resolution::NotFound,
        1 => Resolution::Found(hits[0]..hits[0] + needle.len()),
        n => Resolution::Ambiguous(n),
    }
}

/// Apply a book's pending text patches for one entry, in creation order.
///
/// `patches` is the book's full list as the database returns it (id
/// ascending, which *is* creation order); entries are filtered to
/// `kind == "text"` or `kind == "paragraph"` (both carry literal,
/// source-space find/replace — a paragraph patch's find is the
/// element's whole outer source, built by `epub_spans`), `status ==
/// "pending"` and this `href`. Returns the new bytes — identical to
/// the input when nothing applies — and one outcome per processed
/// patch.
///
/// Applied-in-the-past patches are not re-run: their text is already in
/// the file for real (a bake wrote it there), and re-applying would double
/// every fix.
pub fn apply_text_patches(
    href: &str,
    bytes: &[u8],
    patches: &[PatchRecord],
) -> (Vec<u8>, Vec<PatchOutcome>) {
    let mut out = bytes.to_vec();
    let mut outcomes = Vec::new();
    for p in patches {
        if !matches!(p.kind.as_str(), "text" | "paragraph")
            || p.status != "pending"
            || p.href != href
        {
            continue;
        }
        let resolution = resolve_span(&out, &p.find_text, &p.context_before, &p.context_after);
        if let Resolution::Found(span) = &resolution {
            out.splice(span.clone(), p.replace_text.bytes());
        }
        outcomes.push(PatchOutcome {
            id: p.id,
            resolution,
        });
    }
    (out, outcomes)
}

/// Why a planned patch could not be verified at save time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchRefusal {
    /// The correction matches the original text — nothing to store.
    Unchanged,
    /// The text is not in the entry's current (virtually patched) bytes.
    /// A selection that crosses inline markup lands here: the source
    /// between its two ends contains tags the selection's plain text does
    /// not, so the run cannot be located. Paragraph editing (step 7) is
    /// the surface for those.
    NotFound,
    /// The text occurs more than once, and which occurrence was meant is
    /// not knowable from text alone — the selection's offsets live in
    /// DOM space, and mapping them to source bytes is the span mapper's
    /// job (step 6). Select a wider stretch instead.
    Ambiguous(usize),
}

/// A verified text patch, ready to store: find and replace already in
/// source form (entities escaped), anchors captured from the same bytes
/// the matcher will see at render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedPatch {
    pub find_text: String,
    pub replace_text: String,
    pub context_before: String,
    pub context_after: String,
}

/// DOM text → source form. A selection carries the text as the parser
/// decoded it (`&`, `<`, `>`); the source bytes carry the escaped forms,
/// and matching is literal, so the find and replace strings must be
/// re-escaped before they are matched against or spliced into an entry.
///
/// Only the three characters XML serializers escape in text content are
/// escaped; quotes stay literal, as serializers leave them in text. A
/// book whose source does something else simply fails verification —
/// refused, never stored broken.
pub fn escape_to_source(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Verify a correction and turn it into a storable patch.
///
/// `virtual_bytes` is the entry's source with the book's earlier pending
/// patches already applied — exactly the text this patch will run against
/// at render, since application is in creation order and this one will be
/// last. Callers build it with [`apply_text_patches`] over the prior list;
/// passing raw entry bytes is the zero-prior-patch case of the same thing.
///
/// The span is located anchor-free first: one match verifies, zero or
/// several refuse. Anchors are then *captured* from the bytes around the
/// verified span, so they hold at render by construction — see
/// [`PATCH_CONTEXT_BYTES`].
pub fn plan_text_patch(
    virtual_bytes: &[u8],
    dom_find: &str,
    dom_replace: &str,
) -> Result<PlannedPatch, PatchRefusal> {
    if dom_find == dom_replace {
        return Err(PatchRefusal::Unchanged);
    }
    let find_text = escape_to_source(dom_find);
    let replace_text = escape_to_source(dom_replace);
    let span = match resolve_span(virtual_bytes, &find_text, "", "") {
        Resolution::Found(span) => span,
        Resolution::NotFound => return Err(PatchRefusal::NotFound),
        Resolution::Ambiguous(n) => return Err(PatchRefusal::Ambiguous(n)),
    };
    // Round the context windows to char boundaries so the anchors are
    // valid UTF-8 strings; an entry that is not valid UTF-8 (exotic — XML
    // says it shouldn't be) gets empty anchors rather than guessed ones.
    let (context_before, context_after) = match std::str::from_utf8(virtual_bytes) {
        Ok(text) => {
            let start = floor_char_boundary(text, span.start, PATCH_CONTEXT_BYTES);
            let end = ceil_char_boundary(text, span.end, PATCH_CONTEXT_BYTES);
            (
                text[start..span.start].to_string(),
                text[span.end..end].to_string(),
            )
        }
        Err(_) => (String::new(), String::new()),
    };
    Ok(PlannedPatch {
        find_text,
        replace_text,
        context_before,
        context_after,
    })
}

/// `text[index - back..index]` as a char-boundary-safe start, clamped to
/// the string's beginning.
pub(crate) fn floor_char_boundary(text: &str, index: usize, back: usize) -> usize {
    let mut i = index.saturating_sub(back);
    while i < index && !text.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// `text[index..index + fwd]` as a char-boundary-safe end, clamped to the
/// string's length.
pub(crate) fn ceil_char_boundary(text: &str, index: usize, fwd: usize) -> usize {
    let mut i = (index + fwd).min(text.len());
    while i > index && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A chapter-shaped entry: XML declaration, two paragraphs, an entity,
    /// and a word that repeats so context has real work to do.
    const ENTRY: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
        <html xmlns=\"http://www.w3.org/1999/xhtml\">\n\
        <head><title>Chapter</title></head>\n\
        <body>\n\
        <p>He said teh word &amp; she smiled.</p>\n\
        <p>The teh in the second paragraph, teh again.</p>\n\
        </body>\n\
        </html>\n";

    fn patch(
        id: i64,
        href: &str,
        find: &str,
        replace: &str,
        before: &str,
        after: &str,
    ) -> PatchRecord {
        PatchRecord {
            id,
            book_id: 1,
            kind: "text".into(),
            href: href.into(),
            chapter_index: 0,
            find_text: find.into(),
            replace_text: replace.into(),
            context_before: before.into(),
            context_after: after.into(),
            source: "typo".into(),
            status: "pending".into(),
            created_at: "2026-10-08T00:00:00Z".into(),
            applied_at: None,
        }
    }

    #[test]
    fn zero_patches_is_the_identity() {
        // Byte-stability, base case: with nothing to apply the entry must
        // come back identical, not re-encoded, not re-wrapped.
        let (out, outcomes) = apply_text_patches("c.xhtml", ENTRY.as_bytes(), &[]);
        assert_eq!(out, ENTRY.as_bytes());
        assert!(outcomes.is_empty());
    }

    #[test]
    fn a_patch_touches_only_its_own_span() {
        // Byte-stability, the real promise: everything outside the splice
        // is byte-for-byte the input. Proven by asserting against an
        // expected string built by replacing exactly the one phrase.
        let p = patch(1, "c.xhtml", "teh", "the", "said ", " word");
        let (out, outcomes) = apply_text_patches("c.xhtml", ENTRY.as_bytes(), &[p]);
        let expected = ENTRY.replace("said teh word", "said the word");
        assert_eq!(out, expected.as_bytes());
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].id, 1);
        // The span is absolute within the entry, not relative to its
        // context anchor.
        let anchor = "He said ";
        let start = ENTRY.find(anchor).expect("anchor present") + anchor.len();
        assert_eq!(
            outcomes[0].resolution,
            Resolution::Found(start..start + "teh".len())
        );
    }

    #[test]
    fn context_disambiguates_repeated_text() {
        // "teh" appears three times. With no anchors that is a guess we
        // refuse to make; with anchors it resolves to exactly one.
        let ambiguous = patch(1, "c.xhtml", "teh", "the", "", "");
        let (_, outcomes) = apply_text_patches("c.xhtml", ENTRY.as_bytes(), &[ambiguous]);
        assert_eq!(outcomes[0].resolution, Resolution::Ambiguous(3));

        let anchored = patch(2, "c.xhtml", "teh", "the", "paragraph, ", " again");
        let (out, outcomes) = apply_text_patches("c.xhtml", ENTRY.as_bytes(), &[anchored]);
        // The anchored run: the right occurrence changed, the others kept.
        let expected = ENTRY.replace("paragraph, teh again", "paragraph, the again");
        assert_eq!(out, expected.as_bytes());
        let anchor = "paragraph, ";
        let start = ENTRY.find(anchor).expect("anchor present") + anchor.len();
        assert_eq!(
            outcomes[0].resolution,
            Resolution::Found(start..start + "teh".len())
        );
    }

    #[test]
    fn entities_match_literally() {
        // The creator escapes DOM text back to source form, so a find
        // containing "&amp;" must match the source's "&amp;" literally —
        // the matcher never decodes anything.
        let p = patch(1, "c.xhtml", "word &amp; she", "phrase &amp; she", "said teh ", " smiled");
        let (out, outcomes) = apply_text_patches("c.xhtml", ENTRY.as_bytes(), &[p]);
        let anchor = "He said teh ";
        let start = ENTRY.find(anchor).expect("anchor present") + anchor.len();
        assert_eq!(
            outcomes[0].resolution,
            Resolution::Found(start..start + "word &amp; she".len())
        );
        let expected = ENTRY.replace("word &amp; she smiled", "phrase &amp; she smiled");
        assert_eq!(out, expected.as_bytes());
    }

    #[test]
    fn missing_text_is_flagged_and_nothing_changes() {
        let p = patch(1, "c.xhtml", "qqqq", "xxxx", "", "");
        let (out, outcomes) = apply_text_patches("c.xhtml", ENTRY.as_bytes(), &[p]);
        assert_eq!(outcomes[0].resolution, Resolution::NotFound);
        assert_eq!(out, ENTRY.as_bytes(), "a flagged patch must not touch a byte");
    }

    #[test]
    fn patches_apply_in_order_and_see_each_others_results() {
        // The second patch's find text only exists after the first one has
        // run — a fix built on an earlier fix, the reason application is
        // sequential rather than parallel.
        let first = patch(1, "c.xhtml", "teh word", "the word", "said ", " &");
        let second = patch(2, "c.xhtml", "said the word", "uttered the word", "", " &amp;");
        let (out, outcomes) =
            apply_text_patches("c.xhtml", ENTRY.as_bytes(), &[first.clone(), second]);
        assert!(matches!(outcomes[0].resolution, Resolution::Found(_)));
        assert!(matches!(outcomes[1].resolution, Resolution::Found(_)));
        let expected = ENTRY.replace("said teh word", "uttered the word");
        assert_eq!(out, expected.as_bytes());

        // And the mirror case: a patch targeting text an earlier patch
        // already consumed is flagged, not silently dropped.
        let stale = patch(3, "c.xhtml", "said teh word", "uttered the word", "", " &amp;");
        let (_, outcomes) = apply_text_patches("c.xhtml", ENTRY.as_bytes(), &[first, stale]);
        assert_eq!(outcomes[1].resolution, Resolution::NotFound);
    }

    #[test]
    fn only_pending_text_patches_for_this_href_run() {
        // A patch that was already baked is in the file for real; re-running
        // it would double the fix. Other kinds and other entries are not
        // this function's business.
        let applied = patch(1, "c.xhtml", "teh", "the", "said ", " word");
        let mut already = applied.clone();
        already.status = "applied".into();
        let mut other_file = applied.clone();
        other_file.href = "other.xhtml".into();
        let mut other_kind = applied.clone();
        other_kind.kind = "file".into();

        let (out, outcomes) = apply_text_patches(
            "c.xhtml",
            ENTRY.as_bytes(),
            &[already, other_file, other_kind],
        );
        assert_eq!(out, ENTRY.as_bytes());
        assert!(outcomes.is_empty(), "none of these should have run");
    }

    #[test]
    fn an_empty_find_never_matches() {
        // An empty find would "match" at every position; it is a malformed
        // rule, and matching nothing is the only safe answer.
        let p = patch(1, "c.xhtml", "", "x", "", "");
        let (out, outcomes) = apply_text_patches("c.xhtml", ENTRY.as_bytes(), &[p]);
        assert_eq!(outcomes[0].resolution, Resolution::NotFound);
        assert_eq!(out, ENTRY.as_bytes());
    }
    // ------------------------------------------------------------------
    // Step 4: the save-time planner (DOM text -> verified patch)
    // ------------------------------------------------------------------

    #[test]
    fn dom_text_is_escaped_back_to_source_form() {
        // What the parser decoded, back to what the serializer wrote:
        // the three text-content escapes, and quotes left alone.
        assert_eq!(
            escape_to_source("R & D < 10 > 2 \"truth\""),
            "R &amp; D &lt; 10 &gt; 2 \"truth\""
        );
        // The escaping must compose with matching: an escaped find locates
        // the source's entity form.
        let entry = "<p>He said R &amp; D, twice: R &amp; D.</p>";
        assert_eq!(
            resolve_span(entry.as_bytes(), &escape_to_source("R & D"), "", ""),
            Resolution::Ambiguous(2)
        );
    }

    #[test]
    fn a_correction_verifies_with_context_captured_around_it() {
        let entry = "<p>He said teh word &amp; she smiled.</p>";
        let planned = plan_text_patch(entry.as_bytes(), "teh word &", "the word &")
            .expect("the run verifies");
        assert_eq!(planned.find_text, "teh word &amp;");
        assert_eq!(planned.replace_text, "the word &amp;");
        // The anchors are the bytes immediately around the span — and
        // the span is in source space: it covers the whole entity,
        // semicolon included, so the after-anchor starts at the space
        // that follows `&amp;`, not at its `;`.
        assert_eq!(planned.context_before, "<p>He said ");
        assert_eq!(planned.context_after, " she smiled.</p>");
        // And the planned patch, applied over the same bytes, is exactly
        // the correction.
        let (out, outcomes) = apply_text_patches(
            "c.xhtml",
            entry.as_bytes(),
            &[patch(
                1,
                "c.xhtml",
                &planned.find_text,
                &planned.replace_text,
                &planned.context_before,
                &planned.context_after,
            )],
        );
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "<p>He said the word &amp; she smiled.</p>"
        );
        assert!(matches!(outcomes[0].resolution, Resolution::Found(_)));
    }

    #[test]
    fn an_unchanged_correction_is_refused() {
        let entry = "<p>teh</p>";
        assert_eq!(
            plan_text_patch(entry.as_bytes(), "teh", "teh"),
            Err(PatchRefusal::Unchanged)
        );
    }

    #[test]
    fn a_selection_crossing_markup_is_not_found() {
        // The DOM text of a selection that spans an <em> boundary is
        // contiguous; the source between its ends is not, and the honest
        // answer is a refusal the UI can explain, not a guess.
        let entry = "<p>some <em>emphasis</em> here</p>";
        assert_eq!(
            plan_text_patch(entry.as_bytes(), "some emphasis here", "some stress here"),
            Err(PatchRefusal::NotFound)
        );
    }

    #[test]
    fn an_ambiguous_run_is_refused_not_guessed() {
        let entry = "<p>teh first, teh second.</p>";
        assert_eq!(
            plan_text_patch(entry.as_bytes(), "teh", "the"),
            Err(PatchRefusal::Ambiguous(2))
        );
    }

    #[test]
    fn verification_sees_earlier_pending_fixes() {
        // The planner runs over the entry with the book's earlier pending
        // patches applied — the text a later patch will meet at render. A
        // correction that only exists because an earlier fix created it
        // verifies; the same correction against raw bytes would not.
        let original = "<p>He said teh word.</p>";
        let earlier = patch(1, "c.xhtml", "teh", "the", "said ", " word");
        let (virtual_bytes, _) = apply_text_patches("c.xhtml", original.as_bytes(), &[earlier]);
        // The earlier fix made "said the word"; now a second typo appears
        // in the DOM text the reader shows.
        let planned = plan_text_patch(&virtual_bytes, "He said the word.", "He uttered the word.")
            .expect("verifies against the virtually patched bytes");
        assert_eq!(planned.find_text, "He said the word.");
        // And chaining the two patches reproduces the full correction.
        let second = patch(
            2,
            "c.xhtml",
            &planned.find_text,
            &planned.replace_text,
            &planned.context_before,
            &planned.context_after,
        );
        let (out, _) = apply_text_patches(
            "c.xhtml",
            original.as_bytes(),
            &[patch(1, "c.xhtml", "teh", "the", "said ", " word"), second],
        );
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "<p>He uttered the word.</p>"
        );
    }

    #[test]
    fn context_windows_respect_char_boundaries() {
        // Multibyte text just outside the span: the anchor windows round
        // to whole characters and stay valid strings, never splitting one.
        let entry = "<p>éèêô teh äïöü</p>";
        let planned = plan_text_patch(entry.as_bytes(), "teh", "the")
            .expect("the run verifies");
        assert!(planned.context_before.ends_with("éèêô "));
        assert!(planned.context_after.starts_with(" äïöü"));
        assert!(planned.context_before.len() <= PATCH_CONTEXT_BYTES);
        assert!(planned.context_after.len() <= PATCH_CONTEXT_BYTES);
    }

}
