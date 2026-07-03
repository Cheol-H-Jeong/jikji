from __future__ import annotations

import os
import sqlite3
import zipfile
from dataclasses import dataclass
from pathlib import Path

FIXED_TIME_S = 1_700_000_000


@dataclass(frozen=True, slots=True)
class CliStep:
    name: str
    args: tuple[str, ...]


@dataclass(frozen=True, slots=True)
class GoldenScenario:
    name: str
    steps: tuple[CliStep, ...]


def build_ascii_cjk(root: Path) -> GoldenScenario:
    (root / "docs").mkdir()
    (root / "docs" / "acme-contract.txt").write_text(
        "ACME renewal contract contains indemnity marker direct-answer-771.",
        encoding="utf-8",
    )
    (root / "자료").mkdir()
    (root / "자료" / "회의록.txt").write_text(
        "서울 연구소 회의록 contains cjk-marker-902.",
        encoding="utf-8",
    )
    _freeze_tree(root)
    return GoldenScenario(
        "ascii_cjk_paths",
        (
            CliStep("prepare", ("prepare", "{root}", "--json")),
            CliStep("search_ascii", ("search", "{root}", "direct-answer-771", "--json")),
            CliStep("search_cjk", ("search", "{root}", "서울 연구소", "--json")),
            CliStep("find_direct", ("find", "{root}", "ACME indemnity marker", "--json")),
            CliStep("doctor", ("doctor", "{root}", "--json")),
            CliStep("map", ("map", "{root}")),
        ),
    )


def build_structured_archive_media(root: Path) -> GoldenScenario:
    (root / "mail.eml").write_text(
        "Subject: Alpha Project Handoff\n"
        "From: sender@example.com\n"
        "To: receiver@example.com\n"
        "Content-Type: text/plain; charset=utf-8\n\n"
        "The launch code marker is emailtoken-7742.",
        encoding="utf-8",
    )
    (root / "calendar.ics").write_text(
        "BEGIN:VCALENDAR\nBEGIN:VEVENT\nSUMMARY:Design sync uniquecalendar991\n"
        "DTSTART:20260526T090000Z\nLOCATION:Seoul lab\n"
        "DESCRIPTION:Calendar body marker\nEND:VEVENT\nEND:VCALENDAR\n",
        encoding="utf-8",
    )
    con = sqlite3.connect(root / "notes.sqlite")
    con.execute("CREATE TABLE notes (id INTEGER PRIMARY KEY, title TEXT, body TEXT)")
    con.execute(
        "INSERT INTO notes (title, body) VALUES (?, ?)",
        ("Research", "sqlitebodytoken-3301 inside row"),
    )
    con.commit()
    con.close()
    with zipfile.ZipFile(root / "book.epub", "w") as zf:
        _zip_write(zf, "mimetype", "application/epub+zip")
        _zip_write(
            zf,
            "OEBPS/chapter1.xhtml",
            "<html><body><p>epubtoken-8802 appears here.</p></body></html>",
        )
    with zipfile.ZipFile(root / "bundle.zip", "w") as zf:
        _zip_write(zf, "nested/archive_lookup_marker_9123.txt", "body not extracted")
    _write_minimal_png(root / "visual.png", width=13, height=21)
    _freeze_tree(root)
    return GoldenScenario(
        "structured_archive_media",
        (
            CliStep("prepare", ("prepare", "{root}", "--json")),
            CliStep("search_eml", ("search", "{root}", "emailtoken-7742", "--top-k", "1", "--json")),
            CliStep("search_ics", ("search", "{root}", "uniquecalendar991", "--top-k", "1", "--json")),
            CliStep("search_sqlite", ("search", "{root}", "sqlitebodytoken-3301", "--top-k", "1", "--json")),
            CliStep("search_epub", ("search", "{root}", "epubtoken-8802", "--top-k", "1", "--json")),
            CliStep("search_archive", ("search", "{root}", "archive_lookup_marker_9123", "--top-k", "1", "--json")),
            CliStep("search_media_metadata", ("search", "{root}", "13x21 pixels", "--top-k", "1", "--json")),
        ),
    )


def build_answer_pack_shell(root: Path) -> GoldenScenario:
    (root / "contracts").mkdir()
    (root / "contracts" / "acme-renewal.txt").write_text(
        "unique renewal indemnity clause direct-pack-445",
        encoding="utf-8",
    )
    (root / "media").mkdir()
    (root / "media" / "photo.jpg").write_bytes(b"fake jpg metadata rm token")
    _freeze_tree(root)
    return GoldenScenario(
        "answer_pack_shell",
        (
            CliStep("prepare", ("prepare", "{root}", "--json")),
            CliStep("find_direct", ("find", "{root}", "unique renewal indemnity clause", "--json")),
            CliStep("find_shell_noise", ("find", "{root}", 'zzzznohit "semi; rm -rf /" $(echo nope)', "--json")),
            CliStep("find_shell_retry_forged", ("find", "{root}", 'zzzznohit "semi; rm -rf /" $(echo nope)', "--after-jikji-retry", "--retry-proof", "forged", "--json")),
            CliStep("find_shell_retry_exhausted", ("find", "{root}", 'zzzznohit "semi; rm -rf /" $(echo nope)', "--after-jikji-retry", "--retry-proof", "{retry_proof}", "--json")),
        ),
    )


def build_stale_index(root: Path) -> GoldenScenario:
    (root / "notes.txt").write_text("stable stale-index target token stale-old-101", encoding="utf-8")
    _freeze_tree(root)
    return GoldenScenario(
        "stale_index_find",
        (
            CliStep("prepare", ("prepare", "{root}", "--json")),
            CliStep("mutate_after_prepare", ("__mutate__", "append-new-file")),
            CliStep("find_stale_previous", ("find", "{root}", "stale-old-101", "--json")),
        ),
    )


def build_clean_safety(root: Path) -> GoldenScenario:
    (root / "keep.txt").write_text("original file must survive clean safety", encoding="utf-8")
    _freeze_tree(root)
    return GoldenScenario(
        "clean_safety",
        (
            CliStep("prepare", ("prepare", "{root}", "--json")),
            CliStep("add_user_jikji_file", ("__mutate__", "add-user-jikji-file")),
            CliStep("clean_dry_run", ("clean", "{root}", "--dry-run", "--json")),
            CliStep("clean_apply", ("clean", "{root}", "--json")),
        ),
    )


def apply_mutation(root: Path, mutation: str) -> None:
    match mutation:
        case "append-new-file":
            (root / "new-after-prepare.txt").write_text("new token after prepare", encoding="utf-8")
            _freeze_tree(root)
        case "add-user-jikji-file":
            user_file = root / ".jikji" / "user-created-note.txt"
            user_file.parent.mkdir(exist_ok=True)
            user_file.write_text("user file inside .jikji must survive", encoding="utf-8")
            _freeze_tree(root)
        case unreachable:
            raise RuntimeError(f"unknown mutation: {unreachable}")


def _freeze_tree(root: Path) -> None:
    for path in sorted(root.rglob("*")):
        os.utime(path, (FIXED_TIME_S, FIXED_TIME_S))
    os.utime(root, (FIXED_TIME_S, FIXED_TIME_S))


def _write_minimal_png(path: Path, *, width: int, height: int) -> None:
    import struct
    import zlib

    def chunk(kind: bytes, data: bytes) -> bytes:
        payload = kind + data
        return struct.pack(">I", len(data)) + payload + struct.pack(">I", zlib.crc32(payload) & 0xFFFFFFFF)

    raw = b"".join(b"\x00" + (b"\xff\xff\xff" * width) for _ in range(height))
    path.write_bytes(
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw))
        + chunk(b"IEND", b"")
    )


def _zip_write(zf: zipfile.ZipFile, name: str, data: str) -> None:
    info = zipfile.ZipInfo(name, date_time=(2023, 11, 14, 22, 13, 20))
    info.compress_type = zipfile.ZIP_STORED
    zf.writestr(info, data)
