//! Reading session telemetry and progress checkpointing.

use super::mod_model::ReaderModel;

impl ReaderModel {
    pub(crate) fn progress_pct(&self) -> i64 {
        let count = self.chapter_count;
        if count == 0 {
            return 0;
        }
        ((((self.chapter as f64) + self.fraction) / count as f64) * 100.0)
            .round()
            .clamp(0.0, 100.0) as i64
    }

    pub(crate) fn save_progress(&mut self) {
        if self.chapter_count == 0 {
            return;
        }
        let _ = self.service.catalog().set_reading_progress(
            self.book_id,
            self.chapter,
            self.fraction,
            self.chapter_count,
        );
        let pct = self.progress_pct();
        let _ = self
            .service
            .catalog()
            .auto_finish_if_complete(self.book_id, pct);
    }

    /// Write the elapsed time of the session that is still open.
    ///
    /// Cheap and idempotent: it is the same row every time, and the session
    /// stays open (`ended_at` NULL) so the startup reaper can still recognise
    /// it if we never get to `close_session`.
    pub(crate) fn checkpoint_session(&self) {
        let Some(session_id) = self.session_id else {
            return;
        };
        let seconds = self.session_start.elapsed().as_secs() as i64;
        let _ = self.service.catalog().checkpoint_reading_session(
            session_id,
            seconds,
            self.progress_pct(),
        );
    }

    pub(crate) fn close_session(&mut self) {
        let Some(session_id) = self.session_id.take() else {
            return;
        };
        let seconds = self.session_start.elapsed().as_secs() as i64;
        let _ =
            self.service
                .catalog()
                .end_reading_session(session_id, seconds, self.progress_pct());
    }
}
