use bolomoty::api::fs;
use bolomoty::api::tree_sitter::Lang;
use bolomoty::api::tree_sitter::py::Python;
use bolomoty::api::tree_sitter::rs::Rust;
use bolomoty::consolidate;
use bolomoty::error::BoloError;
use bolomoty::pretty;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

// ── CLI ─────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "bolo", version, about = "Parse codebases into dependency DAGs")]
struct Bolo {
    #[command(subcommand)]
    lang: LangCmd,
}

#[derive(Subcommand)]
enum LangCmd {
    /// Analyze Python source files
    Py(Args),
    /// Analyze Rust source files
    Rs(Args),
}

#[derive(Parser)]
struct Args {
    /// File or directory to analyze
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Output file (omit for stdout)
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Overwrite existing output
    #[arg(short, long)]
    force: bool,

    /// Include files ignored by .gitignore
    #[arg(long)]
    no_ignore: bool,

    /// Only scan immediate directory (not recursive)
    #[arg(long)]
    shallow: bool,

    /// Show file count and exit
    #[arg(long)]
    dry_run: bool,

    /// Number of parallel threads (0 = all cores)
    #[arg(short = 'j', long, default_value = "1")]
    jobs: usize,
}

// ── Entry Point ─────────────────────────────────────────────────────

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            pretty::error(&e.to_string());
            ExitCode::FAILURE
        }
    }
}

// ── Orchestrator ────────────────────────────────────────────────────

fn run() -> Result<(), BoloError> {
    let cli = Bolo::parse();

    let (lang, ext, args): (Box<dyn Lang + Sync>, &str, &Args) = match &cli.lang {
        LangCmd::Py(a) => (Box::new(Python), "py", a),
        LangCmd::Rs(a) => (Box::new(Rust), "rs", a),
    };

    fs::validate_path(&args.path)?;

    if args.dry_run {
        let files = fs::walk_dir(&args.path, ext, args.no_ignore)?;
        pretty::neutral(&format!("{} .{ext} files found", files.len()));
        return Ok(());
    }

    rayon::ThreadPoolBuilder::new()
        .num_threads(args.jobs)
        .build_global()
        .ok();

    let result = if args.shallow {
        consolidate::folder(&args.path, ext, args.no_ignore, &*lang)?
    } else {
        consolidate::recursive(&args.path, ext, args.no_ignore, &*lang)?
    };

    let json = serde_json::to_string_pretty(&result)?;

    match &args.output {
        Some(out) => {
            if out.exists() && !args.force {
                return Err(BoloError::Exists { path: out.clone() });
            }
            fs::write_file(out, &json, true)?;
            pretty::success(&format!(
                "{} files \u{2192} {} ({} bytes)",
                result.len(),
                out.display(),
                json.len()
            ));
        }
        None => println!("{json}"),
    }

    Ok(())
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn parse(args: &[&str]) -> Bolo {
        Bolo::try_parse_from(args).unwrap()
    }

    fn args(cli: &Bolo) -> &Args {
        match &cli.lang {
            LangCmd::Py(a) | LangCmd::Rs(a) => a,
        }
    }

    // ── Subcommand dispatch ──

    #[test]
    fn subcommand_py() {
        let cli = parse(&["bolo", "py"]);
        assert!(matches!(cli.lang, LangCmd::Py(_)));
    }

    #[test]
    fn subcommand_rs() {
        let cli = parse(&["bolo", "rs"]);
        assert!(matches!(cli.lang, LangCmd::Rs(_)));
    }

    #[test]
    fn missing_subcommand_errors() {
        assert!(Bolo::try_parse_from(["bolo"]).is_err());
    }

    #[test]
    fn invalid_subcommand_errors() {
        assert!(Bolo::try_parse_from(["bolo", "go"]).is_err());
    }

    // ── Defaults ──

    #[test]
    fn defaults() {
        let cli = parse(&["bolo", "py"]);
        let a = args(&cli);
        assert_eq!(a.path, PathBuf::from("."));
        assert!(a.output.is_none());
        assert!(!a.force);
        assert!(!a.no_ignore);
        assert!(!a.shallow);
        assert!(!a.dry_run);
        assert_eq!(a.jobs, 1);
    }

    // ── Path positional ──

    #[test]
    fn custom_path() {
        let cli = parse(&["bolo", "rs", "src/"]);
        assert_eq!(args(&cli).path, PathBuf::from("src/"));
    }

    // ── Output flag ──

    #[test]
    fn output_short() {
        let cli = parse(&["bolo", "py", "-o", "out.json"]);
        assert_eq!(args(&cli).output.as_deref(), Some(Path::new("out.json")));
    }

    #[test]
    fn output_long() {
        let cli = parse(&["bolo", "py", "--output", "dag.json"]);
        assert_eq!(args(&cli).output.as_deref(), Some(Path::new("dag.json")));
    }

    #[test]
    fn output_missing_value_errors() {
        assert!(Bolo::try_parse_from(["bolo", "py", "-o"]).is_err());
    }

    // ── Force flag ──

    #[test]
    fn force_short() {
        let cli = parse(&["bolo", "py", "-f"]);
        assert!(args(&cli).force);
    }

    #[test]
    fn force_long() {
        let cli = parse(&["bolo", "py", "--force"]);
        assert!(args(&cli).force);
    }

    // ── Boolean flags ──

    #[test]
    fn no_ignore() {
        let cli = parse(&["bolo", "py", "--no-ignore"]);
        assert!(args(&cli).no_ignore);
    }

    #[test]
    fn shallow() {
        let cli = parse(&["bolo", "rs", "--shallow"]);
        assert!(args(&cli).shallow);
    }

    #[test]
    fn dry_run() {
        let cli = parse(&["bolo", "rs", "--dry-run"]);
        assert!(args(&cli).dry_run);
    }

    // ── Jobs flag ──

    #[test]
    fn jobs_short() {
        let cli = parse(&["bolo", "py", "-j", "4"]);
        assert_eq!(args(&cli).jobs, 4);
    }

    #[test]
    fn jobs_long() {
        let cli = parse(&["bolo", "py", "--jobs", "8"]);
        assert_eq!(args(&cli).jobs, 8);
    }

    #[test]
    fn jobs_zero_means_all_cores() {
        let cli = parse(&["bolo", "py", "-j", "0"]);
        assert_eq!(args(&cli).jobs, 0);
    }

    #[test]
    fn jobs_missing_value_errors() {
        assert!(Bolo::try_parse_from(["bolo", "py", "-j"]).is_err());
    }

    #[test]
    fn jobs_negative_errors() {
        assert!(Bolo::try_parse_from(["bolo", "py", "-j", "-1"]).is_err());
    }

    // ── Flag stacking ──

    #[test]
    fn stack_short_flags() {
        let cli = parse(&["bolo", "py", "-f", "-o", "out.json", "-j", "2"]);
        let a = args(&cli);
        assert!(a.force);
        assert_eq!(a.output.as_deref(), Some(Path::new("out.json")));
        assert_eq!(a.jobs, 2);
    }

    #[test]
    fn stack_all_boolean_flags() {
        let cli = parse(&[
            "bolo",
            "rs",
            "--force",
            "--no-ignore",
            "--shallow",
            "--dry-run",
        ]);
        let a = args(&cli);
        assert!(a.force);
        assert!(a.no_ignore);
        assert!(a.shallow);
        assert!(a.dry_run);
    }

    #[test]
    fn all_flags_with_path() {
        let cli = parse(&[
            "bolo",
            "py",
            "src/",
            "-o",
            "dag.json",
            "-f",
            "--no-ignore",
            "--shallow",
            "--dry-run",
            "-j",
            "4",
        ]);
        let a = args(&cli);
        assert_eq!(a.path, PathBuf::from("src/"));
        assert_eq!(a.output.as_deref(), Some(Path::new("dag.json")));
        assert!(a.force);
        assert!(a.no_ignore);
        assert!(a.shallow);
        assert!(a.dry_run);
        assert_eq!(a.jobs, 4);
    }

    #[test]
    fn flags_before_path() {
        let cli = parse(&["bolo", "rs", "-f", "--shallow", "lib/"]);
        let a = args(&cli);
        assert!(a.force);
        assert!(a.shallow);
        assert_eq!(a.path, PathBuf::from("lib/"));
    }

    #[test]
    fn flags_mixed_around_path() {
        let cli = parse(&["bolo", "py", "-f", "src/", "--no-ignore", "-j", "2"]);
        let a = args(&cli);
        assert!(a.force);
        assert!(a.no_ignore);
        assert_eq!(a.path, PathBuf::from("src/"));
        assert_eq!(a.jobs, 2);
    }
}
