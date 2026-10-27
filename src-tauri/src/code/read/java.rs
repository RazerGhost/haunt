//! Java: classes (interfaces, enums, records, annotations), methods, constructors and lambdas; calls on `this`, a type
//! and `new`; imports from the source folder the file's own package sits in.

use tree_sitter::Node;

use super::parts::{assigned_name, created, field, has_child, named_children, on_name, text};
use super::{Add, Context, Grammar, Receiver};
use crate::code::project::{dirname, file_in, join};
use crate::code::types::FunctionKind;

pub struct Java;

impl Grammar for Java {
    fn grammar(&self) -> tree_sitter::Language {
        tree_sitter_java::LANGUAGE.into()
    }

    fn kind(&self, kind: &str) -> Option<FunctionKind> {
        use FunctionKind::*;
        Some(match kind {
            "method_declaration" | "constructor_declaration" => Method,
            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration" => Class,
            "lambda_expression" => Arrow,
            _ => return None,
        })
    }

    fn name(&self, node: Node, source: &str) -> Option<String> {
        if node.kind() == "lambda_expression" {
            return assigned_name(node, source);
        }
        let name = text(field(node, "name"), source);
        (!name.is_empty()).then(|| name.to_owned())
    }

    /// Declared `public`.
    fn exported(&self, node: Node, source: &str) -> bool {
        named_children(node).iter().any(|child| {
            child.kind() == "modifiers" && text(Some(*child), source).split_whitespace().any(|word| word == "public")
        })
    }

    /// f(), this.m(), Order.make(), order.total(), new Order()
    fn call(&self, node: Node, container: Option<&str>, source: &str) -> Option<(String, Receiver)> {
        match node.kind() {
            "method_invocation" => {
                let receiver = match field(node, "object") {
                    Some(object) if object.kind() == "this" => Receiver::SelfOf(container.map(str::to_owned)),
                    Some(object) if object.kind() == "identifier" => on_name(text(Some(object), source)),
                    _ => Receiver::Plain,
                };
                Some((text(field(node, "name"), source).to_owned(), receiver))
            }
            "object_creation_expression" => created(field(node, "type"), source),
            _ => None,
        }
    }

    /// import a.b.C is a/b/C.java in the source folder the file's own package sits in (src/main/java/…); a static
    /// import or a nested class names a member of a/b.java. `import a.b.*` is left out.
    fn imports(&self, root: Node, ctx: &Context, source: &str, add: &mut Add) {
        let dir = dirname(ctx.path);
        let package = named_children(root)
            .into_iter()
            .find(|child| child.kind() == "package_declaration")
            .and_then(|declaration| named_children(declaration).into_iter().next());
        let package = text(package, source).replace('.', "/");
        let base = if package.is_empty() {
            dir.clone()
        } else if dir == package {
            ".".to_owned()
        } else {
            dir.strip_suffix(&format!("/{package}")).unwrap_or(&dir).to_owned()
        };
        for import in named_children(root).into_iter().filter(|child| child.kind() == "import_declaration") {
            if has_child(import, "asterisk") {
                continue;
            }
            let name = text(named_children(import).into_iter().next(), source).replace('.', "/");
            let outer = dirname(&name);
            add(
                file_in(ctx.root, &[join(&base, &format!("{name}.java")), join(&base, &format!("{outer}.java"))]),
                true,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::{calls, read};
    use super::*;
    use crate::code::languages::Language;

    #[test]
    fn reads_java_classes_nested_types_lambdas_and_calls() {
        let (tree, reading) = read(
            Language::Java,
            "src/main/java/com/shop/Order.java",
            "package com.shop;\n\npublic class Order {\n  public Order() {}\n  public int total() { return this.sum() + Rate.of(1) + helper(); }\n  private int sum() {\n    Runnable r = () -> run();\n    return new Pricing().rate();\n  }\n  interface Shown { void show(); }\n  enum Kind { A; void k() {} }\n  public record Point(int x) { int y() { return x; } }\n}\n",
        );
        assert_eq!(
            tree,
            [
                "Class Order exp :3",
                "  Method Order exp :4",
                "  Method total exp :5",
                "  Method sum :6",
                "    Arrow r :7",
                "  Class Shown :10",
                "    Method show :10",
                "  Class Kind :11",
                "    Method k :11",
                "  Class Point exp :12",
                "    Method y :12"
            ]
        );
        let own = || Receiver::SelfOf(Some("Order".into()));
        assert_eq!(
            calls(&reading),
            [
                ("sum".into(), own()),
                ("of".into(), Receiver::Type("Rate".into())),
                ("helper".into(), Receiver::Plain),
                ("run".into(), Receiver::Plain),
                ("rate".into(), Receiver::Plain),
                ("Pricing".into(), Receiver::Type("Pricing".into()))
            ]
        );
    }
}
