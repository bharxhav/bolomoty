use crate::api::fs;
use crate::api::tree_sitter::{Lang, Syntax};
use crate::clean;
use crate::error::BoloError;
use rayon::prelude::*;
use std::path::Path;

/// Parse and clean files in the immediate directory (non-recursive).
pub fn folder(
    root: &Path,
    ext: &str,
    no_ignore: bool,
    lang: &(dyn Lang + Sync),
) -> Result<Vec<Vec<Syntax>>, BoloError> {
    let files: Vec<_> = fs::walk_dir(root, ext, no_ignore)?
        .into_iter()
        .filter(|f| f.rel_path.components().count() == 1)
        .collect();

    files
        .par_iter()
        .map(|file| -> Result<_, BoloError> {
            let source = file.read()?;
            let mut parser = lang.get_parser();
            let ast = lang
                .parse(&mut parser, &source)
                .map_err(|e| BoloError::Parse {
                    file: file.rel_path.display().to_string(),
                    reason: e.to_string(),
                })?;
            Ok(clean::clean(&file.rel_path, &source, ast))
        })
        .collect::<Result<Vec<_>, _>>()
}

/// Parse and clean all files under a directory tree (recursive).
pub fn recursive(
    root: &Path,
    ext: &str,
    no_ignore: bool,
    lang: &(dyn Lang + Sync),
) -> Result<Vec<Vec<Syntax>>, BoloError> {
    let files = fs::walk_dir(root, ext, no_ignore)?;

    files
        .par_iter()
        .map(|file| -> Result<_, BoloError> {
            let source = file.read()?;
            let mut parser = lang.get_parser();
            let ast = lang
                .parse(&mut parser, &source)
                .map_err(|e| BoloError::Parse {
                    file: file.rel_path.display().to_string(),
                    reason: e.to_string(),
                })?;
            Ok(clean::clean(&file.rel_path, &source, ast))
        })
        .collect::<Result<Vec<_>, _>>()
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tree_sitter::ASTNode;
    use crate::api::tree_sitter::py::Python;
    use crate::api::tree_sitter::rs::Rust;
    use tempfile::TempDir;

    fn file_paths(result: &[Vec<Syntax>]) -> Vec<String> {
        result
            .iter()
            .filter_map(|file_nodes| {
                file_nodes.first().and_then(|s| match &s.node {
                    ASTNode::File(f) => Some(f.path.clone()),
                    _ => None,
                })
            })
            .collect()
    }

    // ── recursive ──

    #[test]
    fn recursive_finds_all_py_files() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("top.py"), "def foo(): pass\n").unwrap();
        std::fs::write(dir.path().join("sub/deep.py"), "def bar(): pass\n").unwrap();

        let result = recursive(dir.path(), "py", false, &Python).unwrap();
        assert_eq!(result.len(), 2);
        let paths = file_paths(&result);
        assert!(paths.iter().any(|p| p.contains("top.py")));
        assert!(paths.iter().any(|p| p.contains("deep.py")));
    }

    #[test]
    fn recursive_finds_all_rs_files() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(dir.path().join("sub/lib.rs"), "fn lib() {}\n").unwrap();

        let result = recursive(dir.path(), "rs", false, &Rust).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn recursive_empty_dir() {
        let dir = TempDir::new().unwrap();
        let result = recursive(dir.path(), "py", false, &Python).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn recursive_each_file_starts_with_file_node() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.py"), "x = 1\n").unwrap();
        std::fs::write(dir.path().join("b.py"), "y = 2\n").unwrap();

        let result = recursive(dir.path(), "py", false, &Python).unwrap();
        for file_nodes in &result {
            assert!(matches!(&file_nodes[0].node, ASTNode::File(_)));
        }
    }

    // ── folder ──

    #[test]
    fn folder_only_immediate_files() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("top.py"), "def foo(): pass\n").unwrap();
        std::fs::write(dir.path().join("sub/deep.py"), "def bar(): pass\n").unwrap();

        let result = folder(dir.path(), "py", false, &Python).unwrap();
        assert_eq!(result.len(), 1);
        let paths = file_paths(&result);
        assert!(paths[0].contains("top.py"));
    }

    #[test]
    fn folder_empty_dir() {
        let dir = TempDir::new().unwrap();
        let result = folder(dir.path(), "py", false, &Python).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn folder_ignores_wrong_ext() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        let result = folder(dir.path(), "py", false, &Python).unwrap();
        assert!(result.is_empty());
    }

    // ── Content correctness ──

    #[test]
    fn parses_functions_correctly() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("app.py"), "def greet():\n    print('hi')\n").unwrap();

        let result = recursive(dir.path(), "py", false, &Python).unwrap();
        let file_nodes = &result[0];
        // File, then maybe Comment, then Function
        let has_greet = file_nodes.iter().any(|s| match &s.node {
            ASTNode::Function(f) => f.name == "greet",
            _ => false,
        });
        assert!(has_greet);
    }

    #[test]
    fn comments_hoisted_by_clean() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("c.py"), "# hello\ndef f(): pass\n").unwrap();

        let result = recursive(dir.path(), "py", false, &Python).unwrap();
        let file_nodes = &result[0];
        // [File, Comment, Function] — comment is second
        assert!(matches!(&file_nodes[0].node, ASTNode::File(_)));
        assert!(matches!(&file_nodes[1].node, ASTNode::Comment));
    }
}
