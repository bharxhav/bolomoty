use super::{ASTNode, Call, Function, Metadata, ParseError, Syntax, Type, metadata_from_span};
use std::collections::HashMap;
use tree_sitter::{Node, Parser};

pub struct Python;

impl super::Lang for Python {
    fn get_parser(&self) -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .expect("failed to load python grammar");
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
    collect_imports_inner(root, src, &mut imports);
    imports
}

fn collect_imports_inner(node: Node, src: &[u8], imports: &mut HashMap<String, String>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "import_statement" => {
                // Plain `import x.y.z` — calls are already qualified.
                // Only aliased imports (`import x as y`) need resolution.
                let mut c = child.walk();
                for n in child.named_children(&mut c) {
                    if n.kind() == "aliased_import" {
                        let name = field_text(n, "name", src);
                        let alias = field_text(n, "alias", src);
                        if !alias.is_empty() {
                            imports.insert(alias, name);
                        }
                    }
                }
            }

            "import_from_statement" => {
                let module = child
                    .child_by_field_name("module_name")
                    .and_then(|n| n.utf8_text(src).ok())
                    .unwrap_or("");
                let module_id = child.child_by_field_name("module_name").map(|n| n.id());

                let mut c = child.walk();
                for n in child.named_children(&mut c) {
                    // Skip the module_name node itself
                    if Some(n.id()) == module_id {
                        continue;
                    }
                    match n.kind() {
                        "dotted_name" => {
                            let name = n.utf8_text(src).unwrap_or("").to_string();
                            imports.insert(name.clone(), qualify(module, &name));
                        }
                        "aliased_import" => {
                            let name = field_text(n, "name", src);
                            let alias = field_text(n, "alias", src);
                            let key = if alias.is_empty() {
                                name.clone()
                            } else {
                                alias
                            };
                            imports.insert(key, qualify(module, &name));
                        }
                        _ => {}
                    }
                }
            }

            // Don't recurse into function/class bodies
            "function_definition" | "class_definition" => {}
            _ => collect_imports_inner(child, src, imports),
        }
    }
}

fn qualify(module: &str, name: &str) -> String {
    if module.is_empty() {
        name.to_string()
    } else if module.ends_with('.') {
        format!("{module}{name}")
    } else {
        format!("{module}.{name}")
    }
}

// ── AST Walk ────────────────────────────────────────────────────────

fn walk(node: Node, src: &[u8], imports: &HashMap<String, String>) -> Vec<Syntax> {
    let mut out = Vec::new();
    let mut cursor = node.walk();

    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "function_definition" => {
                let name = field_text(child, "name", src);
                let contains = body_children(child, src, imports);
                out.push(Syntax {
                    node: ASTNode::Function(Function { name }),
                    metadata: meta(child, src),
                    contains,
                });
            }

            "class_definition" => {
                let name = field_text(child, "name", src);
                let contains = body_children(child, src, imports);
                out.push(Syntax {
                    node: ASTNode::Type(Type { name }),
                    metadata: meta(child, src),
                    contains,
                });
            }

            "call" => {
                let raw = child
                    .child_by_field_name("function")
                    .map(|f| dotted_name(f, src))
                    .unwrap_or_default();
                let name = resolve_call(&raw, imports);
                out.push(Syntax {
                    node: ASTNode::Call(Call { name }),
                    metadata: meta(child, src),
                    contains: vec![],
                });
            }

            "expression_statement" => {
                // Bare string literal → docstring → treat as Comment
                if child.named_child_count() == 1
                    && child.named_child(0).is_some_and(|c| c.kind() == "string")
                {
                    out.push(Syntax {
                        node: ASTNode::Comment,
                        metadata: meta(child, src),
                        contains: vec![],
                    });
                } else {
                    out.extend(walk(child, src, imports));
                }
            }

            "comment" => {
                out.push(Syntax {
                    node: ASTNode::Comment,
                    metadata: meta(child, src),
                    contains: vec![],
                });
            }

            // Imports already collected — skip
            "import_statement" | "import_from_statement" | "future_import_statement" => {}

            // decorated_definition, control flow, etc. — recurse through
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

fn body_children(node: Node, src: &[u8], imports: &HashMap<String, String>) -> Vec<Syntax> {
    node.child_by_field_name("body")
        .map(|b| walk(b, src, imports))
        .unwrap_or_default()
}

/// Resolve `a.b.c` from nested attribute nodes.
fn dotted_name(node: Node, src: &[u8]) -> String {
    match node.kind() {
        "identifier" => node.utf8_text(src).unwrap_or("").to_string(),
        "attribute" => {
            let obj = node
                .child_by_field_name("object")
                .map(|n| dotted_name(n, src))
                .unwrap_or_default();
            let attr = node
                .child_by_field_name("attribute")
                .and_then(|n| n.utf8_text(src).ok())
                .unwrap_or("");
            format!("{obj}.{attr}")
        }
        _ => node.utf8_text(src).unwrap_or("").to_string(),
    }
}

/// Replace the first segment of a dotted call with its import mapping.
fn resolve_call(name: &str, imports: &HashMap<String, String>) -> String {
    let (head, tail) = match name.split_once('.') {
        Some((h, t)) => (h, Some(t)),
        None => (name, None),
    };
    match imports.get(head) {
        Some(module) => match tail {
            Some(rest) => format!("{module}.{rest}"),
            None => module.clone(),
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
        let lang = Python;
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
        let nodes = parse("def greet():\n    pass\n");
        assert_eq!(names(&nodes), vec!["fn:greet"]);
    }

    #[test]
    fn function_with_calls() {
        let src = "def main():\n    print('hello')\n    run()\n";
        let nodes = parse(src);
        assert_eq!(names(&nodes), vec!["fn:main"]);
        let inner = names(&nodes[0].contains);
        assert!(inner.contains(&"call:print".to_string()));
        assert!(inner.contains(&"call:run".to_string()));
    }

    // ── Classes ──

    #[test]
    fn simple_class() {
        let src = "class Foo:\n    pass\n";
        let nodes = parse(src);
        assert_eq!(names(&nodes), vec!["ty:Foo"]);
    }

    #[test]
    fn class_with_methods() {
        let src =
            "class Dog:\n    def bark(self):\n        pass\n    def wag(self):\n        pass\n";
        let nodes = parse(src);
        assert_eq!(names(&nodes), vec!["ty:Dog"]);
        let inner = names(&nodes[0].contains);
        assert_eq!(inner, vec!["fn:bark", "fn:wag"]);
    }

    // ── Calls ──

    #[test]
    fn bare_call() {
        let src = "run()\n";
        let nodes = parse(src);
        assert_eq!(names(&nodes), vec!["call:run"]);
    }

    #[test]
    fn dotted_call() {
        let src = "os.path.join('a', 'b')\n";
        let nodes = parse(src);
        assert_eq!(names(&nodes), vec!["call:os.path.join"]);
    }

    // ── Comments ──

    #[test]
    fn line_comment() {
        let src = "# this is a comment\n";
        let nodes = parse(src);
        assert_eq!(names(&nodes), vec!["comment"]);
    }

    #[test]
    fn docstring_as_comment() {
        let src = "\"\"\"module docstring\"\"\"\n";
        let nodes = parse(src);
        assert_eq!(names(&nodes), vec!["comment"]);
    }

    // ── Import Resolution ──

    #[test]
    fn from_import_resolves() {
        let src = "from os.path import join\njoin('a', 'b')\n";
        let nodes = parse(src);
        assert_eq!(names(&nodes), vec!["call:os.path.join"]);
    }

    #[test]
    fn from_import_dotted_module() {
        let src = "from .models import Request\nRequest()\n";
        let nodes = parse(src);
        assert_eq!(names(&nodes), vec!["call:.models.Request"]);
    }

    #[test]
    fn aliased_import_resolves() {
        let src = "import numpy as np\nnp.array([1])\n";
        let nodes = parse(src);
        assert_eq!(names(&nodes), vec!["call:numpy.array"]);
    }

    #[test]
    fn from_import_with_alias() {
        let src = "from collections import OrderedDict as OD\nOD()\n";
        let nodes = parse(src);
        assert_eq!(names(&nodes), vec!["call:collections.OrderedDict"]);
    }

    #[test]
    fn unresolved_call_stays_raw() {
        let src = "unknown_func()\n";
        let nodes = parse(src);
        assert_eq!(names(&nodes), vec!["call:unknown_func"]);
    }

    // ── resolve_call unit ──

    #[test]
    fn resolve_call_with_mapping() {
        let mut imports = HashMap::new();
        imports.insert("pd".to_string(), "pandas".to_string());
        assert_eq!(resolve_call("pd.DataFrame", &imports), "pandas.DataFrame");
    }

    #[test]
    fn resolve_call_no_mapping() {
        let imports = HashMap::new();
        assert_eq!(resolve_call("foo.bar", &imports), "foo.bar");
    }

    #[test]
    fn resolve_call_bare_name() {
        let mut imports = HashMap::new();
        imports.insert("Request".to_string(), "http.Request".to_string());
        assert_eq!(resolve_call("Request", &imports), "http.Request");
    }

    // ── qualify unit ──

    #[test]
    fn qualify_empty_module() {
        assert_eq!(qualify("", "Foo"), "Foo");
    }

    #[test]
    fn qualify_normal_module() {
        assert_eq!(qualify("os.path", "join"), "os.path.join");
    }

    #[test]
    fn qualify_trailing_dot() {
        assert_eq!(qualify(".", "models"), ".models");
    }

    // ── Metadata ──

    #[test]
    fn metadata_attached() {
        let src = "def foo():\n    pass\n";
        let nodes = parse(src);
        assert!(nodes[0].metadata.chars > 0);
        assert!(nodes[0].metadata.lines >= 2);
    }

    // ── Mixed ──

    #[test]
    fn mixed_top_level() {
        let src = "\
# comment
class Cfg:
    pass

def run():
    Cfg()

hello()
";
        let nodes = parse(src);
        let n = names(&nodes);
        assert!(n.contains(&"comment".to_string()));
        assert!(n.contains(&"ty:Cfg".to_string()));
        assert!(n.contains(&"fn:run".to_string()));
        assert!(n.contains(&"call:hello".to_string()));
    }
}
