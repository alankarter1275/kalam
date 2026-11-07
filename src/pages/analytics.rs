//! P4 — Analytics: honest numbers derived from the catalog and session log.

use crate::db::{Catalog, LibraryStats};
use crate::service::LibraryService;
use crate::widgets::charts::{line_chart, monthly_series, sparkline, streak_strip};
use gtk::prelude::*;
use relm4::prelude::*;
use std::sync::Arc;

#[derive(Debug)]
pub enum AnalyticsMsg {
    Refresh,
    SetGoal(i64),
}

pub struct AnalyticsModel {
    service: LibraryService,
    stats: LibraryStats,
    goal: i64,
    finished_this_year: i64,
    week: [bool; 7],
}

#[relm4::component(pub)]
impl Component for AnalyticsModel {
    type Init = Arc<Catalog>;
    type Input = AnalyticsMsg;
    type Output = ();
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 12,
            set_hexpand: true,

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 10,

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_hexpand: true,

                    gtk::Label {
                        set_label: "Analytics",
                        add_css_class: "kalam-page-title",
                        set_halign: gtk::Align::Start,
                    },
                    gtk::Label {
                        set_label: "Everything below is measured locally — nothing leaves your machine.",
                        add_css_class: "kalam-page-sub",
                        set_halign: gtk::Align::Start,
                    },
                },

                gtk::Button {
                    set_child: Some(&crate::icons::symbolic_with_classes(
                        "view-refresh-symbolic",
                        16,
                        &["kalam-inline-icon"],
                    )),
                    add_css_class: "kalam-secondary-btn",
                    set_valign: gtk::Align::Center,
                    connect_clicked => AnalyticsMsg::Refresh,
                },
            },

            gtk::ScrolledWindow {
                set_vexpand: true,
                set_hexpand: true,
                set_hscrollbar_policy: gtk::PolicyType::Never,

                #[name = "body"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 18,
                },
            },
        }
    }

    fn init(
        catalog: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // A0 step 2: one service call replaces four direct catalog reads,
        // each of which used to swallow its own error.
        let service = LibraryService::new(catalog);
        let snap = service.analytics();
        report_errors(&snap.errors);
        let model = AnalyticsModel {
            service,
            stats: snap.stats,
            goal: snap.goal,
            finished_this_year: snap.finished_this_year,
            week: snap.week,
        };
        let widgets = view_output!();
        rebuild(&widgets.body, &model, &sender);
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
            AnalyticsMsg::Refresh => {
                self.reload();
            }
            AnalyticsMsg::SetGoal(books) => {
                self.service.catalog().set_reading_goal(books);
                crate::notify::success(
                    "Reading goal saved",
                    &format!(
                        "{books} book{} this year",
                        if books == 1 { "" } else { "s" }
                    ),
                );
                self.reload();
            }
        }
        rebuild(&widgets.body, self, &sender);
        self.update_view(widgets, sender);
    }
}

impl AnalyticsModel {
    fn reload(&mut self) {
        let snap = self.service.analytics();
        report_errors(&snap.errors);
        self.stats = snap.stats;
        self.goal = snap.goal;
        self.finished_this_year = snap.finished_this_year;
        self.week = snap.week;
    }
}

/// Surface read failures instead of rendering them as an empty page. The
/// service collects them; deciding what the user sees stays with the UI.
fn report_errors(errors: &[String]) {
    for err in errors {
        crate::notify::error("Could not read the library", err);
    }
}

fn rebuild(body: &gtk::Box, model: &AnalyticsModel, sender: &ComponentSender<AnalyticsModel>) {
    let stats = &model.stats;
    while let Some(child) = body.first_child() {
        body.remove(&child);
    }

    if stats.total_books == 0 {
        let empty = gtk::Label::new(Some(
            "Nothing to measure yet — import a book and start reading.",
        ));
        empty.add_css_class("kalam-placeholder");
        empty.set_halign(gtk::Align::Start);
        body.append(&empty);
        return;
    }

    // ── hero row: three headline cards, each with its own sparkline ─────
    let daily: Vec<i64> = stats.minutes_by_day.iter().map(|(_, s)| *s / 60).collect();

    let hero = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    hero.set_homogeneous(true);

    hero.append(&hero_card(
        "Avg Time /Day",
        &stats.avg_minutes_per_active_day.to_string(),
        "Min/Day",
        &daily,
        "kalam-spark-green",
        &avg_blurb(stats.avg_minutes_per_active_day),
    ));

    hero.append(&hero_card(
        "Books Completed",
        &stats.finished.to_string(),
        "Books",
        &monthly_series(stats),
        "kalam-spark-red",
        &finished_blurb(stats.finished, stats.finished_last_30),
    ));

    hero.append(&hero_card(
        "Current Streak",
        &stats.current_streak_days.to_string(),
        if stats.current_streak_days == 1 {
            "Day"
        } else {
            "Days"
        },
        &daily,
        "kalam-spark-blue",
        &streak_blurb(stats.current_streak_days, stats.longest_streak_days),
    ));
    body.append(&hero);

    // ── goal + weekly streak ────────────────────────────────────────────
    let goal_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);

    let goal_card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    goal_card.add_css_class("kalam-chart-card");
    goal_card.set_hexpand(true);

    let goal_head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let gl = gtk::Label::new(Some("YEARLY GOAL"));
    gl.add_css_class("kalam-section-label");
    gl.set_hexpand(true);
    gl.set_halign(gtk::Align::Start);
    goal_head.append(&gl);
    goal_card.append(&goal_head);

    if model.goal > 0 {
        let figure = gtk::Label::new(Some(&format!(
            "{} / {} books",
            model.finished_this_year, model.goal
        )));
        figure.add_css_class("kalam-hero-value");
        figure.set_halign(gtk::Align::Start);
        goal_card.append(&figure);

        let bar = gtk::ProgressBar::new();
        bar.set_fraction((model.finished_this_year as f64 / model.goal as f64).clamp(0.0, 1.0));
        bar.add_css_class("kalam-mini-progress");
        goal_card.append(&bar);

        let left = (model.goal - model.finished_this_year).max(0);
        let note = gtk::Label::new(Some(&if left == 0 {
            "Goal reached — nicely done.".to_string()
        } else {
            format!("{left} to go this year")
        }));
        note.add_css_class("kalam-hero-blurb");
        note.set_halign(gtk::Align::Start);
        goal_card.append(&note);
    } else {
        let note = gtk::Label::new(Some("No goal set for this year."));
        note.add_css_class("kalam-hero-blurb");
        note.set_halign(gtk::Align::Start);
        goal_card.append(&note);
    }

    let picker = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    for target in [0_i64, 12, 24, 52] {
        let b = gtk::Button::with_label(&if target == 0 {
            "Off".to_string()
        } else {
            format!("{target}")
        });
        b.add_css_class("kalam-mini-btn");
        if target == model.goal {
            b.add_css_class("kalam-mini-btn-active");
        }
        let s = sender.clone();
        b.connect_clicked(move |_| s.input(AnalyticsMsg::SetGoal(target)));
        picker.append(&b);
    }
    let hint = gtk::Label::new(Some("Set goal:"));
    hint.add_css_class("kalam-muted");
    let picker_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    picker_row.append(&hint);
    picker_row.append(&picker);
    goal_card.append(&picker_row);
    goal_row.append(&goal_card);

    let streak_card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    streak_card.add_css_class("kalam-chart-card");
    streak_card.set_hexpand(true);
    let sl = gtk::Label::new(Some("DAILY STREAK"));
    sl.add_css_class("kalam-section-label");
    sl.set_halign(gtk::Align::Start);
    streak_card.append(&sl);
    streak_card.append(&streak_strip(model.week));
    let sn = gtk::Label::new(Some(&format!(
        "{} day streak · longest {}",
        stats.current_streak_days, stats.longest_streak_days
    )));
    sn.add_css_class("kalam-hero-blurb");
    sn.set_halign(gtk::Align::Start);
    streak_card.append(&sn);
    goal_row.append(&streak_card);

    body.append(&goal_row);

    // ── reading activity line chart ─────────────────────────────────────
    if daily.iter().any(|v| *v > 0) {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
        card.add_css_class("kalam-chart-card");

        let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let title = gtk::Label::new(Some(&format!("{} min", daily.iter().sum::<i64>())));
        title.add_css_class("kalam-chart-title");
        title.set_halign(gtk::Align::Start);
        head.append(&title);

        let sub = gtk::Label::new(Some("Read in the last 14 days"));
        sub.add_css_class("kalam-chart-sub");
        sub.set_halign(gtk::Align::Start);
        sub.set_hexpand(true);
        sub.set_valign(gtk::Align::End);
        head.append(&sub);
        card.append(&head);

        let labels: Vec<String> = stats
            .minutes_by_day
            .iter()
            .map(|(d, _)| short_day(d))
            .collect();
        card.append(&line_chart(&daily, &labels));
        body.append(&card);
    }

    // ── library breakdown ───────────────────────────────────────────────
    body.append(&section_label("LIBRARY"));
    body.append(&tile_row(&[
        ("Books", stats.total_books.to_string()),
        ("Finished", stats.finished.to_string()),
        ("Reading", stats.reading.to_string()),
        ("Unread", stats.unread.to_string()),
    ]));

    body.append(&section_label("TIME READ"));
    body.append(&tile_row(&[
        ("All time", human_duration(stats.total_seconds)),
        ("Last 7 days", human_duration(stats.seconds_last_7)),
        ("Last 30 days", human_duration(stats.seconds_last_30)),
        ("Sessions", stats.sessions.to_string()),
    ]));

    body.append(&section_label("MARKUP"));
    body.append(&tile_row(&[
        ("Highlights", stats.highlights.to_string()),
        ("Quotes", stats.quotes.to_string()),
        ("Saved words", stats.saved_words.to_string()),
        ("Shelves", stats.shelves.to_string()),
    ]));

    if !stats.added_by_month.is_empty() {
        body.append(&section_label("BOOKS ADDED PER MONTH"));
        body.append(&bar_chart(
            &stats
                .added_by_month
                .iter()
                .map(|(ym, n)| (short_month(ym), *n))
                .collect::<Vec<_>>(),
            "",
        ));
    }

    // ── leaderboards ────────────────────────────────────────────────────
    let columns = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    columns.set_homogeneous(true);

    if !stats.most_read.is_empty() {
        columns.append(&ranked_list(
            "MOST READ",
            &stats
                .most_read
                .iter()
                .map(|(title, secs)| (title.clone(), human_duration(*secs)))
                .collect::<Vec<_>>(),
        ));
    }
    if !stats.top_tags.is_empty() {
        columns.append(&ranked_list(
            "TOP TAGS",
            &stats
                .top_tags
                .iter()
                .map(|(name, n)| (name.clone(), n.to_string()))
                .collect::<Vec<_>>(),
        ));
    }
    if !stats.top_authors.is_empty() {
        columns.append(&ranked_list(
            "TOP AUTHORS",
            &stats
                .top_authors
                .iter()
                .map(|(name, n)| (name.clone(), n.to_string()))
                .collect::<Vec<_>>(),
        ));
    }
    if columns.first_child().is_some() {
        body.append(&columns);
    }
}

fn avg_blurb(minutes: i64) -> String {
    match minutes {
        0 => "No sessions logged yet — open a book to start tracking.".into(),
        1..=14 => "Short and steady. Every page counts.".into(),
        15..=44 => "A solid daily habit. Keep the momentum!".into(),
        _ => "Deep focus sessions — impressive stamina.".into(),
    }
}

fn finished_blurb(total: i64, last_30: i64) -> String {
    if total == 0 {
        "Finish your first book to start the count.".into()
    } else if last_30 > 0 {
        format!("{last_30} finished in the last 30 days. Great progress!")
    } else {
        "Nothing finished this month — pick one up again?".into()
    }
}

fn streak_blurb(current: i64, longest: i64) -> String {
    if current == 0 {
        "Read today to start a new streak.".into()
    } else if current >= longest && current > 1 {
        "This is your longest streak yet!".into()
    } else {
        format!("Longest so far: {longest} days. Keep going!")
    }
}

/// Headline card: label, big number + unit, a sparkline, and a line of copy.
fn hero_card(
    label: &str,
    value: &str,
    unit: &str,
    series: &[i64],
    spark_class: &str,
    blurb: &str,
) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 6);
    card.add_css_class("kalam-hero-card");

    let l = gtk::Label::new(Some(label));
    l.add_css_class("kalam-hero-label");
    l.set_halign(gtk::Align::Start);
    card.append(&l);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.set_valign(gtk::Align::Center);

    let v = gtk::Label::new(Some(value));
    v.add_css_class("kalam-hero-value");
    v.set_valign(gtk::Align::Baseline);
    row.append(&v);

    let u = gtk::Label::new(Some(unit));
    u.add_css_class("kalam-hero-unit");
    u.set_valign(gtk::Align::Baseline);
    u.set_hexpand(true);
    u.set_halign(gtk::Align::Start);
    row.append(&u);

    row.append(&sparkline(series, spark_class));
    card.append(&row);

    let b = gtk::Label::new(Some(blurb));
    b.add_css_class("kalam-hero-blurb");
    b.set_halign(gtk::Align::Start);
    b.set_xalign(0.0);
    b.set_wrap(true);
    b.set_lines(2);
    b.set_ellipsize(gtk::pango::EllipsizeMode::End);
    card.append(&b);

    card
}

/// Tiny inline trend, drawn with GtkDrawingArea (smooth, unlike stacked boxes).
fn section_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("kalam-section-label");
    label.set_halign(gtk::Align::Start);
    label
}

fn tile_row(items: &[(&str, String)]) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.set_homogeneous(true);
    for (label, value) in items {
        let tile = gtk::Box::new(gtk::Orientation::Vertical, 4);
        tile.add_css_class("kalam-stat-tile");

        let v = gtk::Label::new(Some(value));
        v.add_css_class("kalam-stat-value");
        v.set_halign(gtk::Align::Start);
        tile.append(&v);

        let l = gtk::Label::new(Some(label));
        l.add_css_class("kalam-stat-label");
        l.set_halign(gtk::Align::Start);
        tile.append(&l);

        row.append(&tile);
    }
    row
}

/// Simple CSS-drawn bar chart — no extra drawing dependency.
fn bar_chart(data: &[(String, i64)], unit: &str) -> gtk::Box {
    let chart = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    chart.add_css_class("kalam-bar-chart");
    chart.set_homogeneous(true);

    let max = data.iter().map(|(_, v)| *v).max().unwrap_or(1).max(1);

    for (label, value) in data {
        let col = gtk::Box::new(gtk::Orientation::Vertical, 4);
        col.set_valign(gtk::Align::End);

        let value_label = gtk::Label::new(Some(&if *value > 0 {
            format!("{value}{unit}")
        } else {
            String::new()
        }));
        value_label.add_css_class("kalam-bar-value");
        col.append(&value_label);

        // Height is proportional; 4px keeps empty days visible as a stub.
        let bar = gtk::Box::new(gtk::Orientation::Vertical, 0);
        bar.add_css_class("kalam-bar");
        if *value == 0 {
            bar.add_css_class("kalam-bar-empty");
        }
        let height = if *value == 0 {
            4
        } else {
            ((*value as f64 / max as f64) * 96.0).round().max(6.0) as i32
        };
        bar.set_size_request(-1, height);
        bar.set_valign(gtk::Align::End);
        col.append(&bar);

        let name = gtk::Label::new(Some(label));
        name.add_css_class("kalam-bar-label");
        col.append(&name);

        chart.append(&col);
    }
    chart
}

fn ranked_list(title: &str, rows: &[(String, String)]) -> gtk::Box {
    let col = gtk::Box::new(gtk::Orientation::Vertical, 6);
    col.append(&section_label(title));

    for (name, value) in rows {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.add_css_class("kalam-rank-row");

        let n = gtk::Label::new(Some(name));
        n.set_halign(gtk::Align::Start);
        n.set_hexpand(true);
        n.set_xalign(0.0);
        n.set_ellipsize(gtk::pango::EllipsizeMode::End);
        row.append(&n);

        let v = gtk::Label::new(Some(value));
        v.add_css_class("kalam-progress");
        row.append(&v);

        col.append(&row);
    }
    col
}

/// `4530` → `1h 15m`; small values stay in minutes so short sessions register.
fn human_duration(seconds: i64) -> String {
    if seconds <= 0 {
        return "—".into();
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{}m", minutes.max(1));
    }
    let hours = minutes / 60;
    let rem = minutes % 60;
    if rem == 0 {
        format!("{hours}h")
    } else {
        format!("{hours}h {rem}m")
    }
}

/// `2026-07-27` → `27`
fn short_day(day: &str) -> String {
    day.get(8..10).unwrap_or(day).to_string()
}

/// `2026-07` → `Jul`
fn short_month(ym: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let m: usize = ym.get(5..7).and_then(|s| s.parse().ok()).unwrap_or(1);
    MONTHS
        .get(m.saturating_sub(1))
        .copied()
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn average_blurb_follows_the_minute_bands() {
        assert_eq!(avg_blurb(0), "No sessions logged yet — open a book to start tracking.");
        assert_eq!(avg_blurb(1), "Short and steady. Every page counts.");
        assert_eq!(avg_blurb(14), "Short and steady. Every page counts.");
        assert_eq!(avg_blurb(15), "A solid daily habit. Keep the momentum!");
        assert_eq!(avg_blurb(44), "A solid daily habit. Keep the momentum!");
        assert_eq!(avg_blurb(45), "Deep focus sessions — impressive stamina.");
    }

    #[test]
    fn a_negative_average_falls_through_to_the_top_band() {
        // A `match` on i64 has no arm below zero, so a negative average lands
        // in the catch-all. It cannot happen from real data; this pins what the
        // code does rather than pretending the case cannot arise.
        assert_eq!(avg_blurb(-1), "Deep focus sessions — impressive stamina.");
    }

    #[test]
    fn finished_blurb_checks_the_total_before_it_checks_the_month() {
        assert_eq!(finished_blurb(0, 0), "Finish your first book to start the count.");
        // The zero total wins even when the month has entries, because the
        // first branch never looks at `last_30`.
        assert_eq!(finished_blurb(0, 4), "Finish your first book to start the count.");
        assert_eq!(finished_blurb(9, 0), "Nothing finished this month — pick one up again?");
        assert_eq!(finished_blurb(9, 4), "4 finished in the last 30 days. Great progress!");
    }

    #[test]
    fn streak_blurb_only_celebrates_a_record_longer_than_one_day() {
        assert_eq!(streak_blurb(0, 12), "Read today to start a new streak.");
        assert_eq!(streak_blurb(7, 7), "This is your longest streak yet!");
        assert_eq!(streak_blurb(8, 7), "This is your longest streak yet!");
        assert_eq!(streak_blurb(3, 12), "Longest so far: 12 days. Keep going!");
        // A first-ever one-day streak ties the record but is not celebrated,
        // because `current > 1` is part of the condition.
        assert_eq!(streak_blurb(1, 1), "Longest so far: 1 days. Keep going!");
    }

    #[test]
    fn a_duration_never_reads_as_zero_minutes() {
        assert_eq!(human_duration(0), "—");
        assert_eq!(human_duration(-30), "—");
        // Under a minute still reads "1m": `minutes.max(1)`.
        assert_eq!(human_duration(1), "1m");
        assert_eq!(human_duration(59), "1m");
        assert_eq!(human_duration(60), "1m");
        assert_eq!(human_duration(3599), "59m");
        assert_eq!(human_duration(3600), "1h");
        assert_eq!(human_duration(3660), "1h 1m");
        assert_eq!(human_duration(7200), "2h");
    }

    #[test]
    fn short_day_takes_the_day_digits_or_hands_the_input_back() {
        assert_eq!(short_day("2026-11-08"), "08");
        assert_eq!(short_day("2026-11-08T21:40:00"), "08");
        // Too short for the slice, so the whole string comes back unchanged.
        assert_eq!(short_day("2026-11"), "2026-11");
        assert_eq!(short_day(""), "");
    }
}
