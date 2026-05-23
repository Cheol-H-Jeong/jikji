from __future__ import annotations

import argparse
import json
from pathlib import Path

from .agent_index import build_agent_index
from .config import Config
from .version import __version__


def _config_from_args(args) -> Config:
    cfg = Config()
    cfg.max_files = args.max_files
    cfg.include_hidden = args.include_hidden
    cfg.parse_timeout_s = args.parse_timeout
    cfg.agent_doc_text_max_chars = args.doc_text_max_chars
    cfg.agent_doc_text_chunk_chars = args.doc_text_chunk_chars
    return cfg


def cmd_prepare(args) -> int:
    root = Path(args.path).expanduser().resolve()
    cfg = _config_from_args(args)
    result = build_agent_index(root, cfg)
    if args.json:
        print(json.dumps({
            "root": str(root),
            "index_dir": str(result.index_dir),
            "agent_map": str(result.agent_map),
            "files": result.files,
            "folders": result.folders,
            "docs_parsed": result.docs_parsed,
            "docs_reused": result.docs_reused,
            "docs_failed": result.docs_failed,
            "deleted": result.deleted,
        }, ensure_ascii=False, indent=2))
    else:
        print(f"Jikji prepared: {root}")
        print(f"- files={result.files} folders={result.folders} deleted={result.deleted}")
        print(f"- docs parsed/reused/failed={result.docs_parsed}/{result.docs_reused}/{result.docs_failed}")
        print(f"- map={result.agent_map}")
    return 0


def cmd_map(args) -> int:
    root = Path(args.path).expanduser().resolve()
    for candidate in (root / "000_JIKJI_AGENT_MAP.md", root / ".jikji" / "agent_map.md"):
        if candidate.exists():
            print(candidate.read_text(encoding="utf-8", errors="ignore")[: args.max_chars])
            return 0
    print(f"No Jikji map found under {root}. Run: jikji prepare {root}")
    return 1


def cmd_doctor(args) -> int:
    root = Path(args.path).expanduser().resolve()
    ok = True
    checks = [
        root / ".jikji" / "manifest.json",
        root / ".jikji" / "file_index.jsonl",
        root / ".jikji" / "folder_index.jsonl",
        root / ".jikji" / "document_index.jsonl",
        root / "000_JIKJI_AGENT_MAP.md",
    ]
    for p in checks:
        exists = p.exists()
        ok = ok and exists
        print(("OK   " if exists else "MISS ") + str(p))
    return 0 if ok else 1


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="jikji", description="Prepare local files as agent-readable knowledge maps.")
    parser.add_argument("--version", action="version", version=f"jikji {__version__}")
    sub = parser.add_subparsers(dest="cmd")

    def add_common(p):
        p.add_argument("path", nargs="?", default=".")
        p.add_argument("--max-files", type=int, default=5000)
        p.add_argument("--include-hidden", action="store_true")
        p.add_argument("--parse-timeout", type=float, default=5.0)
        p.add_argument("--doc-text-max-chars", type=int, default=2_000_000)
        p.add_argument("--doc-text-chunk-chars", type=int, default=1_000_000)
        p.add_argument("--json", action="store_true")

    p_prepare = sub.add_parser("prepare", help="create/update .jikji without moving files")
    add_common(p_prepare)
    p_prepare.set_defaults(func=cmd_prepare)

    p_refresh = sub.add_parser("refresh", help="alias for prepare")
    add_common(p_refresh)
    p_refresh.set_defaults(func=cmd_prepare)

    p_map = sub.add_parser("map", help="print the generated Jikji map")
    p_map.add_argument("path", nargs="?", default=".")
    p_map.add_argument("--max-chars", type=int, default=12_000)
    p_map.set_defaults(func=cmd_map)

    p_doctor = sub.add_parser("doctor", help="check whether a folder has Jikji artifacts")
    p_doctor.add_argument("path", nargs="?", default=".")
    p_doctor.set_defaults(func=cmd_doctor)

    args = parser.parse_args(argv)
    if args.cmd is None:
        # Default to safe prepare for agent-skill ergonomics.
        args = parser.parse_args(["prepare", *(argv or [])])
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
