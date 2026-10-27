//! Go: functions, methods by their receiver's type (`Order.Total`), function literals; calls as `f()`, `pkg.F()` and
//! `value.Method()`; imports of the module's own packages, through go.mod, to one file of each.

use tree_sitter::Node;

use super::parts::{assigned_name, descendants, field, package_file, string_value, text};
use super::{Add, Context, Grammar, Receiver};
use crate::code::types::FunctionKind;

pub struct Go;

impl Grammar for Go {
    fn grammar(&self) -> tree_sitter::Language {
        tree_sitter_go::LANGUAGE.into()
    }

    fn kind(&self, kind: &str) -> Option<FunctionKind> {
        use FunctionKind::*;
        Some(match kind {
            "function_declaration" => Function,
            "method_declaration" => Method,
            "func_literal" => Arrow,
            _ => return None,
        })
    }

    /// A method by its receiver's type too: `func (o *Order) Total()` is Order.Total.
    fn name(&self, node: Node, source: &str) -> Option<String> {
        let own = || {
            let name = text(field(node, "name"), source);
            (!name.is_empty()).then(|| name.to_owned())
        };
        match node.kind() {
            "method_declaration" => {
                let method = own()?;
                let receiver = field(node, "receiver")
                    .and_then(|receiver| descendants(receiver, &["type_identifier"]).into_iter().next());
                Some(match receiver {
                    Some(type_) => format!("{}.{method}", text(Some(type_), source)),
                    None => method,
                })
            }
            "func_literal" => assigned_name(node, source),
            _ => own(),
        }
    }

    /// A capital first letter exports.
    fn exported(&self, node: Node, source: &str) -> bool {
        text(field(node, "name"), source).starts_with(|c: char| c.is_uppercase())
    }

    /// f(), pkg.F(), value.Method()
    fn call(&self, node: Node, _container: Option<&str>, source: &str) -> Option<(String, Receiver)> {
        if node.kind() != "call_expression" {
            return None;
        }
        let callee = field(node, "function")?;
        match callee.kind() {
            "selector_expression" => Some((text(field(callee, "field"), source).to_owned(), Receiver::Plain)),
            "identifier" => Some((text(Some(callee), source).to_owned(), Receiver::Plain)),
            _ => None,
        }
    }

    /// The module's own packages, to one file of each (a package is a folder).
    fn imports(&self, root: Node, ctx: &Context, source: &str, add: &mut Add) {
        if ctx.go_module.is_empty() {
            return;
        }
        for spec in descendants(root, &["import_spec"]) {
            let Some(path) = string_value(field(spec, "path"), source) else { continue };
            let folder = if path == ctx.go_module {
                String::new()
            } else if let Some(rest) = path.strip_prefix(&format!("{}/", ctx.go_module)) {
                rest.to_owned()
            } else {
                continue;
            };
            add(package_file(ctx.root, &folder, ".go"), false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::read;
    use crate::code::languages::Language;

    #[test]
    fn reads_go_functions_methods_and_calls() {
        let (tree, reading) = read(
            Language::Go,
            "order/order.go",
            "package order\n\nfunc (o *Order) Total() int {\n\treturn o.sum() + helper()\n}\n\nfunc helper() int {\n\tdouble := func(x int) int { return x * 2 }\n\treturn double(fmt.Len())\n}\n",
        );
        assert_eq!(tree, ["Method Order.Total exp :3", "Function helper :7", "  Arrow double :8"]);
        let names: Vec<&str> = reading.calls.iter().map(|call| call.name.as_str()).collect();
        assert_eq!(names, ["sum", "helper", "double", "Len"]);
    }
}
