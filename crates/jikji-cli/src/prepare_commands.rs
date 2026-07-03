use std::process::ExitCode;

use jikji_core::PrepareOptions;
use jikji_index::{CleanOptions, clean, prepare};

use crate::args::{CleanArgs, PrepareArgs};
use crate::output::print_json;

pub(crate) fn run_prepare(args: PrepareArgs) -> jikji_core::Result<ExitCode> {
    let options = prepare_options_from_args(&args);
    let result = prepare(&args.root, &options)?;
    if !args.no_agent_rules {
        jikji_agent::write_routing_blocks(&args.root)?;
    }
    if args.json {
        print_json(&result)?;
    } else {
        println!("Jikji prepared: {}", result.root.display());
        println!(
            "- files={} folders={} deleted={}",
            result.files, result.folders, result.deleted
        );
        println!("- map={}", result.agent_map.display());
    }
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn prepare_options_from_args(args: &PrepareArgs) -> PrepareOptions {
    PrepareOptions {
        include_hidden: args.include_hidden,
        include_sensitive: args.include_sensitive,
        max_files: normalize_max_files(args.max_files),
        exclude_patterns: args.exclude.clone(),
        max_hash_bytes: args.max_hash_bytes,
        parse_timeout_seconds: args.parse_timeout,
        doc_text_max_chars: args.doc_text_max_chars,
        doc_text_chunk_chars: args.doc_text_chunk_chars,
        enable_media_index: args.enable_media_index,
        media_index_max_mb: args.media_index_max_mb,
    }
}

pub(crate) fn normalize_max_files(max_files: Option<usize>) -> Option<usize> {
    max_files.filter(|limit| *limit > 0)
}

pub(crate) fn run_clean(args: CleanArgs) -> jikji_core::Result<ExitCode> {
    let result = clean(
        &args.root,
        CleanOptions {
            dry_run: args.dry_run,
            force: args.force,
        },
    )?;
    if args.json {
        print_json(&result)?;
    } else if result.ok {
        let label = if result.dry_run {
            "WOULD_REMOVE"
        } else {
            "REMOVED"
        };
        let paths = if result.dry_run {
            &result.would_remove
        } else {
            &result.removed
        };
        for path in paths {
            println!("{label} {}", path.display());
        }
    } else if let Some(error) = &result.error {
        eprintln!("{error}");
    }
    Ok(if result.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}
