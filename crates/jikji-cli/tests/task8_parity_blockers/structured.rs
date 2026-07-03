use std::fs;

use super::support::{
    assert_has_keys, document_row, first_candidate_path, json_cmd, json_file, jsonl, root_arg,
    temp_root, write_epub_fixture, write_png, write_sqlite_fixture, write_zip_fixture,
};

#[test]
fn structured_document_body_search_matches_python_contract() {
    let root = temp_root("task8-structured");
    fs::write(
        root.join("mail.eml"),
        "Subject: Alpha Project Handoff\n\
         From: sender@example.com\n\
         To: receiver@example.com\n\
         Content-Type: text/plain; charset=utf-8\n\n\
         The launch code marker is emailtoken-7742.",
    )
    .expect("write eml");
    fs::write(
        root.join("calendar.ics"),
        "BEGIN:VCALENDAR\n\
         BEGIN:VEVENT\n\
         SUMMARY:Design sync uniquecalendar991\n\
         DTSTART:20260526T090000Z\n\
         LOCATION:Seoul lab\n\
         DESCRIPTION:Calendar body marker\n\
         END:VEVENT\n\
         END:VCALENDAR\n",
    )
    .expect("write ics");
    write_sqlite_fixture(&root.join("notes.sqlite"));
    write_epub_fixture(&root.join("book.epub"));
    write_zip_fixture(&root.join("bundle.zip"));

    let root_arg = root_arg(&root);
    let prepared = json_cmd(&["prepare", &root_arg, "--json"]);
    assert_eq!(prepared["docs_parsed"], 5);

    for (query, expected_path) in [
        ("emailtoken-7742", "mail.eml"),
        ("uniquecalendar991", "calendar.ics"),
        ("sqlitebodytoken-3301", "notes.sqlite"),
        ("epubtoken-8802", "book.epub"),
        ("archive_lookup_marker_9123", "bundle.zip"),
    ] {
        let report = json_cmd(&["search", &root_arg, query, "--top-k", "1", "--json"]);
        assert_eq!(
            first_candidate_path(&report),
            expected_path,
            "query={query}"
        );
    }
}

#[test]
fn image_metadata_search_matches_python_contract_without_media_bridge() {
    let root = temp_root("task8-media");
    write_png(&root.join("visual.png"), 13, 21);

    let root_arg = root_arg(&root);
    json_cmd(&["prepare", &root_arg, "--json"]);

    let row = document_row(&root, "visual.png");
    assert_eq!(row["parse_status"], "success");
    let text_cache = fs::read_to_string(root.join(row["text_cache_path"].as_str().expect("cache")))
        .expect("read text cache");
    assert!(text_cache.contains("Dimensions: 13x21 pixels"));

    let report = json_cmd(&[
        "search",
        &root_arg,
        "13x21 pixels",
        "--top-k",
        "1",
        "--json",
    ]);
    assert_eq!(first_candidate_path(&report), "visual.png");
}

#[test]
fn generated_artifacts_expose_required_python_contract_fields() {
    let root = temp_root("task8-contract-fields");
    fs::create_dir(root.join("docs")).expect("create docs");
    fs::write(root.join("docs").join("notes.txt"), "alpha contract").expect("write text");
    write_png(&root.join("visual.png"), 13, 21);

    let root_arg = root_arg(&root);
    json_cmd(&["prepare", &root_arg, "--json"]);

    let manifest = json_file(root.join(".jikji/manifest.json"));
    assert_has_keys(
        &manifest,
        &[
            "native_text_extensions",
            "parse_errors",
            "parser_required_extensions",
        ],
    );
    assert_file_index_contract(&root);
    assert_folder_index_contract(&root);

    for row in jsonl(root.join(".jikji/document_index.jsonl")) {
        assert_has_keys(&row, &["file_id"]);
    }
}

fn assert_file_index_contract(root: &std::path::Path) {
    for row in jsonl(root.join(".jikji/file_index.jsonl")) {
        assert_has_keys(
            &row,
            &[
                "created",
                "indexed_at",
                "keywords",
                "mime",
                "modified",
                "mtime",
            ],
        );
        if row["path"] == "visual.png" {
            assert_has_keys(
                &row,
                &[
                    "doc_meta_path",
                    "parse_status",
                    "parser_required",
                    "sha256",
                    "summary",
                    "text_cache_path",
                ],
            );
            let cache_path = row["text_cache_path"].as_str().expect("text cache path");
            let cache_text = fs::read_to_string(root.join(cache_path)).expect("read text cache");
            assert!(!cache_text.trim().is_empty());
        }
    }
}

fn assert_folder_index_contract(root: &std::path::Path) {
    for row in jsonl(root.join(".jikji/folder_index.jsonl")) {
        assert_has_keys(
            &row,
            &[
                "child_folders",
                "depth",
                "file_count_direct",
                "folder_id",
                "keywords",
                "subfolder_count_direct",
                "summary",
                "top_extensions_direct",
                "total_size_direct",
            ],
        );
    }
}
