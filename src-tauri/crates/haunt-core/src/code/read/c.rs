//! C: functions by the name inside their declarator, `static` ones the file's own; calls as `f()`, `o->f()` and
//! `o.f()`; imports by `#include "…"` (not `<…>`). C++ (`cpp.rs`) reads the same way and adds its own on top, so what
//! both share is here.

use tree_sitter::Node;

use super::parts::{
    assigned_name, created, descendants, field, named_children, on_name, string_value, text, without_generics,
};
use super::{Add, Context, Grammar, Receiver};
use crate::code::project::{dirname, file_in, join};
use crate::code::types::FunctionKind;

pub struct C;

impl Grammar for C {
    fn grammar(&self) -> tree_sitter::Language {
        tree_sitter_c::LANGUAGE.into()
    }

    fn kind(&self, kind: &str) -> Option<FunctionKind> {
        (kind == "function_definition").then_some(FunctionKind::Function)
    }

    fn name(&self, node: Node, source: &str) -> Option<String> {
        name(node, source)
    }

    fn exported(&self, node: Node, source: &str) -> bool {
        exported(node, source)
    }

    fn call(&self, node: Node, container: Option<&str>, source: &str) -> Option<(String, Receiver)> {
        call(node, container, source)
    }

    fn imports(&self, root: Node, ctx: &Context, source: &str, add: &mut Add) {
        imports(root, ctx, source, add)
    }
}

/// A C or C++ function's or lambda's name.
pub fn name(node: Node, source: &str) -> Option<String> {
    match node.kind() {
        "function_definition" => function_name(node, source),
        "lambda_expression" => assigned_name(node, source),
        _ => {
            let name = text(field(node, "name"), source);
            (!name.is_empty()).then(|| name.to_owned())
        }
    }
}

pub fn exported(node: Node, source: &str) -> bool {
    node.kind() != "lambda_expression" && public(node, source)
}

/// A function's name, from inside its declarator: `int *next(void)` is next, `void Order::total()` is Order::total,
/// `Money::operator int()` is Money::operator int.
fn function_name(node: Node, source: &str) -> Option<String> {
    if let Some(name) = field(node, "declarator").and_then(|declarator| conversion_name(declarator, source)) {
        return Some(name);
    }
    let name: String = text(field(function_declarator(node)?, "declarator"), source).split_whitespace().collect();
    (!name.is_empty()).then_some(name)
}

/// The function_declarator inside a definition or declaration: `int *next(void)` has one under its pointer.
pub fn function_declarator(node: Node) -> Option<Node> {
    let mut declarator = field(node, "declarator")?;
    while declarator.kind() != "function_declarator" {
        declarator = field(declarator, "declarator")
            .or_else(|| named_children(declarator).into_iter().find(|child| child.kind().ends_with("declarator")))?;
    }
    Some(declarator)
}

/// C++: a conversion operator by the type it converts to, without its namespace: `operator int() const` is operator int,
/// `Money::operator std::string()` is Money::operator string. It has no function_declarator of its own to name it.
pub fn conversion_name(declarator: Node, source: &str) -> Option<String> {
    match declarator.kind() {
        "operator_cast" => {
            let mut type_ = field(declarator, "type")?;
            while type_.kind() == "qualified_identifier" {
                type_ = field(type_, "name")?;
            }
            let type_: Vec<&str> = text(Some(type_), source).split_whitespace().collect();
            Some(format!("operator {}", type_.join(" ")))
        }
        "qualified_identifier" => {
            let name = conversion_name(field(declarator, "name")?, source)?;
            let scope: String = text(field(declarator, "scope"), source).split_whitespace().collect();
            Some(format!("{scope}::{name}"))
        }
        _ => None,
    }
}

/// A member as the access before it says (`public:`; a struct's members are public, a class's are not), and anything
/// else unless it is `static` in its file.
pub fn public(node: Node, source: &str) -> bool {
    let at = node.parent().filter(|parent| parent.kind() == "template_declaration").unwrap_or(node);
    let Some(list) = at.parent().filter(|parent| parent.kind() == "field_declaration_list") else {
        return !named_children(node)
            .iter()
            .any(|child| child.kind() == "storage_class_specifier" && text(Some(*child), source) == "static");
    };
    let mut sibling = at.prev_named_sibling();
    while let Some(previous) = sibling {
        if previous.kind() == "access_specifier" {
            return text(Some(previous), source) == "public";
        }
        sibling = previous.prev_named_sibling();
    }
    list.parent().is_some_and(|holder| holder.kind() == "struct_specifier")
}

/// f(), o->f(), o.f(), this->m(), Order::make(), std::sort(), new Order()
pub fn call(node: Node, container: Option<&str>, source: &str) -> Option<(String, Receiver)> {
    match node.kind() {
        "call_expression" => {
            let callee = field(node, "function")?;
            match callee.kind() {
                "identifier" => Some((text(Some(callee), source).to_owned(), Receiver::Plain)),
                "template_function" => Some((text(field(callee, "name"), source).to_owned(), Receiver::Plain)),
                "field_expression" => {
                    let receiver = if text(field(callee, "argument"), source) == "this" {
                        Receiver::SelfOf(container.map(str::to_owned))
                    } else {
                        Receiver::Plain
                    };
                    let member = field(callee, "field")?;
                    let name = if member.kind() == "template_method" { field(member, "name") } else { Some(member) };
                    Some((text(name, source).to_owned(), receiver))
                }
                "qualified_identifier" => {
                    let (scope, name) = text(Some(callee), source).rsplit_once("::")?;
                    Some((without_generics(name), on_name(scope.rsplit("::").next().unwrap_or(scope))))
                }
                _ => None,
            }
        }
        "new_expression" => created(field(node, "type"), source),
        _ => None,
    }
}

/// #include "x.h" from the file's folder, the top, include/ or src/; <x.h> is the system's.
pub fn imports(root: Node, ctx: &Context, source: &str, add: &mut Add) {
    let dir = dirname(ctx.path);
    for include in descendants(root, &["preproc_include"]) {
        let Some(path) = field(include, "path").filter(|path| path.kind() == "string_literal") else {
            continue;
        };
        let Some(name) = string_value(Some(path), source) else { continue };
        let candidates = [join(&dir, &name), name.clone(), format!("include/{name}"), format!("src/{name}")];
        add(file_in(ctx.root, &candidates), false);
    }
}

#[cfg(test)]
mod tests {
    use super::super::testing::read;
    use crate::code::languages::{language_of, Language};

    #[test]
    fn reads_c_functions_static_ones_and_calls() {
        let (tree, reading) = read(
            Language::C,
            "src/order.c",
            "#include \"order.h\"\n#include <stdio.h>\n\nstatic int sum(int x) { return x; }\n\nint total(struct order *o) {\n  return sum(1) + o->rate(2);\n}\n\nint *next(void) { return 0; }\n",
        );
        assert_eq!(tree, ["Function sum :4", "Function total exp :6", "Function next exp :10"]);
        let names: Vec<&str> = reading.calls.iter().map(|call| call.name.as_str()).collect();
        assert_eq!(names, ["sum", "rate"]);
    }

    #[test]
    fn a_header_is_read_as_c() {
        assert_eq!(language_of("include/order.h"), Some(Language::C));
        assert_eq!(language_of("include/order.hpp"), Some(Language::Cpp));
        // A declaration is no function; a static inline one written in the header is.
        let (tree, _) =
            read(Language::C, "include/order.h", "int total(void);\nstatic inline int sum(int x) { return x; }\n");
        assert_eq!(tree, ["Function sum :2"]);
    }
}
