#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeAvailability {
    DisabledByDefault,
    Configured,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaBridgeStatus {
    MetadataOnly,
    Success,
    Unavailable,
    Failed,
    Timeout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Image,
    Audio,
    Video,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaBridgeRequest {
    pub path: PathBuf,
    pub kind: MediaKind,
}

impl MediaBridgeRequest {
    pub fn new(path: PathBuf, kind: MediaKind) -> Self {
        Self { path, kind }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaBridgeConfig {
    enabled: bool,
    python: Option<PathBuf>,
    script: Option<PathBuf>,
    timeout: Duration,
}

impl Default for MediaBridgeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            python: None,
            script: None,
            timeout: Duration::from_secs(30),
        }
    }
}

impl MediaBridgeConfig {
    pub fn enabled_script(python: PathBuf, script: PathBuf, timeout: Duration) -> Self {
        Self {
            enabled: true,
            python: Some(python),
            script: Some(script),
            timeout,
        }
    }

    pub fn enabled_from_env(timeout: Duration) -> Self {
        let python = std::env::var_os("JIKJI_MEDIA_BRIDGE_PYTHON").map(PathBuf::from);
        let script = std::env::var_os("JIKJI_MEDIA_BRIDGE_SCRIPT").map(PathBuf::from);
        Self {
            enabled: python.is_some() || script.is_some(),
            python,
            script,
            timeout,
        }
    }

    pub const fn availability(&self) -> BridgeAvailability {
        if self.enabled {
            BridgeAvailability::Configured
        } else {
            BridgeAvailability::DisabledByDefault
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaBridgeOutcome {
    pub status: MediaBridgeStatus,
    pub text: String,
    pub metadata: BTreeMap<String, String>,
    pub error: String,
    pub python_required_by_default: bool,
}

impl MediaBridgeOutcome {
    fn status(status: MediaBridgeStatus, error: String) -> Self {
        Self {
            status,
            text: String::new(),
            metadata: BTreeMap::new(),
            error,
            python_required_by_default: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BridgeRuntime {
    config: MediaBridgeConfig,
}

impl BridgeRuntime {
    pub fn new(config: MediaBridgeConfig) -> Self {
        Self { config }
    }

    pub fn extract(&self, request: &MediaBridgeRequest) -> MediaBridgeOutcome {
        if !self.config.enabled {
            return MediaBridgeOutcome::status(MediaBridgeStatus::MetadataOnly, String::new());
        }
        let Some(python) = self.config.python.as_ref() else {
            return MediaBridgeOutcome::status(
                MediaBridgeStatus::Unavailable,
                "JIKJI_MEDIA_BRIDGE_PYTHON is not configured".to_owned(),
            );
        };
        let Some(script) = self.config.script.as_ref() else {
            return MediaBridgeOutcome::status(
                MediaBridgeStatus::Unavailable,
                "JIKJI_MEDIA_BRIDGE_SCRIPT is not configured".to_owned(),
            );
        };
        run_python_bridge(
            BridgeCommand {
                python,
                script,
                timeout: self.config.timeout,
            },
            request,
        )
    }
}

pub fn media_bridge_status() -> MediaBridgeOutcome {
    MediaBridgeOutcome::status(MediaBridgeStatus::MetadataOnly, String::new())
}

#[derive(Debug, Serialize)]
struct BridgeRequestPayload<'a> {
    path: &'a str,
    kind: MediaKind,
}

#[derive(Debug, Deserialize)]
struct BridgeResponsePayload {
    #[serde(default)]
    text: String,
    #[serde(default)]
    metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy)]
struct BridgeCommand<'a> {
    python: &'a std::path::Path,
    script: &'a std::path::Path,
    timeout: Duration,
}

fn run_python_bridge(
    command: BridgeCommand<'_>,
    request: &MediaBridgeRequest,
) -> MediaBridgeOutcome {
    let path = request.path.to_string_lossy();
    let payload = BridgeRequestPayload {
        path: path.as_ref(),
        kind: request.kind,
    };
    let input = match serde_json::to_string(&payload) {
        Ok(input) => input,
        Err(error) => {
            return MediaBridgeOutcome::status(MediaBridgeStatus::Failed, error.to_string());
        }
    };
    if command.python.is_absolute() && !command.python.is_file() {
        return MediaBridgeOutcome::status(
            MediaBridgeStatus::Unavailable,
            format!("{}: not found", command.python.display()),
        );
    }
    let spawn = Command::new(command.python)
        .arg(command.script)
        .arg(input)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let child = match spawn {
        Ok(child) => child,
        Err(error) => return spawn_error(command.python, error),
    };
    wait_with_timeout(child, command.timeout)
}

fn wait_with_timeout(mut child: std::process::Child, timeout: Duration) -> MediaBridgeOutcome {
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => return collect_output(child),
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return MediaBridgeOutcome::status(
                        MediaBridgeStatus::Timeout,
                        format!("media bridge timed out after {} ms", timeout.as_millis()),
                    );
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => {
                return MediaBridgeOutcome::status(MediaBridgeStatus::Failed, error.to_string());
            }
        }
    }
}

fn collect_output(child: std::process::Child) -> MediaBridgeOutcome {
    let output = match child.wait_with_output() {
        Ok(output) => output,
        Err(error) => {
            return MediaBridgeOutcome::status(MediaBridgeStatus::Failed, error.to_string());
        }
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(output.stderr.as_slice())
            .trim()
            .to_owned();
        let detail = if stderr.is_empty() {
            format!("media bridge exited with {}", output.status)
        } else {
            stderr
        };
        return MediaBridgeOutcome::status(MediaBridgeStatus::Failed, detail);
    }
    let stdout = String::from_utf8_lossy(output.stdout.as_slice());
    match serde_json::from_str::<BridgeResponsePayload>(stdout.trim()) {
        Ok(payload) => MediaBridgeOutcome {
            status: MediaBridgeStatus::Success,
            text: payload.text,
            metadata: payload.metadata,
            error: String::new(),
            python_required_by_default: false,
        },
        Err(error) => MediaBridgeOutcome::status(MediaBridgeStatus::Failed, error.to_string()),
    }
}

fn spawn_error(python: &std::path::Path, error: io::Error) -> MediaBridgeOutcome {
    let status = if matches!(
        error.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::PermissionDenied
    ) {
        MediaBridgeStatus::Unavailable
    } else {
        MediaBridgeStatus::Failed
    };
    MediaBridgeOutcome::status(status, format!("{}: {error}", python.display()))
}
