pub mod py;
pub mod rs;

use serde::Serialize;
use std::fmt;
use tree_sitter::Parser;

// ── Error ────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct ParseError(pub String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseError {}

// ── Core Types ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct Syntax {
    pub node: ASTNode,
    pub metadata: Metadata,
    pub contains: Vec<Syntax>,
}

#[derive(Debug, Clone, Serialize)]
pub enum ASTNode {
    File(File),
    Function(Function),
    Type(Type),
    Call(Call),
    Comment,
}

// ── Node Data ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct File {
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Function {
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Type {
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Call {
    pub name: String,
}

// ── Metadata ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct Metadata {
    pub chars: usize,
    pub lines: usize,
    pub words: usize,
    pub whitespaces: usize,
    pub newlines: usize,
}

/// Build a [`Metadata`] from a byte‐range in the source.
pub fn metadata_from_span(src: &[u8], start: usize, end: usize) -> Metadata {
    let slice = src.get(start..end).unwrap_or(b"");
    let text = std::str::from_utf8(slice).unwrap_or("");
    let newlines = text.chars().filter(|c| *c == '\n').count();
    Metadata {
        chars: text.chars().count(),
        lines: newlines + 1,
        words: text.split_whitespace().count(),
        whitespaces: text
            .chars()
            .filter(|c| c.is_whitespace() && *c != '\n')
            .count(),
        newlines,
    }
}

// ── Trait ─────────────────────────────────────────────────────────────

pub trait Lang {
    fn get_parser(&self) -> Parser;
    fn parse(&self, parser: &mut Parser, source: &str) -> Result<Vec<Syntax>, ParseError>;
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── ParseError ──

    #[test]
    fn parse_error_display() {
        let err = ParseError("something broke".into());
        assert_eq!(format!("{err}"), "something broke");
    }

    #[test]
    fn parse_error_is_error_trait() {
        let err: Box<dyn std::error::Error> = Box::new(ParseError("test".into()));
        assert_eq!(err.to_string(), "test");
    }

    // ── metadata_from_span ──

    #[test]
    fn metadata_simple_line() {
        let src = b"hello world";
        let m = metadata_from_span(src, 0, src.len());
        assert_eq!(m.chars, 11);
        assert_eq!(m.lines, 1);
        assert_eq!(m.words, 2);
        assert_eq!(m.whitespaces, 1);
        assert_eq!(m.newlines, 0);
    }

    #[test]
    fn metadata_multiline() {
        let src = b"fn main() {\n    println!()\n}\n";
        let m = metadata_from_span(src, 0, src.len());
        assert_eq!(m.newlines, 3);
        assert_eq!(m.lines, 4); // newlines + 1
        assert_eq!(m.words, 5); // fn, main(), {, println!(), }
    }

    #[test]
    fn metadata_empty_span() {
        let src = b"hello";
        let m = metadata_from_span(src, 0, 0);
        assert_eq!(m.chars, 0);
        assert_eq!(m.lines, 1);
        assert_eq!(m.words, 0);
        assert_eq!(m.whitespaces, 0);
        assert_eq!(m.newlines, 0);
    }

    #[test]
    fn metadata_subspan() {
        let src = b"aaaa bbbb cccc";
        //          0123456789...
        let m = metadata_from_span(src, 5, 9); // "bbbb"
        assert_eq!(m.chars, 4);
        assert_eq!(m.words, 1);
        assert_eq!(m.whitespaces, 0);
    }

    #[test]
    fn metadata_out_of_bounds_returns_empty() {
        let src = b"hi";
        let m = metadata_from_span(src, 10, 20);
        assert_eq!(m.chars, 0);
        assert_eq!(m.words, 0);
    }

    #[test]
    fn metadata_tabs_are_whitespace() {
        let src = b"a\tb";
        let m = metadata_from_span(src, 0, src.len());
        assert_eq!(m.whitespaces, 1);
        assert_eq!(m.words, 2);
    }

    #[test]
    fn metadata_only_whitespace() {
        let src = b"   \t  ";
        let m = metadata_from_span(src, 0, src.len());
        assert_eq!(m.chars, 6);
        assert_eq!(m.words, 0);
        assert_eq!(m.whitespaces, 6);
    }

    // ── Syntax serialization ──

    #[test]
    fn syntax_serializes_to_json() {
        let s = Syntax {
            node: ASTNode::Function(Function {
                name: "main".into(),
            }),
            metadata: Metadata {
                chars: 10,
                lines: 1,
                words: 2,
                whitespaces: 1,
                newlines: 0,
            },
            contains: vec![],
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"Function\""));
        assert!(json.contains("\"main\""));
    }

    #[test]
    fn comment_node_serializes() {
        let s = Syntax {
            node: ASTNode::Comment,
            metadata: Metadata {
                chars: 5,
                lines: 1,
                words: 1,
                whitespaces: 0,
                newlines: 0,
            },
            contains: vec![],
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"Comment\""));
    }
}
