"""Archive-content parser.

Strategy: do NOT extract anything.  Just list the archive's member
file names + extensions and emit them as a synthetic body excerpt.
The classifier treats this list like the document body of a
"manifest" file — much better than an empty excerpt because the
member names usually carry the project / contract / report identity
("RTX_GPU_3대_구매계약_세금계산서_*.pdf", "행안부_제안서_v1.hwp",
…), and that identity is exactly what drives folder placement.

Supported containers:
    .zip        — Python stdlib :mod:`zipfile`
    .tar / .tar.gz / .tgz / .tar.bz2 / .tbz / .tar.xz / .txz
                 — Python stdlib :mod:`tarfile`
    .jar / .war — same as zip

7z and rar are listed with the local ``7z`` command when available.
No archive is extracted to disk.
"""
from __future__ import annotations

import logging
import shutil
import subprocess
import tarfile
import zipfile
from pathlib import Path

log = logging.getLogger(__name__)


_TAR_EXTS = (
    ".tar", ".tar.gz", ".tgz", ".tar.bz2", ".tbz",
    ".tar.xz", ".txz",
)
_ZIP_EXTS = (".zip", ".jar", ".war")
_SEVEN_Z_EXTS = (".7z", ".rar")


def _zip_member_name(info: zipfile.ZipInfo) -> str:
    """Return a best-effort decoded ZIP member path.

    Python already decodes UTF-8 ZIP entries correctly.  Older Korean ZIPs
    often omit the UTF-8 flag and store CP949 bytes that Python decodes as
    CP437; round-trip through CP437 and try CP949/EUC-KR to recover names.
    """
    name = info.filename
    if info.flag_bits & 0x800:
        return name
    try:
        raw = name.encode("cp437")
    except UnicodeEncodeError:
        return name
    for enc in ("cp949", "euc-kr"):
        try:
            decoded = raw.decode(enc)
        except UnicodeDecodeError:
            continue
        if decoded and decoded != name:
            return decoded
    return name


def is_archive(path: Path) -> bool:
    name = path.name.lower()
    return name.endswith(_ZIP_EXTS) or name.endswith(_TAR_EXTS) or name.endswith(_SEVEN_Z_EXTS)


def _format_listing(archive_name: str, names: list[str], max_chars: int) -> str:
    """Render the member-name list as a body-style excerpt."""
    if not names:
        return f"[archive: {archive_name}] (비어 있음)"
    # Drop folder-only entries ("dir/"); keep the actual file names.
    files = [n for n in names if n and not n.endswith("/")]
    if not files:
        return f"[archive: {archive_name}] (디렉토리만 {len(names)}개)"
    # The most identity-bearing tokens tend to be near the top of the
    # listing (root-level files first), so we keep arrival order — no
    # alphabetical sort.  Truncate to fit max_chars.
    header = f"[archive: {archive_name} — {len(files)}개 파일]\n"
    body_parts = []
    used = len(header)
    for n in files:
        candidate = n + ", "
        if used + len(candidate) > max_chars:
            body_parts.append("…")
            break
        body_parts.append(candidate)
        used += len(candidate)
    return header + "".join(body_parts).rstrip(", ")


def parse(path: Path, max_chars: int) -> str:
    """Return up to ``max_chars`` of the archive's member-name listing.

    Never extracts file contents — pure name+extension scan.  Returns
    "" only on hard failure (corrupt archive, unsupported variant).
    """
    name = path.name
    name_lc = name.lower()
    try:
        if name_lc.endswith(_ZIP_EXTS):
            with zipfile.ZipFile(str(path), "r") as zf:
                names = [_zip_member_name(zi) for zi in zf.infolist()]
            return _format_listing(name, names, max_chars)
        if name_lc.endswith(_TAR_EXTS):
            # tarfile auto-detects the compression suffix.
            with tarfile.open(str(path), "r:*") as tf:
                names = [m.name for m in tf.getmembers()]
            return _format_listing(name, names, max_chars)
        if name_lc.endswith(_SEVEN_Z_EXTS):
            seven_z = shutil.which("7z")
            if not seven_z:
                return ""
            proc = subprocess.run(  # noqa: S603 - executable resolved by shutil.which.
                [seven_z, "l", "-slt", str(path.resolve())],
                check=False,
                capture_output=True,
                text=True,
                errors="ignore",
                timeout=10,
            )
            if proc.returncode != 0:
                log.warning("7z archive list failed for %s: %s", path, proc.stderr[:200])
                return ""
            names = []
            for line in proc.stdout.splitlines():
                if line.startswith("Path = "):
                    member = line.partition(" = ")[2].strip()
                    if not member:
                        continue
                    try:
                        if Path(member).resolve() == path.resolve():
                            continue
                    except OSError:
                        pass
                    names.append(member)
            return _format_listing(name, names, max_chars)
    except (zipfile.BadZipFile, tarfile.TarError, OSError, subprocess.TimeoutExpired) as exc:
        log.warning("archive parse failed for %s: %s", path, exc)
        return ""
    except Exception as exc:  # pragma: no cover — defensive
        log.warning("archive parse unexpected error for %s: %s", path, exc)
        return ""
    return ""
