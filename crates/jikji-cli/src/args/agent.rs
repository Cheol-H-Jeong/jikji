use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
pub(crate) struct AgentSkillArgs {
    #[arg(long = "agent")]
    pub(crate) agents: Vec<String>,
    #[arg(long)]
    pub(crate) dest: Option<PathBuf>,
    #[arg(long = "prepare-root")]
    pub(crate) prepare_roots: Vec<PathBuf>,
    #[arg(long)]
    pub(crate) no_prepare: bool,
    #[arg(long)]
    pub(crate) foreground_prepare: bool,
    #[arg(long, default_value_t = 5.0)]
    pub(crate) parse_timeout: f64,
    #[arg(long)]
    pub(crate) max_files: Option<usize>,
    #[arg(long)]
    pub(crate) force: bool,
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct SkillAliasArgs {
    #[arg(long)]
    pub(crate) dest: Option<PathBuf>,
    #[arg(long = "prepare-root")]
    pub(crate) prepare_roots: Vec<PathBuf>,
    #[arg(long)]
    pub(crate) no_prepare: bool,
    #[arg(long)]
    pub(crate) foreground_prepare: bool,
    #[arg(long, default_value_t = 5.0)]
    pub(crate) parse_timeout: f64,
    #[arg(long)]
    pub(crate) max_files: Option<usize>,
    #[arg(long)]
    pub(crate) force: bool,
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct SkillExportArgs {
    #[arg(long)]
    pub(crate) dest: Option<PathBuf>,
    #[arg(long = "prepare-root")]
    pub(crate) prepare_roots: Vec<PathBuf>,
    #[arg(long)]
    pub(crate) no_prepare: bool,
    #[arg(long)]
    pub(crate) foreground_prepare: bool,
    #[arg(long, default_value_t = 5.0)]
    pub(crate) parse_timeout: f64,
    #[arg(long)]
    pub(crate) max_files: Option<usize>,
    #[arg(long)]
    pub(crate) force: bool,
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct PostInstallPrepareArgs {
    pub(crate) roots: Vec<PathBuf>,
    #[arg(long)]
    pub(crate) max_files: Option<usize>,
    #[arg(long, default_value_t = 5.0)]
    pub(crate) parse_timeout: f64,
    #[arg(long)]
    pub(crate) json: bool,
}
