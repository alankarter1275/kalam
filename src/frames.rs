//! Frame-level probes for the paint-cycle hunt (7.1 step 4, the
//! instrument run #5's ranked list named next).
//!
//! Every app-side suspect for the remaining 250–550 ms blocks has been
//! cleared by earlier probes: decodes are 3–8 ms, landings 0.1 ms,
//! fills 3–6 ms. What is left between those spans is GTK's own frame
//! work — realizing, laying out, styling, snapshotting the freshly
//! built trees and uploading the swapped textures — and no span covers
//! it. This module adds the missing instrument, entirely inside
//! `KALAM_TIMING=1`, by watching the main window's frame clock:
//!
//! - `frame_after:<span>` — from a measured span's end to the next
//!   frame's before-paint phase: how long the freshly built content
//!   waited before the toolkit began drawing it. Big here means the
//!   main loop was busy after the span — work no span covers yet.
//! - `frame_layout:<span>` — that frame's layout phase (size
//!   negotiation and allocation of the freshly built tree).
//! - `frame_paint:<span>` — that frame's paint phase (snapshot,
//!   render, texture upload) — before-paint to after-paint.
//!
//! Together they split every "block around a page build" three ways:
//! before the frame (app work the spans missed), in the frame's layout
//! (tree construction costs GTK charges at draw time), in the frame's
//! paint (drawing and upload) — or none of them, in which case the
//! block is elsewhere and the hunt moves on.
//!
//! The clock emits `update` only for animation frames, so the probes
//! hook the phases a repaint always runs: `layout` (when requested),
//! `before-paint` and `after-paint` (the paint phase is why the frame
//! exists). The state machine is plain data and f64 milliseconds since
//! a shared anchor, so it is testable headlessly; the GTK half is
//! three tiny signal handlers connected once at realize, doing
//! arithmetic only — no scheduling, no drawing, nothing that could
//! change the behavior being measured.

use gtk::prelude::*;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// Report a phase cost only when it reached this many ms. A normal
/// frame is a few ms; the blocks being hunted start at 250 ms, so
/// 50 ms separates "a busy frame" from noise.
const PHASE_MS: f64 = 50.0;
/// Attribute frames to a watched span only within this window of its
/// ending, so an idle animation far from any build can never spray
/// the log.
const INTEREST_MS: f64 = 2000.0;
/// A layout time further than this before its frame's paint began is
/// a stale leftover from a cycle that never painted, not this frame's
/// layout — ignore it rather than report an absurd number.
const STALE_LAYOUT_MS: f64 = 1000.0;
/// Span ends waiting for a frame: a cap, not a target. If the clock
/// never fires (headless run, or the realize hook lost the race), the
/// oldest entries fall off instead of the list growing forever.
const PENDING_CAP: usize = 64;

/// One probe line to print, built by [`FramesState::frame_after_paint`].
pub(crate) struct FrameLine {
    /// `frame_after:<span>`, `frame_layout:<span>` or
    /// `frame_paint:<span>` — prebuilt so the signal handler stays
    /// dumb and the tests can assert on it.
    pub label: String,
    pub ms: f64,
}

/// The probe's entire state, in ms since an arbitrary common anchor.
#[derive(Default)]
pub(crate) struct FramesState {
    /// Measured spans that have ended but not yet seen a frame.
    pending: Vec<(&'static str, f64)>,
    /// before-paint time of the frame in flight. Always paired with an
    /// after-paint inside the same cycle, so plain overwrite semantics
    /// are self-healing even if a cycle ever aborted between the two.
    paint_start: Option<f64>,
    /// Layout time, if a layout phase ran for the frame in flight.
    layout_start: Option<f64>,
    /// after-paint time of the last completed frame — the boundary
    /// below which a layout time belongs to an earlier cycle.
    last_done: Option<f64>,
    /// Phase costs are attributed to a watched span only while the
    /// clock is inside this window of its ending.
    interest_until: Option<f64>,
    /// The watched span that window belongs to.
    interest_label: Option<&'static str>,
}

impl FramesState {
    /// A measured span ended (a `timing` line was printed).
    pub(crate) fn span_ended(&mut self, label: &'static str, now_ms: f64) {
        if self.pending.len() >= PENDING_CAP {
            self.pending.remove(0);
        }
        self.pending.push((label, now_ms));
        self.interest_until = Some(now_ms + INTEREST_MS);
        self.interest_label = Some(label);
    }

    /// The frame clock's layout phase: the second step of a frame,
    /// run only when something asked for a re-layout.
    pub(crate) fn frame_layout(&mut self, now_ms: f64) {
        self.layout_start = Some(now_ms);
    }

    /// The frame clock's before-paint phase: the paint phase is
    /// beginning. Always emitted together with after-paint.
    pub(crate) fn frame_paint_start(&mut self, now_ms: f64) {
        self.paint_start = Some(now_ms);
    }

    /// The frame clock's after-paint phase: frame processing has
    /// ended. Returns the lines to print.
    pub(crate) fn frame_after_paint(&mut self, now_ms: f64) -> Vec<FrameLine> {
        let mut out = Vec::new();
        let paint = self.paint_start.take().unwrap_or(now_ms);
        // A layout time belongs to this frame only if it ran after the
        // previous frame completed, not long before this frame's paint
        // began, and not after the paint it was supposed to precede.
        let layout = self.layout_start.take().filter(|l| {
            *l <= paint
                && paint - *l <= STALE_LAYOUT_MS
                && self.last_done.is_none_or(|d| *l > d)
        });
        self.last_done = Some(now_ms);
        for (label, ended) in self.pending.drain(..) {
            // `max(0)`: a span that ended while a frame was already in
            // flight reports ~0 — its content is in that frame, and the
            // frame's own phase costs are the rest of the story.
            out.push(FrameLine {
                label: format!("frame_after:{label}"),
                ms: (paint - ended).max(0.0),
            });
        }
        let interest_hot = self
            .interest_until
            .is_some_and(|until| now_ms <= until);
        if interest_hot {
            if let Some(label) = self.interest_label {
                let paint_ms = now_ms - paint;
                if paint_ms >= PHASE_MS {
                    out.push(FrameLine {
                        label: format!("frame_paint:{label}"),
                        ms: paint_ms,
                    });
                }
                if let Some(l) = layout {
                    let layout_ms = paint - l;
                    if layout_ms >= PHASE_MS {
                        out.push(FrameLine {
                            label: format!("frame_layout:{label}"),
                            ms: layout_ms,
                        });
                    }
                }
            }
        }
        out
    }
}

static STATE: OnceLock<Mutex<FramesState>> = OnceLock::new();
static ANCHOR: OnceLock<Instant> = OnceLock::new();

fn state() -> &'static Mutex<FramesState> {
    STATE.get_or_init(|| Mutex::new(FramesState::default()))
}

fn now_ms() -> f64 {
    ANCHOR.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
}

/// Record that a measured span ended. Called only from `timing` code
/// paths that already no-op unless `KALAM_TIMING=1`, so the gating
/// lives there; safe from any thread (worker-side service spans end
/// here too, and their frames are as interesting as the UI's own).
pub fn note_span_end(label: &'static str) {
    let mut s = state().lock().unwrap_or_else(|e| e.into_inner());
    s.span_ended(label, now_ms());
}

/// Hook the main window's frame clock, once, at realize — so the very
/// first frame is captured. Best-effort by design: if the clock is not
/// available, the probe simply sees no frames and prints nothing; a
/// missing instrument, never a broken app.
pub fn install(window: &gtk::Window) {
    if !crate::timing::enabled() {
        return;
    }
    let Some(clock) = window.frame_clock() else {
        return;
    };
    clock.connect_layout(|_clock| {
        let mut s = state().lock().unwrap_or_else(|e| e.into_inner());
        s.frame_layout(now_ms());
    });
    clock.connect_before_paint(|_clock| {
        let mut s = state().lock().unwrap_or_else(|e| e.into_inner());
        s.frame_paint_start(now_ms());
    });
    // Run #6 named the family: every remaining block sits inside the
    // paint phase, and the expensive paints are the ones where new
    // cover textures land. The next fork is WHICH renderer is drawing
    // those frames — a hardware GL renderer and a software fallback
    // behave very differently at 250–640 ms per frame — so the first
    // frame also names it, once. (The renderer certainly exists by
    // then: a frame was just painted.)
    let window_for_name = window.clone();
    let named = std::cell::Cell::new(false);
    clock.connect_after_paint(move |_clock| {
        if !named.replace(true) {
            if let Some(renderer) = window_for_name.renderer() {
                println!("[timing] render_backend     {}", renderer.type_().name());
            }
        }
        let lines = {
            let mut s = state().lock().unwrap_or_else(|e| e.into_inner());
            s.frame_after_paint(now_ms())
        };
        for line in lines {
            println!("[timing] {} {:>8.1} ms", line.label, line.ms);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_span_ended_waits_for_the_next_frame() {
        let mut s = FramesState::default();
        s.span_ended("home_fill", 100.0);
        s.frame_layout(110.0);
        s.frame_paint_start(150.0);
        let lines = s.frame_after_paint(160.0);
        assert_eq!(
            lines.len(),
            1,
            "one line: the wait — layout 40 and paint 10 are below the bar"
        );
        assert_eq!(lines[0].label, "frame_after:home_fill");
        assert_eq!(lines[0].ms, 50.0, "paint start minus span end");
    }

    #[test]
    fn every_pending_span_flushes_on_the_same_frame() {
        let mut s = FramesState::default();
        s.span_ended("a", 100.0);
        s.span_ended("b", 120.0);
        s.span_ended("c", 140.0);
        s.frame_paint_start(200.0);
        let lines = s.frame_after_paint(205.0);
        assert_eq!(lines.len(), 3, "all three waits, one line each");
        assert_eq!(lines[0].label, "frame_after:a");
        assert_eq!(lines[0].ms, 100.0);
        assert_eq!(lines[2].label, "frame_after:c");
        assert_eq!(lines[2].ms, 60.0);
        // And they are not re-reported by the frame after that.
        s.frame_paint_start(300.0);
        let lines = s.frame_after_paint(305.0);
        assert!(lines.is_empty(), "pending was drained");
    }

    #[test]
    fn an_expensive_paint_phase_is_attributed() {
        let mut s = FramesState::default();
        s.span_ended("route_open:library", 100.0);
        s.frame_layout(110.0);
        s.frame_paint_start(120.0);
        // A 360 ms paint phase: snapshot, render, upload — the
        // post-construction-freeze signature the hunt wants.
        let lines = s.frame_after_paint(480.0);
        assert_eq!(lines.len(), 2, "the wait and the paint cost");
        assert_eq!(lines[0].label, "frame_after:route_open:library");
        assert_eq!(lines[0].ms, 20.0);
        assert_eq!(lines[1].label, "frame_paint:route_open:library");
        assert_eq!(lines[1].ms, 360.0);
    }

    #[test]
    fn an_expensive_layout_phase_is_attributed() {
        let mut s = FramesState::default();
        s.span_ended("book_page_rebuild", 100.0);
        // Size negotiation of a freshly built tree, then a cheap paint.
        s.frame_layout(120.0);
        s.frame_paint_start(440.0);
        let lines = s.frame_after_paint(450.0);
        assert_eq!(lines.len(), 2, "the wait and the layout cost");
        assert_eq!(lines[0].label, "frame_after:book_page_rebuild");
        assert_eq!(lines[0].ms, 340.0);
        assert_eq!(lines[1].label, "frame_layout:book_page_rebuild");
        assert_eq!(lines[1].ms, 320.0);
    }

    #[test]
    fn an_expensive_frame_outside_the_interest_window_is_silent() {
        let mut s = FramesState::default();
        s.span_ended("home_fill", 100.0);
        s.frame_paint_start(3000.0);
        // Interest expired at 2100; this 600 ms paint ends long after.
        let lines = s.frame_after_paint(3600.0);
        assert_eq!(lines.len(), 1, "the wait only, no phase attribution");
        assert_eq!(lines[0].label, "frame_after:home_fill");
    }

    #[test]
    fn a_layout_from_an_abandoned_cycle_is_ignored() {
        let mut s = FramesState::default();
        // A layout whose cycle never painted…
        s.frame_layout(10.0);
        // …then a completed frame sets the boundary…
        s.frame_paint_start(100.0);
        s.frame_after_paint(105.0);
        // …so the stale layout must not be charged to the next frame.
        s.span_ended("covers_land", 2000.0);
        s.frame_paint_start(2010.0);
        let lines = s.frame_after_paint(2100.0);
        assert!(
            !lines.iter().any(|l| l.label.starts_with("frame_layout")),
            "the stale layout is dropped, not measured as 2 s"
        );
        // And one from before the last completed frame is dropped too.
        s.frame_layout(50.0);
        s.frame_paint_start(2200.0);
        let lines = s.frame_after_paint(2210.0);
        assert!(
            !lines.iter().any(|l| l.label.starts_with("frame_layout")),
            "a pre-boundary layout belongs to no frame"
        );
    }

    #[test]
    fn a_frame_with_no_recorded_paint_start_still_flushes_the_waits() {
        let mut s = FramesState::default();
        s.span_ended("init_done", 100.0);
        // after-paint with no before-paint seen (cannot happen — both
        // are connected together — but the state must not guess a start
        // it does not have, so the wait is measured to the frame's end).
        let lines = s.frame_after_paint(300.0);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].ms, 200.0);
    }

    #[test]
    fn pending_is_capped_so_a_silent_clock_cannot_grow_it_forever() {
        let mut s = FramesState::default();
        for i in 0..70 {
            // Distinct labels for every index, so the test can prove
            // exactly WHICH entries fell off. (A hand-written label
            // table once stopped at span_5 and called index 6
            // "span_later", so an assert on "span_6" tested a label
            // that never existed; generate, don't enumerate.)
            let label: &'static str = Box::leak(format!("span_{i}").into_boxed_str());
            s.span_ended(label, i as f64);
        }
        assert_eq!(s.pending.len(), PENDING_CAP, "capped at the constant");
        assert_eq!(
            s.pending[0].0, "span_6",
            "the oldest entries fell off, not the newest"
        );
        assert_eq!(s.pending[PENDING_CAP - 1].0, "span_69");
    }
}
