from __future__ import annotations

import shlex

from jikji.agent_index import build_agent_index
from jikji.config import Config


def test_discover_ignores_shell_noise_terms(tmp_path):
    from jikji.discover import discover

    (tmp_path / "media").mkdir()
    (tmp_path / "media" / "photo.jpg").write_bytes(b"fake jpg metadata rm token")
    build_agent_index(tmp_path, Config())

    payload = discover(tmp_path, 'zzzznohit "semi; rm -rf /" $(echo nope)', top_k=3)

    assert payload["confidence"] == "low"
    assert payload["handoff_policy"]["use_payload_directly"] is False
    assert payload["handoff_action"] == "jikji_retry"
    assert payload["answer_pack_version"] == 1
    assert payload["answerability"] == "needs_one_jikji_retry"
    assert payload["allowed_agent_tool_calls"] == 1
    assert payload["allowed_llm_calls"] == 1
    assert payload["max_jikji_retries"] == 1
    assert payload["max_raw_fallback_commands"] == 0
    assert payload["max_verification_reads"] == 0
    assert payload["raw_fallback_allowed"] is False
    assert payload["paths"] == []
    assert "rm" not in " ".join(payload["query_variants"]).casefold()
    assert payload["next_commands"]
    assert payload["retry_proof"]
    forged = discover(tmp_path, 'zzzznohit "semi; rm -rf /" $(echo nope)', top_k=3, retry_exhausted=True)
    assert forged["handoff_action"] == "jikji_retry"
    assert forged["raw_fallback_allowed"] is False
    exhausted = discover(
        tmp_path,
        'zzzznohit "semi; rm -rf /" $(echo nope)',
        top_k=3,
        retry_exhausted=True,
        retry_proof=payload["retry_proof"],
    )
    assert exhausted["handoff_action"] == "raw_fallback_after_retry"
    assert exhausted["answerability"] == "needs_raw_fallback_after_retry"
    assert exhausted["max_raw_fallback_commands"] == 2
    assert exhausted["raw_fallback_allowed"] is True
    assert exhausted["next_commands"] == []


def test_discover_all_shell_noise_does_not_search_raw_tokens(tmp_path):
    from jikji.discover import discover

    (tmp_path / "notes.txt").write_text("rm grep ls bash", encoding="utf-8")
    build_agent_index(tmp_path, Config())

    payload = discover(tmp_path, "rm -rf /", top_k=3)

    assert payload["handoff_action"] == "jikji_retry"
    assert payload["paths"] == []
    assert payload["query_variants"] == [""]
    assert payload["answer_paths"] == []
    assert payload["requires_llm_rerank"] is True
    assert payload["allowed_llm_calls"] == 1


def test_discover_excludes_generated_artifacts_from_answer_paths(tmp_path):
    from jikji.discover import discover

    (tmp_path / ".jikji" / "doc_text").mkdir(parents=True)
    (tmp_path / ".jikji" / "doc_text" / "sha256_fake.txt").write_text(
        "Ferry Building location marker", encoding="utf-8"
    )
    (tmp_path / "Guidebook").mkdir()
    (tmp_path / "Guidebook" / "san-francisco-11-contents.txt").write_text(
        "Ferry Building location marker", encoding="utf-8"
    )
    build_agent_index(tmp_path, Config())

    payload = discover(tmp_path, "Where is the Ferry Building located?", top_k=5)

    assert payload["paths"] == payload["answer_paths"]
    assert payload["paths"]
    assert all(not path.startswith(".jikji/") for path in payload["paths"])
    assert "Guidebook/san-francisco-11-contents.txt" in payload["paths"]


def test_discover_promotes_company_year_path_anchors(tmp_path):
    from jikji.discover import discover

    (tmp_path / "financebench").mkdir()
    for name in ["PFIZER_2015_10K.pdf", "NETFLIX_2015_10K.pdf", "NETFLIX_2017_10K.pdf"]:
        (tmp_path / "financebench" / name).write_text(
            "statement of income cash flows unadjusted EBITDA margin",
            encoding="utf-8",
        )
    build_agent_index(tmp_path, Config())

    payload = discover(tmp_path, "What is the FY2015 unadjusted EBITDA margin for Netflix?", top_k=5)

    assert payload["answer_paths"][0] == "financebench/NETFLIX_2015_10K.pdf"


def test_discover_medium_high_single_file_keeps_small_candidate_set(tmp_path):
    from jikji.answer_pack import answer_pack_for

    payload = answer_pack_for(
        "single_file",
        "medium_high",
        [{"path": "first.txt"}, {"path": "second.txt"}, {"path": "third.txt"}, {"path": "fourth.txt"}],
    )

    assert payload["answer_paths"] == ["first.txt", "second.txt", "third.txt"]
    assert payload["supporting_paths"] == ["fourth.txt"]
    assert payload["requires_llm_rerank"] is True
    assert payload["allowed_llm_calls"] == 1


def test_answer_pack_filters_nested_generated_paths_and_bounds_evidence(tmp_path):
    from jikji.answer_pack import MAX_EVIDENCE_CHARS, answer_pack_for

    payload = answer_pack_for(
        "single_file",
        "high",
        [
            {"path": "/tmp/root/.jikji/doc_text/cache.txt", "evidence": ["x" * 1000]},
            {"path": "nested/.jikji/doc_text/cache.txt", "evidence": ["y" * 1000]},
            {"path": "docs/source.txt", "evidence": ["z" * 1000]},
        ],
    )

    assert payload["answer_paths"] == ["docs/source.txt"]
    assert payload["evidence_pack"][0]["evidence"] == ["z" * MAX_EVIDENCE_CHARS]


def test_answer_pack_accepts_compact_candidate_path_key(tmp_path):
    from jikji.answer_pack import answer_pack_for

    payload = answer_pack_for(
        "single_file",
        "high",
        [{"p": "docs/source.txt", "evidence": ["compact path candidate"]}],
    )

    assert payload["answer_paths"] == ["docs/source.txt"]
    assert payload["evidence_pack"][0]["path"] == "docs/source.txt"


def test_discover_retry_proof_matches_next_command_query(tmp_path):
    from jikji.discover import discover

    (tmp_path / "notes.txt").write_text("ordinary known content", encoding="utf-8")
    build_agent_index(tmp_path, Config())

    for query in (
        "What is my primary sports interest?",
        "legal aid reports zzzznohit",
    ):
        payload = discover(tmp_path, query, top_k=3)
        assert payload["handoff_action"] == "jikji_retry"
        command = shlex.split(payload["next_commands"][0])
        retry_query = command[3]
        proof = command[8]
        assert proof == payload["retry_proof"]

        exhausted = discover(
            tmp_path,
            retry_query,
            top_k=3,
            retry_exhausted=True,
            retry_proof=proof,
        )

        assert exhausted["handoff_action"] == "raw_fallback_after_retry"
        assert exhausted["raw_fallback_allowed"] is True


def test_answer_pack_medium_high_rerank_budget_is_explicit(tmp_path):
    from jikji.answer_pack import answer_pack_for

    payload = answer_pack_for(
        "single_file",
        "medium_high",
        [{"path": "first.txt"}, {"path": "second.txt"}],
    )

    assert payload["requires_llm_rerank"] is True
    assert payload["allowed_llm_calls"] == 1


def test_query_variants_preserve_mixed_case_document_anchors():
    from jikji.discover import query_variants

    variants = query_variants(
        "A client asked if receiving confidential information under the eHandshake NDA "
        "gives them ownership rights."
    )

    anchor_variants = variants[1:]
    assert any("eHandshake" in variant for variant in anchor_variants)


def test_query_variants_do_not_turn_clock_times_into_year_anchors():
    from jikji.discover import query_variants

    variants = query_variants(
        "Please formulate a work plan from 12:00 noon to 5:00 p.m. on December 5, 2025."
    )

    synthetic_year_variant = " ".join(variants[1:])
    assert "2012" not in synthetic_year_variant
    assert "2000" not in synthetic_year_variant


def test_discover_exposes_one_call_multi_search_judge_contract(tmp_path):
    from jikji.discover import discover

    (tmp_path / "contracts").mkdir()
    (tmp_path / "contracts" / "ACME_master_services_agreement.txt").write_text(
        "ACME master services agreement renewal indemnity terms",
        encoding="utf-8",
    )
    (tmp_path / "notes.txt").write_text("ordinary meeting notes", encoding="utf-8")
    build_agent_index(tmp_path, Config())

    payload = discover(tmp_path, "Find the ACME master services agreement", top_k=3)

    assert payload["llm_search_plan"]["mode"] == "one_call_multi_search_judge"
    assert payload["llm_search_plan"]["calls_per_cycle"] == 1
    route_names = {route["route"] for route in payload["search_plan"]["routes"]}
    assert {"lexical_file_map", "graph_route"} <= route_names
    assert all(route["per_route_top_k"] >= 1 for route in payload["search_plan"]["routes"])
    assert payload["judge_candidate_slate"][0]["path"] == "contracts/ACME_master_services_agreement.txt"
    assert payload["allowed_llm_calls"] <= 1
