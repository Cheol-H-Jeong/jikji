#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};

use jikji_core::{Result, io_error};
use serde::Serialize;

mod routing;

pub use routing::{
    AGENT_RULE_FILES, AGENT_RULES_BEGIN, AGENT_RULES_END, remove_routing_block,
    remove_routing_blocks, write_routing_block, write_routing_blocks,
};

pub const SKILL_MARKDOWN: &str = include_str!("../assets/jikji/SKILL.md");

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentSkillInstallResult {
    pub agent: String,
    pub path: PathBuf,
    pub installed: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentRoute {
    pub command: String,
    pub description: String,
}

pub fn default_agent_routes() -> Vec<AgentRoute> {
    vec![
        AgentRoute {
            command: "jikji prepare".to_owned(),
            description: "Create or refresh non-destructive local knowledge maps.".to_owned(),
        },
        AgentRoute {
            command: "jikji find".to_owned(),
            description: "Query an existing Jikji index from a local agent.".to_owned(),
        },
    ]
}

pub fn normalize_agent_name(agent: &str) -> String {
    let normalized = agent.trim().to_lowercase().replace('_', "-");
    match normalized.as_str() {
        "claudecode" | "claude-code" => "claude".to_owned(),
        "open-code" => "opencode".to_owned(),
        "openclone" | "open-clone" | "open-clo" => "openclo".to_owned(),
        "nano-clone" | "nano-clo" => "nanoclo".to_owned(),
        "any" | "universal" => "custom".to_owned(),
        other => other.to_owned(),
    }
}

pub fn expand_agent_selection(agent: &str) -> Result<Vec<String>> {
    let normalized = normalize_agent_name(agent);
    if normalized == "all" {
        return Ok([
            "hermes", "codex", "omx", "claude", "opencode", "openclo", "nanoclo", "generic",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect());
    }
    let _ = default_skill_dest(&normalized)?;
    Ok(vec![normalized])
}

pub fn default_skill_dest(agent: &str) -> Result<PathBuf> {
    let home = home_dir()?;
    let normalized = normalize_agent_name(agent);
    let path = match normalized.as_str() {
        "hermes" => home.join(".hermes/skills/productivity/jikji/SKILL.md"),
        "codex" | "omx" => home.join(".codex/skills/jikji/SKILL.md"),
        "claude" => home.join(".claude/skills/jikji/SKILL.md"),
        "opencode" => home.join(".config/opencode/skills/jikji/SKILL.md"),
        "openclo" => home.join(".openclo/skills/jikji/SKILL.md"),
        "nanoclo" => home.join(".nanoclo/skills/jikji/SKILL.md"),
        "generic" | "custom" => home.join(".local/share/agent-skills/jikji/SKILL.md"),
        _ => {
            return Err(invalid_input(format!(
                "Unknown agent {agent:?}. Known agents: hermes, codex, omx, claude, opencode, openclo, nanoclo, generic"
            )));
        }
    };
    Ok(path)
}

pub fn install_agent_skill(
    agent: &str,
    dest: Option<&Path>,
    force: bool,
) -> Result<AgentSkillInstallResult> {
    let normalized = normalize_agent_name(agent);
    let target = match dest {
        Some(path) => absolute_path(path)?,
        None => default_skill_dest(&normalized)?,
    };
    if target.exists() && !force {
        return Ok(AgentSkillInstallResult {
            agent: normalized,
            path: target,
            installed: false,
            message: "already exists; pass --force to overwrite".to_owned(),
        });
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|source| io_error(parent, source))?;
    }
    fs::write(&target, SKILL_MARKDOWN).map_err(|source| io_error(&target, source))?;
    Ok(AgentSkillInstallResult {
        agent: normalized,
        path: target,
        installed: true,
        message: "installed".to_owned(),
    })
}

pub fn skill_markdown() -> &'static str {
    SKILL_MARKDOWN
}

fn absolute_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    let cwd = env::current_dir().map_err(|source| io_error(".", source))?;
    Ok(cwd.join(path))
}

fn home_dir() -> Result<PathBuf> {
    home_dir_from_vars(
        env::var_os("JIKJI_AGENT_HOME").map(PathBuf::from),
        env::var_os("HOME").map(PathBuf::from),
        env::var_os("USERPROFILE").map(PathBuf::from),
    )
    .ok_or_else(|| invalid_input("HOME or USERPROFILE is not set"))
}

fn home_dir_from_vars(
    agent_home: Option<PathBuf>,
    home: Option<PathBuf>,
    user_profile: Option<PathBuf>,
) -> Option<PathBuf> {
    agent_home.or(home).or(user_profile)
}

fn invalid_input(message: impl Into<String>) -> jikji_core::JikjiError {
    io_error(
        "<agent>",
        Error::new(ErrorKind::InvalidInput, message.into()),
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{
        AGENT_RULES_BEGIN, AGENT_RULES_END, default_agent_routes, expand_agent_selection,
        home_dir_from_vars, install_agent_skill, remove_routing_block, skill_markdown,
        write_routing_block,
    };

    #[test]
    fn default_agent_routes_name_prepare_and_find_surfaces() {
        let routes = default_agent_routes();

        assert!(routes.iter().any(|route| route.command == "jikji prepare"));
        assert!(routes.iter().any(|route| route.command == "jikji find"));
    }

    #[test]
    fn install_agent_skill_is_idempotent_without_force() {
        let dir = tempfile::tempdir().expect("tempdir");
        let dest = dir.path().join("SKILL.md");

        let first = install_agent_skill("any", Some(&dest), false).expect("install");
        let second = install_agent_skill("any", Some(&dest), false).expect("skip");

        assert!(first.installed);
        assert!(!second.installed);
        assert!(
            fs::read_to_string(dest)
                .expect("skill")
                .contains("Jikji Find First")
        );
    }

    #[test]
    fn agent_selection_expands_all_aliases() {
        let agents = expand_agent_selection("all").expect("all");

        assert!(agents.contains(&"codex".to_owned()));
        assert!(agents.contains(&"claude".to_owned()));
        assert_eq!(
            expand_agent_selection("open-clo").expect("alias"),
            ["openclo"]
        );
    }

    #[test]
    fn home_dir_uses_userprofile_when_home_is_absent() {
        let home = home_dir_from_vars(None, None, Some("C:/Users/runneradmin".into()));

        assert_eq!(home, Some("C:/Users/runneradmin".into()));
    }

    #[test]
    fn routing_block_write_and_remove_are_idempotent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("AGENTS.md");
        fs::write(&path, "# Existing\n\nKeep this.\n").expect("seed");

        let first = write_routing_block(&path).expect("write");
        let second = write_routing_block(&path).expect("rewrite");
        let written = fs::read_to_string(&path).expect("read");

        assert!(first.changed);
        assert!(!second.changed);
        assert_eq!(written.matches(AGENT_RULES_BEGIN).count(), 1);
        assert!(written.contains(AGENT_RULES_END));
        assert!(written.contains("Keep this."));

        let removed = remove_routing_block(&path).expect("remove");
        let final_text = fs::read_to_string(&path).expect("final read");

        assert!(removed.changed);
        assert!(!final_text.contains("JIKJI ROUTING"));
        assert!(final_text.contains("Keep this."));
    }

    #[test]
    fn embedded_skill_preserves_safety_contract() {
        let text = skill_markdown();

        assert!(text.contains("Never move, rename, delete, or reorganize"));
        assert!(text.contains("jikji find /explicit/root"));
    }
}
