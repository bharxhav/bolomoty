use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "bolo", version, about = "Parse codebases into dependency DAGs")]
pub struct Bolo {
    #[command(subcommand)]
    pub lang: LangCmd,
}

#[derive(Subcommand)]
pub enum LangCmd {
    /// Analyze Python source files
    Py(Args),
    /// Analyze Rust source files
    Rs(Args),
}

#[derive(Parser)]
pub struct Args {
    /// File or directory to analyze
    #[arg(default_value = ".")]
    pub path: PathBuf,

    /// Output file (omit for stdout)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Overwrite existing output
    #[arg(short, long)]
    pub force: bool,

    /// Include files ignored by .gitignore
    #[arg(long)]
    pub no_ignore: bool,

    /// Only scan immediate directory (not recursive)
    #[arg(long)]
    pub shallow: bool,

    /// Show file count and exit
    #[arg(long)]
    pub dry_run: bool,

    /// Number of parallel threads (0 = all cores)
    #[arg(short = 'j', long, default_value = "1")]
    pub jobs: usize,
}
