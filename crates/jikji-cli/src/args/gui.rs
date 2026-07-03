use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
pub(crate) struct GuiArgs {
    #[arg(default_value = ".")]
    pub(crate) root: PathBuf,
    #[arg(long, default_value = "127.0.0.1")]
    pub(crate) host: String,
    #[arg(long, default_value_t = 8765)]
    pub(crate) port: u16,
    #[arg(long)]
    pub(crate) no_open: bool,
    #[arg(long)]
    pub(crate) prepare: bool,
    #[arg(long)]
    pub(crate) background: bool,
    #[arg(long)]
    pub(crate) json: bool,
    #[arg(long, hide = true)]
    pub(crate) serve_child: bool,
    #[arg(long, hide = true)]
    pub(crate) manage_token: Option<String>,
}
