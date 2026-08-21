#![forbid(unsafe_code)]
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

pub mod beir;
pub mod hippocamp;

pub const DEFAULT_MAX_DOWNLOAD_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum DatasetError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("invalid dataset input: {0}")]
    Invalid(String),
    #[error("path escapes destination: {0}")]
    UnsafePath(String),
    #[error("byte limit exceeded ({limit} bytes): {resource}")]
    ByteLimit { resource: String, limit: u64 },
}

pub type Result<T, E = DatasetError> = std::result::Result<T, E>;

pub trait ResourceFetcher {
    fn fetch_to(&self, resource: &str, destination: &Path, max_bytes: u64) -> Result<u64>;
}

#[derive(Debug, Clone, Default)]
pub struct HttpFetcher {
    pub user_agent: Option<String>,
}

impl ResourceFetcher for HttpFetcher {
    fn fetch_to(&self, resource: &str, destination: &Path, max_bytes: u64) -> Result<u64> {
        let mut request = ureq::get(resource).timeout(Duration::from_secs(30));
        if let Some(user_agent) = &self.user_agent {
            request = request.set("User-Agent", user_agent);
        }
        let response = request
            .call()
            .map_err(|error| DatasetError::Http(error.to_string()))?;
        if let Some(length) = response
            .header("Content-Length")
            .and_then(|value| value.parse::<u64>().ok())
        {
            if length > max_bytes {
                return Err(DatasetError::ByteLimit {
                    resource: resource.into(),
                    limit: max_bytes,
                });
            }
        }
        write_bounded(response.into_reader(), destination, resource, max_bytes)
    }
}

#[derive(Debug, Clone)]
pub struct DirectoryFetcher {
    root: PathBuf,
}

impl DirectoryFetcher {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl ResourceFetcher for DirectoryFetcher {
    fn fetch_to(&self, resource: &str, destination: &Path, max_bytes: u64) -> Result<u64> {
        let source = safe_join(&self.root, Path::new(resource))?;
        let canonical_root = self.root.canonicalize()?;
        let canonical_source = source.canonicalize()?;
        if !canonical_source.starts_with(&canonical_root) {
            return Err(DatasetError::UnsafePath(resource.to_owned()));
        }
        let file = File::open(&canonical_source)?;
        write_bounded(file, destination, resource, max_bytes)
    }
}

pub(crate) fn safe_join(root: &Path, relative: &Path) -> Result<PathBuf> {
    if relative.as_os_str().is_empty() || relative.is_absolute() {
        return Err(DatasetError::UnsafePath(relative.display().to_string()));
    }
    for component in relative.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(DatasetError::UnsafePath(relative.display().to_string()));
        }
    }
    Ok(root.join(relative))
}

pub(crate) fn write_bounded(
    mut reader: impl Read,
    destination: &Path,
    resource: &str,
    max_bytes: u64,
) -> Result<u64> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary =
        destination.with_extension(format!("jikji-download-part-{}", std::process::id()));
    let result = (|| {
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        let mut total = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            total = total.saturating_add(read as u64);
            if total > max_bytes {
                return Err(DatasetError::ByteLimit {
                    resource: resource.into(),
                    limit: max_bytes,
                });
            }
            output.write_all(&buffer[..read])?;
        }
        output.sync_all()?;
        fs::rename(&temporary, destination)?;
        Ok(total)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
