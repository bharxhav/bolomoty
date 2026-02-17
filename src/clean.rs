use crate::api::tree_sitter::{ASTNode, File, Metadata, Syntax, metadata_from_span};
use std::path::Path;

/// Strip all comments (nested or otherwise) and hoist a merged Comment to the top.
///
/// Returns: `[File(path), Comment(merged), ...stripped_nodes]`
pub fn clean(path: &Path, source: &str, nodes: Vec<Syntax>) -> Vec<Syntax> {
    let mut comment_meta = Metadata {
        chars: 0,
        lines: 0,
        words: 0,
        whitespaces: 0,
        newlines: 0,
    };
    let stripped = strip_comments(nodes, &mut comment_meta);
    let file_meta = metadata_from_span(source.as_bytes(), 0, source.len());

    let mut out = Vec::with_capacity(stripped.len() + 2);
    out.push(Syntax {
        node: ASTNode::File(File {
            path: path.display().to_string(),
        }),
        metadata: file_meta,
        contains: vec![],
    });

    if comment_meta.chars > 0 {
        out.push(Syntax {
            node: ASTNode::Comment,
            metadata: comment_meta,
            contains: vec![],
        });
    }

    out.extend(stripped);
    out
}

fn strip_comments(nodes: Vec<Syntax>, acc: &mut Metadata) -> Vec<Syntax> {
    nodes
        .into_iter()
        .filter_map(|mut s| match &s.node {
            ASTNode::Comment => {
                acc.chars += s.metadata.chars;
                acc.lines += s.metadata.lines;
                acc.words += s.metadata.words;
                acc.whitespaces += s.metadata.whitespaces;
                acc.newlines += s.metadata.newlines;
                None
            }
            _ => {
                s.contains = strip_comments(s.contains, acc);
                Some(s)
            }
        })
        .collect()
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tree_sitter::{Call, Function, Type};
    use std::path::Path;

    fn meta(chars: usize, words: usize) -> Metadata {
        Metadata {
            chars,
            lines: 1,
            words,
            whitespaces: 0,
            newlines: 0,
        }
    }

    fn names(nodes: &[Syntax]) -> Vec<String> {
        nodes
            .iter()
            .map(|s| match &s.node {
                ASTNode::Function(f) => format!("fn:{}", f.name),
                ASTNode::Type(t) => format!("ty:{}", t.name),
                ASTNode::Call(c) => format!("call:{}", c.name),
                ASTNode::Comment => "comment".into(),
                ASTNode::File(f) => format!("file:{}", f.path),
            })
            .collect()
    }

    // ── clean ──

    #[test]
    fn clean_empty_nodes() {
        let source = "";
        let result = clean(Path::new("test.py"), source, vec![]);
        assert_eq!(names(&result), vec!["file:test.py"]);
    }

    #[test]
    fn clean_no_comments() {
        let source = "def foo(): pass";
        let nodes = vec![Syntax {
            node: ASTNode::Function(Function { name: "foo".into() }),
            metadata: meta(15, 3),
            contains: vec![],
        }];
        let result = clean(Path::new("test.py"), source, nodes);
        // No comment node inserted when there are no comments
        assert_eq!(names(&result), vec!["file:test.py", "fn:foo"]);
    }

    #[test]
    fn clean_with_comments() {
        let source = "# comment\ndef foo(): pass";
        let nodes = vec![
            Syntax {
                node: ASTNode::Comment,
                metadata: meta(9, 2),
                contains: vec![],
            },
            Syntax {
                node: ASTNode::Function(Function { name: "foo".into() }),
                metadata: meta(15, 3),
                contains: vec![],
            },
        ];
        let result = clean(Path::new("test.py"), source, nodes);
        assert_eq!(names(&result), vec!["file:test.py", "comment", "fn:foo"]);
    }

    #[test]
    fn clean_merges_multiple_comments() {
        let source = "# one\n# two";
        let nodes = vec![
            Syntax {
                node: ASTNode::Comment,
                metadata: meta(5, 2),
                contains: vec![],
            },
            Syntax {
                node: ASTNode::Comment,
                metadata: meta(5, 2),
                contains: vec![],
            },
        ];
        let result = clean(Path::new("x.py"), source, nodes);
        assert_eq!(names(&result), vec!["file:x.py", "comment"]);
        // Merged metadata
        let comment = &result[1];
        assert_eq!(comment.metadata.chars, 10);
        assert_eq!(comment.metadata.words, 4);
    }

    #[test]
    fn clean_strips_nested_comments() {
        let source = "def foo():\n    # inner\n    pass";
        let nodes = vec![Syntax {
            node: ASTNode::Function(Function { name: "foo".into() }),
            metadata: meta(30, 5),
            contains: vec![Syntax {
                node: ASTNode::Comment,
                metadata: meta(7, 2),
                contains: vec![],
            }],
        }];
        let result = clean(Path::new("test.py"), source, nodes);
        assert_eq!(names(&result), vec!["file:test.py", "comment", "fn:foo"]);
        // Nested comment stripped from contains
        assert!(result[2].contains.is_empty());
    }

    #[test]
    fn clean_only_comments() {
        let source = "# just comments";
        let nodes = vec![Syntax {
            node: ASTNode::Comment,
            metadata: meta(15, 3),
            contains: vec![],
        }];
        let result = clean(Path::new("c.py"), source, nodes);
        assert_eq!(names(&result), vec!["file:c.py", "comment"]);
    }

    #[test]
    fn clean_file_node_has_full_metadata() {
        let source = "hello world\nsecond line\n";
        let result = clean(Path::new("test.py"), source, vec![]);
        let file_meta = &result[0].metadata;
        assert_eq!(file_meta.chars, source.len());
        assert_eq!(file_meta.newlines, 2);
    }

    #[test]
    fn clean_preserves_nesting() {
        let source = "class Foo:\n    def bar(self):\n        baz()";
        let nodes = vec![Syntax {
            node: ASTNode::Type(Type { name: "Foo".into() }),
            metadata: meta(44, 6),
            contains: vec![Syntax {
                node: ASTNode::Function(Function { name: "bar".into() }),
                metadata: meta(30, 4),
                contains: vec![Syntax {
                    node: ASTNode::Call(Call { name: "baz".into() }),
                    metadata: meta(5, 1),
                    contains: vec![],
                }],
            }],
        }];
        let result = clean(Path::new("test.py"), source, nodes);
        // Type → Function → Call nesting preserved
        assert_eq!(names(&result[1].contains), vec!["fn:bar"]);
        assert_eq!(names(&result[1].contains[0].contains), vec!["call:baz"]);
    }
}
