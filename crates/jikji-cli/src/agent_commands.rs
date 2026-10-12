use std::process::ExitCode;

use jikji_agent::{expand_agent_selection, install_agent_skill, skill_markdown};
use serde_json::json;

use crate::args::{AgentSkillArgs, SkillAliasArgs, SkillExportArgs};
use crate::output::print_json;
use crate::post_install_commands::{PostInstallRequest, prepare_after_skill_install};

pub(crate) fn run_agent_skill_install(args: AgentSkillArgs) -> jikji_core::Result<ExitCode> {
    let selected = if args.agents.is_empty() {
        if args.dest.is_some() {
            vec!["custom".to_owned()]
        } else {
            vec!["all".to_owned()]
        }
    } else {
        args.agents
    };
    let mut agents = Vec::new();
    for item in selected {
        agents.extend(expand_agent_selection(&item)?);
    }
    agents.sort();
    agents.dedup();
    if args.dest.is_some() && agents.len() > 1 {
        return Err(jikji_core::io_error(
            "<agent-skill-install>",
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "--dest can only be used with one --agent value",
            ),
        ));
    }
    let mut results = Vec::new();
    for agent in agents {
        results.push(install_agent_skill(
            &agent,
            args.dest.as_deref(),
            args.force,
        )?);
    }
    let post_install_prepare = prepare_after_skill_install(PostInstallRequest {
        roots: args.prepare_roots,
        no_prepare: args.no_prepare,
        foreground: args.foreground_prepare,
        parse_timeout: args.parse_timeout,
        max_files: args.max_files,
    })?;
    let discovery_routing = discovery_routing_contract();
    let payload = json!({
        "installed_any": results.iter().any(|item| item.installed),
        "results": results,
        "post_install_prepare": post_install_prepare,
        "after_install_protocol": "For local file/folder/document discovery with an unknown target path or corpus content under an explicit or inferred bounded root, call jikji find ROOT \\\"query\\\" --json first; do not use this route for general QA, explanations, coding advice, web search, known-path reads or edits, stats, process/git/build/test/package commands, directory listings, or LSP symbol lookup.",
        "discovery_routing": discovery_routing,
        "installed_skill_source": "embedded://skills/jikji/SKILL.md"
    });
    if args.json {
        print_json(&payload)?;
    } else {
        println!("{payload}");
    }
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn run_skill_alias(
    agent: &'static str,
    args: SkillAliasArgs,
) -> jikji_core::Result<ExitCode> {
    run_agent_skill_install(AgentSkillArgs {
        agents: vec![agent.to_owned()],
        dest: args.dest,
        prepare_roots: args.prepare_roots,
        no_prepare: args.no_prepare,
        foreground_prepare: args.foreground_prepare,
        parse_timeout: args.parse_timeout,
        max_files: args.max_files,
        force: args.force,
        json: args.json,
    })
}

pub(crate) fn run_skill_export(args: SkillExportArgs) -> jikji_core::Result<ExitCode> {
    if let Some(dest) = args.dest {
        let result = install_agent_skill("custom", Some(&dest), args.force)?;
        let post_install_prepare = prepare_after_skill_install(PostInstallRequest {
            roots: args.prepare_roots,
            no_prepare: args.no_prepare,
            foreground: args.foreground_prepare,
            parse_timeout: args.parse_timeout,
            max_files: args.max_files,
        })?;
        let discovery_routing = discovery_routing_contract();
        let payload = json!({
            "path": result.path,
            "installed": result.installed,
            "message": result.message,
            "post_install_prepare": post_install_prepare,
            "usage": "Point a local agent's skill loader at this SKILL.md; use Jikji only when the bounded local-discovery scope gate holds.",
            "discovery_routing": discovery_routing,
            "installed_skill_source": "embedded://skills/jikji/SKILL.md"
        });
        if args.json {
            print_json(&payload)?;
        } else {
            println!("{payload}");
        }
        return Ok(ExitCode::SUCCESS);
    }
    if args.json {
        print_json(&json!({
            "source": "embedded://skills/jikji/SKILL.md",
            "skill_markdown": skill_markdown(),
            "usage": "Install this identical SKILL.md into Hermes, Codex, Claude, OpenCode, or another Markdown skill loader.",
            "discovery_routing": discovery_routing_contract()
        }))?;
    } else {
        print!("{}", skill_markdown());
    }
    Ok(ExitCode::SUCCESS)
}

fn discovery_routing_contract() -> serde_json::Value {
    json!({
        "trigger": "local_file_folder_document_discovery_intent_with_unknown_target_or_corpus_content",
        "scope": "explicit_or_inferred_bounded_root",
        "all_conditions_required": true,
        "root_existence_alone_is_not_trigger": true,
        "file_or_search_words_alone_are_not_trigger": true,
        "mixed_task_scope": "discovery_substep_only",
        "retry_and_stop_scope": "discovery_only",
        "explicit_jikji_administration_allowed": true,
        "first_command": "jikji find ROOT \\\"query\\\" --json",
        "jikji_first": true,
        "excluded_intents": [
            "general_qa",
            "explanations",
            "coding_advice",
            "web_search",
            "known_path_read_or_edit",
            "stats",
            "process_commands",
            "git_commands",
            "build_test_package_commands",
            "directory_listing",
            "lsp_symbol_lookup"
        ],
        "success_raw_fallback_allowed": false,
        "failure_sequence": ["jikji_find", "exactly_one_jikji_retry", "bounded_raw_fallback_if_allowed"],
        "max_jikji_retries": 1,
        "raw_fallback_requires_handoff_action": "raw_fallback_after_retry",
        "required_response_fields": [
            "index_status",
            "handoff_action",
            "answerability",
            "retry_proof",
            "raw_fallback_allowed",
            "max_jikji_retries",
            "max_raw_fallback_commands",
            "tool_call_policy"
        ],
        "targets": ["hermes", "codex", "claude", "opencode"]
    })
}
