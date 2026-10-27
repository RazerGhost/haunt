//! Python: functions, classes and their methods, lambdas assigned to a name; calls on `self`/`cls`, a class and a
//! module; imports by `import` and `from … import`, relative ones too.

use tree_sitter::Node;

use super::parts::{assigned_name, descendants, field, text};
use super::{Add, Context, Grammar, Receiver};
use crate::code::project::{dirname, file_in, join};
use crate::code::types::FunctionKind;

pub struct Python;

impl Grammar for Python {
    fn grammar(&self) -> tree_sitter::Language {
        tree_sitter_python::LANGUAGE.into()
    }

    fn kind(&self, kind: &str) -> Option<FunctionKind> {
        use FunctionKind::*;
        Some(match kind {
            "function_definition" => Function,
            "class_definition" => Class,
            "lambda" => Arrow,
            _ => return None,
        })
    }

    fn name(&self, node: Node, source: &str) -> Option<String> {
        if node.kind() == "lambda" {
            return assigned_name(node, source);
        }
        let name = text(field(node, "name"), source);
        (!name.is_empty()).then(|| name.to_owned())
    }

    /// Names starting with an underscore are the module's or class's own.
    fn exported(&self, node: Node, source: &str) -> bool {
        !text(field(node, "name"), source).starts_with('_')
    }

    fn is_async(&self, node: Node, source: &str) -> bool {
        node.kind() == "function_definition" && text(Some(node), source).starts_with("async")
    }

    fn method_in_type(&self, kind: &str) -> bool {
        kind == "function_definition"
    }

    /// f(), self.m(), Order.make(), module.f()
    fn call(&self, node: Node, container: Option<&str>, source: &str) -> Option<(String, Receiver)> {
        if node.kind() != "call" {
            return None;
        }
        let callee = field(node, "function")?;
        match callee.kind() {
            "attribute" => {
                let object = text(field(callee, "object"), source);
                let name = text(field(callee, "attribute"), source).to_owned();
                let receiver = if object == "self" || object == "cls" {
                    Receiver::SelfOf(container.map(str::to_owned))
                } else if object.starts_with(|c: char| c.is_ascii_uppercase()) && !object.contains('.') {
                    Receiver::Type(object.to_owned())
                } else {
                    Receiver::Plain
                };
                Some((name, receiver))
            }
            "identifier" => Some((text(Some(callee), source).to_owned(), Receiver::Plain)),
            _ => None,
        }
    }

    /// import a.b, from a.b import c, from . import c, from ..x import y
    fn imports(&self, root: Node, ctx: &Context, source: &str, add: &mut Add) {
        let dir = dirname(ctx.path);
        for statement in descendants(root, &["import_statement", "import_from_statement"]) {
            if statement.kind() == "import_statement" {
                for name in descendants(statement, &["dotted_name"]) {
                    // `import a.b as c`: the dotted name is the module.
                    if name.parent().is_some_and(|parent| parent.kind() == "aliased_import")
                        && field(name.parent().unwrap(), "alias") == Some(name)
                    {
                        continue;
                    }
                    add(module_file(ctx, &dir, text(Some(name), source), &[]), false);
                }
                continue;
            }
            let module = text(field(statement, "module_name"), source);
            // The names imported: `from . import a, b` names modules; `from m import f` names something in m.
            let names: Vec<&str> = {
                let mut cursor = statement.walk();
                statement
                    .children_by_field_name("name", &mut cursor)
                    .map(|name| text(field(name, "name").or(Some(name)), source))
                    .collect()
            };
            add(module_file(ctx, &dir, module, &names), false);
        }
    }
}

/// The file of a Python module: `a.b` is `a/b.py` or `a/b/__init__.py` (at the top, or under `src/`); a relative one
/// (`.x`, `..`) is found from the importing file's folder. With `names`, a name that is a module itself is preferred
/// (`from . import util` is util.py).
fn module_file(ctx: &Context, dir: &str, module: &str, names: &[&str]) -> Option<String> {
    let dots = module.chars().take_while(|c| *c == '.').count();
    let rest = module[dots..].replace('.', "/");
    let bases: Vec<String> = if dots > 0 {
        let mut base = dir.to_owned();
        for _ in 1..dots {
            base = join(&base, "..");
        }
        vec![if rest.is_empty() { base } else { join(&base, &rest) }]
    } else {
        vec![rest.clone(), format!("src/{rest}")]
    };
    for base in &bases {
        for name in names {
            if let Some(found) = file_in(ctx.root, &[format!("{base}/{name}.py"), format!("{base}/{name}/__init__.py")])
            {
                return Some(found);
            }
        }
        if let Some(found) = file_in(ctx.root, &[format!("{base}.py"), format!("{base}/__init__.py")]) {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::super::testing::read;
    use super::*;
    use crate::code::languages::Language;

    #[test]
    fn reads_python_classes_methods_lambdas_and_calls() {
        let (tree, reading) = read(
            Language::Python,
            "app/order.py",
            "class Order:\n    def total(self):\n        return self._sum()\n\n    def _sum(self):\n        return helper()\n\nasync def helper():\n    return Pricing.rate()\n\ndouble = lambda x: x * 2\n",
        );
        assert_eq!(
            tree,
            [
                "Class Order exp :1",
                "  Method total exp :2",
                "  Method _sum :5",
                "Function helper exp :8",
                "Arrow double exp :11"
            ]
        );
        assert!(reading.tree[1].is_async);
        let calls: Vec<(String, Receiver)> =
            reading.calls.iter().map(|call| (call.name.clone(), call.receiver.clone())).collect();
        assert_eq!(
            calls,
            [
                ("_sum".into(), Receiver::SelfOf(Some("Order".into()))),
                ("helper".into(), Receiver::Plain),
                ("rate".into(), Receiver::Type("Pricing".into()))
            ]
        );
    }
}
