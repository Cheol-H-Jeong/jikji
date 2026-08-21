use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use jikji_public_datasets::beir::{
    BeirFetchOptions, BeirMaterializeOptions, fetch_beir_dataset, materialize_beir_dataset,
};
use jikji_public_datasets::hippocamp::{
    HippoCampFetchOptions, HippoCampImportOptions, fetch_subset, import_eval_set,
};
use jikji_public_datasets::{DatasetError, ResourceFetcher, Result};
use tempfile::tempdir;
use zip::write::SimpleFileOptions;

#[derive(Default)]
struct FixtureFetcher {
    resources: BTreeMap<String, Vec<u8>>,
}
impl FixtureFetcher {
    fn with(mut self, key: &str, value: impl Into<Vec<u8>>) -> Self {
        self.resources.insert(key.into(), value.into());
        self
    }
}
impl ResourceFetcher for FixtureFetcher {
    fn fetch_to(&self, resource: &str, destination: &Path, max_bytes: u64) -> Result<u64> {
        let bytes = self
            .resources
            .get(resource)
            .ok_or_else(|| DatasetError::Invalid(format!("missing fixture: {resource}")))?;
        if bytes.len() as u64 > max_bytes {
            return Err(DatasetError::ByteLimit {
                resource: resource.into(),
                limit: max_bytes,
            });
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(destination, bytes)?;
        Ok(bytes.len() as u64)
    }
}

fn beir_zip(path: &Path, unsafe_entry: bool) {
    let file = File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let entries = if unsafe_entry {
        vec![("../escape", "bad")]
    } else {
        vec![
            (
                "toy/corpus.jsonl",
                "{\"_id\":\"doc/1\",\"title\":\"제목\",\"text\":\"본문\"}\n{\"_id\":\"doc2\",\"title\":\"Two\",\"text\":\"Text\"}\n",
            ),
            ("toy/queries.jsonl", "{\"_id\":\"1\",\"text\":\"질문\"}\n"),
            (
                "toy/qrels/test.tsv",
                "query-id\tcorpus-id\tscore\n1\tdoc/1\t1\n1\tdoc2\t0\n",
            ),
        ]
    };
    for (name, body) in entries {
        zip.start_file(name, options).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}

#[test]
fn beir_local_zip_fetches_and_materializes_jsonl() {
    let temp = tempdir().unwrap();
    let fixture = temp.path().join("fixture.zip");
    beir_zip(&fixture, false);
    let fetcher = FixtureFetcher::default().with("fixture.zip", fs::read(&fixture).unwrap());
    let mut fetch = BeirFetchOptions::new("toy", temp.path().join("out"));
    fetch.resource = Some("fixture.zip".into());
    let fetched = fetch_beir_dataset(&fetcher, &fetch).unwrap();
    let mut materialize = BeirMaterializeOptions::new("toy", temp.path().join("out"));
    materialize.source_dir = Some(fetched.source_dir);
    let result = materialize_beir_dataset(&materialize).unwrap();
    assert_eq!((result.documents, result.cases, result.qrels), (2, 1, 1));
    let row: serde_json::Value =
        serde_json::from_str(fs::read_to_string(result.eval_set_path).unwrap().trim()).unwrap();
    assert_eq!(row["expected_paths"], serde_json::json!(["docs/doc_1.md"]));
    assert!(result.corpus_root.join("docs/doc_1.md").is_file());
}

#[test]
fn beir_rejects_zip_traversal_and_cleans_partial_extract() {
    let temp = tempdir().unwrap();
    let fixture = temp.path().join("bad.zip");
    beir_zip(&fixture, true);
    let fetcher = FixtureFetcher::default().with("bad.zip", fs::read(fixture).unwrap());
    let mut options = BeirFetchOptions::new("toy", temp.path().join("out"));
    options.resource = Some("bad.zip".into());
    assert!(matches!(
        fetch_beir_dataset(&fetcher, &options),
        Err(DatasetError::UnsafePath(_))
    ));
    assert!(!temp.path().join("out/source/.toy.extracting").exists());
}

#[test]
fn hippocamp_local_manifest_fetches_with_limits_and_imports() {
    let temp = tempdir().unwrap();
    let annotation = r#"[{"question":"어디?","file_path":["notes/a.txt","../secret"],"QA_type":"Fact QA","evidence":[{"evidence_text":"근거"}],"answer":"답"},{"question":"missing","file_path":["none.txt"]}]"#;
    let tree = r#"[{"type":"file","path":"Adam/Subset/Adam_Subset/notes/a.txt","size":5},{"type":"file","path":"Adam/Subset/Adam_Subset/big.bin","size":99},{"type":"directory","path":"Adam/Subset/Adam_Subset/notes"}]"#;
    let fetcher = FixtureFetcher::default().with("annotation", annotation.as_bytes()).with("tree", tree.as_bytes()).with("https://huggingface.co/datasets/MMMem-org/HippoCamp/resolve/main/Adam/Subset/Adam_Subset/notes/a.txt", b"hello".as_slice());
    let mut options = HippoCampFetchOptions::new(temp.path());
    options.annotation_resource = Some("annotation".into());
    options.tree_resource = Some("tree".into());
    options.max_file_bytes = 200;
    options.max_files = 1;
    let fetched = fetch_subset(&fetcher, &options).unwrap();
    assert_eq!((fetched.files_downloaded, fetched.skipped), (1, 1));
    let mut import = HippoCampImportOptions::new(&fetched.root);
    import.annotation = Some(fetched.annotation_path);
    let result = import_eval_set(&import).unwrap();
    assert_eq!((result.cases, result.skipped_cases), (1, 1));
    assert_eq!(result.scenarios["hippocamp_fact_qa"], 1);
    let row: serde_json::Value =
        serde_json::from_str(fs::read_to_string(result.eval_set_path).unwrap().trim()).unwrap();
    assert_eq!(row["expected_paths"], serde_json::json!(["notes/a.txt"]));
}

#[test]
fn fetcher_byte_limit_is_enforced_before_materialization() {
    let temp = tempdir().unwrap();
    let fetcher = FixtureFetcher::default().with("large", b"12345".as_slice());
    let error = fetcher
        .fetch_to("large", &temp.path().join("file"), 4)
        .unwrap_err();
    assert!(matches!(error, DatasetError::ByteLimit { limit: 4, .. }));
    assert!(!temp.path().join("file").exists());
}
