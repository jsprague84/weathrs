use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::jobs::ForecastJob;

/// File-based storage for scheduler jobs
pub struct JobStorage {
    jobs: Arc<RwLock<HashMap<String, ForecastJob>>>,
    file_path: String,
}

impl JobStorage {
    /// Create a new job storage with the given file path
    pub fn new(file_path: impl Into<String>) -> Self {
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
            file_path: file_path.into(),
        }
    }

    /// Load jobs from file
    pub async fn load(&self) -> Result<(), std::io::Error> {
        let path = Path::new(&self.file_path);

        if !path.exists() {
            tracing::debug!("Job storage file does not exist, starting fresh");
            return Ok(());
        }

        let content = tokio::fs::read_to_string(path).await?;
        let jobs: HashMap<String, ForecastJob> = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut storage = self.jobs.write().await;
        *storage = jobs;

        tracing::info!(count = storage.len(), "Loaded jobs from storage");

        Ok(())
    }

    /// Save jobs to file
    pub async fn save(&self) -> Result<(), std::io::Error> {
        let jobs = self.jobs.read().await;
        let content = serde_json::to_string_pretty(&*jobs)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Create parent directory if needed
        if let Some(parent) = Path::new(&self.file_path).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        tokio::fs::write(&self.file_path, content).await?;

        tracing::debug!(count = jobs.len(), "Saved jobs to storage");

        Ok(())
    }

    /// Add or update a job
    pub async fn upsert(&self, job: ForecastJob) -> Result<(), std::io::Error> {
        {
            let mut jobs = self.jobs.write().await;
            jobs.insert(job.id.clone(), job);
        }
        self.save().await
    }

    /// Get a job by ID
    pub async fn get(&self, id: &str) -> Option<ForecastJob> {
        let jobs = self.jobs.read().await;
        jobs.get(id).cloned()
    }

    /// Remove a job by ID
    pub async fn remove(&self, id: &str) -> Result<bool, std::io::Error> {
        let existed = {
            let mut jobs = self.jobs.write().await;
            jobs.remove(id).is_some()
        };

        if existed {
            self.save().await?;
        }

        Ok(existed)
    }

    /// Get all jobs
    pub async fn get_all(&self) -> Vec<ForecastJob> {
        let jobs = self.jobs.read().await;
        jobs.values().cloned().collect()
    }

    /// Get all enabled jobs
    pub async fn get_enabled(&self) -> Vec<ForecastJob> {
        let jobs = self.jobs.read().await;
        jobs.values().filter(|j| j.enabled).cloned().collect()
    }

    /// Get job count
    pub async fn count(&self) -> usize {
        let jobs = self.jobs.read().await;
        jobs.len()
    }

    /// Check if a job exists
    pub async fn exists(&self, id: &str) -> bool {
        let jobs = self.jobs.read().await;
        jobs.contains_key(id)
    }
}
