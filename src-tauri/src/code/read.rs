//! One PHP, Rust, Lua, Python or Go file through tree-sitter (the native grammars):
//!
//! - Functions: the same tree as the sidecar builds for TypeScript: a function written inside another is its child, and
//!   an unnamed function only stays when a named one sits inside it.
//! - Imports: resolved to files per language (PHP `use` through Composer's PSR-4 map and `require`/`include`; Rust `mod`
//!   and `use`; Lua `require`; Python `import` and `from … import`; Go `import` of the module's own packages, through
//!   go.mod, to a file of that package).
//! - Calls: by the callee's name and how it is called; `guess.rs` matches them to functions across files.

use std::collections::HashMap;

use tree_sitter::{Node, Parser};

use super::languages::Language;
use super::project::{dirname, file_in, join, rust_crate_root};
use super::types::{FileImport, FunctionKind, FunctionNode};

/// How a call names its callee: plainly, on the function's own object (`$this->`, `self.`), or on a type (`A::`).
#[derive(Debug, Clone, PartialEq)]
pub enum Receiver {
    Plain,
    SelfOf(Option<String>),
    Type(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    /// The id of the calling function.
    pub from: String,
    pub name: String,
    pub receiver: Receiver,
}

/// What one file holds, before calls are matched across files.
#[derive(Debug, Clone, Default)]
pub struct FileReading {
    pub tree: Vec<FunctionNode>,
    pub calls: Vec<Call>,
    pub imports: Vec<FileImport>,
    /// For PHP: the classes the file declares, by fully qualified name.
    pub classes: Vec<String>,
}

/// What resolving a file's imports needs.
pub struct Context<'a> {
    pub root: &'a str,
    pub path: &'a str,
    /// PSR-4 prefixes to folders, from composer.json.
    pub psr4: &'a [(String, String)],
    /// Rust crates of the repository: crate name (with underscores) to its root module file.
    pub crates: &'a HashMap<String, String>,
    /// The Go module's path from go.mod; empty without one.
    pub go_module: &'a str,
}

fn grammar(language: Language) -> Option<tree_sitter::Language> {
    Some(match language {
        Language::Php => tree_sitter_php::LANGUAGE_PHP.into(),
        Language::Rust => tree_sitter_rust::LANGUAGE.into(),
        Language::Lua => tree_sitter_lua::LANGUAGE.into(),
        Language::Python => tree_sitter_python::LANGUAGE.into(),
        Language::Go => tree_sitter_go::LANGUAGE.into(),
        Language::Typescript => return None,
    })
}

/* ---------- a node's parts ---------- */

fn text<'s>(node: Option<Node>, source: &'s str) -> &'s str {
    node.and_then(|node| node.utf8_text(source.as_bytes()).ok()).unwrap_or("")
}

fn named_children<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

fn field<'t>(node: Node<'t>, name: &str) -> Option<Node<'t>> {
    node.child_by_field_name(name)
}

fn has_child(node: Node, kind: &str) -> bool {
    named_children(node).iter().any(|child| child.kind() == kind)
}

/// Every node under `node` (itself included) whose kind is one of `kinds`, in order.
fn descendants<'t>(node: Node<'t>, kinds: &[&str]) -> Vec<Node<'t>> {
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
fn last(value: &str) -> &str {
    value.split(['\\', '.']).flat_map(|part| part.split("::")).filter(|part| !part.is_empty()).last().unwrap_or("")
}

fn unquote(value: &str) -> &str {
    let value = value.strip_prefix(['\'', '"']).unwrap_or(value);
    value.strip_suffix(['\'', '"']).unwrap_or(value)
}

/* ---------- functions ---------- */

fn kind_of(language: Language, kind: &str) -> Option<FunctionKind> {
    use FunctionKind::*;
    Some(match (language, kind) {
        (Language::Php, "function_definition") => Function,
        (Language::Php, "method_declaration") => Method,
        (Language::Php, "class_declaration" | "interface_declaration" | "trait_declaration" | "enum_declaration") => {
            Class
        }
        (Language::Php, "anonymous_function" | "anonymous_function_creation_expression") => Function,
        (Language::Php, "arrow_function") => Arrow,
        (Language::Rust, "function_item" | "function_signature_item") => Function,
        (Language::Rust, "impl_item" | "trait_item" | "mod_item") => Class,
        (Language::Rust, "closure_expression") => Arrow,
        (Language::Lua, "function_declaration" | "function_definition") => Function,
        (Language::Python, "function_definition") => Function,
        (Language::Python, "class_definition") => Class,
        (Language::Python, "lambda") => Arrow,
        (Language::Go, "function_declaration") => Function,
        (Language::Go, "method_declaration") => Method,
        (Language::Go, "func_literal") => Arrow,
        _ => return None,
    })
}

/// Rust items that are containers only when they have a body: `mod util;` declares a file, it holds nothing.
fn is_container(language: Language, node: Node) -> bool {
    kind_of(language, node.kind()).is_some() && (node.kind() != "mod_item" || field(node, "body").is_some())
}

/// The name a function gets from what it is assigned to: `$f = function`, `local f = function`, `let f = |x|`.
fn assigned_name(node: Node, source: &str) -> Option<String> {
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
    None
}

fn name_of(language: Language, node: Node, source: &str) -> Option<String> {
    let own = || {
        let name = text(field(node, "name"), source);
        (!name.is_empty()).then(|| name.to_owned())
    };
    match language {
        Language::Php => own().or_else(|| assigned_name(node, source)),
        Language::Rust => match node.kind() {
            "impl_item" => {
                let target = text(field(node, "type"), source);
                Some(match field(node, "trait") {
                    Some(trait_) => format!("impl {} for {target}", text(Some(trait_), source)),
                    None => format!("impl {target}"),
                })
            }
            "closure_expression" => assigned_name(node, source),
            _ => own(),
        },
        Language::Lua if node.kind() == "function_declaration" => own(),
        Language::Lua => assigned_name(node, source),
        Language::Python if node.kind() == "lambda" => assigned_name(node, source),
        Language::Python => own(),
        // A Go method by its receiver's type too: `func (o *Order) Total()` is Order.Total.
        Language::Go => match node.kind() {
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
        },
        Language::Typescript => None,
    }
}

fn exported(language: Language, node: Node, source: &str) -> bool {
    match language {
        Language::Php => {
            node.kind() != "method_declaration"
                || !named_children(node)
                    .iter()
                    .any(|child| child.kind() == "visibility_modifier" && text(Some(*child), source) != "public")
        }
        Language::Rust => has_child(node, "visibility_modifier"),
        // `local function f` stays in its file; `function M.f` and `function f` do not.
        Language::Lua => {
            node.kind() == "function_declaration" && {
                let source = text(Some(node), source);
                !(source.starts_with("local") && !source[5..].starts_with(|c: char| c.is_alphanumeric() || c == '_'))
            }
        }
        // Python: names starting with an underscore are the module's or class's own.
        Language::Python => !text(field(node, "name"), source).starts_with('_'),
        // Go: a capital first letter exports.
        Language::Go => text(field(node, "name"), source).starts_with(|c: char| c.is_uppercase()),
        Language::Typescript => false,
    }
}

fn is_async(language: Language, node: Node, source: &str) -> bool {
    match language {
        Language::Rust => named_children(node).iter().any(|child| {
            child.kind() == "function_modifiers"
                && text(Some(*child), source).split_whitespace().any(|word| word == "async")
        }),
        Language::Python => node.kind() == "function_definition" && text(Some(node), source).starts_with("async"),
        _ => false,
    }
}

/* ---------- reading a file ---------- */

struct Reader<'a> {
    language: Language,
    source: &'a str,
    path: &'a str,
    reading: FileReading,
    namespace: String,
}

impl Reader<'_> {
    fn visit(&mut self, node: Node, into: &mut Vec<FunctionNode>, owner: Option<&str>, container: Option<&str>) {
        let source = self.source;
        if self.language == Language::Php && node.kind() == "namespace_definition" {
            self.namespace = text(field(node, "name"), source).to_owned();
        }
        if is_container(self.language, node) {
            let name = name_of(self.language, node, source);
            let mut kind = kind_of(self.language, node.kind()).expect("a container");
            let plain_in_type = matches!(
                (self.language, node.kind()),
                (Language::Rust, "function_item") | (Language::Python, "function_definition")
            );
            if plain_in_type && container.is_some() {
                kind = FunctionKind::Method;
            }
            let start = node.start_position();
            let mut item = FunctionNode {
                id: format!("{}:{}:{}", self.path, start.row + 1, start.column + 1),
                name: name.clone().unwrap_or_else(|| "(anonymous)".into()),
                kind,
                line: start.row as u32 + 1,
                end_line: node.end_position().row as u32 + 1,
                exported: exported(self.language, node, source),
                is_async: is_async(self.language, node, source),
                anonymous: name.is_none(),
                test: false,
                children: Vec::new(),
            };
            let is_type = kind == FunctionKind::Class;
            if self.language == Language::Php && is_type {
                if let Some(name) = &name {
                    let qualified =
                        if self.namespace.is_empty() { name.clone() } else { format!("{}\\{name}", self.namespace) };
                    self.reading.classes.push(qualified);
                }
            }
            // The functions in a class, impl or trait are its methods (`$this`, `self`); an inline Rust `mod` holds plain ones.
            let opens = is_type && node.kind() != "mod_item";
            let inner_container =
                if opens { name.clone().or(container.map(str::to_owned)) } else { container.map(str::to_owned) };
            let calls_before = self.reading.calls.len();
            let mut children = Vec::new();
            for child in named_children(node) {
                self.visit(child, &mut children, Some(&item.id), inner_container.as_deref());
            }
            item.children = children;
            if item.anonymous && !item.children.iter().any(|child| !child.anonymous || !child.children.is_empty()) {
                // Not kept: its calls count for the function it is written in, or go when it is written at the top level.
                let made: Vec<Call> = self.reading.calls.drain(calls_before..).collect();
                if let Some(owner) = owner {
                    for mut call in made {
                        if call.from == item.id {
                            call.from = owner.to_owned();
                        }
                        self.reading.calls.push(call);
                    }
                }
                return;
            }
            into.push(item);
            return;
        }
        if let (Some((name, receiver)), Some(owner)) = (call_of(self.language, node, container, source), owner) {
            self.reading.calls.push(Call { from: owner.to_owned(), name, receiver });
        }
        if self.language == Language::Php {
            if let Some(owner) = owner {
                if let Some(call) = php_hook_callback(node, owner, source) {
                    self.reading.calls.push(call);
                }
            }
        }
        for child in named_children(node) {
            self.visit(child, into, owner, container);
        }
    }
}

/// Reads one file: its functions, its call sites by name, and its imports resolved to files.
pub fn read_file(language: Language, ctx: &Context, source: &str) -> FileReading {
    let Some(grammar) = grammar(language) else { return FileReading::default() };
    let mut parser = Parser::new();
    if parser.set_language(&grammar).is_err() {
        return FileReading::default();
    }
    let Some(tree) = parser.parse(source, None) else { return FileReading::default() };
    let mut reader =
        Reader { language, source, path: ctx.path, reading: FileReading::default(), namespace: String::new() };
    let mut top = Vec::new();
    reader.visit(tree.root_node(), &mut top, None, None);
    reader.reading.tree = top;
    reader.reading.imports = imports_of(language, tree.root_node(), ctx, source);
    reader.reading
}

/// The callee of a call node by name, and how it is called; None when the node is not a call.
fn call_of(language: Language, node: Node, container: Option<&str>, source: &str) -> Option<(String, Receiver)> {
    let own = || Receiver::SelfOf(container.map(str::to_owned));
    match language {
        Language::Php => match node.kind() {
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
        },
        Language::Rust => {
            if node.kind() != "call_expression" {
                return None;
            }
            let callee = field(node, "function")?;
            match callee.kind() {
                "field_expression" => {
                    let receiver = if text(field(callee, "value"), source) == "self" { own() } else { Receiver::Plain };
                    Some((text(field(callee, "field"), source).to_owned(), receiver))
                }
                "scoped_identifier" => {
                    let type_ = last(text(field(callee, "path"), source));
                    let name = text(field(callee, "name"), source).to_owned();
                    let receiver = if type_.starts_with(|c: char| c.is_ascii_uppercase()) {
                        Receiver::Type(type_.to_owned())
                    } else {
                        Receiver::Plain
                    };
                    Some((name, receiver))
                }
                "generic_function" => Some((last(text(field(callee, "function"), source)).to_owned(), Receiver::Plain)),
                _ => Some((last(text(Some(callee), source)).to_owned(), Receiver::Plain)),
            }
        }
        // Lua: f(), M.f(), obj:m()
        Language::Lua => {
            if node.kind() != "function_call" {
                return None;
            }
            let callee = field(node, "name")?;
            match callee.kind() {
                "method_index_expression" => {
                    let receiver = if text(field(callee, "table"), source) == "self" { own() } else { Receiver::Plain };
                    Some((text(field(callee, "method"), source).to_owned(), receiver))
                }
                "dot_index_expression" => Some((text(field(callee, "field"), source).to_owned(), Receiver::Plain)),
                _ if text(Some(callee), source) == "require" => None,
                _ => Some((text(Some(callee), source).to_owned(), Receiver::Plain)),
            }
        }
        // Python: f(), self.m(), Order.make(), module.f()
        Language::Python => {
            if node.kind() != "call" {
                return None;
            }
            let callee = field(node, "function")?;
            match callee.kind() {
                "attribute" => {
                    let object = text(field(callee, "object"), source);
                    let name = text(field(callee, "attribute"), source).to_owned();
                    let receiver = if object == "self" || object == "cls" {
                        own()
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
        // Go: f(), pkg.F(), value.Method()
        Language::Go => {
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
        Language::Typescript => None,
    }
}

/// WordPress hooks as calls: `add_action('init', 'boot')` and `add_filter('the_title', [$this, 'title'])` make WordPress
/// call that function later, so the function that registers the hook counts as calling it.
fn php_hook_callback(node: Node, owner: &str, source: &str) -> Option<Call> {
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
            (!name.is_empty()).then(|| Call { from: owner.to_owned(), name, receiver: Receiver::Plain })
        }
        "array_creation_expression" => {
            let elements: Vec<&str> =
                named_children(value).into_iter().map(|element| text(Some(element), source)).collect();
            let method = unquote(elements.get(1)?).to_owned();
            let receiver = if elements[0].contains("$this") { Receiver::SelfOf(None) } else { Receiver::Plain };
            (!method.is_empty()).then(|| Call { from: owner.to_owned(), name: method, receiver })
        }
        _ => None,
    }
}

/* ---------- imports ---------- */

/// A string literal's content: its string_content parts, or the text between its quotes.
fn string_value(node: Option<Node>, source: &str) -> Option<String> {
    let node = node?;
    let parts = descendants(node, &["string_content", "string_value"]);
    if !parts.is_empty() {
        return Some(parts.iter().map(|part| text(Some(*part), source)).collect());
    }
    let whole = text(Some(node), source);
    let quoted = whole.len() >= 2 && whole.starts_with(['\'', '"']) && whole.ends_with(['\'', '"']);
    quoted.then(|| whole[1..whole.len() - 1].to_owned())
}

fn imports_of(language: Language, root_node: Node, ctx: &Context, source: &str) -> Vec<FileImport> {
    let mut out = Vec::new();
    let mut add = |to: Option<String>, type_only: bool| {
        if let Some(to) = to.filter(|to| to != ctx.path) {
            out.push(FileImport { to, type_only });
        }
    };
    let dir = dirname(ctx.path);
    match language {
        Language::Php => {
            for clause in descendants(root_node, &["namespace_use_clause"]) {
                let name =
                    named_children(clause).into_iter().find(|child| matches!(child.kind(), "qualified_name" | "name"));
                if let Some(name) = name {
                    add(psr4_file(ctx, text(Some(name), source).trim_start_matches('\\')), true);
                }
            }
            let includes =
                ["require_expression", "require_once_expression", "include_expression", "include_once_expression"];
            for include in descendants(root_node, &includes) {
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
        Language::Rust => {
            let file = ctx.path.rsplit('/').next().unwrap_or(ctx.path);
            let is_root_file = matches!(file, "main.rs" | "lib.rs" | "mod.rs");
            // Where this file's submodules live: next to main.rs, lib.rs and mod.rs, else in a folder named after the file.
            let child_dir =
                if is_root_file { dir.clone() } else { ctx.path.strip_suffix(".rs").unwrap_or(ctx.path).to_owned() };
            for item in descendants(root_node, &["mod_item"]) {
                if field(item, "body").is_some() {
                    continue;
                }
                let name = text(field(item, "name"), source);
                add(file_in(ctx.root, &[format!("{child_dir}/{name}.rs"), format!("{child_dir}/{name}/mod.rs")]), true);
            }
            for declaration in descendants(root_node, &["use_declaration"]) {
                let Some(argument) = field(declaration, "argument") else { continue };
                let path = use_path(text(Some(argument), source));
                add(rust_module_file(ctx, &path, &child_dir, &dir), true);
            }
        }
        // Lua: require("a.b") or require "a.b"
        Language::Lua => {
            for call in descendants(root_node, &["function_call"]) {
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
        // Python: import a.b, from a.b import c, from . import c, from ..x import y
        Language::Python => {
            for statement in descendants(root_node, &["import_statement", "import_from_statement"]) {
                if statement.kind() == "import_statement" {
                    for name in descendants(statement, &["dotted_name"]) {
                        // `import a.b as c`: the dotted name is the module.
                        if name.parent().is_some_and(|parent| parent.kind() == "aliased_import")
                            && field(name.parent().unwrap(), "alias") == Some(name)
                        {
                            continue;
                        }
                        add(python_module_file(ctx, &dir, text(Some(name), source), &[]), false);
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
                add(python_module_file(ctx, &dir, module, &names), false);
            }
        }
        // Go: the module's own packages, to one file of each (a package is a folder).
        Language::Go => {
            if ctx.go_module.is_empty() {
                return out;
            }
            for spec in descendants(root_node, &["import_spec"]) {
                let Some(path) = string_value(field(spec, "path"), source) else { continue };
                let folder = if path == ctx.go_module {
                    String::new()
                } else if let Some(rest) = path.strip_prefix(&format!("{}/", ctx.go_module)) {
                    rest.to_owned()
                } else {
                    continue;
                };
                add(go_package_file(ctx.root, &folder), false);
            }
        }
        Language::Typescript => {}
    }
    out
}

/// The file of a Python module: `a.b` is `a/b.py` or `a/b/__init__.py` (at the top, or under `src/`); a relative one
/// (`.x`, `..`) is found from the importing file's folder. With `names`, a name that is a module itself is preferred
/// (`from . import util` is util.py).
fn python_module_file(ctx: &Context, dir: &str, module: &str, names: &[&str]) -> Option<String> {
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

/// A Go package's first file by name, tests left out: the file its importers lean on stands for the folder.
fn go_package_file(root: &str, folder: &str) -> Option<String> {
    let dir = std::path::Path::new(root).join(folder);
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".go") && !name.ends_with("_test.go"))
        .collect();
    names.sort();
    let name = names.into_iter().next()?;
    Some(if folder.is_empty() { name } else { format!("{folder}/{name}") })
}

/// The segments of a `use` path, without a trailing `{…}` group, `::*` or `as` alias: `crate::a::{b, c}` is crate, a.
fn use_path(argument: &str) -> Vec<String> {
    let mut path = argument.trim();
    if path.ends_with('}') {
        if let Some(open) = path.find('{') {
            path = &path[..open];
        }
    }
    path = path.strip_suffix("::*").unwrap_or(path);
    if let Some((before, alias)) = path.rsplit_once(" as ") {
        if alias.trim().chars().all(|c| c.is_alphanumeric() || c == '_') {
            path = before.trim_end();
        }
    }
    path.split("::").filter(|segment| !segment.is_empty()).map(str::to_owned).collect()
}

/// A PHP class's file by Composer's PSR-4 map: `App\Models\User` with `App\` → `app/` is `app/Models/User.php`.
fn psr4_file(ctx: &Context, name: &str) -> Option<String> {
    ctx.psr4.iter().find_map(|(prefix, folder)| {
        let rest = name.strip_prefix(prefix.as_str())?;
        file_in(ctx.root, &[join(folder, &format!("{}.php", rest.replace('\\', "/")))])
    })
}

fn module_folder(file: &str) -> String {
    let name = file.rsplit('/').next().unwrap_or(file);
    if matches!(name, "main.rs" | "lib.rs" | "mod.rs") {
        dirname(file)
    } else {
        file.strip_suffix(".rs").unwrap_or(file).to_owned()
    }
}

/// The file of the deepest module a Rust `use` path names that exists: `crate::a::b::Item` is `src/a/b.rs`.
fn rust_module_file(ctx: &Context, path: &[String], child_dir: &str, dir: &str) -> Option<String> {
    let first = path.first()?.as_str();
    let rest = &path[1..];
    let base = match first {
        "crate" => rust_crate_root(ctx.root, ctx.path)?,
        "self" => ctx.path.to_owned(),
        "super" => {
            let base = if ctx.path.ends_with("/mod.rs") || ctx.path == "mod.rs" {
                join(dir, "../mod.rs")
            } else {
                format!("{dir}/mod.rs")
            };
            if file_in(ctx.root, std::slice::from_ref(&base)).is_some() {
                base
            } else {
                file_in(ctx.root, &[format!("{dir}.rs")]).unwrap_or(base)
            }
        }
        name => ctx.crates.get(name)?.clone(),
    };
    let mut found = if first == "self" { None } else { Some(base.clone()) };
    let mut folder = if first == "self" { child_dir.to_owned() } else { module_folder(&base) };
    for segment in rest {
        let Some(next) = file_in(ctx.root, &[format!("{folder}/{segment}.rs"), format!("{folder}/{segment}/mod.rs")])
        else {
            break;
        };
        folder = module_folder(&next);
        found = Some(next);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outline(items: &[FunctionNode], depth: usize, out: &mut Vec<String>) {
        for item in items {
            let exported = if item.exported { " exp" } else { "" };
            out.push(format!("{}{:?} {}{exported} :{}", "  ".repeat(depth), item.kind, item.name, item.line));
            outline(&item.children, depth + 1, out);
        }
    }

    fn read(language: Language, path: &str, source: &str) -> (Vec<String>, FileReading) {
        let crates = HashMap::new();
        let ctx = Context { root: ".", path, psr4: &[], crates: &crates, go_module: "" };
        let reading = read_file(language, &ctx, source);
        let mut lines = Vec::new();
        outline(&reading.tree, 0, &mut lines);
        (lines, reading)
    }

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

    #[test]
    fn reads_rust_impls_traits_and_closures() {
        let (tree, reading) = read(
            Language::Rust,
            "src/lib.rs",
            "pub struct A;\nimpl A {\n    pub async fn run(&self) { self.step(); B::make(); }\n    fn step(&self) {}\n}\nimpl Show for A { fn show(&self) {} }\nfn plain() { let f = |x: u32| x + 1; f(1); }\n",
        );
        assert_eq!(
            tree,
            [
                "Class impl A :2",
                "  Method run exp :3",
                "  Method step :4",
                "Class impl Show for A :6",
                "  Method show :6",
                "Function plain :7",
                "  Arrow f :7"
            ]
        );
        assert!(reading.tree[0].children[0].is_async);
        let names: Vec<&str> = reading.calls.iter().map(|call| call.name.as_str()).collect();
        assert_eq!(names, ["step", "make", "f"]);
        assert_eq!(reading.calls[1].receiver, Receiver::Type("B".into()));
    }

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

    #[test]
    fn a_use_path_without_its_group_or_alias() {
        assert_eq!(use_path("crate::a::{b, c}"), ["crate", "a"]);
        assert_eq!(use_path("super::util::*"), ["super", "util"]);
        assert_eq!(use_path("std::io::Read as R"), ["std", "io", "Read"]);
    }
}
