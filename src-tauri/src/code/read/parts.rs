//! A node's parts and what more than one language reads the same way: text, children, names, string literals, a call
//! on a name, `new T()`, and a package's first file.

use tree_sitter::Node;

use super::Receiver;

pub fn text<'s>(node: Option<Node>, source: &'s str) -> &'s str {
    node.and_then(|node| node.utf8_text(source.as_bytes()).ok()).unwrap_or("")
}

pub fn named_children<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

pub fn field<'t>(node: Node<'t>, name: &str) -> Option<Node<'t>> {
    node.child_by_field_name(name)
}

pub fn has_child(node: Node, kind: &str) -> bool {
    named_children(node).iter().any(|child| child.kind() == kind)
}

pub fn is_field(parent: Node, name: &str, node: Node) -> bool {
    field(parent, name).is_some_and(|value| value.id() == node.id())
}

/// Every node under `node` (itself included) whose kind is one of `kinds`, in order.
pub fn descendants<'t>(node: Node<'t>, kinds: &[&str]) -> Vec<Node<'t>> {
    let mut out = Vec::new();
    let mut stack = vec![node];
    while let Some(next) = stack.pop() {
        if kinds.contains(&next.kind()) {
            out.push(next);
        }
        let mut children = named_children(next);
        children.reverse();
        stack.extend(children);
    }
    out
}

/// The last part of a qualified name: `App\Models\User` is `User`, `a::b` is `b`, `M.f` is `f`.
pub fn last(value: &str) -> &str {
    value.split(['\\', '.']).flat_map(|part| part.split("::")).filter(|part| !part.is_empty()).last().unwrap_or("")
}

pub fn unquote(value: &str) -> &str {
    let value = value.strip_prefix(['\'', '"']).unwrap_or(value);
    value.strip_suffix(['\'', '"']).unwrap_or(value)
}

/// `List<int>` is List.
pub fn without_generics(name: &str) -> String {
    name.split('<').next().unwrap_or(name).trim().to_owned()
}

/// The name a function gets from what it is assigned to: `$f = function`, `local f = function`, `let f = |x|`.
pub fn assigned_name(node: Node, source: &str) -> Option<String> {
    let mut parent = node.parent()?;
    // Lua: `local f = function () end` is variable_declaration > assignment_statement > expression_list > function.
    if parent.kind() == "expression_list" {
        parent = parent.parent()?;
    }
    if matches!(
        parent.kind(),
        "assignment_expression" | "assignment_statement" | "assignment" | "short_var_declaration"
    ) {
        let left = field(parent, "left")
            .or_else(|| named_children(parent).into_iter().find(|child| child.kind() == "variable_list"))?;
        let target = if matches!(left.kind(), "variable_list" | "expression_list") {
            named_children(left).into_iter().next()?
        } else {
            left
        };
        return Some(text(Some(target), source).trim_start_matches('$').to_owned());
    }
    if parent.kind() == "let_declaration" {
        let name = text(field(parent, "pattern"), source);
        return (!name.is_empty()).then(|| name.to_owned());
    }
    // Java and C# `Runnable r = () -> …`, C++ `auto f = [](int x) { … }`.
    if matches!(parent.kind(), "variable_declarator" | "init_declarator") {
        let name = text(field(parent, "name").or_else(|| field(parent, "declarator")), source);
        return (!name.is_empty()).then(|| name.to_owned());
    }
    None
}

/// A call on a name: on a type when it is capitalised (`Order.make()`), else on a value or a namespace.
pub fn on_name(name: &str) -> Receiver {
    if name.starts_with(|c: char| c.is_ascii_uppercase()) {
        Receiver::Type(name.to_owned())
    } else {
        Receiver::Plain
    }
}

/// `new Order(…)` as a call to the class Order, as PHP's `new` is.
pub fn created(type_: Option<Node>, source: &str) -> Option<(String, Receiver)> {
    let name = last(&without_generics(text(type_, source))).to_owned();
    (!name.is_empty()).then(|| (name.clone(), Receiver::Type(name)))
}

/// A string literal's content: its string_content parts, or the text between its quotes.
pub fn string_value(node: Option<Node>, source: &str) -> Option<String> {
    let node = node?;
    let parts = descendants(node, &["string_content", "string_value"]);
    if !parts.is_empty() {
        return Some(parts.iter().map(|part| text(Some(*part), source)).collect());
    }
    let whole = text(Some(node), source);
    let quoted = whole.len() >= 2 && whole.starts_with(['\'', '"']) && whole.ends_with(['\'', '"']);
    quoted.then(|| whole[1..whole.len() - 1].to_owned())
}

/// A Go package's or C# namespace's first file by name, Go's tests left out: the file its importers lean on stands for
/// the folder.
pub fn package_file(root: &str, folder: &str, extension: &str) -> Option<String> {
    let dir = std::path::Path::new(root).join(folder);
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(extension) && !name.ends_with("_test.go"))
        .collect();
    names.sort();
    let name = names.into_iter().next()?;
    Some(if folder.is_empty() { name } else { format!("{folder}/{name}") })
}
