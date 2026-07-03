use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use jikji_core::{JikjiError, Result, io_error};
use serde_json::{Value, json};

use crate::file_io::unix_seconds_now;

pub(crate) struct LockGuard {
    path: PathBuf,
}

impl LockGuard {
    pub(crate) fn acquire(index_dir: &Path) -> Result<Self> {
        let path = index_dir.join(".lock");
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                let payload =
                    json!({"pid": std::process::id(), "started_at_unix": unix_seconds_now()});
                file.write_all(payload.to_string().as_bytes())
                    .map_err(|source| io_error(&path, source))?;
                Ok(Self { path })
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if remove_stale_lock(&path)? {
                    return Self::acquire(index_dir);
                }
                Err(JikjiError::Locked(path))
            }
            Err(source) => Err(io_error(path, source)),
        }
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn remove_stale_lock(path: &Path) -> Result<bool> {
    let metadata = fs::metadata(path).map_err(|source| io_error(path, source))?;
    let now = unix_seconds_now();
    let age_from_mtime = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| now.saturating_sub(duration.as_secs()));
    let age_from_payload = fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<BTreeMap<String, Value>>(&text).ok())
        .and_then(|payload| payload.get("started_at_unix").and_then(Value::as_u64))
        .map_or(age_from_mtime, |started| now.saturating_sub(started));
    if age_from_mtime.max(age_from_payload) < 3600 {
        return Ok(false);
    }
    fs::remove_file(path).map_err(|source| io_error(path, source))?;
    Ok(true)
}
