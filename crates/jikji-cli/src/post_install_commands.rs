use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use jikji_core::PrepareOptions;
use jikji_index::prepare;
use serde_json::{Value, json};

use crate::args::PostInstallPrepareArgs;
use crate::output::print_json;
use crate::post_install_background::start_background_post_install_prepare;
use crate::prepare_commands::normalize_max_files;

const COMMON_RELS: &[&str] = &[
    "Documents",
    "Downloads",
    "Desktop",
    "문서",
    "다운로드",
    "바탕화면",
    "데스크탑",
    "OneDrive/Documents",
    "OneDrive/문서",
    "Google Drive",
    "Dropbox",
    "iCloud Drive",
];

const DOCUMENT_EXTS: &[&str] = &[
    "pdf", "hwp", "hwpx", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "rtf", "odt", "ods", "odp",
];

pub(crate) struct PostInstallRequest {
    pub(crate) roots: Vec<PathBuf>,
    pub(crate) no_prepare: bool,
    pub(crate) foreground: bool,
    pub(crate) parse_timeout: f64,
    pub(crate) max_files: Option<usize>,
}

pub(crate) fn prepare_after_skill_install(
    request: PostInstallRequest,
) -> jikji_core::Result<Value> {
    if request.no_prepare {
        return Ok(json!({"mode": "disabled", "roots": []}));
    }
    let (selected_roots, selection) = if request.roots.is_empty() {
        select_default_roots()
    } else {
        let selected = dedupe_roots(request.roots.iter().cloned());
        (
            selected.clone(),
            json!({"source": "explicit_prepare_root", "common_roots": selected, "document_heavy_roots": []}),
        )
    };
    if selected_roots.is_empty() {
        return Ok(json!({"mode": "none", "roots": [], "selection": selection}));
    }
    if !request.foreground {
        return Ok(start_background_post_install_prepare(
            &request,
            &selected_roots,
            selection,
        ));
    }
    let mut prepared = Vec::new();
    for root in selected_roots {
        let result = prepare(
            &root,
            &PrepareOptions {
                max_files: normalize_max_files(request.max_files),
                parse_timeout_seconds: request.parse_timeout,
                ..PrepareOptions::default()
            },
        )?;
        jikji_agent::write_routing_blocks(&root)?;
        prepared.push(json!({
            "root": result.root,
            "ok": true,
            "files": result.files,
            "agent_map": result.agent_map,
        }));
    }
    Ok(json!({
        "mode": "foreground",
        "roots": prepared,
        "parse_timeout": request.parse_timeout,
        "selection": selection,
    }))
}

pub(crate) fn run_post_install_prepare(
    args: PostInstallPrepareArgs,
) -> jikji_core::Result<ExitCode> {
    let payload = prepare_after_skill_install(PostInstallRequest {
        roots: args.roots,
        no_prepare: false,
        foreground: true,
        parse_timeout: args.parse_timeout,
        max_files: args.max_files,
    })?;
    if args.json {
        print_json(&payload)?;
    } else {
        println!("{payload}");
    }
    Ok(ExitCode::SUCCESS)
}

fn select_default_roots() -> (Vec<PathBuf>, Value) {
    let home = post_install_home();
    let common_roots = common_roots(&home);
    let document_roots = document_heavy_roots(&home, &common_roots);
    let roots = dedupe_roots(common_roots.iter().chain(document_roots.iter()).cloned());
    (
        roots,
        json!({
            "source": "auto_common_and_document_roots",
            "home": home,
            "common_roots": common_roots,
            "document_heavy_roots": document_roots,
            "document_extensions": DOCUMENT_EXTS,
        }),
    )
}

pub(crate) fn post_install_home() -> PathBuf {
    std::env::var_os("JIKJI_POST_INSTALL_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("JIKJI_AGENT_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn common_roots(home: &Path) -> Vec<PathBuf> {
    dedupe_roots(
        COMMON_RELS
            .iter()
            .map(|rel| home.join(rel))
            .filter(|path| path.is_dir()),
    )
}

fn document_heavy_roots(home: &Path, covered_roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut context = ScanContext {
        covered_roots,
        roots: &mut roots,
    };
    scan_document_heavy_dirs(home, &mut context, &mut ScanBudget::default());
    dedupe_roots(roots)
}

struct ScanContext<'a> {
    covered_roots: &'a [PathBuf],
    roots: &'a mut Vec<PathBuf>,
}

#[derive(Default)]
struct ScanBudget {
    dirs_seen: usize,
    files_seen: usize,
}

fn scan_document_heavy_dirs(dir: &Path, context: &mut ScanContext<'_>, budget: &mut ScanBudget) {
    if budget.dirs_seen > 4_000
        || budget.files_seen > 60_000
        || is_under_any(dir, context.covered_roots)
    {
        return;
    }
    budget.dirs_seen += 1;
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut document_files = 0usize;
    let mut child_dirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if !name.starts_with('.') && name != "node_modules" && name != ".jikji" {
                child_dirs.push(path);
            }
            continue;
        }
        budget.files_seen += 1;
        let ext = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if DOCUMENT_EXTS.contains(&ext.to_ascii_lowercase().as_str()) {
            document_files += 1;
        }
    }
    if document_files >= 3 {
        context.roots.push(dir.to_path_buf());
    }
    for child in child_dirs {
        scan_document_heavy_dirs(&child, context, budget);
    }
}

fn dedupe_roots(roots: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut selected = Vec::new();
    for root in roots {
        if !root.is_dir() || is_under_any(&root, &selected) {
            continue;
        }
        selected.push(root);
        if selected.len() >= 5 {
            break;
        }
    }
    selected
}

fn is_under_any(path: &Path, roots: &[PathBuf]) -> bool {
    roots
        .iter()
        .any(|root| path == root || path.starts_with(root))
}
