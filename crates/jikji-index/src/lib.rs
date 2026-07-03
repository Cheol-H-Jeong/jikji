#![forbid(unsafe_code)]

mod artifact_rows;
mod artifact_writer;
mod artifacts;
mod clean;
mod clean_agent_rules;
mod clean_cache;
mod clean_targets;
mod doc_cache;
mod doc_chunks;
mod doc_media;
mod doc_prune;
mod doc_text_cache;
mod doctor;
mod file_io;
mod lock;
mod scan;

use std::path::PathBuf;

use jikji_core::{PrepareOptions, WorkspaceRoot, generated_artifact_manifest};

pub use artifacts::{PrepareResult, prepare};
pub use clean::{CleanOptions, CleanResult, clean};
pub use doctor::{DoctorReport, doctor, read_map};
pub use scan::{ScanResult, scan_root};

#[derive(Debug, Clone, PartialEq)]
pub struct PreparePlan {
    pub root: WorkspaceRoot,
    pub options: PrepareOptions,
    pub generated_artifact_count: usize,
}

pub fn plan_prepare(root: WorkspaceRoot, options: PrepareOptions) -> PreparePlan {
    let generated_artifact_count = generated_artifact_manifest().generated_artifacts.len();

    PreparePlan {
        root,
        options,
        generated_artifact_count,
    }
}

pub fn parser_registry_for_indexing() -> &'static str {
    "parser-owned-by-jikji-parser"
}

pub fn workspace_root(path: PathBuf) -> WorkspaceRoot {
    WorkspaceRoot::new(path)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use jikji_core::{GENERATED_ARTIFACTS, PrepareOptions, ROOT_AGENT_MAP, WorkspaceRoot};

    use super::plan_prepare;

    #[test]
    fn prepare_plan_tracks_generated_artifact_boundary_without_writing_files() {
        let root = WorkspaceRoot::new(PathBuf::from("/tmp/jikji-fixture"));
        let plan = plan_prepare(root, PrepareOptions::default());

        assert_eq!(plan.generated_artifact_count, GENERATED_ARTIFACTS.len());
        assert!(GENERATED_ARTIFACTS.contains(&ROOT_AGENT_MAP));
    }
}
