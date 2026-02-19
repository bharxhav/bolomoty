use crate::error::BoloError;
use ignore::WalkBuilder;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

// ── Output Type ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct File {
    /// Absolute, canonicalized path.
    pub path: PathBuf,
    /// Path relative to the walk root.
    pub rel_path: PathBuf,
}

impl File {
    /// Read the file contents into a string.
    pub fn read(&self) -> Result<String, BoloError> {
        fs::read_to_string(&self.path).map_err(|e| BoloError::Read {
            path: self.path.clone(),
            reason: e.to_string(),
        })
    }
}

// ── Validation ─────────────────────────────────────────────────────

pub fn validate_path(path: &Path) -> Result<(), BoloError> {
    let meta = fs::metadata(path).map_err(|e| BoloError::InvalidPath {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;
    if !meta.is_file() && !meta.is_dir() {
        return Err(BoloError::InvalidPath {
            path: path.to_path_buf(),
            reason: "not a file or directory".into(),
        });
    }
    Ok(())
}

// ── Discovery ──────────────────────────────────────────────────────

pub fn walk_dir(path: &Path, ext: &str, no_ignore: bool) -> Result<Vec<File>, BoloError> {
    let root = path.canonicalize().map_err(|e| BoloError::Walk {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;

    if root.is_file() {
        return if matches_ext(&root, ext) {
            Ok(vec![File {
                rel_path: PathBuf::from(root.file_name().unwrap()),
                path: root,
            }])
        } else {
            Err(BoloError::Walk {
                path: root,
                reason: format!("file does not have a .{ext} extension"),
            })
        };
    }

    let mut files = Vec::new();

    for entry in WalkBuilder::new(&root).git_ignore(!no_ignore).build() {
        let entry = entry.map_err(|e| BoloError::Walk {
            path: root.clone(),
            reason: e.to_string(),
        })?;

        let Some(ft) = entry.file_type() else {
            continue;
        };
        if !ft.is_file() {
            continue;
        }
        if !matches_ext(entry.path(), ext) {
            continue;
        }

        let abs = entry.into_path();
        let rel = abs.strip_prefix(&root).unwrap_or(&abs).to_path_buf();

        files.push(File {
            path: abs,
            rel_path: rel,
        });
    }

    files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    Ok(files)
}

fn matches_ext(path: &Path, ext: &str) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

// ── Output ─────────────────────────────────────────────────────────

pub fn ensure_dir(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)
}

pub fn write_file(path: &Path, content: &str, mkdir: bool) -> Result<(), BoloError> {
    let parent = match path.parent() {
        Some(p) => p,
        None => Path::new("."),
    };

    if mkdir {
        ensure_dir(parent).map_err(|e| BoloError::Write {
            path: path.to_path_buf(),
            reason: e.to_string(),
        })?;
    }

    let mut tmp = tempfile::NamedTempFile::new_in(parent).map_err(|e| BoloError::Write {
        path: path.to_path_buf(),
        reason: e.to_string(),
    })?;
    tmp.write_all(content.as_bytes())
        .map_err(|e| BoloError::Write {
            path: path.to_path_buf(),
            reason: e.to_string(),
        })?;
    tmp.persist(path).map_err(|e| BoloError::Write {
        path: path.to_path_buf(),
        reason: e.error.to_string(),
    })?;
    Ok(())
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ── validate_path ──

    #[test]
    fn validate_existing_file() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("test.py");
        fs::write(&file, "").unwrap();
        assert!(validate_path(&file).is_ok());
    }

    #[test]
    fn validate_existing_dir() {
        let dir = TempDir::new().unwrap();
        assert!(validate_path(dir.path()).is_ok());
    }

    #[test]
    fn validate_missing_path() {
        let dir = TempDir::new().unwrap();
        let err = validate_path(&dir.path().join("nope")).unwrap_err();
        assert!(matches!(err, BoloError::InvalidPath { .. }));
    }

    // ── walk_dir ──

    #[test]
    fn walk_filters_by_extension() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.py"), "").unwrap();
        fs::write(dir.path().join("b.rs"), "").unwrap();
        fs::write(dir.path().join("c.py"), "").unwrap();

        let files = walk_dir(dir.path(), "py", false).unwrap();
        let names: Vec<_> = files.iter().map(|f| f.rel_path.to_str().unwrap()).collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"a.py"));
        assert!(names.contains(&"c.py"));
    }

    #[test]
    fn walk_recursive() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("sub/deep")).unwrap();
        fs::write(dir.path().join("top.py"), "").unwrap();
        fs::write(dir.path().join("sub/mid.py"), "").unwrap();
        fs::write(dir.path().join("sub/deep/bot.py"), "").unwrap();

        let files = walk_dir(dir.path(), "py", false).unwrap();
        assert_eq!(files.len(), 3);
    }

    #[test]
    fn walk_sorted_deterministic() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("z.py"), "").unwrap();
        fs::write(dir.path().join("a.py"), "").unwrap();
        fs::write(dir.path().join("m.py"), "").unwrap();

        let files = walk_dir(dir.path(), "py", false).unwrap();
        let names: Vec<_> = files.iter().map(|f| f.rel_path.clone()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    #[test]
    fn walk_empty_dir() {
        let dir = TempDir::new().unwrap();
        let files = walk_dir(dir.path(), "py", false).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn walk_single_file_matching() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("main.rs");
        fs::write(&file, "fn main() {}").unwrap();

        let files = walk_dir(&file, "rs", false).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].rel_path.to_str().unwrap(), "main.rs");
    }

    #[test]
    fn walk_single_file_wrong_ext() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("main.rs");
        fs::write(&file, "").unwrap();

        let err = walk_dir(&file, "py", false).unwrap_err();
        assert!(matches!(err, BoloError::Walk { .. }));
    }

    #[test]
    fn walk_respects_gitignore() {
        let dir = TempDir::new().unwrap();
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir.path())
            .status()
            .unwrap();

        fs::write(dir.path().join(".gitignore"), "ignored.py\n").unwrap();
        fs::write(dir.path().join("keep.py"), "").unwrap();
        fs::write(dir.path().join("ignored.py"), "").unwrap();

        let files = walk_dir(dir.path(), "py", false).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].rel_path.to_str().unwrap(), "keep.py");
    }

    #[test]
    fn walk_no_ignore_includes_gitignored() {
        let dir = TempDir::new().unwrap();
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir.path())
            .status()
            .unwrap();

        fs::write(dir.path().join(".gitignore"), "ignored.py\n").unwrap();
        fs::write(dir.path().join("keep.py"), "").unwrap();
        fs::write(dir.path().join("ignored.py"), "").unwrap();

        let files = walk_dir(dir.path(), "py", true).unwrap();
        assert_eq!(files.len(), 2);
    }

    // ── matches_ext ──

    #[test]
    fn ext_case_insensitive() {
        assert!(matches_ext(Path::new("file.PY"), "py"));
        assert!(matches_ext(Path::new("file.py"), "PY"));
        assert!(matches_ext(Path::new("file.Rs"), "rs"));
    }

    #[test]
    fn ext_no_extension() {
        assert!(!matches_ext(Path::new("Makefile"), "py"));
    }

    #[test]
    fn ext_wrong_extension() {
        assert!(!matches_ext(Path::new("file.rs"), "py"));
    }

    // ── File::read ──

    #[test]
    fn file_read_contents() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.py");
        fs::write(&path, "import os\n").unwrap();

        let file = File {
            path: path.clone(),
            rel_path: PathBuf::from("test.py"),
        };
        assert_eq!(file.read().unwrap(), "import os\n");
    }

    // ── ensure_dir ──

    #[test]
    fn ensure_dir_nested() {
        let dir = TempDir::new().unwrap();
        let deep = dir.path().join("a/b/c");
        ensure_dir(&deep).unwrap();
        assert!(deep.is_dir());
    }

    #[test]
    fn ensure_dir_idempotent() {
        let dir = TempDir::new().unwrap();
        let deep = dir.path().join("a/b");
        ensure_dir(&deep).unwrap();
        ensure_dir(&deep).unwrap(); // no error on second call
        assert!(deep.is_dir());
    }

    // ── write_file ──

    #[test]
    fn write_basic() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("out.json");
        write_file(&path, "{}", true).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "{}");
    }

    #[test]
    fn write_creates_parents() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("a/b/out.json");
        write_file(&path, "data", true).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "data");
    }

    #[test]
    fn write_no_mkdir_fails_when_parent_missing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("missing/out.json");
        assert!(write_file(&path, "data", false).is_err());
    }

    #[test]
    fn write_overwrites_existing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("out.json");
        write_file(&path, "old", true).unwrap();
        write_file(&path, "new", true).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    }

    #[test]
    fn write_is_atomic() {
        // If we can read after write, the rename landed.
        // The real guarantee: a crash mid-write leaves the original intact.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("out.json");
        write_file(&path, "first", true).unwrap();
        write_file(&path, "second", true).unwrap();

        // No partial content — it's either "first" or "second".
        let content = fs::read_to_string(&path).unwrap();
        assert!(content == "second");
    }
}
