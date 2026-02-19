use bolomoty::cli::Bolo;
use clap::CommandFactory;
use std::fs;
use std::path::PathBuf;

fn main() {
    let out_dir = std::env::var("MAN_OUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("target"));

    fs::create_dir_all(&out_dir).expect("failed to create output dir");

    let cmd = Bolo::command();
    let man = clap_mangen::Man::new(cmd);

    let path = out_dir.join("bolo.1");
    let mut buf = Vec::new();
    man.render(&mut buf).expect("failed to render man page");
    fs::write(&path, buf).expect("failed to write man page");

    println!("wrote {}", path.display());
}
