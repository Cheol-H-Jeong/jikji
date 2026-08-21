use std::fs;
use std::path::Path;

use jikji_media_bridge::{
    BridgeAvailability, BridgeRuntime, MediaBridgeRequest, MediaBridgeStatus, MediaKind,
};
use tempfile::tempdir;

#[test]
fn image_metadata_is_extracted_without_python() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("sample.png");
    write_png(&path, 31, 47);

    let runtime = BridgeRuntime::new();
    let outcome = runtime.extract(&MediaBridgeRequest::new(path, MediaKind::Image));

    assert_eq!(runtime.availability(), BridgeAvailability::Native);
    assert_eq!(outcome.status, MediaBridgeStatus::MetadataOnly);
    assert_eq!(
        outcome.metadata.get("engine").map(String::as_str),
        Some("rust-native")
    );
    assert_eq!(
        outcome.metadata.get("width").map(String::as_str),
        Some("31")
    );
    assert_eq!(
        outcome.metadata.get("height").map(String::as_str),
        Some("47")
    );
    assert!(!outcome.python_required_by_default);
}

#[test]
fn wav_metadata_is_extracted_without_asr_runtime() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("sample.wav");
    write_wav(&path, 8_000, 1, 16, 16_000);

    let outcome = BridgeRuntime::new().extract(&MediaBridgeRequest::new(path, MediaKind::Audio));

    assert_eq!(outcome.status, MediaBridgeStatus::MetadataOnly);
    assert_eq!(outcome.text, "");
    assert_eq!(
        outcome.metadata.get("channels").map(String::as_str),
        Some("1")
    );
    assert_eq!(
        outcome.metadata.get("sample_rate_hz").map(String::as_str),
        Some("8000")
    );
    assert_eq!(
        outcome.metadata.get("bits_per_sample").map(String::as_str),
        Some("16")
    );
    assert_eq!(
        outcome.metadata.get("duration_ms").map(String::as_str),
        Some("1000")
    );
}

#[test]
fn missing_media_is_unavailable_without_panic() {
    let outcome = BridgeRuntime::new().extract(&MediaBridgeRequest::new(
        Path::new("/missing/jikji-media.mp4").to_path_buf(),
        MediaKind::Video,
    ));

    assert_eq!(outcome.status, MediaBridgeStatus::Unavailable);
    assert!(outcome.error.contains("No such file") || outcome.error.contains("not found"));
}

#[test]
fn malformed_media_remains_metadata_only() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("broken.jpg");
    fs::write(&path, b"not-a-jpeg").expect("write fixture");

    let outcome = BridgeRuntime::new().extract(&MediaBridgeRequest::new(path, MediaKind::Image));

    assert_eq!(outcome.status, MediaBridgeStatus::MetadataOnly);
    assert_eq!(
        outcome.metadata.get("format").map(String::as_str),
        Some("jpg")
    );
    assert!(!outcome.metadata.contains_key("width"));
}

fn write_png(path: &Path, width: u32, height: u32) {
    let mut bytes = Vec::from(&b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR"[..]);
    bytes.extend(width.to_be_bytes());
    bytes.extend(height.to_be_bytes());
    bytes.extend(b"\x08\x02\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00IEND\xaeB`\x82");
    fs::write(path, bytes).expect("write png");
}

fn write_wav(path: &Path, sample_rate: u32, channels: u16, bits: u16, data_bytes: u32) {
    let byte_rate = sample_rate * u32::from(channels) * u32::from(bits) / 8;
    let block_align = channels * bits / 8;
    let mut bytes = Vec::new();
    bytes.extend(b"RIFF");
    bytes.extend((36 + data_bytes).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16_u32.to_le_bytes());
    bytes.extend(1_u16.to_le_bytes());
    bytes.extend(channels.to_le_bytes());
    bytes.extend(sample_rate.to_le_bytes());
    bytes.extend(byte_rate.to_le_bytes());
    bytes.extend(block_align.to_le_bytes());
    bytes.extend(bits.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend(data_bytes.to_le_bytes());
    bytes.resize(bytes.len() + data_bytes as usize, 0);
    fs::write(path, bytes).expect("write wav");
}
