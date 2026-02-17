use super::{ASTNode, Call, Function, Metadata, ParseError, Syntax, Type, metadata_from_span};
use std::collections::HashMap;
use tree_sitter::{Node, Parser};

pub struct Rust;

impl super::Lang for Rust {
    fn get_parser(&self) -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("failed to load rust grammar");
        parser
    }

    fn parse(&self, parser: &mut Parser, source: &str) -> Result<Vec<Syntax>, ParseError> {
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| ParseError("parse returned None".into()))?;
        let src = source.as_bytes();
        let root = tree.root_node();
        let imports = collect_imports(root, src);
        Ok(walk(root, src, &imports))
    }
}

// ── Import Collection ───────────────────────────────────────────────

fn collect_imports(root: Node, src: &[u8]) -> HashMap<String, String> {
    let mut imports = HashMap::new();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if child.kind() == "use_declaration" {
            let mut c = child.walk();
            for n in child.named_children(&mut c) {
                collect_use_tree(n, src, "", &mut imports);
            }
        }
    }
    imports
}

fn collect_use_tree(node: Node, src: &[u8], prefix: &str, imports: &mut HashMap<String, String>) {
    match node.kind() {
        "self" => {
            if !prefix.is_empty() {
                let local = prefix.rsplit("::").next().unwrap_or(prefix).to_string();
                imports.insert(local, prefix.to_string());
            }
        }
        "identifier" | "type_identifier" => {
            let name = node.utf8_text(src).unwrap_or("").to_string();
            let full = qualify(prefix, &name);
            imports.insert(name, full);
        }
        "scoped_identifier" | "scoped_type_identifier" => {
            let full = scoped_path(node, src);
            let local = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(src).ok())
                .unwrap_or("")
                .to_string();
            let full = qualify(prefix, &full);
            imports.insert(local, full);
        }
        "use_as_clause" => {
            let path_str = node
                .child_by_field_name("path")
                .map(|n| scoped_path(n, src))
                .unwrap_or_default();
            let alias = node
                .child_by_field_name("alias")
                .and_then(|n| n.utf8_text(src).ok())
                .unwrap_or("")
                .to_string();
            let full = qualify(prefix, &path_str);
            imports.insert(alias, full);
        }
        "scoped_use_list" => {
            let path = node
                .child_by_field_name("path")
                .map(|n| scoped_path(n, src))
                .unwrap_or_default();
            let new_prefix = qualify(prefix, &path);
            let mut c = node.walk();
            for child in node.named_children(&mut c) {
                if child.kind() == "use_list" {
                    collect_use_tree(child, src, &new_prefix, imports);
                }
            }
        }
        "use_list" => {
            let mut c = node.walk();
            for child in node.named_children(&mut c) {
                collect_use_tree(child, src, prefix, imports);
            }
        }
        "use_wildcard" => {}
        _ => {}
    }
}

fn qualify(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}::{name}")
    }
}

/// Recursively build `a::b::c` from scoped_identifier chains.
fn scoped_path(node: Node, src: &[u8]) -> String {
    match node.kind() {
        "identifier" | "type_identifier" | "self" | "crate" | "super" | "metavariable" => {
            node.utf8_text(src).unwrap_or("").to_string()
        }
        "scoped_identifier" | "scoped_type_identifier" => {
            let prefix = node
                .child_by_field_name("path")
                .map(|n| scoped_path(n, src))
                .unwrap_or_default();
            let name = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(src).ok())
                .unwrap_or("");
            if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{prefix}::{name}")
            }
        }
        _ => node.utf8_text(src).unwrap_or("").to_string(),
    }
}

// ── AST Walk ────────────────────────────────────────────────────────

fn walk(node: Node, src: &[u8], imports: &HashMap<String, String>) -> Vec<Syntax> {
    let mut out = Vec::new();
    let mut cursor = node.walk();

    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "function_item" => {
                let name = field_text(child, "name", src);
                let body = child
                    .child_by_field_name("body")
                    .map(|b| walk(b, src, imports))
                    .unwrap_or_default();
                out.push(Syntax {
                    node: ASTNode::Function(Function { name }),
                    metadata: meta(child, src),
                    contains: body,
                });
            }

            "struct_item" | "enum_item" | "type_item" => {
                let name = field_text(child, "name", src);
                out.push(Syntax {
                    node: ASTNode::Type(Type { name }),
                    metadata: meta(child, src),
                    contains: vec![],
                });
            }

            "trait_item" => {
                let name = field_text(child, "name", src);
                let body = child
                    .child_by_field_name("body")
                    .map(|b| walk(b, src, imports))
                    .unwrap_or_default();
                out.push(Syntax {
                    node: ASTNode::Type(Type { name }),
                    metadata: meta(child, src),
                    contains: body,
                });
            }

            "impl_item" => {
                let type_name = child
                    .child_by_field_name("type")
                    .map(|n| n.utf8_text(src).unwrap_or("").to_string())
                    .unwrap_or_default();
                let trait_name = child
                    .child_by_field_name("trait")
                    .and_then(|n| n.utf8_text(src).ok())
                    .map(|s| s.to_string());
                let label = match trait_name {
                    Some(t) => format!("{t} for {type_name}"),
                    None => type_name,
                };
                let body = child
                    .child_by_field_name("body")
                    .map(|b| walk(b, src, imports))
                    .unwrap_or_default();
                out.push(Syntax {
                    node: ASTNode::Type(Type { name: label }),
                    metadata: meta(child, src),
                    contains: body,
                });
            }

            "call_expression" => {
                let raw = child
                    .child_by_field_name("function")
                    .map(|f| call_name(f, src))
                    .unwrap_or_default();
                let name = resolve_call(&raw, imports);
                out.push(Syntax {
                    node: ASTNode::Call(Call { name }),
                    metadata: meta(child, src),
                    contains: vec![],
                });
            }

            "macro_invocation" => {
                let raw = child
                    .child_by_field_name("macro")
                    .map(|m| scoped_path(m, src))
                    .unwrap_or_default();
                let name = resolve_call(&raw, imports);
                out.push(Syntax {
                    node: ASTNode::Call(Call {
                        name: format!("{name}!"),
                    }),
                    metadata: meta(child, src),
                    contains: vec![],
                });
            }

            "line_comment" | "block_comment" => {
                out.push(Syntax {
                    node: ASTNode::Comment,
                    metadata: meta(child, src),
                    contains: vec![],
                });
            }

            "use_declaration" => {}
            "attribute_item" | "inner_attribute_item" | "mod_item" => {}

            _ => out.extend(walk(child, src, imports)),
        }
    }

    out
}

// ── Helpers ─────────────────────────────────────────────────────────

fn field_text(node: Node, field: &str, src: &[u8]) -> String {
    node.child_by_field_name(field)
        .and_then(|n| n.utf8_text(src).ok())
        .unwrap_or("")
        .to_string()
}

/// Extract a call's name from its function expression.
fn call_name(node: Node, src: &[u8]) -> String {
    match node.kind() {
        "identifier" | "type_identifier" => node.utf8_text(src).unwrap_or("").to_string(),
        "scoped_identifier" | "scoped_type_identifier" => scoped_path(node, src),
        "field_expression" => {
            let obj = node
                .child_by_field_name("value")
                .map(|n| call_name(n, src))
                .unwrap_or_default();
            let field = node
                .child_by_field_name("field")
                .and_then(|n| n.utf8_text(src).ok())
                .unwrap_or("");
            format!("{obj}.{field}")
        }
        _ => node.utf8_text(src).unwrap_or("").to_string(),
    }
}

/// Replace the first segment of a call with its import mapping.
fn resolve_call(name: &str, imports: &HashMap<String, String>) -> String {
    let (head, sep, tail) = if let Some((h, t)) = name.split_once("::") {
        (h, "::", Some(t))
    } else if let Some((h, t)) = name.split_once('.') {
        (h, ".", Some(t))
    } else {
        (name, "", None)
    };

    if matches!(head, "self" | "super" | "crate") {
        return name.to_string();
    }

    match imports.get(head) {
        Some(resolved) => match tail {
            Some(rest) => format!("{resolved}{sep}{rest}"),
            None => resolved.clone(),
        },
        None => name.to_string(),
    }
}

fn meta(node: Node, src: &[u8]) -> Metadata {
    metadata_from_span(src, node.start_byte(), node.end_byte())
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::tree_sitter::Lang;

    fn parse(source: &str) -> Vec<Syntax> {
        let lang = Rust;
        let mut parser = lang.get_parser();
        lang.parse(&mut parser, source).unwrap()
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

    // ── Empty ──

    #[test]
    fn empty_source() {
        let nodes = parse("");
        assert!(nodes.is_empty());
    }

    // ── Functions ──

    #[test]
    fn simple_function() {
        let nodes = parse("fn greet() {}");
        assert_eq!(names(&nodes), vec!["fn:greet"]);
    }

    #[test]
    fn function_with_calls() {
        let src = "fn main() { foo(); bar(); }";
        let nodes = parse(src);
        assert_eq!(names(&nodes), vec!["fn:main"]);
        let inner = names(&nodes[0].contains);
        assert!(inner.contains(&"call:foo".to_string()));
        assert!(inner.contains(&"call:bar".to_string()));
    }

    // ── Structs / Enums / Type Aliases ──

    #[test]
    fn struct_item() {
        let nodes = parse("struct Config { x: i32 }");
        assert_eq!(names(&nodes), vec!["ty:Config"]);
        assert!(nodes[0].contains.is_empty());
    }

    #[test]
    fn enum_item() {
        let nodes = parse("enum Color { Red, Blue }");
        assert_eq!(names(&nodes), vec!["ty:Color"]);
    }

    #[test]
    fn type_alias() {
        let nodes = parse("type Result<T> = std::result::Result<T, Error>;");
        assert_eq!(names(&nodes), vec!["ty:Result"]);
    }

    // ── Traits ──

    #[test]
    fn trait_with_default_methods() {
        let src = "trait Lang { fn parse() {} fn get() {} }";
        let nodes = parse(src);
        assert_eq!(names(&nodes), vec!["ty:Lang"]);
        let inner = names(&nodes[0].contains);
        assert_eq!(inner, vec!["fn:parse", "fn:get"]);
    }

    #[test]
    fn trait_signatures_not_captured() {
        // Trait method signatures (no body) are not function_item nodes
        let src = "trait Lang { fn parse(); }";
        let nodes = parse(src);
        assert_eq!(names(&nodes), vec!["ty:Lang"]);
        assert!(nodes[0].contains.is_empty());
    }

    // ── Impl Blocks ──

    #[test]
    fn impl_block() {
        let src = "struct Foo; impl Foo { fn new() {} }";
        let nodes = parse(src);
        let n = names(&nodes);
        assert!(n.contains(&"ty:Foo".to_string())); // both struct and impl
        let impl_node = nodes.iter().find(|s| match &s.node {
            ASTNode::Type(t) => t.name == "Foo" && !s.contains.is_empty(),
            _ => false,
        });
        assert!(impl_node.is_some());
        assert_eq!(names(&impl_node.unwrap().contains), vec!["fn:new"]);
    }

    #[test]
    fn impl_trait_for_type() {
        let src = "trait Greet {} struct Dog; impl Greet for Dog { fn hello() {} }";
        let nodes = parse(src);
        let n = names(&nodes);
        assert!(n.contains(&"ty:Greet for Dog".to_string()));
    }

    // ── Calls ──

    #[test]
    fn bare_call() {
        let src = "fn f() { run() }";
        let nodes = parse(src);
        let inner = names(&nodes[0].contains);
        assert_eq!(inner, vec!["call:run"]);
    }

    #[test]
    fn scoped_call() {
        let src = "fn f() { std::io::stdin() }";
        let nodes = parse(src);
        let inner = names(&nodes[0].contains);
        assert_eq!(inner, vec!["call:std::io::stdin"]);
    }

    #[test]
    fn method_call() {
        let src = "fn f() { parser.parse() }";
        let nodes = parse(src);
        let inner = names(&nodes[0].contains);
        assert_eq!(inner, vec!["call:parser.parse"]);
    }

    // ── Macros ──

    #[test]
    fn macro_invocation() {
        let src = "fn f() { println!(\"hi\") }";
        let nodes = parse(src);
        let inner = names(&nodes[0].contains);
        assert_eq!(inner, vec!["call:println!"]);
    }

    #[test]
    fn scoped_macro() {
        let src = "fn f() { std::write!(buf, \"x\") }";
        let nodes = parse(src);
        let inner = names(&nodes[0].contains);
        assert_eq!(inner, vec!["call:std::write!"]);
    }

    // ── Comments ──

    #[test]
    fn line_comment() {
        let nodes = parse("// a comment\n");
        assert_eq!(names(&nodes), vec!["comment"]);
    }

    #[test]
    fn block_comment() {
        let nodes = parse("/* block */\n");
        assert_eq!(names(&nodes), vec!["comment"]);
    }

    // ── Import Resolution ──

    #[test]
    fn use_resolves_call() {
        let src = "use std::collections::HashMap;\nfn f() { HashMap::new() }";
        let nodes = parse(src);
        let inner = names(&nodes[0].contains);
        assert_eq!(inner, vec!["call:std::collections::HashMap::new"]);
    }

    #[test]
    fn use_braces_resolves() {
        let src = "use std::io::{Read, Write};\nfn f() { Read::read() }";
        let nodes = parse(src);
        let inner = names(&nodes[0].contains);
        assert_eq!(inner, vec!["call:std::io::Read::read"]);
    }

    #[test]
    fn use_alias_resolves() {
        let src = "use std::collections::HashMap as Map;\nfn f() { Map::new() }";
        let nodes = parse(src);
        let inner = names(&nodes[0].contains);
        assert_eq!(inner, vec!["call:std::collections::HashMap::new"]);
    }

    #[test]
    fn self_prefix_not_resolved() {
        let imports = HashMap::new();
        assert_eq!(resolve_call("self.foo", &imports), "self.foo");
    }

    #[test]
    fn crate_prefix_not_resolved() {
        let imports = HashMap::new();
        assert_eq!(
            resolve_call("crate::util::run", &imports),
            "crate::util::run"
        );
    }

    // ── resolve_call unit ──

    #[test]
    fn resolve_call_with_mapping() {
        let mut imports = HashMap::new();
        imports.insert(
            "HashMap".to_string(),
            "std::collections::HashMap".to_string(),
        );
        assert_eq!(
            resolve_call("HashMap::new", &imports),
            "std::collections::HashMap::new"
        );
    }

    #[test]
    fn resolve_call_no_mapping() {
        let imports = HashMap::new();
        assert_eq!(resolve_call("foo::bar", &imports), "foo::bar");
    }

    #[test]
    fn resolve_call_dot_separator() {
        let mut imports = HashMap::new();
        imports.insert("parser".to_string(), "tree_sitter::Parser".to_string());
        assert_eq!(
            resolve_call("parser.parse", &imports),
            "tree_sitter::Parser.parse"
        );
    }

    // ── qualify unit ──

    #[test]
    fn qualify_empty_prefix() {
        assert_eq!(qualify("", "HashMap"), "HashMap");
    }

    #[test]
    fn qualify_with_prefix() {
        assert_eq!(
            qualify("std::collections", "HashMap"),
            "std::collections::HashMap"
        );
    }

    // ── Skipped nodes ──

    #[test]
    fn use_declarations_not_in_output() {
        let src = "use std::io;\nfn main() {}";
        let nodes = parse(src);
        assert_eq!(names(&nodes), vec!["fn:main"]);
    }

    #[test]
    fn attributes_not_in_output() {
        let src = "#[derive(Debug)]\nstruct Foo;";
        let nodes = parse(src);
        assert_eq!(names(&nodes), vec!["ty:Foo"]);
    }

    // ── Metadata ──

    #[test]
    fn metadata_attached() {
        let src = "fn foo() {\n    bar()\n}\n";
        let nodes = parse(src);
        assert!(nodes[0].metadata.chars > 0);
        assert!(nodes[0].metadata.lines >= 3);
    }

    // ── Mixed ──

    #[test]
    fn mixed_top_level() {
        let src = "\
// comment
struct Cfg;
fn run() { Cfg {} }
";
        let nodes = parse(src);
        let n = names(&nodes);
        assert!(n.contains(&"comment".to_string()));
        assert!(n.contains(&"ty:Cfg".to_string()));
        assert!(n.contains(&"fn:run".to_string()));
    }
}
