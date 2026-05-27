from jikji.eval import _is_duplicate_query, _query_filename_anchors, _quoted_query_terms


def test_duplicate_query_does_not_trigger_on_backup_folder_context():
    query = "`USB백업_20260416` 폴더 맥락에서 ‘과업대비표’ 단서가 있는 한글 문서 파일을 찾아줘."
    assert not _is_duplicate_query(query)


def test_duplicate_query_triggers_on_same_content_copy_request():
    query = "‘0x0412’와 내용이 같은 사본이나 백업 파일들을 찾아줘."
    assert _is_duplicate_query(query)


def test_long_quoted_filename_anchor_is_preserved_and_compacted():
    long_name = "제출서류 서식1_연구개발계획서(신청서)_미디어팔레트시장가치창출형기술개발"
    query = f"‘{long_name} ’와 내용이 같은 사본이나 백업 파일들을 찾아줘."
    assert long_name.casefold() in _quoted_query_terms(query)
    anchors = _query_filename_anchors(query)
    assert "제출서류서식1연구개발계획서신청서미디어팔레트시장가치창출형기술개발" in anchors
