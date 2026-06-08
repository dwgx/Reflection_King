use std::path::{Path, PathBuf};

use tokio::fs;
use uuid::Uuid;

use crate::Result;

#[derive(Debug, Clone)]
pub struct StoragePaths {
    root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct JobPaths {
    pub temp_dir: PathBuf,
    pub public_dir: PathBuf,
    pub input_path: PathBuf,
    pub output_path: PathBuf,
}

impl StoragePaths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn tmp_dir(&self) -> PathBuf {
        self.root.join("tmp")
    }

    pub fn public_dir(&self) -> PathBuf {
        self.root.join("public")
    }

    pub fn database_path(&self) -> PathBuf {
        self.root.join("reflection.db")
    }

    pub async fn ensure(&self) -> Result<()> {
        fs::create_dir_all(self.tmp_dir()).await?;
        fs::create_dir_all(self.public_dir()).await?;
        Ok(())
    }

    pub async fn prepare_job(&self, job_id: Uuid) -> Result<JobPaths> {
        let temp_dir = self.tmp_dir().join(job_id.to_string());
        let public_dir = self.public_dir().join(job_id.to_string());
        fs::create_dir_all(&temp_dir).await?;
        fs::create_dir_all(&public_dir).await?;

        Ok(JobPaths {
            input_path: temp_dir.join("input.media"),
            output_path: public_dir.join("audio.mp3"),
            temp_dir,
            public_dir,
        })
    }

    pub fn public_job_dir(&self, job_id: Uuid) -> PathBuf {
        self.public_dir().join(job_id.to_string())
    }
}
