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
    let payload = json!({
        "installed_any": results.iter().any(|item| item.installed),
        "results": results,
        "post_install_prepare": post_install_prepare,
        "after_install_protocol": "When this SKILL.md is in an agent's skill directory, local file/document discovery requests under an explicit root should trigger Jikji first: jikji find ROOT \"query\" --json, then follow handoff_action."
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
        let payload = json!({
            "path": result.path,
            "installed": result.installed,
            "message": result.message,
            "post_install_prepare": post_install_prepare,
            "usage": "Point any coding/local agent's skill loader at this SKILL.md, or paste it into that agent's persistent instructions."
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
            "usage": "Install this SKILL.md into any coding/local agent that supports Markdown skills or persistent prompt snippets."
        }))?;
    } else {
        print!("{}", skill_markdown());
    }
    Ok(ExitCode::SUCCESS)
}
