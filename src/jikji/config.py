"""Jikji configuration."""
from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class Config:
    """Runtime options for non-destructive local knowledge-map preparation."""

    ignore_patterns: list[str] = field(
        default_factory=lambda: [".*", "~$*", "Thumbs.db", ".DS_Store", "desktop.ini"]
    )
    include_hidden: bool = False
    max_files: int = 5000
    parse_timeout_s: float = 5.0
    agent_doc_text_max_chars: int = 2_000_000
    agent_doc_text_chunk_chars: int = 1_000_000
