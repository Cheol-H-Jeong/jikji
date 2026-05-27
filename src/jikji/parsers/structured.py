"""Parsers for agent-relevant structured local files.

These parsers are intentionally dependency-light and read-only.  They turn
common "not quite document" formats (mail, calendars, SQLite databases, EPUB
books) into bounded plain text so Jikji search can find embedded local content.
"""
from __future__ import annotations

import logging
import re
import sqlite3
import zipfile
from email import policy
from email.message import EmailMessage, Message
from email.parser import BytesParser
from html.parser import HTMLParser
from pathlib import Path
from xml.etree import ElementTree as ET

log = logging.getLogger(__name__)

_ENCODINGS = ("utf-8", "utf-8-sig", "cp949", "euc-kr", "latin-1")
_EMAIL_HEADERS = ("subject", "from", "to", "cc", "bcc", "date", "reply-to", "message-id")
_ICS_FIELDS = {
    "SUMMARY",
    "DTSTART",
    "DTEND",
    "DUE",
    "LOCATION",
    "DESCRIPTION",
    "RRULE",
    "ORGANIZER",
    "ATTENDEE",
    "CATEGORIES",
    "STATUS",
    "UID",
    "URL",
    "COMMENT",
    "CONTACT",
    "RESOURCES",
    "X-WR-CALNAME",
    "X-WR-CALDESC",
}
_SQLITE_SKIP_TYPES = ("BLOB", "BINARY", "VARBINARY", "IMAGE")
_EPUB_TEXT_EXTS = (".xhtml", ".html", ".htm", ".xml")


def _cap(parts: list[str], max_chars: int) -> str:
    text = "\n".join(part.strip() for part in parts if str(part).strip())
    return re.sub(r"\n{3,}", "\n\n", text).strip()[:max_chars]


def _read_text(path: Path, max_bytes: int) -> str:
    try:
        raw = path.read_bytes()[:max_bytes]
    except OSError as exc:
        log.warning("read failed %s: %s", path, exc)
        return ""
    for enc in _ENCODINGS:
        try:
            return raw.decode(enc)
        except UnicodeDecodeError:
            continue
    return raw.decode("utf-8", errors="ignore")


class _HTMLText(HTMLParser):
    def __init__(self):
        super().__init__()
        self.parts: list[str] = []
        self._skip = 0

    def handle_starttag(self, tag, attrs):  # noqa: D401
        if tag.lower() in {"script", "style", "head"}:
            self._skip += 1

    def handle_endtag(self, tag):
        if tag.lower() in {"script", "style", "head"} and self._skip > 0:
            self._skip -= 1

    def handle_data(self, data):
        if self._skip == 0:
            txt = re.sub(r"\s+", " ", data).strip()
            if txt:
                self.parts.append(txt)


def _html_to_text(raw: str) -> str:
    parser = _HTMLText()
    try:
        parser.feed(raw)
    except Exception:
        return re.sub(r"<[^>]+>", " ", raw)
    return "\n".join(parser.parts)


def _message_parts(msg: Message) -> tuple[list[str], list[str], list[str]]:
    plain_parts: list[str] = []
    html_parts: list[str] = []
    attachments: list[str] = []
    for part in msg.walk() if msg.is_multipart() else [msg]:
        disposition = (part.get_content_disposition() or "").lower()
        filename = part.get_filename()
        if filename:
            attachments.append(filename)
        if disposition == "attachment":
            continue
        ctype = (part.get_content_type() or "").lower()
        try:
            if isinstance(part, EmailMessage):
                content = part.get_content()
            else:
                payload = part.get_payload(decode=True)
                if payload is None:
                    content = part.get_payload()
                else:
                    charset = part.get_content_charset() or "utf-8"
                    content = payload.decode(charset, errors="ignore")
        except Exception:
            continue
        if not isinstance(content, str):
            continue
        content = content.strip()
        if not content:
            continue
        if ctype == "text/plain":
            plain_parts.append(content)
        elif ctype == "text/html":
            html_parts.append(_html_to_text(content))
    return plain_parts, html_parts, attachments


def parse_eml(path: Path, max_chars: int) -> str:
    """Extract headers, body text, and attachment names from an RFC 822 email."""
    try:
        msg = BytesParser(policy=policy.default).parsebytes(path.read_bytes())
    except Exception as exc:
        log.warning("eml parse failed %s: %s", path, exc)
        return ""

    parts: list[str] = [f"# Email: {path.name}"]
    for key in _EMAIL_HEADERS:
        value = msg.get(key)
        if value:
            parts.append(f"{key.title()}: {str(value).strip()}")

    plain_parts, html_parts, attachments = _message_parts(msg)
    body_parts = list(dict.fromkeys([*plain_parts, *html_parts]))
    body = "\n\n".join(body_parts).strip()
    if body:
        parts.append("\n# Body\n" + body)
    if attachments:
        parts.append("# Attachments\n" + "\n".join(dict.fromkeys(attachments)))
    return _cap(parts, max_chars)


def _unfold_ics_lines(text: str) -> list[str]:
    lines = text.replace("\r\n", "\n").replace("\r", "\n").split("\n")
    out: list[str] = []
    for line in lines:
        if not line:
            continue
        if line[:1] in {" ", "\t"} and out:
            out[-1] += line[1:]
        else:
            out.append(line)
    return out


def _ics_unescape(value: str) -> str:
    return (
        value.replace("\\n", "\n")
        .replace("\\N", "\n")
        .replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\\\", "\\")
    )


def parse_ics(path: Path, max_chars: int) -> str:
    """Extract searchable VEVENT/VTODO fields from an iCalendar file."""
    text = _read_text(path, max_chars * 8)
    if not text:
        return ""
    parts: list[str] = [f"# Calendar: {path.name}"]
    current: list[str] = []
    in_component = False
    component_count = 0
    for raw in _unfold_ics_lines(text):
        if ":" not in raw:
            continue
        key_raw, value = raw.split(":", 1)
        key = key_raw.split(";", 1)[0].upper()
        if key in {"BEGIN"} and value.upper() in {"VEVENT", "VTODO", "VJOURNAL"}:
            in_component = True
            current = [f"## {value.upper()}"]
            continue
        if key == "END" and in_component:
            if current:
                parts.extend(current)
                component_count += 1
            current = []
            in_component = False
            if len("\n".join(parts)) >= max_chars or component_count >= 50:
                break
            continue
        if key in _ICS_FIELDS:
            target = current if in_component else parts
            target.append(f"{key}: {_ics_unescape(value).strip()}")
    return _cap(parts, max_chars)


def _quote_ident(name: str) -> str:
    return '"' + name.replace('"', '""') + '"'


def _sqlite_uri(path: Path) -> str:
    return path.resolve().as_uri() + "?mode=ro&immutable=1"


def _sqlite_text_columns(columns: list[tuple]) -> list[str]:
    out: list[str] = []
    for col in columns:
        name = str(col[1])
        type_name = str(col[2] or "").upper()
        if any(skip in type_name for skip in _SQLITE_SKIP_TYPES):
            continue
        # SQLite has dynamic typing; many app DBs store text in STRING/MEMO or
        # loosely typed columns.  For search recall, sample every non-BLOB-ish
        # column and skip bytes values row-by-row below.
        out.append(name)
    return out[:16]


def parse_sqlite(path: Path, max_chars: int) -> str:
    """Extract table/column names and bounded samples from a SQLite database."""
    parts: list[str] = [f"# SQLite database: {path.name}"]
    try:
        con = sqlite3.connect(_sqlite_uri(path), uri=True, timeout=1.0)
    except sqlite3.Error as exc:
        log.warning("sqlite open failed %s: %s", path, exc)
        return ""
    try:
        con.execute("PRAGMA query_only=ON")
        con.execute("PRAGMA trusted_schema=OFF")
        rows = con.execute(
            "SELECT name FROM sqlite_schema "
            "WHERE type='table' AND name NOT LIKE 'sqlite_%' "
            "ORDER BY name LIMIT 50"
        ).fetchall()
        for (table_name,) in rows:
            table = str(table_name)
            try:
                cols = con.execute(f"PRAGMA table_info({_quote_ident(table)})").fetchall()
            except sqlite3.Error:
                continue
            col_names = [str(col[1]) for col in cols]
            text_cols = _sqlite_text_columns(cols)
            parts.append(f"## Table: {table}")
            if col_names:
                parts.append("Columns: " + ", ".join(col_names[:40]))
            if not text_cols:
                continue
            try:
                selected = ", ".join(_quote_ident(col) for col in text_cols)
                samples = con.execute(
                    f"SELECT {selected} FROM {_quote_ident(table)} LIMIT 32"
                ).fetchall()
            except sqlite3.Error:
                continue
            for sample in samples:
                values = []
                for value in sample:
                    if value is None or isinstance(value, bytes):
                        continue
                    text = re.sub(r"\s+", " ", str(value)).strip()
                    if text:
                        values.append(text[:500])
                if values:
                    parts.append(" | ".join(values))
            if len("\n".join(parts)) >= max_chars:
                break
    except sqlite3.Error as exc:
        log.warning("sqlite parse failed %s: %s", path, exc)
    finally:
        con.close()
    return _cap(parts, max_chars)


def _epub_spine_names(zf: zipfile.ZipFile) -> list[str]:
    try:
        container = zf.read("META-INF/container.xml")
        root = ET.fromstring(container)
    except Exception:
        return []
    opf_name = ""
    for elem in root.iter():
        if elem.tag.rsplit("}", 1)[-1] == "rootfile":
            opf_name = elem.attrib.get("full-path", "")
            break
    if not opf_name:
        return []
    try:
        opf_root = ET.fromstring(zf.read(opf_name))
    except Exception:
        return []
    base = str(Path(opf_name).parent).replace(".", "", 1).strip("/")
    manifest: dict[str, str] = {}
    spine_ids: list[str] = []
    for elem in opf_root.iter():
        local = elem.tag.rsplit("}", 1)[-1]
        if local == "item":
            item_id = elem.attrib.get("id", "")
            href = elem.attrib.get("href", "")
            media_type = elem.attrib.get("media-type", "")
            if item_id and href and (href.lower().endswith(_EPUB_TEXT_EXTS) or "xhtml" in media_type or "html" in media_type):
                manifest[item_id] = f"{base}/{href}" if base else href
        elif local == "itemref":
            ref = elem.attrib.get("idref", "")
            if ref:
                spine_ids.append(ref)
    return [manifest[item_id] for item_id in spine_ids if item_id in manifest]


def parse_epub(path: Path, max_chars: int) -> str:
    """Extract bounded text from EPUB XHTML/HTML payloads without unpacking."""
    parts: list[str] = [f"# EPUB: {path.name}"]
    total = 0
    try:
        with zipfile.ZipFile(path) as zf:
            spine_names = _epub_spine_names(zf)
            fallback_names = [n for n in zf.namelist() if n.lower().endswith(_EPUB_TEXT_EXTS)]
            seen: set[str] = set()
            names: list[str] = []
            for candidate in [*spine_names, *fallback_names]:
                normalized = candidate.replace("\\", "/")
                if normalized in seen or normalized not in zf.namelist():
                    continue
                seen.add(normalized)
                names.append(normalized)
            for name in names[:80]:
                try:
                    raw = zf.read(name)[: max_chars * 4]
                except (KeyError, OSError):
                    continue
                text = ""
                for enc in _ENCODINGS:
                    try:
                        text = raw.decode(enc)
                        break
                    except UnicodeDecodeError:
                        continue
                if not text:
                    text = raw.decode("utf-8", errors="ignore")
                clean = _html_to_text(text)
                if not clean.strip():
                    continue
                parts.append(f"## {name}\n{clean}")
                total += len(clean)
                if total >= max_chars:
                    break
    except zipfile.BadZipFile as exc:
        log.warning("epub open failed %s: %s", path, exc)
        return ""
    except OSError as exc:
        log.warning("epub read failed %s: %s", path, exc)
        return ""
    return _cap(parts, max_chars)
