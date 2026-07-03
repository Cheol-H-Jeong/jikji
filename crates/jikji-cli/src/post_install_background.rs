use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::post_install_commands::{PostInstallRequest, post_install_home};

pub(crate) fn start_background_post_install_prepare(
    request: &PostInstallRequest,
    selected_roots: &[PathBuf],
    selection: Value,
) -> Value {
    let log_dir = post_install_home()
        .join(".local")
        .join("share")
        .join("jikji")
        .join("post_install");
    let log_path = log_dir.join(format!("prepare_{}.json", post_install_stamp()));
    let queued_roots: Vec<Value> = selected_roots
        .iter()
        .map(|root| json!({"root": root, "status": "queued"}))
        .collect();
    let policy = post_install_policy();

    match spawn_prepare_child(request, selected_roots, &log_path) {
        Ok(child) => json!({
            "mode": "background",
            "started": true,
            "pid": child.id(),
            "log": log_path,
            "roots": queued_roots,
            "selection": selection,
            "policy": policy,
        }),
        Err(error) => json!({
            "mode": "background",
            "started": false,
            "pid": null,
            "log": log_path,
            "roots": queued_roots,
            "selection": selection,
            "policy": policy,
            "error": error.to_string(),
        }),
    }
}

fn spawn_prepare_child(
    request: &PostInstallRequest,
    selected_roots: &[PathBuf],
    log_path: &Path,
) -> std::io::Result<std::process::Child> {
    let Some(log_dir) = log_path.parent() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "post-install log path has no parent directory",
        ));
    };
    fs::create_dir_all(log_dir)?;
    let log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    let stderr_log = log.try_clone()?;
    let executable = std::env::current_exe()?;
    let mut command = Command::new(executable);
    command
        .arg("post-install-prepare")
        .args(selected_roots)
        .arg("--parse-timeout")
        .arg(request.parse_timeout.to_string())
        .arg("--json")
        .current_dir(post_install_home())
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr_log));
    if let Some(max_files) = request.max_files {
        command.arg("--max-files").arg(max_files.to_string());
    }
    command.spawn()
}

fn post_install_stamp() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs().to_string(),
        Err(_) => "0".to_owned(),
    }
}

fn post_install_policy() -> Value {
    let cpu_count = match std::thread::available_parallelism() {
        Ok(count) => usize::from(count),
        Err(_) => 1,
    };
    json!({
        "cpu_count": cpu_count,
        "memory_gib": null,
        "max_default_roots": 5,
        "concurrency": 1,
        "note": "post-install prepare runs sequentially in one background process",
    })
}
