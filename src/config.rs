use std::{
    env,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::DEFAULT_PORT;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub data_dir: PathBuf,
    pub port: u16,
}

impl AppConfig {
    pub fn resolve(data_dir: Option<PathBuf>, port: Option<u16>) -> Result<Self> {
        let data_dir = match data_dir {
            Some(path) => path,
            None => default_data_dir()?,
        };
        let port = port.unwrap_or(DEFAULT_PORT);
        if port == 0 {
            bail!("port 0 is not supported because SDK clients need a stable loopback address");
        }
        Ok(Self { data_dir, port })
    }

    pub fn database_path(&self) -> PathBuf {
        self.data_dir.join("printlatch.sqlite3")
    }

    pub fn jobs_dir(&self) -> PathBuf {
        self.data_dir.join("jobs")
    }

    pub fn captures_dir(&self) -> PathBuf {
        self.data_dir.join("captures")
    }

    pub fn ensure_directories(&self) -> Result<()> {
        for path in [&self.data_dir, &self.jobs_dir(), &self.captures_dir()] {
            std::fs::create_dir_all(path)
                .with_context(|| format!("could not create {}", path.display()))?;
        }
        Ok(())
    }
}

pub fn default_data_dir() -> Result<PathBuf> {
    #[cfg(windows)]
    {
        let base = env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .context("LOCALAPPDATA is not set")?;
        return Ok(base.join("PrintLatch"));
    }

    #[cfg(not(windows))]
    {
        if let Some(base) = env::var_os("XDG_DATA_HOME").map(PathBuf::from) {
            return Ok(base.join("printlatch"));
        }
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .context("HOME is not set")?;
        Ok(home.join(".local").join("share").join("printlatch"))
    }
}

pub fn canonicalize_existing(path: &Path) -> Result<PathBuf> {
    path.canonicalize()
        .with_context(|| format!("could not resolve {}", path.display()))
}
