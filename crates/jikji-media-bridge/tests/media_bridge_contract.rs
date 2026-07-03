use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use jikji_media_bridge::{
    BridgeRuntime, MediaBridgeConfig, MediaBridgeRequest, MediaBridgeStatus, MediaKind,
};
use tempfile::tempdir;

fn python3() -> PathBuf {
    std::env::var_os("PYTHON")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("python3"))
}

#[test]
fn media_bridge_reports_metadata_only_when_disabled_by_default() {
    let config = MediaBridgeConfig::default();
    let request = MediaBridgeRequest::new(PathBuf::from("sample.png"), MediaKind::Image);

    let outcome = BridgeRuntime::new(config).extract(&request);

    assert_eq!(outcome.status, MediaBridgeStatus::MetadataOnly);
    assert_eq!(outcome.text, "");
    assert!(!outcome.python_required_by_default);
}

#[test]
fn fake_python_bridge_success_returns_text_and_metadata() {
    let tmp = tempdir().expect("tempdir");
    let script = tmp.path().join("bridge.py");
    fs::write(
        &script,
        "import json\nprint(json.dumps({'text': 'ocr-token-1234', 'metadata': {'engine': 'fake'}}))\n",
    )
    .expect("write script");
    let request = MediaBridgeRequest::new(tmp.path().join("image.png"), MediaKind::Image);
    let config = MediaBridgeConfig::enabled_script(python3(), script, Duration::from_secs(5));

    let outcome = BridgeRuntime::new(config).extract(&request);

    assert_eq!(outcome.status, MediaBridgeStatus::Success);
    assert_eq!(outcome.text, "ocr-token-1234");
    assert_eq!(outcome.metadata.get("engine"), Some(&"fake".to_owned()));
}

#[test]
fn fake_python_bridge_failure_is_reported_without_panic() {
    let tmp = tempdir().expect("tempdir");
    let script = tmp.path().join("bridge_fail.py");
    fs::write(
        &script,
        "import sys\nsys.stderr.write('backend missing')\nsys.exit(7)\n",
    )
    .expect("write script");
    let request = MediaBridgeRequest::new(tmp.path().join("audio.wav"), MediaKind::Audio);
    let config = MediaBridgeConfig::enabled_script(python3(), script, Duration::from_secs(5));

    let outcome = BridgeRuntime::new(config).extract(&request);

    assert_eq!(outcome.status, MediaBridgeStatus::Failed);
    assert!(outcome.error.contains("backend missing"));
}

#[test]
fn missing_python_bridge_is_unavailable_without_panic() {
    let request = MediaBridgeRequest::new(PathBuf::from("clip.mp4"), MediaKind::Video);
    let config = MediaBridgeConfig::enabled_script(
        PathBuf::from("/missing/python"),
        PathBuf::from("bridge.py"),
        Duration::from_secs(1),
    );

    let outcome = BridgeRuntime::new(config).extract(&request);

    assert_eq!(outcome.status, MediaBridgeStatus::Unavailable);
    assert!(outcome.error.contains("/missing/python"));
}

#[test]
fn env_configured_missing_python_reports_unavailable_status() {
    let request = MediaBridgeRequest::new(PathBuf::from("clip.mp4"), MediaKind::Video);
    let config = MediaBridgeConfig::enabled_script(
        PathBuf::from("/missing/jikji-env-python"),
        PathBuf::from("bridge.py"),
        Duration::from_secs(1),
    );

    let outcome = BridgeRuntime::new(config).extract(&request);

    assert_eq!(outcome.status, MediaBridgeStatus::Unavailable);
    assert!(outcome.error.contains("/missing/jikji-env-python"));
}

#[test]
fn bridge_timeout_is_controlled_without_hanging() {
    let tmp = tempdir().expect("tempdir");
    let script = tmp.path().join("bridge_sleep.py");
    let mut file = fs::File::create(&script).expect("create script");
    file.write_all(b"import time\ntime.sleep(10)\n")
        .expect("write");
    let request = MediaBridgeRequest::new(tmp.path().join("video.mp4"), MediaKind::Video);
    let config = MediaBridgeConfig::enabled_script(python3(), script, Duration::from_millis(100));

    let outcome = BridgeRuntime::new(config).extract(&request);

    assert_eq!(outcome.status, MediaBridgeStatus::Timeout);
}
