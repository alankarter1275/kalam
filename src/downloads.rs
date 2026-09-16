use std::sync::{Arc, Mutex, OnceLock};
use std::collections::HashMap;
use anyhow::Result;
use crate::sources::SourceManager;
use crate::db::Catalog;
use crate::epub_writer::{generate_epub, WebChapter};
pub static DOWNLOAD_MANAGER: OnceLock<Arc<DownloadManager>> = OnceLock::new();

#[derive(Debug, Clone)]
pub enum JobStatus {
    Pending,
    Downloading { chapter_idx: usize, total: usize },
    Packaging,
    Done,
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
        self.jobs.lock().unwrap().insert(id.clone(), job);
        
        let jobs = self.jobs.clone();
        let source_mgr = self.source_mgr.clone();
        let catalog = self.catalog.clone();
        
        let j_id = id.clone();
        let r_id = remote_id.to_string();
        let s_id = source_id.to_string();
        
        std::thread::spawn(move || {
            if let Err(e) = Self::run_job(j_id.clone(), r_id, s_id, source_mgr, catalog, jobs.clone()) {
                if let Some(j) = jobs.lock().unwrap().get_mut(&j_id) {
                    j.status = JobStatus::Failed(e.to_string());
                }
            }
        });
        
        id
    }
    
    fn run_job(
        id: String,
        remote_id: String,
        source_id: String,
        source_mgr: Arc<SourceManager>,
        catalog: Arc<Catalog>,
        jobs: Arc<Mutex<HashMap<String, DownloadJob>>>,
    ) -> Result<()> {
        let source = source_mgr.get(&source_id).ok_or_else(|| anyhow::anyhow!("Source not found"))?;
        let details = source.get_details(&remote_id)?;
        let remote_chapters = source.get_chapters(&remote_id)?;
        
        let total = remote_chapters.len();
        
        let mut web_chapters = Vec::new();
        
        for (i, rc) in remote_chapters.into_iter().enumerate() {
            if let Some(j) = jobs.lock().unwrap().get_mut(&id) {
                j.status = JobStatus::Downloading { chapter_idx: i + 1, total };
            }
            
            // In a real app we'd add rate limiting and retries here
            if let Ok(crate::sources::traits::ChapterContent::Html(html)) = source.get_chapter_content(&rc.chapter_id) {
                web_chapters.push(WebChapter {
                    title: rc.title.clone(),
                    html_content: html,
                });
            }
        }
        
        if let Some(j) = jobs.lock().unwrap().get_mut(&id) {
            j.status = JobStatus::Packaging;
        }
        
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
        
        if let Some(j) = jobs.lock().unwrap().get_mut(&id) {
            j.status = JobStatus::Done;
        }
        
        Ok(())
    }
    
    pub fn get_jobs(&self) -> Vec<DownloadJob> {
        let mut jobs: Vec<_> = self.jobs.lock().unwrap().values().cloned().collect();
        jobs.sort_by_key(|job| std::cmp::Reverse(job.created_at));
        jobs
    }
}
