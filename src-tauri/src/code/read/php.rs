//! PHP: functions, classes (interfaces, traits, enums) and their methods, closures and arrow functions; calls on
//! `$this`, `self::`, a class and `new`, and WordPress hooks; imports by `use` through Composer's PSR-4 map and by
//! `require`/`include`.

use tree_sitter::Node;

use super::parts::{assigned_name, descendants, field, last, named_children, string_value, text, unquote};
use super::{Add, Context, FileReading, Grammar, Receiver, Target};
use crate::code::project::{dirname, file_in, join};
use crate::code::types::{FunctionKind, FunctionNode};

/// The namespace the file is in so far, for its classes' full names.
#[derive(Default)]
pub struct Php {
    namespace: String,
}

impl Grammar for Php {
    fn grammar(&self) -> tree_sitter::Language {
        tree_sitter_php::LANGUAGE_PHP.into()
    }

    fn kind(&self, kind: &str) -> Option<FunctionKind> {
        use FunctionKind::*;
        Some(match kind {
            "function_definition" => Function,
            "method_declaration" => Method,
            "class_declaration" | "interface_declaration" | "trait_declaration" | "enum_declaration" => Class,
            "anonymous_function" | "anonymous_function_creation_expression" => Function,
            "arrow_function" => Arrow,
            _ => return None,
        })
    }

    fn name(&self, node: Node, source: &str) -> Option<String> {
        let name = text(field(node, "name"), source);
        if name.is_empty() {
            assigned_name(node, source)
        } else {
            Some(name.to_owned())
        }
    }

    fn exported(&self, node: Node, source: &str) -> bool {
        node.kind() != "method_declaration"
            || !named_children(node)
                .iter()
                .any(|child| child.kind() == "visibility_modifier" && text(Some(*child), source) != "public")
    }

    fn enter(&mut self, node: Node, source: &str) {
        if node.kind() == "namespace_definition" {
            self.namespace = text(field(node, "name"), source).to_owned();
        }
    }

    fn listed(&mut self, _node: Node, item: &FunctionNode, reading: &mut FileReading) {
        if item.kind == FunctionKind::Class && !item.anonymous {
            let name = &item.name;
            let qualified =
                if self.namespace.is_empty() { name.clone() } else { format!("{}\\{name}", self.namespace) };
            reading.classes.push(qualified);
        }
    }

    fn call(&self, node: Node, container: Option<&str>, source: &str) -> Option<(String, Receiver)> {
        let own = || Receiver::SelfOf(container.map(str::to_owned));
        match node.kind() {
            "function_call_expression" => {
                Some((last(text(field(node, "function"), source)).to_owned(), Receiver::Plain))
            }
            "member_call_expression" | "nullsafe_member_call_expression" => {
                let receiver = if text(field(node, "object"), source) == "$this" { own() } else { Receiver::Plain };
                Some((text(field(node, "name"), source).to_owned(), receiver))
            }
            "scoped_call_expression" => {
                let scope = text(field(node, "scope"), source);
                let receiver = if ["self", "static", "parent"].contains(&scope) {
                    own()
                } else {
                    Receiver::Type(last(scope).to_owned())
                };
                Some((text(field(node, "name"), source).to_owned(), receiver))
            }
            "object_creation_expression" => {
                let created =
                    named_children(node).into_iter().find(|child| matches!(child.kind(), "name" | "qualified_name"))?;
                let name = last(text(Some(created), source)).to_owned();
                Some((name.clone(), Receiver::Type(name)))
            }
            _ => None,
        }
    }

    fn other_call(&mut self, node: Node, _container: Option<&str>, source: &str) -> Option<(String, Receiver, Target)> {
        hook_callback(node, source)
    }

    fn imports(&self, root: Node, ctx: &Context, source: &str, add: &mut Add) {
        let dir = dirname(ctx.path);
        for clause in descendants(root, &["namespace_use_clause"]) {
            let name =
                named_children(clause).into_iter().find(|child| matches!(child.kind(), "qualified_name" | "name"));
            if let Some(name) = name {
                add(psr4_file(ctx, text(Some(name), source).trim_start_matches('\\')), true);
            }
        }
        let includes =
            ["require_expression", "require_once_expression", "include_expression", "include_once_expression"];
        for include in descendants(root, &includes) {
            let Some(expression) = named_children(include).into_iter().next() else { continue };
            // 'x.php', __DIR__ . '/x.php', plugin_dir_path(__FILE__) . 'x.php', dirname(__FILE__) . '/x.php'
            let binary = expression.kind() == "binary_expression";
            let Some(literal) =
                string_value(if binary { field(expression, "right") } else { Some(expression) }, source)
            else {
                continue;
            };
            let relative_to_file = binary || literal.starts_with('.');
            let first = if relative_to_file { join(&dir, &literal) } else { literal.clone() };
            add(file_in(ctx.root, &[first, join(&dir, &literal)]), false);
        }
    }
}

/// WordPress hooks as calls: `add_action('init', 'boot')` and `add_filter('the_title', [$this, 'title'])` make WordPress
/// call that function later, so the function that registers the hook counts as calling it.
fn hook_callback(node: Node, source: &str) -> Option<(String, Receiver, Target)> {
    const HOOKS: [&str; 5] =
        ["add_action", "add_filter", "add_shortcode", "register_activation_hook", "register_deactivation_hook"];
    if node.kind() != "function_call_expression" || !HOOKS.contains(&text(field(node, "function"), source)) {
        return None;
    }
    // The callback is the second argument of all of them: add_action($hook, $callback), register_activation_hook($file, $callback).
    let arguments = named_children(field(node, "arguments")?);
    let callback = *arguments.get(1)?;
    let value = named_children(callback).into_iter().next().unwrap_or(callback);
    match value.kind() {
        "string" | "encapsed_string" => {
            let name = unquote(text(Some(value), source)).split("::").last().unwrap_or("").to_owned();
            (!name.is_empty()).then_some((name, Receiver::Plain, Target::Any))
        }
        "array_creation_expression" => {
            let elements: Vec<&str> =
                named_children(value).into_iter().map(|element| text(Some(element), source)).collect();
            let method = unquote(elements.get(1)?).to_owned();
            let receiver = if elements[0].contains("$this") { Receiver::SelfOf(None) } else { Receiver::Plain };
            (!method.is_empty()).then_some((method, receiver, Target::Any))
        }
        _ => None,
    }
}

/// A PHP class's file by Composer's PSR-4 map: `App\Models\User` with `App\` → `app/` is `app/Models/User.php`.
fn psr4_file(ctx: &Context, name: &str) -> Option<String> {
    ctx.psr4.iter().find_map(|(prefix, folder)| {
        let rest = name.strip_prefix(prefix.as_str())?;
        file_in(ctx.root, &[join(folder, &format!("{}.php", rest.replace('\\', "/")))])
    })
}

#[cfg(test)]
mod tests {
    use super::super::testing::read;
    use super::*;
    use crate::code::languages::Language;

    #[test]
    fn reads_php_classes_methods_and_calls() {
        let (tree, reading) = read(
            Language::Php,
            "app/Order.php",
            "<?php\nnamespace App;\nclass Order {\n  public function total() { return $this->sum(); }\n  private function sum() { return helper(); }\n}\nfunction helper() { return 1; }\n$f = function () { return 2; };\n",
        );
        // An assigned function takes the variable's name.
        assert_eq!(
            tree,
            [
                "Class Order exp :3",
                "  Method total exp :4",
                "  Method sum :5",
                "Function helper exp :7",
                "Function f exp :8"
            ]
        );
        assert_eq!(reading.classes, ["App\\Order"]);
        let calls: Vec<(String, Receiver)> =
            reading.calls.iter().map(|call| (call.name.clone(), call.receiver.clone())).collect();
        assert_eq!(calls, [("sum".into(), Receiver::SelfOf(Some("Order".into()))), ("helper".into(), Receiver::Plain)]);
    }
}
