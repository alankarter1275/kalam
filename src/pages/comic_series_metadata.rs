//! Comic-series details dialogs: hand editing and provider lookup.
//!
//! A comic series is not a book, so it does not borrow the book
//! metadata editor's form. These dialogs work at series level
//! (decision 46): Edit details writes the reader's own values and
//! locks the fields it touches, while Find metadata stages a
//! MyAnimeList or Comic Vine candidate field by field. Nothing here
//! rewrites a chapter title or a chapter cover.

use crate::db::{ComicSeriesEdit, ComicSeriesUpdateOutcome};
use crate::metadata::{self, Candidate, SourceId};
use crate::models::{
    ComicCredit, ComicSeries, COMIC_SERIES_FIELD_COVER, COMIC_SERIES_FIELD_CREDITS,
    COMIC_SERIES_FIELD_DESCRIPTION, COMIC_SERIES_FIELD_END_DATE, COMIC_SERIES_FIELD_GENRES,
    COMIC_SERIES_FIELD_KNOWN_CHAPTERS, COMIC_SERIES_FIELD_KNOWN_VOLUMES,
    COMIC_SERIES_FIELD_MEDIA_TYPE, COMIC_SERIES_FIELD_ORIGINAL_TITLE,
    COMIC_SERIES_FIELD_PUBLISHED, COMIC_SERIES_FIELD_PUBLISHER,
    COMIC_SERIES_FIELD_SERIALIZATION, COMIC_SERIES_FIELD_STATUS, COMIC_SERIES_FIELD_TITLE,
};
use crate::service::{ComicSeriesMetadataApplyOutcome, LibraryService};
use crate::widgets::dialog::{dialog_shell, present_dialog, text_field, DialogSize, TextField};
use crate::widgets::in_app_dialog::{DialogExit, InAppDialog};
use crate::widgets::pill_button::{boxed_ghost_button, primary_button};
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

fn clear_box(host: &gtk::Box) {
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
}

fn meta_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("kalam-ui-meta");
    label.set_halign(gtk::Align::Start);
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_width_chars(1);
    label.set_max_width_chars(58);
    label
}

fn dialog_scroll(child: &gtk::Box) -> gtk::ScrolledWindow {
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::External);
    scroll.set_hexpand(true);
    scroll.set_vexpand(true);
    scroll.set_child(Some(child));
    scroll
}

fn locked_label(base: &str, series: &ComicSeries, field: &str) -> String {
    if series.is_manual_field(field) {
        format!("{base} · edited")
    } else {
        base.to_string()
    }
}

fn field_label(field: &str) -> &'static str {
    match field {
        COMIC_SERIES_FIELD_TITLE => "Title",
        COMIC_SERIES_FIELD_ORIGINAL_TITLE => "Original title",
        COMIC_SERIES_FIELD_CREDITS => "Credits",
        COMIC_SERIES_FIELD_DESCRIPTION => "Description",
        COMIC_SERIES_FIELD_STATUS => "Status",
        COMIC_SERIES_FIELD_PUBLISHER => "Publisher",
        COMIC_SERIES_FIELD_SERIALIZATION => "Serialization",
        COMIC_SERIES_FIELD_PUBLISHED => "Started",
        COMIC_SERIES_FIELD_END_DATE => "Ended",
        COMIC_SERIES_FIELD_MEDIA_TYPE => "Type",
        COMIC_SERIES_FIELD_GENRES => "Genres",
        COMIC_SERIES_FIELD_KNOWN_VOLUMES => "Known volumes",
        COMIC_SERIES_FIELD_KNOWN_CHAPTERS => "Known chapters",
        COMIC_SERIES_FIELD_COVER => "Cover",
        _ => "Field",
    }
}

fn compact_compare(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn text_view_value(view: &gtk::TextView) -> String {
    let buffer = view.buffer();
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), false)
        .to_string()
}

fn labelled_text_view(
    host: &gtk::Box,
    label: &str,
    initial: &str,
    height: i32,
) -> gtk::TextView {
    let caption = gtk::Label::new(Some(&label.to_ascii_uppercase()));
    caption.add_css_class("kalam-ui-field-label");
    caption.set_halign(gtk::Align::Start);
    caption.set_margin_top(14);
    host.append(&caption);

    let view = gtk::TextView::new();
    view.add_css_class("kalam-comic-series-text-editor");
    view.set_wrap_mode(gtk::WrapMode::WordChar);
    view.set_accepts_tab(false);
    view.set_left_margin(10);
    view.set_right_margin(10);
    view.set_top_margin(8);
    view.set_bottom_margin(8);
    view.buffer().set_text(initial);

    let scroll = gtk::ScrolledWindow::builder()
        .min_content_height(height)
        .max_content_height(height)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&view)
        .build();
    scroll.add_css_class("kalam-comic-series-text-scroll");
    scroll.set_margin_top(6);
    host.append(&scroll);
    view
}

fn format_credits(credits: &[ComicCredit]) -> String {
    credits
        .iter()
        .map(|credit| {
            let role = credit.role.trim();
            let name = credit.name.trim();
            if role.is_empty() {
                name.to_string()
            } else {
                format!("{role}: {name}")
            }
        })
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parse the credits box: one credit per line, `Role: Name`, or a
/// bare name when no role is known. A colon far into a line is part
/// of the name, not a role separator.
fn parse_credits(text: &str) -> Vec<ComicCredit> {
    let mut credits = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let credit = match line.split_once(':') {
            Some((role, name))
                if !role.trim().is_empty()
                    && role.chars().count() <= 40
                    && !name.trim().is_empty() =>
            {
                ComicCredit {
                    role: role.trim().to_string(),
                    name: name.trim().to_string(),
                }
            }
            _ => ComicCredit {
                role: String::new(),
                name: line.to_string(),
            },
        };
        if !credits.iter().any(|existing: &ComicCredit| {
            existing.role.eq_ignore_ascii_case(&credit.role)
                && existing.name.eq_ignore_ascii_case(&credit.name)
        }) {
            credits.push(credit);
        }
    }
    credits
}

fn parse_genres(text: &str) -> Vec<String> {
    let mut genres: Vec<String> = Vec::new();
    for genre in text.split(',') {
        let genre = genre.split_whitespace().collect::<Vec<_>>().join(" ");
        if !genre.is_empty()
            && !genres
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(&genre))
        {
            genres.push(genre);
        }
    }
    genres
}

fn parse_count(label: &str, text: &str) -> Result<Option<i64>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let value = text
        .parse::<i64>()
        .map_err(|_| format!("{label} must be a whole number."))?;
    if value < 0 {
        return Err(format!("{label} cannot be negative."));
    }
    Ok((value > 0).then_some(value))
}

fn count_initial(value: Option<i64>) -> String {
    value.map(|value| value.to_string()).unwrap_or_default()
}

fn changed_manual_fields(series: &ComicSeries, edit: &ComicSeriesEdit) -> Vec<String> {
    let mut changed: Vec<String> = Vec::new();
    let mut push = |field: &'static str, differs: bool| {
        if differs {
            changed.push(field.to_string());
        }
    };
    push(
        COMIC_SERIES_FIELD_TITLE,
        compact_compare(&edit.title) != compact_compare(&series.title),
    );
    push(
        COMIC_SERIES_FIELD_ORIGINAL_TITLE,
        compact_compare(&edit.original_title) != compact_compare(&series.original_title),
    );
    push(
        COMIC_SERIES_FIELD_CREDITS,
        edit.credits != series.credits
            || (edit.credits.is_empty()
                && compact_compare(&edit.author) != compact_compare(&series.author)),
    );
    push(
        COMIC_SERIES_FIELD_DESCRIPTION,
        compact_compare(&edit.description) != compact_compare(&series.description),
    );
    push(
        COMIC_SERIES_FIELD_STATUS,
        compact_compare(&edit.status) != compact_compare(&series.status),
    );
    push(
        COMIC_SERIES_FIELD_PUBLISHER,
        compact_compare(&edit.publisher) != compact_compare(&series.publisher),
    );
    push(
        COMIC_SERIES_FIELD_SERIALIZATION,
        compact_compare(&edit.serialization) != compact_compare(&series.serialization),
    );
    push(
        COMIC_SERIES_FIELD_PUBLISHED,
        compact_compare(&edit.published) != compact_compare(&series.published),
    );
    push(
        COMIC_SERIES_FIELD_END_DATE,
        compact_compare(&edit.end_date) != compact_compare(&series.end_date),
    );
    push(
        COMIC_SERIES_FIELD_MEDIA_TYPE,
        compact_compare(&edit.media_type) != compact_compare(&series.media_type),
    );
    push(
        COMIC_SERIES_FIELD_GENRES,
        edit.genres
            .iter()
            .map(|genre| genre.to_lowercase())
            .collect::<Vec<_>>()
            != series
                .genres
                .iter()
                .map(|genre| genre.to_lowercase())
                .collect::<Vec<_>>(),
    );
    push(
        COMIC_SERIES_FIELD_KNOWN_VOLUMES,
        edit.known_volumes != series.known_volumes,
    );
    push(
        COMIC_SERIES_FIELD_KNOWN_CHAPTERS,
        edit.known_chapters != series.known_chapters,
    );
    push(
        COMIC_SERIES_FIELD_COVER,
        edit.remove_fetched_cover && series.metadata_cover_path.is_some(),
    );
    changed
}

fn source_line(series: &ComicSeries) -> String {
    let Some(source) = SourceId::from_key(series.metadata_source.trim()) else {
        return String::new();
    };
    let mut parts = vec![source.label().to_string()];
    if !series.metadata_source_id.trim().is_empty() {
        parts.push(format!("ID {}", series.metadata_source_id.trim()));
    }
    if !series.metadata_fetched_at.trim().is_empty() {
        parts.push(format!(
            "fetched {}",
            series
                .metadata_fetched_at
                .trim()
                .chars()
                .take(10)
                .collect::<String>()
        ));
    }
    parts.join(" · ")
}

// ---------------------------------------------------------------------------
// Edit details
// ---------------------------------------------------------------------------

pub fn open_edit_comic_series_dialog(
    anchor: &impl IsA<gtk::Widget>,
    service: LibraryService,
    series: ComicSeries,
    on_saved: impl Fn(String) + 'static,
) {
    let on_saved: Rc<dyn Fn(String)> = Rc::new(on_saved);
    let shell = dialog_shell(DialogSize::List, Some(&series.title));
    let form = gtk::Box::new(gtk::Orientation::Vertical, 0);
    form.set_hexpand(true);

    let title_field = text_field(
        &locked_label("Title", &series, COMIC_SERIES_FIELD_TITLE),
        "Series title",
        &series.title,
    );
    form.append(&title_field.widget());

    let original_field = text_field(
        &locked_label(
            "Original title",
            &series,
            COMIC_SERIES_FIELD_ORIGINAL_TITLE,
        ),
        "Original-language title, if different",
        &series.original_title,
    );
    original_field.widget().set_margin_top(14);
    form.append(&original_field.widget());

    let author_field = text_field(
        &locked_label("Author line", &series, COMIC_SERIES_FIELD_CREDITS),
        "Used when no role credits are listed",
        &series.author,
    );
    author_field.widget().set_margin_top(14);
    form.append(&author_field.widget());

    let credits_view = labelled_text_view(
        &form,
        &locked_label("Credits", &series, COMIC_SERIES_FIELD_CREDITS),
        &format_credits(&series.credits),
        104,
    );
    let credits_hint = meta_label("One credit per line, as “Role: Name” — Story, Art, Writer, Penciller… A bare name is fine when no role is known.");
    credits_hint.set_margin_top(6);
    form.append(&credits_hint);

    let status_type_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    status_type_row.set_margin_top(14);
    let status_field = text_field(
        &locked_label("Status", &series, COMIC_SERIES_FIELD_STATUS),
        "Ongoing, Completed, Hiatus…",
        &series.status,
    );
    status_field.widget().set_hexpand(true);
    status_type_row.append(&status_field.widget());
    let type_field = text_field(
        &locked_label("Type", &series, COMIC_SERIES_FIELD_MEDIA_TYPE),
        "Manga, Manhwa, Comic…",
        &series.media_type,
    );
    type_field.widget().set_hexpand(true);
    status_type_row.append(&type_field.widget());
    form.append(&status_type_row);

    let publisher_field = text_field(
        &locked_label("Publisher", &series, COMIC_SERIES_FIELD_PUBLISHER),
        "Publisher",
        &series.publisher,
    );
    publisher_field.widget().set_margin_top(14);
    form.append(&publisher_field.widget());

    let serialization_field = text_field(
        &locked_label(
            "Serialization",
            &series,
            COMIC_SERIES_FIELD_SERIALIZATION,
        ),
        "Magazine or platform it runs in",
        &series.serialization,
    );
    serialization_field.widget().set_margin_top(14);
    form.append(&serialization_field.widget());

    let dates_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    dates_row.set_margin_top(14);
    let published_field = text_field(
        &locked_label("Started", &series, COMIC_SERIES_FIELD_PUBLISHED),
        "e.g. 2021 or 2021-04-09",
        &series.published,
    );
    published_field.widget().set_hexpand(true);
    dates_row.append(&published_field.widget());
    let end_field = text_field(
        &locked_label("Ended", &series, COMIC_SERIES_FIELD_END_DATE),
        "Leave empty while it runs",
        &series.end_date,
    );
    end_field.widget().set_hexpand(true);
    dates_row.append(&end_field.widget());
    form.append(&dates_row);

    let genres_field = text_field(
        &locked_label("Genres", &series, COMIC_SERIES_FIELD_GENRES),
        "Comma separated",
        &series.genres.join(", "),
    );
    genres_field.widget().set_margin_top(14);
    form.append(&genres_field.widget());

    let counts_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    counts_row.set_margin_top(14);
    let volumes_field = text_field(
        &locked_label(
            "Known volumes",
            &series,
            COMIC_SERIES_FIELD_KNOWN_VOLUMES,
        ),
        "Unknown",
        &count_initial(series.known_volumes),
    );
    volumes_field.widget().set_hexpand(true);
    counts_row.append(&volumes_field.widget());
    let chapters_field = text_field(
        &locked_label(
            "Known chapters",
            &series,
            COMIC_SERIES_FIELD_KNOWN_CHAPTERS,
        ),
        "Unknown",
        &count_initial(series.known_chapters),
    );
    chapters_field.widget().set_hexpand(true);
    counts_row.append(&chapters_field.widget());
    form.append(&counts_row);

    let description_view = labelled_text_view(
        &form,
        &locked_label("Description", &series, COMIC_SERIES_FIELD_DESCRIPTION),
        &series.description,
        150,
    );

    let source = source_line(&series);
    if !source.is_empty() {
        let source_label = meta_label(&format!("Metadata source: {source}"));
        source_label.set_margin_top(14);
        form.append(&source_label);
        if !series.metadata_source_url.trim().is_empty() {
            let url = meta_label(series.metadata_source_url.trim());
            url.set_selectable(true);
            form.append(&url);
        }
    }

    let remove_cover_check = if series.metadata_cover_path.is_some() {
        let check = gtk::CheckButton::with_label("Remove the fetched series cover");
        check.add_css_class("kalam-comic-series-check");
        check.set_margin_top(14);
        form.append(&check);
        let note = meta_label("The first chapter's cover is shown again. Chapter covers themselves are never changed.");
        form.append(&note);
        Some(check)
    } else {
        None
    };

    let clear_locks_check = if !series.manual_fields.is_empty() {
        let check =
            gtk::CheckButton::with_label("Remove protection from previously edited fields");
        check.add_css_class("kalam-comic-series-check");
        check.set_margin_top(14);
        form.append(&check);
        let note = meta_label("Fields you change in this save will still be protected.");
        form.append(&note);
        Some(check)
    } else {
        None
    };

    let lock_note = meta_label(
        "Fields you change here stay as you set them when metadata is fetched again.",
    );
    lock_note.set_margin_top(14);
    form.append(&lock_note);

    shell.body().append(&dialog_scroll(&form));
    let status = meta_label("");
    status.set_margin_top(8);
    shell.body().append(&status);

    let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
    let footer = shell.footer();
    let slot_for_cancel = slot.clone();
    footer.append(&boxed_ghost_button("Cancel", move || {
        if let Some(dialog) = slot_for_cancel.borrow().clone() {
            dialog.close();
        }
    }));

    let series_for_save = series.clone();
    let slot_for_save = slot.clone();
    let footer_for_save = footer.clone();
    footer.append(&primary_button("Save details", move || {
        let title = title_field.value().trim().to_string();
        if title.is_empty() {
            status.set_text("A series needs a title.");
            return;
        }
        let known_volumes = match parse_count("Known volumes", &volumes_field.value()) {
            Ok(value) => value,
            Err(err) => {
                status.set_text(&err);
                return;
            }
        };
        let known_chapters = match parse_count("Known chapters", &chapters_field.value()) {
            Ok(value) => value,
            Err(err) => {
                status.set_text(&err);
                return;
            }
        };

        let mut edit = ComicSeriesEdit {
            title,
            original_title: original_field.value(),
            author: author_field.value(),
            credits: parse_credits(&text_view_value(&credits_view)),
            description: text_view_value(&description_view),
            status: status_field.value(),
            publisher: publisher_field.value(),
            serialization: serialization_field.value(),
            published: published_field.value(),
            end_date: end_field.value(),
            media_type: type_field.value(),
            genres: parse_genres(&genres_field.value()),
            known_volumes,
            known_chapters,
            remove_fetched_cover: remove_cover_check
                .as_ref()
                .is_some_and(|check| check.is_active()),
            clear_manual_fields: clear_locks_check
                .as_ref()
                .is_some_and(|check| check.is_active()),
            manual_fields: Vec::new(),
        };
        edit.manual_fields = changed_manual_fields(&series_for_save, &edit);

        footer_for_save.set_sensitive(false);
        status.set_text("Saving…");
        let service = service.clone();
        let series_id = series_for_save.id;
        let slot = slot_for_save.clone();
        let footer = footer_for_save.clone();
        let status_for_done = status.clone();
        let on_saved = on_saved.clone();
        crate::tasks::spawn(
            "Saving series details",
            move |_reporter| -> Result<(ComicSeriesUpdateOutcome, Option<String>), String> {
                let outcome = service.update_comic_series_details(series_id, &edit)?;
                let cover_error = outcome.replaced_cover_file.as_deref().and_then(|old| {
                    crate::db::remove_comic_series_cover(old)
                        .err()
                        .map(|err| format!("The old series cover could not be removed: {err}"))
                });
                Ok((outcome, cover_error))
            },
            |_| {},
            move |result| match result {
                Ok((outcome, cover_error)) => {
                    if let Some(err) = cover_error {
                        crate::notify::error("Series details saved", &err);
                    } else {
                        crate::notify::success(
                            "Series details saved",
                            &format!("“{}” was updated.", outcome.title),
                        );
                    }
                    if let Some(dialog) = slot.borrow().clone() {
                        dialog.close();
                    }
                    on_saved(outcome.title.clone());
                }
                Err(err) => {
                    footer.set_sensitive(true);
                    status_for_done.set_text(&err);
                    crate::notify::error("Could not save series details", &err);
                }
            },
        );
    }));

    match present_dialog(anchor, "Edit series details", DialogExit::UnsavedInput, &shell) {
        Some(dialog) => *slot.borrow_mut() = Some(dialog),
        None => crate::notify::error(
            "Could not open the dialog",
            "The series details dialog could not be opened.",
        ),
    }
}

// ---------------------------------------------------------------------------
// Find metadata
// ---------------------------------------------------------------------------

/// Late-bound dialog actions. The buttons are built before the
/// search/review state machine they invoke, so those actions are
/// installed into shared slots once both halves exist.
type DialogAction = Rc<dyn Fn()>;
type CandidateAction = Rc<dyn Fn(Candidate)>;
type SharedDialogAction = Rc<RefCell<Option<DialogAction>>>;
type SharedCandidateAction = Rc<RefCell<Option<CandidateAction>>>;

#[derive(Clone)]
struct ReviewState {
    candidate: Candidate,
    checks: Vec<(String, gtk::CheckButton)>,
}

struct FieldProposal {
    field: &'static str,
    label: &'static str,
    current: String,
    incoming: String,
    locked: bool,
}

fn candidate_published(candidate: &Candidate) -> String {
    if !candidate.published.trim().is_empty() {
        candidate.published.clone()
    } else {
        candidate
            .first_year
            .map(|year| year.to_string())
            .unwrap_or_default()
    }
}

fn candidate_field_proposals(series: &ComicSeries, candidate: &Candidate) -> Vec<FieldProposal> {
    let mut proposals = Vec::new();
    let mut push_string = |field: &'static str, current: &str, incoming: &str| {
        let incoming = incoming.trim();
        if incoming.is_empty() || compact_compare(incoming) == compact_compare(current) {
            return;
        }
        proposals.push(FieldProposal {
            field,
            label: field_label(field),
            current: current.trim().to_string(),
            incoming: incoming.to_string(),
            locked: series.is_manual_field(field),
        });
    };

    push_string(COMIC_SERIES_FIELD_TITLE, &series.title, &candidate.title);
    push_string(
        COMIC_SERIES_FIELD_ORIGINAL_TITLE,
        &series.original_title,
        &candidate.original_title,
    );
    push_string(
        COMIC_SERIES_FIELD_CREDITS,
        &series.credits_summary(),
        &candidate.credits_summary(),
    );
    push_string(
        COMIC_SERIES_FIELD_DESCRIPTION,
        &series.description,
        &candidate.description,
    );
    push_string(
        COMIC_SERIES_FIELD_STATUS,
        &series.status,
        &candidate.status,
    );
    push_string(
        COMIC_SERIES_FIELD_PUBLISHER,
        &series.publisher,
        &candidate.publisher,
    );
    push_string(
        COMIC_SERIES_FIELD_SERIALIZATION,
        &series.serialization,
        &candidate.serialization,
    );
    push_string(
        COMIC_SERIES_FIELD_PUBLISHED,
        &series.published,
        &candidate_published(candidate),
    );
    push_string(
        COMIC_SERIES_FIELD_END_DATE,
        &series.end_date,
        &candidate.end_date,
    );
    push_string(
        COMIC_SERIES_FIELD_MEDIA_TYPE,
        &series.media_type,
        &candidate.media_type,
    );
    push_string(
        COMIC_SERIES_FIELD_GENRES,
        &series.genres.join(", "),
        &candidate.tags.join(", "),
    );

    for (field, current, incoming) in [
        (
            COMIC_SERIES_FIELD_KNOWN_VOLUMES,
            series.known_volumes,
            candidate.known_volumes,
        ),
        (
            COMIC_SERIES_FIELD_KNOWN_CHAPTERS,
            series.known_chapters,
            candidate.known_chapters,
        ),
    ] {
        if let Some(incoming) = incoming.filter(|value| *value > 0) {
            if current != Some(incoming) {
                proposals.push(FieldProposal {
                    field,
                    label: field_label(field),
                    current: current.map(|value| value.to_string()).unwrap_or_default(),
                    incoming: incoming.to_string(),
                    locked: series.is_manual_field(field),
                });
            }
        }
    }

    if candidate.cover.is_some() {
        let current = if series.metadata_cover_path.is_some() {
            "Fetched series cover"
        } else if series.cover_path.is_some() {
            "Cover from a chapter"
        } else {
            "No cover"
        };
        proposals.push(FieldProposal {
            field: COMIC_SERIES_FIELD_COVER,
            label: field_label(COMIC_SERIES_FIELD_COVER),
            current: current.to_string(),
            incoming: "New series cover".to_string(),
            locked: series.is_manual_field(COMIC_SERIES_FIELD_COVER),
        });
    }

    proposals
}

fn merge_candidate_detail(search: Candidate, mut detail: Candidate) -> Candidate {
    if detail.title.trim().is_empty() {
        detail.title = search.title.clone();
    }
    if detail.authors.trim().is_empty() {
        detail.authors = search.authors.clone();
    }
    if detail.series.is_none() {
        detail.series = search.series.clone();
    }
    if detail.tags.is_empty() {
        detail.tags = search.tags.clone();
    }
    if detail.first_year.is_none() {
        detail.first_year = search.first_year;
    }
    if detail.publisher.trim().is_empty() {
        detail.publisher = search.publisher.clone();
    }
    if detail.published.trim().is_empty() {
        detail.published = search.published.clone();
    }
    if detail.cover.is_none() {
        detail.cover = search.cover.clone();
    }
    if detail.description.trim().is_empty() {
        detail.description = search.description.clone();
    }
    if detail.detail_key.is_none() {
        detail.detail_key = search.detail_key.clone();
    }
    if detail.source.is_none() {
        detail.source = search.source;
    }
    if detail.credits.is_empty() {
        detail.credits = search.credits.clone();
    }
    if detail.aliases.is_empty() {
        detail.aliases = search.aliases.clone();
    }
    if detail.original_title.trim().is_empty() {
        detail.original_title = search.original_title.clone();
    }
    if detail.serialization.trim().is_empty() {
        detail.serialization = search.serialization.clone();
    }
    if detail.status.trim().is_empty() {
        detail.status = search.status.clone();
    }
    if detail.media_type.trim().is_empty() {
        detail.media_type = search.media_type.clone();
    }
    if detail.end_date.trim().is_empty() {
        detail.end_date = search.end_date.clone();
    }
    if detail.known_volumes.is_none() {
        detail.known_volumes = search.known_volumes;
    }
    if detail.known_chapters.is_none() {
        detail.known_chapters = search.known_chapters;
    }
    if detail.rating.is_none() {
        detail.rating = search.rating;
    }
    if detail.external_id.is_none() {
        detail.external_id = search.external_id.clone();
    }
    if detail.source_url.is_none() {
        detail.source_url = search.source_url.clone();
    }
    detail
}

fn fetch_candidate_detail(
    service: &LibraryService,
    candidate: Candidate,
) -> (Candidate, Option<String>) {
    let Some(source_id) = candidate.source else {
        return (candidate, None);
    };
    let Some(source) = metadata::source_by_id(service.catalog(), source_id) else {
        return (
            candidate,
            Some(format!(
                "{} is disabled, so only the search result can be reviewed.",
                source_id.label()
            )),
        );
    };

    let mut notes = Vec::new();
    match source.fetch_candidate_details(&candidate) {
        Ok(detail) => return (merge_candidate_detail(candidate, detail), None),
        Err(err) => notes.push(err.to_string()),
    }
    if let Some(external_id) = candidate.external_id.as_deref() {
        match source.fetch_by_external_id(external_id) {
            Ok(Some(detail)) => return (merge_candidate_detail(candidate, detail), None),
            Ok(None) => {}
            Err(err) => notes.push(err.to_string()),
        }
    }
    if let Some(key) = candidate.detail_key.as_deref() {
        if let Ok(description) = source.fetch_description(key) {
            if !description.trim().is_empty() {
                let mut enriched = candidate.clone();
                enriched.description = description;
                return (enriched, None);
            }
        }
    }
    let note = if notes.is_empty() {
        None
    } else {
        Some(format!(
            "Full details could not be fetched ({}). Showing the search result.",
            notes.join("; ")
        ))
    };
    (candidate, note)
}

fn refresh_linked_candidate(
    service: &LibraryService,
    series: &ComicSeries,
) -> Result<Option<Candidate>, String> {
    let source_id = SourceId::from_key(series.metadata_source.trim())
        .ok_or_else(|| "This series is not linked to a metadata source yet.".to_string())?;
    if source_id.kind() != metadata::SourceKind::Comic {
        return Err(format!("{} does not describe comic series.", source_id.label()));
    }
    let external_id = series.metadata_source_id.trim();
    if external_id.is_empty() {
        return Err(format!(
            "The {} link has no record ID to refresh.",
            source_id.label()
        ));
    }
    let source = metadata::source_by_id(service.catalog(), source_id).ok_or_else(|| {
        format!(
            "{} is disabled in Settings > Metadata sources.",
            source_id.label()
        )
    })?;
    source
        .fetch_by_external_id(external_id)
        .map_err(|err| err.to_string())
}

fn outcome_summary(outcome: &ComicSeriesMetadataApplyOutcome) -> String {
    let mut parts = Vec::new();
    if outcome.update.applied_fields.is_empty() {
        parts.push("Source link saved; no fields needed changing.".to_string());
    } else {
        parts.push(format!(
            "Updated {}.",
            outcome
                .update
                .applied_fields
                .iter()
                .map(|field| field_label(field).to_lowercase())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !outcome.update.protected_fields.is_empty() {
        parts.push(format!(
            "Left hand-edited {} unchanged.",
            outcome
                .update
                .protected_fields
                .iter()
                .map(|field| field_label(field).to_lowercase())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if let Some(err) = &outcome.cover_error {
        parts.push(err.clone());
    }
    parts.join(" ")
}

fn source_badge(candidate: &Candidate) -> Option<gtk::Label> {
    let source = candidate.source?;
    let badge = gtk::Label::new(Some(source.badge()));
    badge.add_css_class("kalam-card-badge");
    badge.add_css_class(source.css_class());
    badge.set_valign(gtk::Align::Center);
    badge.set_tooltip_text(Some(source.label()));
    Some(badge)
}

#[allow(clippy::too_many_arguments)]
fn build_review(
    host: &gtk::Box,
    series: &ComicSeries,
    candidate: &Candidate,
    review_state: &Rc<RefCell<Option<ReviewState>>>,
    apply_button: &gtk::Button,
    back_button: &gtk::Button,
    can_go_back: bool,
    status: &gtk::Label,
    generation: &Rc<Cell<u64>>,
    review_generation: u64,
    update_apply_label: &Rc<dyn Fn()>,
) {
    clear_box(host);
    let proposals = candidate_field_proposals(series, candidate);

    let heading_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    if let Some(badge) = source_badge(candidate) {
        heading_row.append(&badge);
    }
    let heading = gtk::Label::new(Some("Review changes"));
    heading.add_css_class("kalam-title-medium");
    heading.set_halign(gtk::Align::Start);
    heading.set_hexpand(true);
    heading_row.append(&heading);
    host.append(&heading_row);

    let source = candidate
        .source
        .map(|source| source.label())
        .unwrap_or("the source");
    let from = meta_label(&format!("From {source}: {}", candidate.title));
    host.append(&from);
    if candidate
        .external_id
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        host.append(&meta_label(
            "This result has no source ID, so it can be applied but not refreshed by ID later.",
        ));
    }

    let mut checks: Vec<(String, gtk::CheckButton)> = Vec::new();
    if proposals.is_empty() {
        host.append(&meta_label(
            "This record matches what Kalam already has. You can still save it as the linked source for future refreshes.",
        ));
        status.set_text("Nothing differs — linking the source is the only change.");
    } else {
        status.set_text("Choose what to update. Hand-edited fields stay locked.");
        for proposal in &proposals {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            row.add_css_class("kalam-comic-review-row");
            if proposal.locked {
                row.add_css_class("locked");
            }

            let check = gtk::CheckButton::new();
            check.add_css_class("kalam-comic-series-check");
            check.set_active(!proposal.locked);
            check.set_sensitive(!proposal.locked);
            check.set_valign(gtk::Align::Start);
            check.set_tooltip_text(Some(if proposal.locked {
                "Edited by hand — metadata cannot overwrite this field"
            } else {
                "Apply this field"
            }));
            {
                let update_apply_label = update_apply_label.clone();
                check.connect_toggled(move |_| update_apply_label());
            }
            row.append(&check);
            checks.push((proposal.field.to_string(), check));

            let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
            text.set_hexpand(true);
            let label_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let label = gtk::Label::new(Some(&proposal.label.to_ascii_uppercase()));
            label.add_css_class("kalam-ui-field-label");
            label.set_halign(gtk::Align::Start);
            label_row.append(&label);
            if proposal.locked {
                let lock = gtk::Label::new(Some("EDITED BY HAND"));
                lock.add_css_class("kalam-comic-series-lock");
                lock.set_valign(gtk::Align::Center);
                label_row.append(&lock);
            }
            text.append(&label_row);

            let current = if proposal.current.is_empty() {
                "—".to_string()
            } else {
                proposal.current.clone()
            };
            let current_label = meta_label(&format!("Now: {current}"));
            current_label.set_lines(3);
            current_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            text.append(&current_label);

            let incoming_label = gtk::Label::new(Some(&format!("New: {}", proposal.incoming)));
            incoming_label.add_css_class("kalam-comic-review-value");
            incoming_label.set_halign(gtk::Align::Start);
            incoming_label.set_xalign(0.0);
            incoming_label.set_wrap(true);
            incoming_label.set_width_chars(1);
            incoming_label.set_max_width_chars(58);
            incoming_label.set_lines(if proposal.field == COMIC_SERIES_FIELD_DESCRIPTION {
                6
            } else {
                3
            });
            incoming_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            text.append(&incoming_label);
            row.append(&text);

            if proposal.field == COMIC_SERIES_FIELD_COVER {
                if let Some(cover) = candidate.cover.clone() {
                    let picture = gtk::Picture::new();
                    picture.set_size_request(72, 108);
                    picture.set_content_fit(gtk::ContentFit::Cover);
                    picture.add_css_class("kalam-comic-review-cover");
                    picture.set_valign(gtk::Align::Start);
                    row.append(&picture);
                    let generation = generation.clone();
                    crate::tasks::spawn_internal(
                        "Loading series cover preview",
                        move |_reporter| {
                            metadata::fetch_thumbnail(&cover).ok().and_then(|bytes| {
                                crate::preload::decode_rgba_bytes(&bytes, 72, 108)
                            })
                        },
                        |_| {},
                        move |pixels| {
                            if generation.get() != review_generation {
                                return;
                            }
                            if let Some(pixels) = pixels {
                                if pixels.width > 0
                                    && pixels.height > 0
                                    && pixels.rgba.len()
                                        == (pixels.width as usize)
                                            * (pixels.height as usize)
                                            * 4
                                {
                                    let bytes =
                                        gtk::glib::Bytes::from_owned(pixels.rgba);
                                    let texture: gtk::gdk::Texture =
                                        gtk::gdk::MemoryTexture::new(
                                            pixels.width,
                                            pixels.height,
                                            gtk::gdk::MemoryFormat::R8g8b8a8,
                                            &bytes,
                                            pixels.width as usize * 4,
                                        )
                                        .upcast();
                                    picture.set_paintable(Some(&texture));
                                }
                            }
                        },
                    );
                }
            }
            host.append(&row);
        }
    }

    *review_state.borrow_mut() = Some(ReviewState {
        candidate: candidate.clone(),
        checks,
    });
    apply_button.set_visible(true);
    apply_button.set_sensitive(candidate.source.is_some());
    back_button.set_visible(can_go_back);
    update_apply_label();
}

fn show_candidate_results(
    host: &gtk::Box,
    candidates: &[Candidate],
    on_pick: &Rc<dyn Fn(Candidate)>,
) {
    clear_box(host);
    if candidates.is_empty() {
        host.append(&meta_label(
            "No matches. Try the series title on its own, or check the comic source credentials in Settings > Metadata sources.",
        ));
        return;
    }

    for candidate in candidates {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        row.add_css_class("kalam-list-row");
        row.add_css_class("kalam-comic-candidate-row");
        if let Some(badge) = source_badge(candidate) {
            row.append(&badge);
        }

        let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text.set_hexpand(true);
        let title = gtk::Label::new(Some(&candidate.title));
        title.add_css_class("kalam-card-title");
        title.set_halign(gtk::Align::Start);
        title.set_xalign(0.0);
        title.set_wrap(true);
        title.set_width_chars(1);
        title.set_max_width_chars(50);
        text.append(&title);
        if !candidate.original_title.trim().is_empty()
            && !candidate
                .original_title
                .trim()
                .eq_ignore_ascii_case(candidate.title.trim())
        {
            text.append(&meta_label(candidate.original_title.trim()));
        }
        let summary = candidate.summary();
        if !summary.is_empty() {
            let meta = meta_label(&summary);
            meta.set_lines(2);
            meta.set_ellipsize(gtk::pango::EllipsizeMode::End);
            text.append(&meta);
        }
        row.append(&text);

        let review = gtk::Button::with_label("Review");
        review.add_css_class("kalam-mini-btn");
        review.set_valign(gtk::Align::Center);
        let candidate_for_pick = candidate.clone();
        let on_pick = on_pick.clone();
        review.connect_clicked(move |_| on_pick(candidate_for_pick.clone()));
        row.append(&review);
        host.append(&row);
    }
}

pub fn open_find_comic_series_metadata_dialog(
    anchor: &impl IsA<gtk::Widget>,
    service: LibraryService,
    series: ComicSeries,
    staged_candidate: Option<Candidate>,
    on_saved: impl Fn(String) + 'static,
) {
    let on_saved: Rc<dyn Fn(String)> = Rc::new(on_saved);
    let shell = dialog_shell(DialogSize::List, Some(&series.title));

    let generation = Rc::new(Cell::new(0_u64));
    let review_state: Rc<RefCell<Option<ReviewState>>> = Rc::new(RefCell::new(None));
    let results_state: Rc<RefCell<Vec<Candidate>>> = Rc::new(RefCell::new(Vec::new()));
    let show_review_holder: SharedCandidateAction = Rc::new(RefCell::new(None));
    let show_results_holder: SharedDialogAction = Rc::new(RefCell::new(None));
    let search_holder: SharedDialogAction = Rc::new(RefCell::new(None));
    let refresh_holder: SharedDialogAction = Rc::new(RefCell::new(None));

    let search_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let query_field: TextField = text_field("Search title", "Series title", &series.title);
    query_field.widget().set_hexpand(true);
    search_row.append(&query_field.widget());
    let search_holder_for_button = search_holder.clone();
    let search_button = primary_button("Search", move || {
        if let Some(search) = search_holder_for_button.borrow().clone() {
            search();
        }
    });
    search_button.set_valign(gtk::Align::End);
    search_row.append(&search_button);
    shell.body().append(&search_row);

    let linked = source_line(&series);
    let refresh_button = if !linked.is_empty() {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        row.set_margin_top(10);
        let label = meta_label(&format!("Linked to {linked}"));
        label.set_hexpand(true);
        row.append(&label);
        let refresh_holder_for_button = refresh_holder.clone();
        let button = boxed_ghost_button("Refresh linked record", move || {
            if let Some(refresh) = refresh_holder_for_button.borrow().clone() {
                refresh();
            }
        });
        row.append(&button);
        shell.body().append(&row);
        Some(button)
    } else {
        None
    };

    let status = meta_label("Search by the series title. A result is reviewed before anything is saved.");
    status.set_margin_top(10);
    shell.body().append(&status);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
    content.set_hexpand(true);
    content.append(&meta_label(
        "MyAnimeList and Comic Vine are searched separately from book sources. Their credentials live in Settings > Metadata sources.",
    ));
    let content_scroll = dialog_scroll(&content);
    content_scroll.set_margin_top(6);
    shell.body().append(&content_scroll);

    let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
    let footer = shell.footer();

    let show_results_holder_for_back = show_results_holder.clone();
    let generation_for_back = generation.clone();
    let status_for_back = status.clone();
    let back_button = boxed_ghost_button("Back to results", move || {
        generation_for_back.set(generation_for_back.get().wrapping_add(1));
        status_for_back.set_text("Choose another result to review.");
        if let Some(show) = show_results_holder_for_back.borrow().clone() {
            show();
        }
    });
    back_button.set_visible(false);
    footer.append(&back_button);

    let slot_for_cancel = slot.clone();
    footer.append(&boxed_ghost_button("Cancel", move || {
        if let Some(dialog) = slot_for_cancel.borrow().clone() {
            dialog.close();
        }
    }));

    let apply_service = service.clone();
    let apply_series_id = series.id;
    let apply_review_state = review_state.clone();
    let apply_generation = generation.clone();
    let apply_status = status.clone();
    let apply_footer = footer.clone();
    let apply_slot = slot.clone();
    let apply_on_saved = on_saved.clone();
    let apply_button = primary_button("Apply selected", move || {
        let Some(review) = apply_review_state.borrow().clone() else {
            return;
        };
        let selected: Vec<String> = review
            .checks
            .iter()
            .filter(|(_, check)| check.is_active())
            .map(|(field, _)| field.clone())
            .collect();
        let candidate = review.candidate.clone();
        apply_generation.set(apply_generation.get().wrapping_add(1));
        let apply_gen = apply_generation.get();
        apply_footer.set_sensitive(false);
        apply_status.set_text("Applying the selected fields…");

        let service = apply_service.clone();
        let generation = apply_generation.clone();
        let footer = apply_footer.clone();
        let status = apply_status.clone();
        let slot = apply_slot.clone();
        let on_saved = apply_on_saved.clone();
        crate::tasks::spawn(
            "Applying series metadata",
            move |_reporter| {
                service.apply_comic_series_candidate(apply_series_id, &candidate, &selected)
            },
            |_| {},
            move |result| {
                if generation.get() != apply_gen {
                    return;
                }
                match result {
                    Ok(outcome) => {
                        let title = outcome.update.title.clone();
                        let summary = outcome_summary(&outcome);
                        if outcome.cover_error.is_some() {
                            crate::notify::error("Series metadata saved", &summary);
                        } else {
                            crate::notify::success("Series metadata saved", &summary);
                        }
                        if let Some(dialog) = slot.borrow().clone() {
                            dialog.close();
                        }
                        on_saved(title);
                    }
                    Err(err) => {
                        footer.set_sensitive(true);
                        status.set_text(&err);
                        crate::notify::error("Could not apply series metadata", &err);
                    }
                }
            },
        );
    });
    apply_button.set_visible(false);
    footer.append(&apply_button);

    let set_busy: Rc<dyn Fn(bool)> = {
        let search_button = search_button.clone();
        let refresh_button = refresh_button.clone();
        let apply_button = apply_button.clone();
        Rc::new(move |busy: bool| {
            search_button.set_sensitive(!busy);
            if let Some(button) = &refresh_button {
                button.set_sensitive(!busy);
            }
            if busy {
                apply_button.set_sensitive(false);
            }
        })
    };

    let update_apply_label: Rc<dyn Fn()> = {
        let review_state = review_state.clone();
        let apply_button = apply_button.clone();
        Rc::new(move || {
            let any_selected = review_state.borrow().as_ref().is_some_and(|review| {
                review.checks.iter().any(|(_, check)| check.is_active())
            });
            apply_button.set_label(if any_selected {
                "Apply selected"
            } else {
                "Link this source"
            });
        })
    };

    let show_review: Rc<dyn Fn(Candidate)> = {
        let content = content.clone();
        let series = series.clone();
        let review_state = review_state.clone();
        let apply_button = apply_button.clone();
        let back_button = back_button.clone();
        let status = status.clone();
        let generation = generation.clone();
        let results_state = results_state.clone();
        let update_apply_label = update_apply_label.clone();
        Rc::new(move |candidate: Candidate| {
            let review_generation = generation.get();
            let can_go_back = !results_state.borrow().is_empty();
            build_review(
                &content,
                &series,
                &candidate,
                &review_state,
                &apply_button,
                &back_button,
                can_go_back,
                &status,
                &generation,
                review_generation,
                &update_apply_label,
            );
        })
    };
    *show_review_holder.borrow_mut() = Some(show_review.clone());

    let pick_candidate: Rc<dyn Fn(Candidate)> = {
        let service = service.clone();
        let generation = generation.clone();
        let status = status.clone();
        let show_review_holder = show_review_holder.clone();
        let set_busy = set_busy.clone();
        Rc::new(move |candidate: Candidate| {
            generation.set(generation.get().wrapping_add(1));
            let detail_generation = generation.get();
            set_busy(true);
            status.set_text("Fetching the full record…");
            let service = service.clone();
            let generation_for_done = generation.clone();
            let status_for_done = status.clone();
            let holder_for_done = show_review_holder.clone();
            let set_busy_for_done = set_busy.clone();
            crate::tasks::spawn(
                "Fetching series details",
                move |_reporter| fetch_candidate_detail(&service, candidate),
                |_| {},
                move |(candidate, note)| {
                    if generation_for_done.get() != detail_generation {
                        set_busy_for_done(false);
                        return;
                    }
                    set_busy_for_done(false);
                    if let Some(show) = holder_for_done.borrow().clone() {
                        show(candidate);
                    }
                    if let Some(note) = note {
                        status_for_done.set_text(&note);
                    }
                },
            );
        })
    };

    let show_results: Rc<dyn Fn()> = {
        let content = content.clone();
        let results_state = results_state.clone();
        let review_state = review_state.clone();
        let apply_button = apply_button.clone();
        let back_button = back_button.clone();
        let pick_candidate = pick_candidate.clone();
        Rc::new(move || {
            *review_state.borrow_mut() = None;
            apply_button.set_visible(false);
            back_button.set_visible(false);
            let candidates = results_state.borrow().clone();
            show_candidate_results(&content, &candidates, &pick_candidate);
        })
    };
    *show_results_holder.borrow_mut() = Some(show_results.clone());

    let search_action: Rc<dyn Fn()> = {
        let service = service.clone();
        let query_field = query_field.clone();
        let generation = generation.clone();
        let status = status.clone();
        let results_state = results_state.clone();
        let show_results_holder = show_results_holder.clone();
        let set_busy = set_busy.clone();
        Rc::new(move || {
            let query = query_field.value().trim().to_string();
            if query.is_empty() {
                status.set_text("Type a series title to search for.");
                return;
            }
            let sources = metadata::enabled_comic_sources(service.catalog());
            if sources.is_empty() {
                status.set_text(
                    "No comic metadata sources are enabled — see Settings > Metadata sources.",
                );
                return;
            }
            generation.set(generation.get().wrapping_add(1));
            let search_generation = generation.get();
            set_busy(true);
            status.set_text("Searching MyAnimeList and Comic Vine…");
            let generation_for_done = generation.clone();
            let status_for_done = status.clone();
            let results_for_done = results_state.clone();
            let holder_for_done = show_results_holder.clone();
            let set_busy_for_done = set_busy.clone();
            crate::tasks::spawn(
                "Searching comic metadata",
                move |_reporter| metadata::search_all(sources, &query, 12),
                |_| {},
                move |(candidates, errors)| {
                    if generation_for_done.get() != search_generation {
                        set_busy_for_done(false);
                        return;
                    }
                    set_busy_for_done(false);
                    let count = candidates.len();
                    *results_for_done.borrow_mut() = candidates;
                    if let Some(show) = holder_for_done.borrow().clone() {
                        show();
                    }
                    let error_note = if errors.is_empty() {
                        String::new()
                    } else {
                        format!(
                            " ({})",
                            errors
                                .iter()
                                .map(|(id, err)| format!("{}: {err}", id.label()))
                                .collect::<Vec<_>>()
                                .join("; ")
                        )
                    };
                    status_for_done.set_text(&if count == 0 {
                        format!("No matches.{error_note}")
                    } else {
                        format!(
                            "{count} match{} — review one before anything is saved.{error_note}",
                            if count == 1 { "" } else { "es" }
                        )
                    });
                },
            );
        })
    };
    *search_holder.borrow_mut() = Some(search_action.clone());
    {
        let search_action = search_action.clone();
        query_field.entry().connect_activate(move |_| search_action());
    }

    let refresh_action: Rc<dyn Fn()> = {
        let service = service.clone();
        let series = series.clone();
        let generation = generation.clone();
        let status = status.clone();
        let results_state = results_state.clone();
        let show_review_holder = show_review_holder.clone();
        let set_busy = set_busy.clone();
        Rc::new(move || {
            generation.set(generation.get().wrapping_add(1));
            let refresh_generation = generation.get();
            results_state.borrow_mut().clear();
            set_busy(true);
            status.set_text("Refreshing the linked record…");
            let service = service.clone();
            let series_for_task = series.clone();
            let generation_for_done = generation.clone();
            let status_for_done = status.clone();
            let holder_for_done = show_review_holder.clone();
            let set_busy_for_done = set_busy.clone();
            crate::tasks::spawn(
                "Refreshing series metadata",
                move |_reporter| refresh_linked_candidate(&service, &series_for_task),
                |_| {},
                move |result| {
                    if generation_for_done.get() != refresh_generation {
                        set_busy_for_done(false);
                        return;
                    }
                    set_busy_for_done(false);
                    match result {
                        Ok(Some(candidate)) => {
                            if let Some(show) = holder_for_done.borrow().clone() {
                                show(candidate);
                            }
                        }
                        Ok(None) => {
                            status_for_done.set_text(
                                "The linked source no longer has that record. Search to link a different one.",
                            );
                        }
                        Err(err) => status_for_done.set_text(&err),
                    }
                },
            );
        })
    };
    *refresh_holder.borrow_mut() = Some(refresh_action);

    match present_dialog(
        anchor,
        "Find series metadata",
        DialogExit::UnsavedInput,
        &shell,
    ) {
        Some(dialog) => {
            *slot.borrow_mut() = Some(dialog);
            if let Some(candidate) = staged_candidate {
                generation.set(generation.get().wrapping_add(1));
                show_review(candidate);
            }
        }
        None => crate::notify::error(
            "Could not open the dialog",
            "The series metadata dialog could not be opened.",
        ),
    }
}

/// A synthetic candidate for the screenshot hook only. It lets CI
/// photograph the review state without network access or credentials.
pub fn staged_review_candidate(series: &ComicSeries) -> Candidate {
    let author = if series.author.trim().is_empty() || series.author == "Unknown" {
        "Ada Meridian".to_string()
    } else {
        series.author.clone()
    };
    Candidate {
        title: series.title.clone(),
        authors: author.clone(),
        series: Some(series.title.clone()),
        tags: vec!["Adventure".into(), "Fantasy".into(), "Drama".into()],
        first_year: Some(2021),
        publisher: "Meridian Press".into(),
        published: "2021".into(),
        description: "A cartographer whose maps rearrange the coastline each night is hired to chart a route that should not exist — and discovers that every harbour on it remembers a different version of her.".into(),
        source: Some(SourceId::MyAnimeList),
        credits: vec![crate::metadata::CandidateCredit {
            role: "Story & Art".into(),
            name: author,
        }],
        original_title: "Kami no Meridian".into(),
        serialization: "Monthly Meridian".into(),
        status: "Ongoing".into(),
        media_type: "Manga".into(),
        known_volumes: Some(8),
        known_chapters: Some(64),
        rating: Some(8.4),
        external_id: Some("424242".into()),
        source_url: Some("https://myanimelist.net/manga/424242".into()),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn series() -> ComicSeries {
        ComicSeries {
            id: 7,
            title: "Paper Meridian".into(),
            sort_title: "paper meridian".into(),
            author: "Ada Meridian".into(),
            description: "Old summary.".into(),
            cover_book_id: None,
            cover_path: None,
            metadata_cover_path: None,
            status: "Ongoing".into(),
            total_chapters: 2,
            completed_chapters: 0,
            unread_chapters: 2,
            created_at: String::new(),
            updated_at: String::new(),
            last_read_at: None,
            original_title: String::new(),
            credits: Vec::new(),
            publisher: String::new(),
            serialization: String::new(),
            published: String::new(),
            end_date: String::new(),
            media_type: String::new(),
            genres: Vec::new(),
            known_volumes: None,
            known_chapters: None,
            metadata_source: String::new(),
            metadata_source_id: String::new(),
            metadata_source_url: String::new(),
            metadata_fetched_at: String::new(),
            manual_fields: vec![COMIC_SERIES_FIELD_TITLE.into()],
        }
    }

    #[test]
    fn credits_parse_roles_and_bare_names() {
        let credits = parse_credits("Story: Ada Meridian\nBea Meridian\nArt: Ada Meridian\n");
        assert_eq!(credits.len(), 3);
        assert_eq!(credits[0].role, "Story");
        assert_eq!(credits[1].role, "");
        assert_eq!(credits[1].name, "Bea Meridian");
        assert_eq!(parse_count("Volumes", " 12 ").unwrap(), Some(12));
        assert_eq!(parse_count("Volumes", "").unwrap(), None);
        assert!(parse_count("Volumes", "-1").is_err());
        assert_eq!(parse_genres("Adventure, adventure, Drama"), vec!["Adventure", "Drama"]);
    }

    #[test]
    fn review_proposals_only_offer_differing_nonempty_fields() {
        let series = series();
        let candidate = staged_review_candidate(&series);
        let proposals = candidate_field_proposals(&series, &candidate);
        let fields: Vec<&str> = proposals.iter().map(|proposal| proposal.field).collect();
        assert!(fields.contains(&COMIC_SERIES_FIELD_DESCRIPTION));
        assert!(fields.contains(&COMIC_SERIES_FIELD_CREDITS));
        assert!(fields.contains(&COMIC_SERIES_FIELD_KNOWN_CHAPTERS));
        assert!(!fields.contains(&COMIC_SERIES_FIELD_STATUS), "same status");
        assert!(!fields.contains(&COMIC_SERIES_FIELD_END_DATE), "empty end date");

        let mut changed_title = candidate.clone();
        changed_title.title = "The Paper Meridian".into();
        let proposals = candidate_field_proposals(&series, &changed_title);
        let title = proposals
            .iter()
            .find(|proposal| proposal.field == COMIC_SERIES_FIELD_TITLE)
            .expect("title differs");
        assert!(title.locked, "hand-edited title must be locked in review");
    }

    #[test]
    fn detail_merge_fills_holes_from_search() {
        let search = Candidate {
            title: "Paper Meridian".into(),
            authors: "Ada Meridian".into(),
            tags: vec!["Adventure".into()],
            external_id: Some("42".into()),
            source: Some(SourceId::ComicVine),
            ..Default::default()
        };
        let detail = Candidate {
            description: "Full description.".into(),
            ..Default::default()
        };
        let merged = merge_candidate_detail(search, detail);
        assert_eq!(merged.title, "Paper Meridian");
        assert_eq!(merged.description, "Full description.");
        assert_eq!(merged.external_id.as_deref(), Some("42"));
        assert_eq!(merged.tags, vec!["Adventure"]);
    }

    #[test]
    fn changed_fields_ignore_whitespace_only_edits() {
        let series = series();
        let edit = ComicSeriesEdit {
            title: " Paper   Meridian ".into(),
            author: series.author.clone(),
            description: "Old summary.".into(),
            status: series.status.clone(),
            ..Default::default()
        };
        assert!(changed_manual_fields(&series, &edit).is_empty());
    }
}
