use colored::Colorize;

pub fn error(msg: &str) {
    eprintln!("{} {msg}", "error:".red().bold());
}

pub fn warn(msg: &str) {
    eprintln!("{} {msg}", "warn:".yellow().bold());
}

pub fn success(msg: &str) {
    eprintln!("{} {msg}", "done:".green().bold());
}

pub fn neutral(msg: &str) {
    eprintln!("{msg}");
}
