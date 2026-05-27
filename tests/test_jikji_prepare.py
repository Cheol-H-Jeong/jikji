from __future__ import annotations

import json
import os

from jikji.agent_index import build_agent_index
from jikji.config import Config


def _jsonl(path):
    return [json.loads(line) for line in path.read_text(encoding="utf-8").split("\n") if line]


def test_prepare_is_non_destructive_and_writes_jikji_artifacts(tmp_path):
    src = tmp_path / "기존" / "회의"
    src.mkdir(parents=True)
    doc = src / "회의록.txt"
    doc.write_text("Jikji smoke", encoding="utf-8")

    result = build_agent_index(tmp_path, Config())

    assert doc.exists()
    assert result.files == 1
    assert (tmp_path / ".jikji" / "agent_map.md").exists()
    assert (tmp_path / "000_JIKJI_AGENT_MAP.md").exists()
    rows = _jsonl(tmp_path / ".jikji" / "file_index.jsonl")
    assert rows[0]["path"] == "기존/회의/회의록.txt"


def test_prepare_prunes_deleted_document_cache(tmp_path):
    doc = tmp_path / "보고서.rtf"
    doc.write_text(r"{\rtf1\ansi Jikji stale body}", encoding="utf-8")
    build_agent_index(tmp_path, Config())
    rows = _jsonl(tmp_path / ".jikji" / "document_index.jsonl")
    assert rows
    cache = tmp_path / rows[0]["text_cache_path"]
    assert cache.exists()

    doc.unlink()
    build_agent_index(tmp_path, Config())

    assert not cache.exists()
    file_rows = _jsonl(tmp_path / ".jikji" / "file_index.jsonl")
    assert any(r.get("status") == "deleted" and r.get("path") == "보고서.rtf" for r in file_rows)


def test_prepare_emits_agent_search_standard_artifacts(tmp_path):
    doc = tmp_path / "보고서.rtf"
    doc.write_text(r"{\rtf1\ansi Jikji standard body}", encoding="utf-8")

    build_agent_index(tmp_path, Config())

    manifest = json.loads((tmp_path / ".jikji" / "manifest.json").read_text(encoding="utf-8"))
    assert manifest["non_destructive"] is True
    assert manifest["cache_key_policy"]
    assert ".jikji/doc_text/" in manifest["owned_paths"]
    assert not (tmp_path / ".jikji" / "search_terms.jsonl").exists()
    assert (tmp_path / ".jikji" / "file_cards.jsonl").exists()
    assert (tmp_path / ".jikji" / "chunk_map.jsonl").exists()
    assert (tmp_path / ".jikji" / "search_index.sqlite").exists()
    assert (tmp_path / ".jikji" / "duplicate_map.jsonl").exists()
    assert (tmp_path / ".jikji" / "autorag_manifest.json").exists()

    rows = _jsonl(tmp_path / ".jikji" / "document_index.jsonl")
    assert rows[0]["file_id"].startswith("sha256:")
    meta = json.loads((tmp_path / rows[0]["doc_meta_path"]).read_text(encoding="utf-8"))
    assert meta["schema_version"] == 1
    assert meta["file_id"] == rows[0]["file_id"]
    card = _jsonl(tmp_path / ".jikji" / "file_cards.jsonl")[0]
    assert "content_terms" in card
    assert "rare_terms" in card
    assert "evidence_previews" in card
    assert "filename_lookup_keys" in card
    chunk = _jsonl(tmp_path / ".jikji" / "chunk_map.jsonl")[0]
    assert chunk["path"] == "보고서.rtf"
    assert "content_terms" in chunk


def test_prepare_skips_sensitive_names_by_default(tmp_path):
    safe = tmp_path / "notes.txt"
    secret = tmp_path / ".env"
    safe.write_text("public", encoding="utf-8")
    secret.write_text("TOKEN=secret", encoding="utf-8")

    build_agent_index(tmp_path, Config(include_hidden=True))

    paths = {row["path"] for row in _jsonl(tmp_path / ".jikji" / "file_index.jsonl")}
    assert "notes.txt" in paths
    assert ".env" not in paths


def test_doctor_json_reports_ok(tmp_path, capsys):
    from jikji.__main__ import main

    (tmp_path / "notes.txt").write_text("hello", encoding="utf-8")
    assert main(["prepare", str(tmp_path), "--json"]) == 0
    capsys.readouterr()

    assert main(["doctor", str(tmp_path), "--json"]) == 0
    out = capsys.readouterr().out
    report = json.loads(out)
    assert report["ok"] is True
    assert report["errors"] == []
    assert report["manifest"]["search_index_schema_version"] == 1


def test_clean_removes_only_jikji_artifacts(tmp_path, capsys):
    from jikji.__main__ import main

    doc = tmp_path / "keep.txt"
    doc.write_text("original file must survive", encoding="utf-8")
    assert main(["prepare", str(tmp_path), "--json"]) == 0
    capsys.readouterr()

    assert (tmp_path / ".jikji").exists()
    assert (tmp_path / "000_JIKJI_AGENT_MAP.md").exists()
    assert main(["clean", str(tmp_path), "--dry-run", "--json"]) == 0
    dry = json.loads(capsys.readouterr().out)
    assert dry["dry_run"] is True
    assert str(tmp_path / ".jikji") in dry["would_remove"]
    assert (tmp_path / ".jikji").exists()
    assert doc.exists()

    assert main(["clean", str(tmp_path), "--json"]) == 0
    cleaned = json.loads(capsys.readouterr().out)
    assert cleaned["ok"] is True
    assert cleaned["preserved_original_files"] is True
    assert not (tmp_path / ".jikji").exists()
    assert not (tmp_path / "000_JIKJI_AGENT_MAP.md").exists()
    assert doc.read_text(encoding="utf-8") == "original file must survive"


def test_clean_refuses_unverified_jikji_dir_without_force(tmp_path, capsys):
    from jikji.__main__ import main

    user_note = tmp_path / ".jikji" / "not-from-jikji.txt"
    user_note.parent.mkdir()
    user_note.write_text("do not remove without force", encoding="utf-8")

    assert main(["clean", str(tmp_path), "--json"]) == 1
    refused = json.loads(capsys.readouterr().out)
    assert refused["reason"] == "missing_manifest"
    assert user_note.exists()


def test_prepare_recovers_stale_lock(tmp_path):
    index_dir = tmp_path / ".jikji"
    index_dir.mkdir()
    (index_dir / ".lock").write_text('{"pid": 99999999, "started_at": "2000-01-01T00:00:00Z"}', encoding="utf-8")
    (tmp_path / "notes.txt").write_text("hello", encoding="utf-8")

    result = build_agent_index(tmp_path, Config())

    assert result.files == 1
    assert not (index_dir / ".lock").exists()


def test_prepare_recovers_old_lock_even_if_pid_exists(tmp_path):
    index_dir = tmp_path / ".jikji"
    index_dir.mkdir()
    (index_dir / ".lock").write_text(
        json.dumps({"pid": os.getpid(), "started_at": "2000-01-01T00:00:00+00:00"}),
        encoding="utf-8",
    )
    (tmp_path / "notes.txt").write_text("hello", encoding="utf-8")

    result = build_agent_index(tmp_path, Config())

    assert result.files == 1
    assert not (index_dir / ".lock").exists()


def test_prepare_recovers_old_empty_lock(tmp_path):
    index_dir = tmp_path / ".jikji"
    index_dir.mkdir()
    lock = index_dir / ".lock"
    lock.write_text("", encoding="utf-8")
    old = 946684800  # 2000-01-01
    os.utime(lock, (old, old))
    (tmp_path / "notes.txt").write_text("hello", encoding="utf-8")

    result = build_agent_index(tmp_path, Config())

    assert result.files == 1
    assert not lock.exists()


def test_eval_generate_and_run_scores_local_search(tmp_path, capsys):
    from jikji.__main__ import main

    docs = tmp_path / "projects" / "apollo"
    docs.mkdir(parents=True)
    target = docs / "mission-notes.txt"
    target.write_text(
        "Apollo lunar telemetry rendezvous checklist uniqueanchor",
        encoding="utf-8",
    )
    (tmp_path / "finance-report.md").write_text("budget forecast margin", encoding="utf-8")

    assert main(["prepare", str(tmp_path), "--json"]) == 0
    capsys.readouterr()
    assert main(["eval-generate", str(tmp_path), "--cases", "20", "--json"]) == 0
    generated = json.loads(capsys.readouterr().out)
    assert generated["cases"] > 0
    assert (tmp_path / ".jikji" / "eval" / "eval_set.jsonl").exists()
    scenarios = set(generated["scenarios"])
    assert "filename_exact" in scenarios
    assert "lexical_content" in scenarios

    assert main(["eval", str(tmp_path), "--json"]) == 0
    report = json.loads(capsys.readouterr().out)
    assert report["metrics"]["cases"] == generated["cases"]
    assert report["metrics"]["hit_at_1"] > 0
    assert (tmp_path / ".jikji" / "eval" / "eval_report.json").exists()

    assert main(["bench-analyze", str(tmp_path), "--top-k", "10", "--json"]) == 0
    analysis = json.loads(capsys.readouterr().out)
    assert analysis["summary"]["cases"] == generated["cases"]
    assert (tmp_path / ".jikji" / "eval" / "bench_analysis.json").exists()

    realistic = tmp_path / "realistic.jsonl"
    assert main([
        "eval-generate-realistic",
        str(tmp_path),
        "--cases",
        "10",
        "--out",
        str(realistic),
        "--json",
    ]) == 0
    realistic_payload = json.loads(capsys.readouterr().out)
    assert realistic_payload["cases"] > 0
    assert realistic.exists()

    holdout = tmp_path / "holdout.jsonl"
    assert main([
        "eval-generate-holdout",
        str(tmp_path),
        "--cases",
        "12",
        "--out",
        str(holdout),
        "--json",
    ]) == 0
    holdout_payload = json.loads(capsys.readouterr().out)
    assert holdout_payload["locked_holdout"] is True
    assert holdout_payload["cases"] > 0
    assert holdout_payload["sha256"]
    assert holdout.exists()
    profile = json.loads(holdout.with_suffix(".profile.json").read_text(encoding="utf-8"))
    assert profile["anti_overfit_protocol"]["do_not_inspect_cases_while_tuning"] is True
    assert profile["generator"] == "jikji-holdout-scorer-blind"

    assert main(["search", str(tmp_path), "uniqueanchor", "--top-k", "3", "--json"]) == 0
    search_report = json.loads(capsys.readouterr().out)
    assert search_report["index_status"] == "ready"
    assert search_report["candidates"]
    assert search_report["candidates"][0]["path"] == "projects/apollo/mission-notes.txt"


def test_search_auto_prepares_missing_index(tmp_path, capsys):
    from jikji.__main__ import main

    (tmp_path / "notes.txt").write_text("automatic prepare uniqueanchor", encoding="utf-8")

    assert main(["search", str(tmp_path), "uniqueanchor", "--json"]) == 0
    report = json.loads(capsys.readouterr().out)

    assert report["index_status"] == "prepared_now"
    assert report["foreground_prepared"] is True
    assert (tmp_path / ".jikji" / "search_index.sqlite").exists()
    assert report["candidates"][0]["path"] == "notes.txt"


def test_search_stale_index_starts_background_refresh(tmp_path, capsys, monkeypatch):
    from jikji import __main__ as cli

    calls = []

    class FakePopen:
        def __init__(self, cmd, **kwargs):
            calls.append((cmd, kwargs))
            stdout = kwargs.get("stdout")
            if stdout:
                stdout.close()

    (tmp_path / "notes.txt").write_text("stale index uniqueanchor", encoding="utf-8")
    assert cli.main(["prepare", str(tmp_path), "--json"]) == 0
    capsys.readouterr()
    monkeypatch.setattr(cli.subprocess, "Popen", FakePopen)

    assert cli.main(["search", str(tmp_path), "uniqueanchor", "--stale-after-seconds", "0", "--json"]) == 0
    report = json.loads(capsys.readouterr().out)

    assert report["index_status"] == "stale_using_previous_index"
    assert report["background_refresh_started"] is True
    assert calls
    assert "prepare" in calls[0][0]
    assert report["candidates"][0]["path"] == "notes.txt"


def test_hippocamp_import_and_bench_raw_vs_jikji(tmp_path, capsys):
    from jikji.__main__ import main

    root = tmp_path / "Adam_Subset"
    root.mkdir()
    target = root / "contractnli" / "Tazza-CAFFE-Confidentiality-Agreement.rtf"
    target.parent.mkdir()
    target.write_text(r"{\rtf1\ansi confidential information employee representative exception}", encoding="utf-8")
    other = root / "notes.txt"
    other.write_text("unrelated grocery reminder", encoding="utf-8")
    annotation = tmp_path / "Adam_Subset.annotation.json"
    annotation.write_text(json.dumps([
        {
            "id": "1",
            "file_path": ["contractnli/Tazza-CAFFE-Confidentiality-Agreement.rtf"],
            "question": "Which agreement mentions confidential information and employee representatives?",
            "QA_type": "semantic",
            "evidence": [{"evidence_text": "confidential information employee representative"}],
            "answer": "Tazza Caffe confidentiality agreement",
        }
    ]), encoding="utf-8")

    assert main(["prepare", str(root), "--json"]) == 0
    capsys.readouterr()
    assert main(["hippocamp-import", str(root), "--annotation", str(annotation), "--json"]) == 0
    imported = json.loads(capsys.readouterr().out)
    assert imported["cases"] == 1
    assert not (root / ".jikji" / "eval" / "hippocamp_eval_set.jsonl").exists()
    assert main(["bench-run", str(root), "--eval-set", imported["eval_set"], "--json"]) == 0
    bench = json.loads(capsys.readouterr().out)
    assert set(bench["metrics"]) == {"raw", "jikji"}
    assert not str(bench["report"]).startswith(str(root / ".jikji"))
    assert bench["metrics"]["raw"]["cases"] == 1
    assert bench["metrics"]["jikji"]["hit_at_1"] == 1.0


def test_prepare_emits_semantic_hints_for_agent_search(tmp_path):
    company = tmp_path / "Company" / "4_Manufacturing" / "SEA & ANCHOR PTE. LTD. (201503267M) - Singapore Company.pdf"
    company.parent.mkdir(parents=True)
    company.write_text("Date Incorporation 2015 SSIC manufacturing entity", encoding="utf-8")

    build_agent_index(tmp_path, Config())

    rows = _jsonl(tmp_path / ".jikji" / "file_index.jsonl")
    row = rows[0]
    assert "semantic_hints" in row
    hints = {str(x).casefold() for x in row["semantic_hints"]}
    assert "company" in hints
    assert any("manufacturing" in hint for hint in hints)
    assert "folder_terms" in row


def test_prepare_never_indexes_jikji_workspace_even_when_hidden_included(tmp_path):
    (tmp_path / "notes.txt").write_text("hello", encoding="utf-8")
    build_agent_index(tmp_path, Config(include_hidden=True))
    build_agent_index(tmp_path, Config(include_hidden=True))

    paths = {row["path"] for row in _jsonl(tmp_path / ".jikji" / "file_index.jsonl")}
    assert "notes.txt" in paths
    assert not any(path.startswith(".jikji/") for path in paths)


def test_hermes_bench_rejects_eval_leaks(tmp_path):
    from jikji.hermes_bench import assert_no_leak_root

    root = tmp_path / "root"
    root.mkdir()
    leaked_eval = root / ".jikji" / "eval" / "cases.jsonl"
    leaked_eval.parent.mkdir(parents=True)
    leaked_eval.write_text("{}", encoding="utf-8")

    try:
        assert_no_leak_root(root, leaked_eval)
    except RuntimeError as exc:
        assert "no-leak check failed" in str(exc)
    else:  # pragma: no cover
        raise AssertionError("expected no-leak check failure")


def test_hermes_jikji_prompt_is_tool_first(tmp_path):
    from jikji.hermes_bench import _prompt

    prompt = _prompt(tmp_path, "jikji", {"query": "find notes", "id": "case"}, candidate_top_k=5)

    assert "JIKJI TOOL-FIRST MODE" in prompt
    assert "JIKJI SEARCH RESULT" in prompt
    assert "Do not call rg/find/ls/cat" in prompt
    assert "return several candidates" in prompt


def test_bench_run_rejects_annotation_leak(tmp_path, capsys):
    from jikji.__main__ import main

    root = tmp_path / "Adam_Subset"
    root.mkdir()
    (root / "target.txt").write_text("answer", encoding="utf-8")
    (root / "Adam_Subset.json").write_text("[]", encoding="utf-8")
    eval_set = tmp_path / "eval.jsonl"
    eval_set.write_text(json.dumps({
        "id": "case",
        "scenario": "leak",
        "query": "answer",
        "expected_paths": ["target.txt"],
    }) + "\n", encoding="utf-8")

    assert main(["prepare", str(root), "--json"]) == 0
    capsys.readouterr()
    try:
        main(["bench-run", str(root), "--eval-set", str(eval_set), "--json"])
    except RuntimeError as exc:
        assert "no-leak check failed" in str(exc)
    else:  # pragma: no cover
        raise AssertionError("expected bench-run leak rejection")


def test_hermes_skill_install_to_explicit_dest(tmp_path):
    from jikji.hermes_bench import install_hermes_skill

    dest = tmp_path / "skills" / "jikji" / "SKILL.md"
    result = install_hermes_skill(dest=dest)

    assert result.installed is True
    assert dest.exists()
    assert "Jikji" in dest.read_text(encoding="utf-8")


def test_bench_iterate_records_requested_iterations(tmp_path, capsys):
    from jikji.__main__ import main

    root = tmp_path / "corpus"
    root.mkdir()
    target = root / "Company" / "4_Manufacturing" / "SEA ANCHOR Singapore Company.txt"
    target.parent.mkdir(parents=True)
    target.write_text("manufacturing company Date Incorporation after 2015", encoding="utf-8")
    (root / "notes.txt").write_text("unrelated", encoding="utf-8")

    assert main(["prepare", str(root), "--json"]) == 0
    capsys.readouterr()
    eval_set = tmp_path / "external_eval.jsonl"
    eval_set.write_text(json.dumps({
        "id": "case-1",
        "scenario": "company",
        "query": "find the manufacturing company incorporation record",
        "expected_paths": ["Company/4_Manufacturing/SEA ANCHOR Singapore Company.txt"],
    }) + "\n", encoding="utf-8")

    out = tmp_path / "loop.json"
    assert main([
        "bench-iterate",
        str(root),
        "--eval-set",
        str(eval_set),
        "--iterations",
        "3",
        "--out",
        str(out),
        "--json",
    ]) == 0
    payload = json.loads(capsys.readouterr().out)
    assert payload["iterations"] == 3
    report = json.loads(out.read_text(encoding="utf-8"))
    assert report["completed_iterations"] == 3


def test_prepare_searches_eml_ics_sqlite_and_epub_body_text(tmp_path, capsys):
    import sqlite3
    import zipfile

    from jikji.__main__ import main

    eml = tmp_path / "mail.eml"
    eml.write_text(
        "Subject: Alpha Project Handoff\n"
        "From: sender@example.com\n"
        "To: receiver@example.com\n"
        "Content-Type: text/plain; charset=utf-8\n\n"
        "The launch code marker is emailtoken-7742.",
        encoding="utf-8",
    )

    ics = tmp_path / "calendar.ics"
    ics.write_text(
        "BEGIN:VCALENDAR\n"
        "BEGIN:VEVENT\n"
        "SUMMARY:Design sync uniquecalendar991\n"
        "DTSTART:20260526T090000Z\n"
        "LOCATION:Seoul lab\n"
        "DESCRIPTION:Calendar body marker\n"
        "END:VEVENT\n"
        "END:VCALENDAR\n",
        encoding="utf-8",
    )

    db = tmp_path / "notes.sqlite"
    con = sqlite3.connect(db)
    con.execute("CREATE TABLE notes (id INTEGER PRIMARY KEY, title TEXT, body TEXT)")
    con.execute("INSERT INTO notes (title, body) VALUES (?, ?)", ("Research", "sqlitebodytoken-3301 inside row"))
    con.commit()
    con.close()

    epub = tmp_path / "book.epub"
    with zipfile.ZipFile(epub, "w") as zf:
        zf.writestr("mimetype", "application/epub+zip")
        zf.writestr("OEBPS/chapter1.xhtml", "<html><body><h1>Chapter</h1><p>epubtoken-8802 appears here.</p></body></html>")

    assert main(["prepare", str(tmp_path), "--json"]) == 0
    payload = json.loads(capsys.readouterr().out)
    assert payload["docs_parsed"] >= 4

    expected = {
        "emailtoken-7742": "mail.eml",
        "uniquecalendar991": "calendar.ics",
        "sqlitebodytoken-3301": "notes.sqlite",
        "epubtoken-8802": "book.epub",
    }
    for query, path in expected.items():
        assert main(["search", str(tmp_path), query, "--top-k", "1", "--json"]) == 0
        report = json.loads(capsys.readouterr().out)
        assert report["candidates"][0]["path"] == path

    rows = _jsonl(tmp_path / ".jikji" / "document_index.jsonl")
    by_path = {row["path"]: row for row in rows}
    for path in expected.values():
        assert by_path[path]["parser_required"] is True
        assert by_path[path]["parse_status"] == "success"
        assert (tmp_path / by_path[path]["text_cache_path"]).exists()


def test_archive_member_names_are_cached_and_searchable(tmp_path, capsys):
    import zipfile

    from jikji.__main__ import main

    archive = tmp_path / "bundle.zip"
    with zipfile.ZipFile(archive, "w") as zf:
        zf.writestr("nested/archive_lookup_marker_9123.txt", "body not extracted")

    assert main(["prepare", str(tmp_path), "--json"]) == 0
    capsys.readouterr()
    rows = _jsonl(tmp_path / ".jikji" / "document_index.jsonl")
    row = next(row for row in rows if row["path"] == "bundle.zip")
    assert row["parse_status"] == "archive_listing"
    assert "archive_lookup_marker_9123" in (tmp_path / row["text_cache_path"]).read_text(encoding="utf-8")

    assert main(["search", str(tmp_path), "archive_lookup_marker_9123", "--top-k", "1", "--json"]) == 0
    report = json.loads(capsys.readouterr().out)
    assert report["candidates"][0]["path"] == "bundle.zip"


def test_optional_media_parsers_do_not_require_external_tools(tmp_path):
    from jikji.parsers.media import parse_audio, parse_image

    png = tmp_path / "empty.png"
    png.write_bytes(b"not a real png")
    wav = tmp_path / "empty.wav"
    wav.write_bytes(b"not a real wav")

    assert isinstance(parse_image(png, 1000), str)
    assert isinstance(parse_audio(wav, 1000), str)


def test_structured_parser_recall_edge_cases(tmp_path):
    import sqlite3
    import zipfile

    from jikji.parsers.archive import _zip_member_name
    from jikji.parsers.structured import parse_eml, parse_ics, parse_sqlite

    eml = tmp_path / "multipart.eml"
    eml.write_text(
        "MIME-Version: 1.0\n"
        "Subject: Multipart\n"
        "Content-Type: multipart/alternative; boundary=abc\n\n"
        "--abc\nContent-Type: text/plain; charset=utf-8\n\nplainonlytoken\n"
        "--abc\nContent-Type: text/html; charset=utf-8\n\n<html><body>htmlonlytoken</body></html>\n"
        "--abc--\n",
        encoding="utf-8",
    )
    eml_text = parse_eml(eml, 4000)
    assert "plainonlytoken" in eml_text
    assert "htmlonlytoken" in eml_text

    ics = tmp_path / "links.ics"
    ics.write_text(
        "BEGIN:VCALENDAR\n"
        "X-WR-CALNAME:Roadmap calendar\n"
        "BEGIN:VEVENT\n"
        "SUMMARY:Link meeting\n"
        "URL:https://example.test/meeting-url-token\n"
        "COMMENT:commenttoken-ics\n"
        "END:VEVENT\n"
        "END:VCALENDAR\n",
        encoding="utf-8",
    )
    ics_text = parse_ics(ics, 4000)
    assert "meeting-url-token" in ics_text
    assert "commenttoken-ics" in ics_text
    assert "Roadmap calendar" in ics_text

    db = tmp_path / "loose.db"
    con = sqlite3.connect(db)
    con.execute("CREATE TABLE items (id INTEGER, note STRING, payload BLOB)")
    con.execute("INSERT INTO items VALUES (?, ?, ?)", (7, "stringaffinitytoken", b"blobtoken"))
    con.commit()
    con.close()
    db_text = parse_sqlite(db, 4000)
    assert "stringaffinitytoken" in db_text
    assert "blobtoken" not in db_text

    info = zipfile.ZipInfo("회의록.hwp".encode("cp949").decode("cp437"))
    info.flag_bits = 0
    assert _zip_member_name(info) == "회의록.hwp"


def test_image_ocr_uses_absolute_path_for_dash_prefixed_names(tmp_path, monkeypatch):
    from jikji.parsers.media import parse_image

    calls = tmp_path / "calls.txt"
    fake = tmp_path / "tesseract"
    fake.write_text(
        "#!/bin/sh\n"
        "printf '%s\n' \"$1\" > \"$JIKJI_FAKE_TESS_CALLS\"\n"
        "printf 'dash OCR token'\n",
        encoding="utf-8",
    )
    fake.chmod(0o755)
    monkeypatch.setenv("PATH", f"{tmp_path}:{os.environ.get('PATH', '')}")
    monkeypatch.setenv("JIKJI_FAKE_TESS_CALLS", str(calls))
    image = tmp_path / "-scan.png"
    image.write_bytes(b"not a real image; fake tesseract ignores it")

    parsed = parse_image(image, 1000)

    assert "dash OCR token" in parsed
    first_arg = calls.read_text(encoding="utf-8").strip()
    assert first_arg == str(image.resolve())
    assert not first_arg.startswith("-")
