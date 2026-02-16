pub mod py;
pub mod rs;

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

#[derive(Debug, Clone)]
pub struct Syntax {
    pub node: ASTNode,
    pub metadata: Option<Metadata>,
}

#[derive(Debug, Clone)]
pub enum ASTNode {
    Function(Function),
    Type(Type),
    Import(Import),
    Comment(Comment),
    Call(Call),
}

// ── Node Data ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct Type {
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct Import {
    pub module: String,
}

#[derive(Debug, Clone)]
pub struct Comment {
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct Call {
    pub name: String,
}

// ── Metadata ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Metadata {
    pub chars: usize,
    pub lines: usize,
    pub words: usize,
    pub whitespaces: usize,
    pub newlines: usize,
}

// ── Trait ─────────────────────────────────────────────────────────────

pub trait Lang {
    fn get_parser(&self) -> Parser;
    fn parse(&self, parser: &mut Parser, source: &str) -> Result<Vec<Syntax>, ParseError>;
}
