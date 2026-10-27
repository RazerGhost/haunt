//! Lua: `function M.f`, `local function f` and functions assigned to a name; calls as `f()`, `M.f()` and `obj:m()`;
//! imports by `require`.

use tree_sitter::Node;

use super::parts::{assigned_name, descendants, field, string_value, text};
use super::{Add, Context, Grammar, Receiver};
use crate::code::project::{dirname, file_in, join};
use crate::code::types::FunctionKind;

pub struct Lua;

impl Grammar for Lua {
    fn grammar(&self) -> tree_sitter::Language {
        tree_sitter_lua::LANGUAGE.into()
    }

    fn kind(&self, kind: &str) -> Option<FunctionKind> {
        matches!(kind, "function_declaration" | "function_definition").then_some(FunctionKind::Function)
    }

    fn name(&self, node: Node, source: &str) -> Option<String> {
        if node.kind() == "function_declaration" {
            let name = text(field(node, "name"), source);
            (!name.is_empty()).then(|| name.to_owned())
        } else {
            assigned_name(node, source)
        }
    }

    /// `local function f` stays in its file; `function M.f` and `function f` do not.
    fn exported(&self, node: Node, source: &str) -> bool {
        node.kind() == "function_declaration" && {
            let source = text(Some(node), source);
            !(source.starts_with("local") && !source[5..].starts_with(|c: char| c.is_alphanumeric() || c == '_'))
        }
    }

    fn call(&self, node: Node, container: Option<&str>, source: &str) -> Option<(String, Receiver)> {
        if node.kind() != "function_call" {
            return None;
        }
        let callee = field(node, "name")?;
        match callee.kind() {
            "method_index_expression" => {
                let receiver = if text(field(callee, "table"), source) == "self" {
                    Receiver::SelfOf(container.map(str::to_owned))
                } else {
                    Receiver::Plain
                };
                Some((text(field(callee, "method"), source).to_owned(), receiver))
            }
            "dot_index_expression" => Some((text(field(callee, "field"), source).to_owned(), Receiver::Plain)),
            _ if text(Some(callee), source) == "require" => None,
            _ => Some((text(Some(callee), source).to_owned(), Receiver::Plain)),
        }
    }

    /// require("a.b") or require "a.b"
    fn imports(&self, root: Node, ctx: &Context, source: &str, add: &mut Add) {
        let dir = dirname(ctx.path);
        for call in descendants(root, &["function_call"]) {
            if text(field(call, "name"), source) != "require" {
                continue;
            }
            let Some(module) = string_value(field(call, "arguments"), source) else { continue };
            let path = module.replace('.', "/");
            let candidates = [
                format!("{path}.lua"),
                format!("{path}/init.lua"),
                format!("lua/{path}.lua"),
                format!("lua/{path}/init.lua"),
                format!("src/{path}.lua"),
                format!("src/{path}/init.lua"),
                join(&dir, &format!("{path}.lua")),
            ];
            add(file_in(ctx.root, &candidates), false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::read;
    use crate::code::languages::Language;

    #[test]
    fn reads_lua_functions_and_methods() {
        let (tree, reading) = read(
            Language::Lua,
            "lua/m.lua",
            "local M = {}\nfunction M.start() M.step() end\nlocal function step() end\nfunction M:go() self:run() end\nlocal f = function() step() end\nreturn M\n",
        );
        assert_eq!(tree, ["Function M.start exp :2", "Function step :3", "Function M:go exp :4", "Function f :5"]);
        let names: Vec<&str> = reading.calls.iter().map(|call| call.name.as_str()).collect();
        assert_eq!(names, ["step", "run", "step"]);
    }
}
