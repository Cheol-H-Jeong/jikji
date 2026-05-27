# Jikji Schema Reference

## manifest.json

Required fields include:

- `schema_version`
- `generated_at`
- `root`
- `files`
- `folders`
- `documents`
- `docs_parsed`
- `docs_reused`
- `docs_failed`
- `parse_errors`
- `deleted_since_last_index`
- `mode`
- `non_destructive`
- `cache_key_policy`
- `owned_paths`
- `retired_cleanup_paths`
- `parser_required_extensions`
- `native_text_extensions`

## file_index.jsonl

One JSON object per row:

- `status`: `present` or `deleted`
- `path`, `name`, `ext`, `mime`
- `size`, `mtime`, `mtime_ns`, `created`, `modified`
- `sha256`
- `parser_required`
- `parse_status`
- `text_cache_path`
- `doc_meta_path`
- `keywords`: deterministic local tokens, no LLM/cloud
- `summary`: deterministic local excerpt/summary, no LLM/cloud
- `indexed_at`

## folder_index.jsonl

- `folder_id`
- `path`
- `name`
- `depth`
- `file_count_direct`
- `subfolder_count_direct`
- `total_size_direct`
- `top_extensions_direct`
- `child_folders`
- `keywords`: deterministic local tokens, no LLM/cloud
- `summary`: deterministic local folder count string, no LLM/cloud

## document_index.jsonl

Extends file rows with:

- `file_id`: `sha256:<hex>` when hash is available
- `text_cache_path`
- `doc_meta_path`
- `parse_status`

`parser_required=true` for:

```text
.pdf .doc .docx .ppt .pptx .pps .ppsx .xls .xlsx .hwp .hwpx .odt .rtf
```

## doc_meta/sha256_*.json

## search_index.sqlite

Generated SQLite accelerator for instant local search. It is not the source of
truth; it can be rebuilt from `file_cards.jsonl` and `chunk_map.jsonl`.

- `meta`: schema/version/counts
- `docs`: one JSON search row per file
- `terms`: prebuilt lexical inverted index
- `filename_keys`: compact filename lookup keys
- `idf`: deterministic term weights

Minimal rich metadata envelope:

- `schema_version`
- `file_id`
- `path`
- `title`
- `author`
- `subject`
- `created`
- `modified`
- `page_count`
- `source`
- `exif`
- `office`
- `parser`

## parse_errors.jsonl

Rows include:

- `path`
- `code`
- `stage`
- `error`

Known v0.2 codes:

- `access_denied`
- `hash_oversize`
- `parser_crashed`
- `parser_unsupported`
- `encrypted`
- `oversize`


## eval/eval_set.jsonl

Generated local search evaluation cases:

- `id`
- `scenario`
- `query`
- `expected_paths`
- `evidence`

Known scenarios:

- `filename_exact`
- `filename_partial`
- `lexical_content`
- `semantic_description`
- `file_description`

## eval/eval_report.json

Generated evaluation report:

- `root`
- `eval_set`
- `metrics`
- `details`

Metrics include `hit_at_1`, `hit_at_3`, `hit_at_5`, `mrr`, and `by_scenario`.
