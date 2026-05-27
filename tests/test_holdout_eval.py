from pathlib import Path

from jikji.holdout_eval import generate_holdout_eval_set


def test_holdout_generator_is_scorer_blind_by_import_text():
    source = Path("src/jikji/holdout_eval.py").read_text(encoding="utf-8")
    assert "from .eval" not in source
    assert "import jikji.eval" not in source
    for forbidden in (
        "_filename_lookup_keys",
        "_query_filename_anchors",
        "_score_map",
        "search_with_index",
    ):
        assert forbidden not in source


def test_holdout_generation_writes_checksum_and_case_hashes(tmp_path):
    root = tmp_path
    (root / ".jikji").mkdir()
    cards = root / ".jikji" / "file_cards.jsonl"
    cards.write_text(
        '\n'.join([
            '{"path":"alpha/report-one.txt","name":"report-one.txt","ext":".txt","content_terms":["apollo","telemetry","rendezvous"],"rare_terms":["uniquealpha"],"phrase_signatures":["apollo telemetry rendezvous"],"evidence_previews":["apollo telemetry rendezvous checklist"]}',
            '{"path":"alpha/report-one (1).txt","name":"report-one (1).txt","ext":".txt","content_terms":["apollo","telemetry","rendezvous"],"rare_terms":["uniquealpha"],"phrase_signatures":["apollo telemetry rendezvous"],"evidence_previews":["apollo telemetry rendezvous checklist copy"]}',
            '{"path":"beta/budget.xlsx","name":"budget.xlsx","ext":".xlsx","content_terms":["budget","forecast","margin"],"rare_terms":["uniquebeta"],"phrase_signatures":["budget forecast margin"],"evidence_previews":["budget forecast margin sheet"]}',
        ])
        + '\n',
        encoding="utf-8",
    )
    (root / ".jikji" / "duplicate_map.jsonl").write_text(
        '{"group_id":"near_demo","representative":"alpha/report-one.txt","members":["alpha/report-one.txt","alpha/report-one (1).txt"]}\n',
        encoding="utf-8",
    )
    out = root / "holdout.jsonl"
    result = generate_holdout_eval_set(root, max_cases=8, out=out)
    assert result.cases > 0
    assert result.checksum
    assert result.profile_path.exists()
    profile = result.profile_path.read_text(encoding="utf-8")
    assert "do_not_inspect_cases_while_tuning" in profile
    assert "case_sha256_manifest" in profile
