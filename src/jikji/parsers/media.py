"""Optional local media parsers (OCR/transcription/metadata).

No heavyweight dependency is required.  If local tools or optional Python
packages are installed, Jikji uses them with bounded output; otherwise it falls
back to lightweight metadata.  Preferred CPU backends (auto-detected, optional):

* Images: `RapidOCR <https://github.com/RapidAI/RapidOCR>`_ (ONNXRuntime, CPU,
  offline) when importable, else the ``tesseract`` binary.
* Audio/video speech: `faster-whisper
  <https://github.com/SYSTRAN/faster-whisper>`_ (CTranslate2, CPU INT8) when
  importable, else the ``whisper`` CLI.  Transcription stays opt-in via
  ``JIKJI_ENABLE_TRANSCRIPTION`` because it is expensive.
* Video frames: optional on-screen-text OCR via ffmpeg keyframes + RapidOCR,
  gated by ``JIKJI_ENABLE_VIDEO_OCR``.

Images always expose lightweight visual metadata (format/dimensions and
selected datetime EXIF when available) regardless of OCR availability.
"""
from __future__ import annotations

import importlib.util
import json
import logging
import os
import shutil
import struct
import subprocess
import tempfile
import threading
from pathlib import Path

log = logging.getLogger(__name__)

_IMAGE_EXTS = {".png", ".jpg", ".jpeg", ".tif", ".tiff", ".webp", ".bmp", ".gif"}
_AUDIO_EXTS = {".mp3", ".wav", ".m4a", ".flac", ".ogg", ".aac", ".opus", ".wma"}
_VIDEO_EXTS = {".mp4", ".mov", ".mkv", ".avi", ".webm", ".m4v", ".wmv", ".flv", ".mpg", ".mpeg"}
_EXIF_DATETIME_TAGS = ("DateTimeOriginal", "DateTimeDigitized", "DateTime")


def _env_flag(name: str, default: bool = False) -> bool:
    raw = os.environ.get(name)
    if raw is None:
        return default
    return raw.strip().lower() in {"1", "true", "yes", "on"}


def _env_float(name: str, default: float) -> float:
    raw = os.environ.get(name)
    if not raw:
        return default
    try:
        return max(0.1, float(raw))
    except ValueError:
        return default


def _run(cmd: list[str], *, timeout: float) -> str:
    try:
        proc = subprocess.run(  # noqa: S603 - command path is resolved by shutil.which/caller.
            cmd,
            check=False,
            capture_output=True,
            text=True,
            errors="ignore",
            timeout=timeout,
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        log.debug("media command failed %s: %s", cmd[:2], exc)
        return ""
    if proc.returncode != 0:
        log.debug("media command non-zero %s: %s", cmd[:2], proc.stderr[:200])
        return ""
    return proc.stdout.strip()


# --- Optional CPU model backends (lazy, process-wide singletons) ------------
#
# RapidOCR (ONNXRuntime) and faster-whisper (CTranslate2) are heavy to import
# and to construct, so we build each at most once per process behind a lock and
# remember a hard failure so we never retry a broken backend per file.

_RAPIDOCR_ENGINE = None
_RAPIDOCR_LOCK = threading.Lock()
_RAPIDOCR_FAILED = False

_WHISPER_MODEL = None
_WHISPER_LOCK = threading.Lock()
_WHISPER_FAILED = False


def _module_available(*names: str) -> bool:
    for name in names:
        try:
            if importlib.util.find_spec(name) is not None:
                return True
        except (ImportError, ValueError):
            continue
    return False


def _rapidocr_available() -> bool:
    return _module_available("rapidocr", "rapidocr_onnxruntime")


def image_ocr_available() -> bool:
    """Return whether local image/PDF OCR can run (RapidOCR or Tesseract)."""
    return _rapidocr_available() or shutil.which("tesseract") is not None


def _rapidocr_engine():
    global _RAPIDOCR_ENGINE, _RAPIDOCR_FAILED
    if _RAPIDOCR_ENGINE is not None or _RAPIDOCR_FAILED:
        return _RAPIDOCR_ENGINE
    with _RAPIDOCR_LOCK:
        if _RAPIDOCR_ENGINE is not None or _RAPIDOCR_FAILED:
            return _RAPIDOCR_ENGINE
        try:
            try:
                from rapidocr import RapidOCR  # rapidocr>=2
            except ImportError:
                from rapidocr_onnxruntime import RapidOCR  # rapidocr-onnxruntime>=1
            _RAPIDOCR_ENGINE = RapidOCR()
        except Exception as exc:  # pragma: no cover - import/runtime specific
            log.debug("rapidocr init failed: %s", exc)
            _RAPIDOCR_FAILED = True
            _RAPIDOCR_ENGINE = None
    return _RAPIDOCR_ENGINE


def _rapidocr_texts(result) -> list[str]:
    """Pull recognized strings out of either RapidOCR result shape."""
    # rapidocr>=2 returns an object exposing ``.txts``.
    txts = getattr(result, "txts", None)
    if txts:
        return [str(t).strip() for t in txts if str(t).strip()]
    # rapidocr-onnxruntime returns ``(detections, elapse)`` where each
    # detection is ``[box, text, score]``; ``None`` when nothing is found.
    payload = result[0] if isinstance(result, tuple) and result else result
    out: list[str] = []
    if isinstance(payload, list):
        for item in payload:
            if isinstance(item, (list, tuple)) and len(item) >= 2 and str(item[1]).strip():
                out.append(str(item[1]).strip())
    return out


def _ocr_image_rapidocr(path: Path, max_chars: int) -> str:
    engine = _rapidocr_engine()
    if engine is None:
        return ""
    try:
        result = engine(str(path.resolve()))
    except Exception as exc:  # pragma: no cover - backend/runtime specific
        log.debug("rapidocr run failed %s: %s", path, exc)
        return ""
    return "\n".join(_rapidocr_texts(result))[:max_chars]


def _format_from_suffix(path: Path) -> str:
    ext = path.suffix.lower().lstrip(".")
    if ext in {"jpg", "jpeg"}:
        return "JPEG"
    if ext:
        return ext.upper()
    return "image"


def _jpeg_dimensions(data: bytes) -> tuple[int, int] | None:
    if not data.startswith(b"\xff\xd8"):
        return None
    sof_markers = {
        0xC0,
        0xC1,
        0xC2,
        0xC3,
        0xC5,
        0xC6,
        0xC7,
        0xC9,
        0xCA,
        0xCB,
        0xCD,
        0xCE,
        0xCF,
    }
    idx = 2
    while idx + 9 < len(data):
        if data[idx] != 0xFF:
            idx += 1
            continue
        while idx < len(data) and data[idx] == 0xFF:
            idx += 1
        if idx >= len(data):
            break
        marker = data[idx]
        idx += 1
        if marker in {0x01, *range(0xD0, 0xD9)}:
            continue
        if idx + 2 > len(data):
            break
        block_len = int.from_bytes(data[idx:idx + 2], "big")
        if block_len < 2 or idx + block_len > len(data):
            break
        if marker in sof_markers and block_len >= 7:
            height = int.from_bytes(data[idx + 3:idx + 5], "big")
            width = int.from_bytes(data[idx + 5:idx + 7], "big")
            if width > 0 and height > 0:
                return width, height
        idx += block_len
    return None


def _webp_dimensions(data: bytes) -> tuple[int, int] | None:
    if len(data) < 30 or not (data.startswith(b"RIFF") and data[8:12] == b"WEBP"):
        return None
    chunk = data[12:16]
    if chunk == b"VP8X" and len(data) >= 30:
        width = int.from_bytes(data[24:27], "little") + 1
        height = int.from_bytes(data[27:30], "little") + 1
        return width, height
    if chunk == b"VP8L" and len(data) >= 25 and data[20] == 0x2F:
        b0, b1, b2, b3 = data[21:25]
        width = 1 + (((b1 & 0x3F) << 8) | b0)
        height = 1 + (((b3 & 0x0F) << 10) | (b2 << 2) | ((b1 & 0xC0) >> 6))
        return width, height
    if chunk == b"VP8 " and len(data) >= 30 and data[23:26] == b"\x9d\x01\x2a":
        width = int.from_bytes(data[26:28], "little") & 0x3FFF
        height = int.from_bytes(data[28:30], "little") & 0x3FFF
        if width > 0 and height > 0:
            return width, height
    return None


def _header_image_metadata(path: Path) -> dict[str, object]:
    """Return format/dimensions from common image headers without dependencies."""
    meta: dict[str, object] = {"format": _format_from_suffix(path)}
    try:
        with path.open("rb") as fh:
            data = fh.read(65536)
    except OSError as exc:
        log.debug("image header read failed %s: %s", path, exc)
        return meta
    if len(data) >= 24 and data.startswith(b"\x89PNG\r\n\x1a\n") and data[12:16] == b"IHDR":
        meta["format"] = "PNG"
        meta["width"] = int.from_bytes(data[16:20], "big")
        meta["height"] = int.from_bytes(data[20:24], "big")
    elif len(data) >= 10 and data[:6] in {b"GIF87a", b"GIF89a"}:
        meta["format"] = "GIF"
        meta["width"] = int.from_bytes(data[6:8], "little")
        meta["height"] = int.from_bytes(data[8:10], "little")
    elif len(data) >= 26 and data.startswith(b"BM"):
        meta["format"] = "BMP"
        try:
            meta["width"] = struct.unpack_from("<i", data, 18)[0]
            meta["height"] = abs(struct.unpack_from("<i", data, 22)[0])
        except struct.error:
            pass
    elif dims := _jpeg_dimensions(data):
        meta["format"] = "JPEG"
        meta["width"], meta["height"] = dims
    elif dims := _webp_dimensions(data):
        meta["format"] = "WEBP"
        meta["width"], meta["height"] = dims
    return meta


def _image_metadata(path: Path) -> list[str]:
    fallback = _header_image_metadata(path)
    parts: list[str] = [f"# Image: {path.name}"]
    try:
        from PIL import ExifTags, Image  # type: ignore
    except ImportError:
        image_format = str(fallback.get("format") or _format_from_suffix(path))
        parts.append(f"Format: {image_format}")
        width = fallback.get("width")
        height = fallback.get("height")
        if isinstance(width, int) and isinstance(height, int) and width > 0 and height > 0:
            parts.append(f"Dimensions: {width}x{height} pixels")
        return parts
    try:
        with Image.open(path) as image:
            parts.append(f"Format: {image.format or fallback.get('format') or _format_from_suffix(path)}")
            parts.append(f"Dimensions: {image.width}x{image.height} pixels")
            parts.append(f"Color mode: {image.mode}")
            frames = int(getattr(image, "n_frames", 1) or 1)
            if frames > 1:
                parts.append(f"Frames: {frames}")
            exif = image.getexif()
            if exif:
                tags = getattr(ExifTags, "TAGS", {})
                wanted = {label: key for key, label in tags.items() if label in _EXIF_DATETIME_TAGS}
                for label in _EXIF_DATETIME_TAGS:
                    value = exif.get(wanted.get(label))
                    if value:
                        parts.append(f"EXIF {label}: {str(value).strip()[:80]}")
                        break
    except Exception as exc:
        log.debug("image metadata failed %s: %s", path, exc)
        image_format = str(fallback.get("format") or _format_from_suffix(path))
        parts.append(f"Format: {image_format}")
        width = fallback.get("width")
        height = fallback.get("height")
        if isinstance(width, int) and isinstance(height, int) and width > 0 and height > 0:
            parts.append(f"Dimensions: {width}x{height} pixels")
    return parts


def _ocr_image_tesseract(path: Path, max_chars: int) -> str:
    tesseract = shutil.which("tesseract")
    if not tesseract:
        return ""
    timeout = _env_float("JIKJI_OCR_TIMEOUT", 15.0)
    lang = os.environ.get("JIKJI_TESSERACT_LANG", "").strip()
    cmd = [tesseract, str(path.resolve()), "stdout", "--psm", os.environ.get("JIKJI_TESSERACT_PSM", "6")]
    if lang:
        cmd.extend(["-l", lang])
    return _run(cmd, timeout=timeout)[:max_chars]


def _ocr_image(path: Path, max_chars: int) -> str:
    # Prefer RapidOCR (CPU, offline, multilingual) when installed; otherwise
    # fall back to a local Tesseract binary.
    if _rapidocr_available():
        text = _ocr_image_rapidocr(path, max_chars)
        if text:
            return text
    return _ocr_image_tesseract(path, max_chars)


def parse_image(path: Path, max_chars: int) -> str:
    parts = _image_metadata(path)
    ocr = _ocr_image(path, max_chars)
    if ocr:
        parts.append("# OCR text\n" + ocr)
    return "\n".join(parts)[:max_chars]


def _ffprobe_metadata(path: Path) -> list[str]:
    ffprobe = shutil.which("ffprobe")
    if not ffprobe:
        return []
    raw = _run(
        [
            ffprobe,
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            str(path.resolve()),
        ],
        timeout=_env_float("JIKJI_FFPROBE_TIMEOUT", 8.0),
    )
    if not raw:
        return []
    try:
        data = json.loads(raw)
    except json.JSONDecodeError:
        return []
    parts: list[str] = []
    fmt = data.get("format") if isinstance(data, dict) else {}
    if isinstance(fmt, dict):
        if fmt.get("format_long_name"):
            parts.append(f"Format: {fmt['format_long_name']}")
        if fmt.get("duration"):
            parts.append(f"Duration seconds: {fmt['duration']}")
        tags = fmt.get("tags") or {}
        if isinstance(tags, dict):
            for key in ("title", "artist", "album", "album_artist", "genre", "date", "comment"):
                value = tags.get(key) or tags.get(key.upper())
                if value:
                    parts.append(f"{key.title()}: {str(value)[:300]}")
    streams = data.get("streams") if isinstance(data, dict) else []
    if isinstance(streams, list):
        for stream in streams[:4]:
            if isinstance(stream, dict) and stream.get("codec_type"):
                parts.append(
                    "Stream: "
                    + " ".join(
                        str(x)
                        for x in (stream.get("codec_type"), stream.get("codec_name"), stream.get("language"))
                        if x
                    )
                )
    return parts


def _faster_whisper_available() -> bool:
    return _module_available("faster_whisper")


def _faster_whisper_model():
    global _WHISPER_MODEL, _WHISPER_FAILED
    if _WHISPER_MODEL is not None or _WHISPER_FAILED:
        return _WHISPER_MODEL
    with _WHISPER_LOCK:
        if _WHISPER_MODEL is not None or _WHISPER_FAILED:
            return _WHISPER_MODEL
        try:
            from faster_whisper import WhisperModel

            name = os.environ.get("JIKJI_WHISPER_MODEL", "tiny")
            compute = os.environ.get("JIKJI_WHISPER_COMPUTE", "int8")
            _WHISPER_MODEL = WhisperModel(name, device="cpu", compute_type=compute)
        except Exception as exc:  # pragma: no cover - import/runtime specific
            log.debug("faster-whisper init failed: %s", exc)
            _WHISPER_FAILED = True
            _WHISPER_MODEL = None
    return _WHISPER_MODEL


def _transcribe_faster_whisper(path: Path, max_chars: int) -> str:
    model = _faster_whisper_model()
    if model is None:
        return ""
    language = os.environ.get("JIKJI_WHISPER_LANGUAGE", "").strip() or None
    try:
        segments, _info = model.transcribe(
            str(path.resolve()), language=language, vad_filter=True
        )
    except Exception as exc:  # pragma: no cover - backend/runtime specific
        log.debug("faster-whisper run failed %s: %s", path, exc)
        return ""
    parts: list[str] = []
    total = 0
    try:
        for seg in segments:
            text = (getattr(seg, "text", "") or "").strip()
            if not text:
                continue
            parts.append(text)
            total += len(text) + 1
            if total >= max_chars:
                break
    except Exception as exc:  # pragma: no cover - lazy decode can raise
        log.debug("faster-whisper decode failed %s: %s", path, exc)
        if not parts:
            return ""
    return " ".join(parts)[:max_chars]


def _transcribe_whisper_cli(path: Path, max_chars: int) -> str:
    whisper = shutil.which("whisper")
    if not whisper:
        return ""
    model = os.environ.get("JIKJI_WHISPER_MODEL", "tiny")
    timeout = _env_float("JIKJI_TRANSCRIBE_TIMEOUT", 120.0)
    with tempfile.TemporaryDirectory(prefix="jikji-whisper-") as tmp:
        cmd = [
            whisper,
            str(path.resolve()),
            "--model",
            model,
            "--output_format",
            "txt",
            "--output_dir",
            tmp,
            "--fp16",
            "False",
        ]
        language = os.environ.get("JIKJI_WHISPER_LANGUAGE", "").strip()
        if language:
            cmd.extend(["--language", language])
        try:
            proc = subprocess.run(  # noqa: S603 - executable resolved by shutil.which.
                cmd,
                check=False,
                capture_output=True,
                text=True,
                errors="ignore",
                timeout=timeout,
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            log.debug("whisper failed %s: %s", path, exc)
            return ""
        if proc.returncode != 0:
            log.debug("whisper non-zero %s: %s", path, proc.stderr[:200])
            return ""
        for txt in Path(tmp).glob("*.txt"):
            try:
                return txt.read_text(encoding="utf-8", errors="ignore")[:max_chars]
            except OSError:
                continue
    return ""


def _transcribe_media(path: Path, max_chars: int) -> str:
    """Speech-to-text via faster-whisper (preferred) or the whisper CLI.

    The caller is responsible for the ``JIKJI_ENABLE_TRANSCRIPTION`` gate.
    """
    if _faster_whisper_available():
        text = _transcribe_faster_whisper(path, max_chars)
        if text:
            return text
    return _transcribe_whisper_cli(path, max_chars)


def _transcribe_audio(path: Path, max_chars: int) -> str:
    if not _env_flag("JIKJI_ENABLE_TRANSCRIPTION", default=False):
        return ""
    max_mb = _env_float("JIKJI_TRANSCRIBE_MAX_MB", 25.0)
    try:
        if path.stat().st_size > max_mb * 1024 * 1024:
            return ""
    except OSError:
        return ""
    return _transcribe_media(path, max_chars)


def parse_audio(path: Path, max_chars: int) -> str:
    parts: list[str] = [f"# Audio: {path.name}"]
    parts.extend(_ffprobe_metadata(path))
    transcript = _transcribe_audio(path, max_chars)
    if transcript:
        parts.append("# Transcript\n" + transcript)
    if len(parts) == 1:
        return ""
    return "\n".join(parts)[:max_chars]


def _extract_audio_track(path: Path, tmp: str) -> Path | None:
    """Decode a bounded mono 16 kHz WAV from the video for transcription."""
    ffmpeg = shutil.which("ffmpeg")
    if not ffmpeg:
        return None
    out = Path(tmp) / "audio.wav"
    max_seconds = int(_env_float("JIKJI_VIDEO_AUDIO_MAX_SECONDS", 900.0))
    _run(
        [
            ffmpeg,
            "-y",
            "-i",
            str(path.resolve()),
            "-vn",
            "-ac",
            "1",
            "-ar",
            "16000",
            "-t",
            str(max_seconds),
            "-f",
            "wav",
            str(out),
        ],
        timeout=_env_float("JIKJI_FFMPEG_TIMEOUT", 90.0),
    )
    try:
        if out.exists() and out.stat().st_size > 0:
            return out
    except OSError:
        pass
    return None


def _transcribe_video(path: Path, max_chars: int) -> str:
    if not _env_flag("JIKJI_ENABLE_TRANSCRIPTION", default=False):
        return ""
    with tempfile.TemporaryDirectory(prefix="jikji-video-audio-") as tmp:
        audio = _extract_audio_track(path, tmp)
        if audio is None:
            return ""
        return _transcribe_media(audio, max_chars)


def _video_keyframe_ocr(path: Path, max_chars: int) -> str:
    if not _env_flag("JIKJI_ENABLE_VIDEO_OCR", default=False):
        return ""
    if not _rapidocr_available() and shutil.which("tesseract") is None:
        return ""
    ffmpeg = shutil.which("ffmpeg")
    if not ffmpeg:
        return ""
    frames = max(1, int(_env_float("JIKJI_VIDEO_OCR_FRAMES", 5.0)))
    interval = max(1, int(_env_float("JIKJI_VIDEO_OCR_INTERVAL_SECONDS", 30.0)))
    with tempfile.TemporaryDirectory(prefix="jikji-video-ocr-") as tmp:
        pattern = str(Path(tmp) / "frame_%03d.png")
        _run(
            [
                ffmpeg,
                "-y",
                "-i",
                str(path.resolve()),
                "-vf",
                f"fps=1/{interval}",
                "-frames:v",
                str(frames),
                pattern,
            ],
            timeout=_env_float("JIKJI_FFMPEG_TIMEOUT", 90.0),
        )
        seen: set[str] = set()
        texts: list[str] = []
        total = 0
        for frame in sorted(Path(tmp).glob("frame_*.png")):
            text = _ocr_image(frame, max_chars - total).strip()
            if not text or text in seen:
                continue
            seen.add(text)
            texts.append(text)
            total += len(text) + 1
            if total >= max_chars:
                break
    return "\n".join(texts)[:max_chars]


def parse_video(path: Path, max_chars: int) -> str:
    parts: list[str] = [f"# Video: {path.name}"]
    parts.extend(_ffprobe_metadata(path))
    transcript = _transcribe_video(path, max_chars)
    if transcript:
        parts.append("# Transcript\n" + transcript)
    on_screen = _video_keyframe_ocr(path, max_chars)
    if on_screen:
        parts.append("# On-screen text\n" + on_screen)
    if len(parts) == 1:
        return ""
    return "\n".join(parts)[:max_chars]
