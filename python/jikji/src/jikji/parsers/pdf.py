"""PDF excerpt extractor using pypdf.

Resilient against the most common real-world failure modes:

* **Encrypted PDFs** (``"File has not been decrypted"``): try the
  empty password first (publishers often "encrypt" with no password
  to enable copy-protection metadata), and fall back to the title /
  author / subject metadata when extraction is genuinely blocked.
* **Malformed cross-references**: ``strict=False`` already lets
  pypdf recover most of these.
* **Per-page extraction crash**: skip the page, keep collecting
  text from later pages.

Returns ``""`` only as a last resort — and even then only after a
metadata-based fallback has been attempted, so the file is at least
findable in the search index by its embedded title.
"""
from __future__ import annotations

import logging
import os
import shutil
import subprocess
import tempfile
from pathlib import Path

log = logging.getLogger(__name__)


def _env_float(name: str, default: float) -> float:
    raw = os.environ.get(name)
    if not raw:
        return default
    try:
        return max(0.1, float(raw))
    except ValueError:
        return default


def _run_text_tool(cmd: list[str], *, timeout: float) -> str:
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
        log.debug("pdf helper failed %s: %s", cmd[:2], exc)
        return ""
    if proc.returncode != 0:
        log.debug("pdf helper non-zero %s: %s", cmd[:2], proc.stderr[:200])
        return ""
    return proc.stdout.strip()


def _pdftotext_fallback(path: Path, max_chars: int) -> str:
    exe = shutil.which("pdftotext")
    if not exe:
        return ""
    text = _run_text_tool(
        [exe, "-layout", "-enc", "UTF-8", "-f", "1", "-l", "5", str(path.resolve()), "-"],
        timeout=_env_float("JIKJI_PDFTOTEXT_TIMEOUT", 10.0),
    )
    return text[:max_chars]


def _pdf_ocr_fallback(path: Path, max_chars: int) -> str:
    """OCR the first few pages if local Poppler+tesseract are installed."""
    pdftoppm = shutil.which("pdftoppm")
    tesseract = shutil.which("tesseract")
    if not pdftoppm or not tesseract:
        return ""
    pages = int(os.environ.get("JIKJI_PDF_OCR_PAGES", "2") or "2")
    pages = max(1, min(5, pages))
    timeout = _env_float("JIKJI_PDF_OCR_TIMEOUT", 30.0)
    lang = os.environ.get("JIKJI_TESSERACT_LANG", "").strip()
    with tempfile.TemporaryDirectory(prefix="jikji-pdf-ocr-") as tmp:
        prefix = str(Path(tmp) / "page")
        try:
            proc = subprocess.run(  # noqa: S603 - executable resolved by shutil.which.
                [pdftoppm, "-r", "160", "-f", "1", "-l", str(pages), "-png", str(path.resolve()), prefix],
                check=False,
                capture_output=True,
                text=True,
                errors="ignore",
                timeout=timeout,
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            log.debug("pdftoppm failed %s: %s", path, exc)
            return ""
        if proc.returncode != 0:
            log.debug("pdftoppm non-zero %s: %s", path, proc.stderr[:200])
            return ""
        parts: list[str] = []
        for image in sorted(Path(tmp).glob("page-*.png"))[:pages]:
            cmd = [tesseract, str(image.resolve()), "stdout", "--psm", os.environ.get("JIKJI_TESSERACT_PSM", "6")]
            if lang:
                cmd.extend(["-l", lang])
            text = _run_text_tool(cmd, timeout=timeout)
            if text:
                parts.append(text)
            if len("\n".join(parts)) >= max_chars:
                break
    return "\n".join(parts).strip()[:max_chars]


def _safe_metadata_text(reader) -> str:
    """Fallback: build a short text from PDF metadata + structural
    hints when the page-text extraction path is blocked (encrypted /
    no extractable text)."""
    parts: list[str] = []
    try:
        meta = reader.metadata or {}
    except Exception:
        meta = {}
    for key in ("/Title", "/Subject", "/Author", "/Keywords", "/Producer"):
        try:
            val = meta.get(key) if hasattr(meta, "get") else None
        except Exception:
            val = None
        if isinstance(val, str) and val.strip():
            parts.append(val.strip())
    try:
        n = len(reader.pages)
        if n:
            parts.append(f"[PDF · {n}쪽]")
    except Exception:
        pass
    return "\n".join(parts)


def parse(path: Path, max_chars: int) -> str:
    try:
        from pypdf import PdfReader  # type: ignore
    except ImportError:
        log.warning("pypdf not installed; skipping PDF %s", path)
        return ""

    try:
        reader = PdfReader(str(path), strict=False)
    except Exception as exc:
        log.warning("pdf open failed %s: %s", path, exc)
        fallback = _pdftotext_fallback(path, max_chars)
        if fallback:
            return fallback[:max_chars]
        return ""

    # Many "encrypted" PDFs use an empty owner password — pypdf still
    # refuses to extract until decrypt() succeeds.  Try the empty
    # password before giving up.
    try:
        encrypted = bool(getattr(reader, "is_encrypted", False))
    except Exception:
        encrypted = False
    if encrypted:
        for pwd in ("", " "):
            try:
                if reader.decrypt(pwd):
                    encrypted = False
                    log.info("pdf decrypted with empty password: %s", path)
                    break
            except Exception:
                pass
        if encrypted:
            # Real password protection — surface the metadata so the
            # search index isn't blind to this file, but log info-level
            # rather than warning (this is *expected* for some inputs).
            meta = _safe_metadata_text(reader)
            if meta:
                log.info("pdf encrypted, using metadata fallback: %s", path)
                return meta[:max_chars]
            log.info("pdf encrypted with no metadata fallback: %s", path)
            return ""

    chunks: list[str] = []
    total = 0
    try:
        pages = list(reader.pages[:5])
    except Exception as exc:
        log.debug("pdf page list failed for %s: %s", path, exc)
        pages = []
    for page in pages:
        try:
            txt = page.extract_text() or ""
        except Exception as exc:
            # Encrypted page in an otherwise-readable file → fall back
            # to metadata after we've drained any pages we can.
            log.debug("pdf page extract failed: %s", exc)
            continue
        if not txt:
            continue
        chunks.append(txt)
        total += len(txt)
        if total >= max_chars:
            break

    joined = "\n".join(chunks).strip()
    if joined:
        return joined[:max_chars]

    # pypdf sometimes misses text in malformed PDFs that Poppler can read.
    fallback = _pdftotext_fallback(path, max_chars)
    if fallback:
        log.info("pdf body empty, using pdftotext fallback: %s", path)
        return fallback[:max_chars]

    # If the PDF is scanned and local OCR tooling exists, OCR the first pages.
    ocr = _pdf_ocr_fallback(path, max_chars)
    if ocr:
        log.info("pdf body empty, using OCR fallback: %s", path)
        return ocr[:max_chars]

    # Page-text extraction yielded nothing — try metadata as a last
    # resort so the file shows up in the search index by its title.
    meta = _safe_metadata_text(reader)
    if meta:
        log.info("pdf body empty, using metadata fallback: %s", path)
        return meta[:max_chars]
    return ""
