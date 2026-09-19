use std::sync::{Arc, Mutex, OnceLock};
use std::collections::HashMap;
use anyhow::Result;
use crate::sources::SourceManager;
use crate::db::Catalog;
use crate::epub_writer::{generate_epub, WebChapter};
use crate::tasks::Reporter;
pub static DOWNLOAD_MANAGER: OnceLock<Arc<DownloadManager>> = OnceLock::new();

#[derive(Debug, Clone)]
pub enum JobStatus {
    Pending,
    Downloading { chapter_idx: usize, total: usize },
    Packaging,
    Done,
    /// The task's cancellation flag was set and the worker stopped between
    /// chapters. Distinct from [`JobStatus::Failed`] on purpose: nothing was
    /// imported and nothing is half-written, so it is a clean stop the user
    /// asked for rather than something that went wrong.
    Cancelled,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct DownloadJob {
    #[allow(dead_code)]
    pub id: String,
    pub title: String,
    #[allow(dead_code)]
    pub source_id: String,
    #[allow(dead_code)]
    pub remote_id: String,
    pub status: JobStatus,
    pub created_at: std::time::Instant,
}

pub struct DownloadManager {
    jobs: Arc<Mutex<HashMap<String, DownloadJob>>>,
    source_mgr: Arc<SourceManager>,
    catalog: Arc<Catalog>,
}

/// The job map, tolerant of a panic in another thread.
///
/// `Mutex::lock` returns `Err` once a holder has panicked, and the six
/// `.unwrap()` calls this replaces would then fail on *every* later access —
/// including on the UI thread, which reads this map to draw the download
/// list. One crashed download would take the page down with it, which is
/// exactly the cascade `crate::tasks` exists to prevent. A poisoned map is
/// still perfectly readable, so take the data and carry on.
fn locked<R>(
    jobs: &Mutex<HashMap<String, DownloadJob>>,
    f: impl FnOnce(&mut HashMap<String, DownloadJob>) -> R,
) -> R {
    let mut guard = match jobs.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    f(&mut guard)
}

impl DownloadManager {
    pub fn new(source_mgr: Arc<SourceManager>, catalog: Arc<Catalog>) -> Self {
        Self {
            jobs: Arc::new(Mutex::new(HashMap::new())),
            source_mgr,
            catalog,
        }
    }

    pub fn queue_download(&self, title: &str, source_id: &str, remote_id: &str) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let job = DownloadJob {
            id: id.clone(),
            title: title.to_string(),
            source_id: source_id.to_string(),
            remote_id: remote_id.to_string(),
            status: JobStatus::Pending,
            created_at: std::time::Instant::now(),
        };
        locked(&self.jobs, |map| {
            map.insert(id.clone(), job);
        });

        let jobs = self.jobs.clone();
        let source_mgr = self.source_mgr.clone();
        let catalog = self.catalog.clone();

        let j_id = id.clone();
        let r_id = remote_id.to_string();
        let s_id = source_id.to_string();

        // Roadmap 1.3: this was a bare `std::thread::spawn` with six
        // `lock().unwrap()` calls. `tasks::spawn` adds the three things a bare
        // spawn cannot give: a cancellation flag the worker can check, a place
        // in the running-task registry so `cancel_all()` at exit reaches it,
        // and a progress channel. A download is the one operation here that
        // can run for minutes, so it is the one that most needs to be
        // stoppable.
        crate::tasks::spawn(
            format!("Downloading {title}"),
            move |reporter| {
                // Take the error apart before taking the lock, so a formatting
                // panic cannot poison the map on the way to reporting one.
                let failed = Self::run_job(
                    j_id.clone(),
                    r_id,
                    s_id,
                    source_mgr,
                    catalog,
                    jobs.clone(),
                    &reporter,
                )
                .err()
                .map(|e| e.to_string());
                if let Some(message) = failed {
                    locked(&jobs, |map| {
                        if let Some(j) = map.get_mut(&j_id) {
                            j.status = JobStatus::Failed(message);
                        }
                    });
                }
            },
            // Progress already lives in the job map, which the page polls.
            // Duplicating it down this channel would give the same fact two
            // sources of truth; the channel stays available to 1.14.
            |_update| {},
            |_| {},
        );

        id
    }

    #[allow(clippy::too_many_arguments)]
    fn run_job(
        id: String,
        remote_id: String,
        source_id: String,
        source_mgr: Arc<SourceManager>,
        catalog: Arc<Catalog>,
        jobs: Arc<Mutex<HashMap<String, DownloadJob>>>,
        reporter: &Reporter,
    ) -> Result<()> {
        let source = source_mgr.get(&source_id).ok_or_else(|| anyhow::anyhow!("Source not found"))?;
        let details = source.get_details(&remote_id)?;
        let remote_chapters = source.get_chapters(&remote_id)?;

        let total = remote_chapters.len();

        let mut web_chapters = Vec::new();

        for (i, rc) in remote_chapters.into_iter().enumerate() {
            // Cancellation is cooperative — nothing can safely kill a thread
            // mid-write — so it is checked between chapters. A cancelled
            // download therefore never leaves a half-built EPUB behind, which
            // is the reason to check here rather than anywhere later.
            if reporter.cancelled() {
                locked(&jobs, |map| {
                    if let Some(j) = map.get_mut(&id) {
                        j.status = JobStatus::Cancelled;
                    }
                });
                return Ok(());
            }
            locked(&jobs, |map| {
                if let Some(j) = map.get_mut(&id) {
                    j.status = JobStatus::Downloading { chapter_idx: i + 1, total };
                }
            });

            // In a real app we'd add rate limiting and retries here
            if let Ok(crate::sources::traits::ChapterContent::Html(html)) = source.get_chapter_content(&rc.chapter_id) {
                web_chapters.push(WebChapter {
                    title: rc.title.clone(),
                    html_content: html,
                });
            }
        }

        locked(&jobs, |map| {
            if let Some(j) = map.get_mut(&id) {
                j.status = JobStatus::Packaging;
            }
        });

        let cover_bytes = if let Some(url) = &details.cover_url {
            source.fetch_image(url).ok()
        } else {
            None
        };

        let safe_title = details.title.replace("/", "_").replace("\\", "_");
        let file_name = format!("{} - {}.epub", safe_title, details.author);
        let out_dir = std::env::temp_dir().join("kalam_downloads");
        std::fs::create_dir_all(&out_dir)?;
        let out_path = out_dir.join(&file_name);

        // Generate EPUB
        generate_epub(&out_path, &details.title, &details.author, cover_bytes.as_deref(), web_chapters)?;

        // Import into Kalam Library
        let _import_res = crate::epub::import_epub(&catalog, &out_path)?;
        let _ = std::fs::remove_file(&out_path);

        locked(&jobs, |map| {
            if let Some(j) = map.get_mut(&id) {
                j.status = JobStatus::Done;
            }
        });

        Ok(())
    }

    pub fn get_jobs(&self) -> Vec<DownloadJob> {
        let mut jobs: Vec<_> = locked(&self.jobs, |map| map.values().cloned().collect());
        jobs.sort_by_key(|job| std::cmp::Reverse(job.created_at));
        jobs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(id: &str) -> DownloadJob {
        DownloadJob {
            id: id.to_string(),
            title: format!("book {id}"),
            source_id: "s".to_string(),
            remote_id: "r".to_string(),
            status: JobStatus::Pending,
            created_at: std::time::Instant::now(),
        }
    }

    /// The exact failure `locked` exists to prevent. A worker that panics
    /// while holding the lock poisons the mutex, and the `.unwrap()` this
    /// replaced would then fail on every later read — including on the UI
    /// thread drawing the download list, turning one crashed download into an
    /// unusable page. The data is intact; only the lock's flag is set.
    #[test]
    fn a_panicking_worker_does_not_make_the_job_list_unreadable() {
        let jobs: Mutex<HashMap<String, DownloadJob>> = Mutex::new(HashMap::new());
        jobs.lock().unwrap().insert("a".into(), job("a"));

        // Poison it the way a crashed download thread would.
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = jobs.lock().unwrap();
            panic!("a download thread died holding the lock");
        }));
        assert!(panicked.is_err(), "the closure should have panicked");
        assert!(jobs.is_poisoned(), "and the mutex should now be poisoned");

        // Under `.unwrap()` this line panicked. It must not now.
        let ids = locked(&jobs, |map| map.keys().cloned().collect::<Vec<_>>());
        assert_eq!(ids, vec!["a".to_string()], "the entry survived the panic");
    }

    /// `locked` must not be a workaround that only copes with the broken
    /// case — it is the normal path too, so it has to behave identically
    /// when nothing has panicked.
    #[test]
    fn locked_writes_are_visible_to_later_reads_when_nothing_panicked() {
        let jobs: Mutex<HashMap<String, DownloadJob>> = Mutex::new(HashMap::new());
        locked(&jobs, |map| {
            map.insert("a".into(), job("a"));
        });
        locked(&jobs, |map| {
            map.get_mut("a").unwrap().status = JobStatus::Cancelled;
        });
        let status = locked(&jobs, |map| map["a"].status.clone());
        assert!(
            matches!(status, JobStatus::Cancelled),
            "got {status:?}"
        );
        assert!(!jobs.is_poisoned(), "an ordinary write must not poison");
    }
}
