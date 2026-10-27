//! One PHP, Rust, Lua, Python, Go, Java, C#, Ruby, C or C++ file through tree-sitter (the native grammars):
//!
//! - Functions: the same tree as the sidecar builds for TypeScript: a function written inside another is its child, and
//!   an unnamed function only stays when a named one sits inside it.
//! - Imports: resolved to files per language (PHP `use` through Composer's PSR-4 map and `require`/`include`; Rust `mod`
//!   and `use`; Lua `require`; Python `import` and `from … import`; Go `import` of the module's own packages, through
//!   go.mod, to a file of that package; Java `import` from the file's source folder; C# `using` of a namespace, to its
//!   folder's first file; Ruby `require` and `require_relative`; C and C++ `#include "…"`, not `<…>`).
//! - Calls: by the callee's name and how it is called; `guess.rs` matches them to functions across files. A Ruby name
//!   alone (`total`) is a call unless it is a local variable of its method; a block's calls count for its method.
//! - C++ methods only declared in their class are listed as methods without calls, unless the file defines them.
//! - C# properties and indexers with code, and operators (`operator +`, `implicit operator Money`), are methods.

use std::collections::{HashMap, HashSet};

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
    /// For C++: the ids of the methods only declared in their class (`int total();`), with no body here.
    pub declarations: Vec<String>,
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
        Language::Java => tree_sitter_java::LANGUAGE.into(),
        Language::Csharp => tree_sitter_c_sharp::LANGUAGE.into(),
        Language::Ruby => tree_sitter_ruby::LANGUAGE.into(),
        Language::C => tree_sitter_c::LANGUAGE.into(),
        Language::Cpp => tree_sitter_cpp::LANGUAGE.into(),
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
        (Language::Java | Language::Csharp, "method_declaration" | "constructor_declaration") => Method,
        (
            Language::Java,
            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration",
        ) => Class,
        (Language::Java | Language::Csharp, "lambda_expression") => Arrow,
        (Language::Csharp, "local_function_statement" | "anonymous_method_expression") => Function,
        (
            Language::Csharp,
            "property_declaration" | "indexer_declaration" | "operator_declaration" | "conversion_operator_declaration",
        ) => Method,
        (
            Language::Csharp,
            "class_declaration"
            | "struct_declaration"
            | "interface_declaration"
            | "record_declaration"
            | "namespace_declaration",
        ) => Class,
        (Language::Ruby, "method") => Function,
        (Language::Ruby, "singleton_method") => Method,
        (Language::Ruby, "class" | "module") => Class,
        (Language::Ruby, "lambda") => Arrow,
        (Language::C | Language::Cpp, "function_definition") => Function,
        (Language::Cpp, "class_specifier" | "struct_specifier" | "namespace_definition") => Class,
        (Language::Cpp, "lambda_expression") => Arrow,
        _ => return None,
    })
}

/// Items that are containers only when they have a body: Rust's `mod util;` declares a file and C++'s `class Order;`
/// (or `struct order *o`) names a type; they hold nothing. A C# property or indexer counts when it has code: `=> …` or
/// an accessor with a body; an auto-property (`{ get; set; }`) is a field.
fn is_container(language: Language, node: Node) -> bool {
    if kind_of(language, node.kind()).is_none() {
        return false;
    }
    match node.kind() {
        "mod_item" | "class_specifier" | "struct_specifier" => field(node, "body").is_some(),
        "property_declaration" | "indexer_declaration" => {
            field(node, "value").is_some_and(|value| value.kind() == "arrow_expression_clause")
                || field(node, "accessors").is_some_and(|accessors| {
                    named_children(accessors).iter().any(|accessor| field(*accessor, "body").is_some())
                })
        }
        _ => true,
    }
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
    // Java and C# `Runnable r = () -> …`, C++ `auto f = [](int x) { … }`.
    if matches!(parent.kind(), "variable_declarator" | "init_declarator") {
        let name = text(field(parent, "name").or_else(|| field(parent, "declarator")), source);
        return (!name.is_empty()).then(|| name.to_owned());
    }
    None
}

/// A C or C++ function's name, from inside its declarator: `int *next(void)` is next, `void Order::total()` is
/// Order::total.
fn c_function_name(node: Node, source: &str) -> Option<String> {
    let name: String = text(field(c_function_declarator(node)?, "declarator"), source).split_whitespace().collect();
    (!name.is_empty()).then_some(name)
}

/// The function_declarator inside a C or C++ definition or declaration: `int *next(void)` has one under its pointer.
fn c_function_declarator(node: Node) -> Option<Node> {
    let mut declarator = field(node, "declarator")?;
    while declarator.kind() != "function_declarator" {
        declarator = field(declarator, "declarator")
            .or_else(|| named_children(declarator).into_iter().find(|child| child.kind().ends_with("declarator")))?;
    }
    Some(declarator)
}

/// C++: a method only declared in its class (`int total() const;`, `virtual void f() = 0;`, `Order();`), by its name;
/// not a field holding a function pointer (`int (*f)(int);`) nor a friend.
fn cpp_declared_method(node: Node, source: &str) -> Option<String> {
    if !matches!(node.kind(), "field_declaration" | "declaration") {
        return None;
    }
    let at = node.parent().filter(|parent| parent.kind() == "template_declaration").unwrap_or(node);
    at.parent().filter(|parent| parent.kind() == "field_declaration_list")?;
    let name = field(c_function_declarator(node)?, "declarator")?;
    if !matches!(name.kind(), "field_identifier" | "identifier" | "destructor_name" | "operator_name") {
        return None;
    }
    let name: String = text(Some(name), source).split_whitespace().collect();
    (!name.is_empty()).then_some(name)
}

/// The type a C++ method written outside its class belongs to: `Order::total` is Order, `shop::Box<T>::get` is Box.
pub fn scope_of(name: &str) -> Option<&str> {
    let (scope, _) = name.rsplit_once("::")?;
    let scope = scope.rsplit("::").next().unwrap_or(scope);
    let scope = scope.split('<').next().unwrap_or(scope);
    (!scope.is_empty()).then_some(scope)
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
        Language::Java | Language::Csharp => match node.kind() {
            "lambda_expression" | "anonymous_method_expression" => assigned_name(node, source),
            // C#: `operator +`, `implicit operator Money`, and an indexer as `this[]`.
            "operator_declaration" => Some(format!("operator {}", text(field(node, "operator"), source))),
            "conversion_operator_declaration" => {
                let mut cursor = node.walk();
                let way = node
                    .children(&mut cursor)
                    .find(|child| matches!(child.kind(), "implicit" | "explicit"))
                    .map(|child| format!("{} ", child.kind()))
                    .unwrap_or_default();
                Some(format!("{way}operator {}", text(field(node, "type"), source)))
            }
            "indexer_declaration" => Some("this[]".into()),
            _ => own(),
        },
        // Ruby: `def self.make` is self.make.
        Language::Ruby => match node.kind() {
            "singleton_method" => Some(format!("{}.{}", text(field(node, "object"), source), own()?)),
            "lambda" => assigned_name(node, source),
            _ => own(),
        },
        Language::C | Language::Cpp => match node.kind() {
            "function_definition" => c_function_name(node, source),
            "lambda_expression" => assigned_name(node, source),
            _ => own(),
        },
        Language::Typescript => None,
    }
}

/// Ruby: a method after `private` or `protected` in its class, or written as `private def x`, is the class's own.
fn ruby_public(node: Node, source: &str) -> bool {
    let wrapped = node.parent().filter(|parent| parent.kind() == "argument_list").and_then(|list| list.parent());
    if wrapped.is_some_and(|call| matches!(text(field(call, "method"), source), "private" | "protected")) {
        return false;
    }
    let mut sibling = node.prev_named_sibling();
    while let Some(previous) = sibling {
        if previous.kind() == "identifier" {
            match text(Some(previous), source) {
                "private" | "protected" => return false,
                "public" => return true,
                _ => {}
            }
        }
        sibling = previous.prev_named_sibling();
    }
    true
}

/// C and C++: a member as the access before it says (`public:`; a struct's members are public, a class's are not), and
/// anything else unless it is `static` in its file.
fn c_public(node: Node, source: &str) -> bool {
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
        // Java and C#: declared `public`; a C# namespace is open to all.
        Language::Java => named_children(node).iter().any(|child| {
            child.kind() == "modifiers" && text(Some(*child), source).split_whitespace().any(|word| word == "public")
        }),
        Language::Csharp => {
            node.kind() == "namespace_declaration"
                || named_children(node)
                    .iter()
                    .any(|child| child.kind() == "modifier" && text(Some(*child), source) == "public")
        }
        Language::Ruby => node.kind() != "lambda" && (node.kind() != "method" || ruby_public(node, source)),
        Language::C | Language::Cpp => node.kind() != "lambda_expression" && c_public(node, source),
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
        Language::Csharp => {
            named_children(node).iter().any(|child| child.kind() == "modifier" && text(Some(*child), source) == "async")
        }
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
    /// Ruby: the local variables of each method (or class body, or file), by the scope node's id.
    ruby_locals: HashMap<usize, HashSet<String>>,
    /// C++: the methods only declared in their class, as (id, class, name).
    declared: Vec<(String, String, String)>,
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
                (Language::Rust, "function_item")
                    | (Language::Python, "function_definition")
                    | (Language::Ruby, "method")
                    | (Language::Cpp, "function_definition")
            );
            if plain_in_type && container.is_some() {
                kind = FunctionKind::Method;
            }
            // C++ `void Order::total() {}`: a method of Order, written outside it.
            let outside = if self.language == Language::Cpp { name.as_deref().and_then(scope_of) } else { None }
                .map(str::to_owned);
            if outside.is_some() && kind == FunctionKind::Function {
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
            // The functions in a class, impl or trait are its methods (`$this`, `self`); an inline Rust `mod`, a C#
            // namespace and a C++ namespace hold plain ones.
            let opens =
                is_type && !matches!(node.kind(), "mod_item" | "namespace_declaration" | "namespace_definition");
            let inner_container = if opens {
                name.clone().or(container.map(str::to_owned))
            } else {
                outside.or(container.map(str::to_owned))
            };
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
        // C++ `int total();` in its class: a method with no calls, so the class shows its whole interface.
        if self.language == Language::Cpp {
            if let (Some(name), Some(class)) = (cpp_declared_method(node, source), container) {
                let start = node.start_position();
                let id = format!("{}:{}:{}", self.path, start.row + 1, start.column + 1);
                self.declared.push((id.clone(), class.to_owned(), name.clone()));
                into.push(FunctionNode {
                    id,
                    name,
                    kind: FunctionKind::Method,
                    line: start.row as u32 + 1,
                    end_line: node.end_position().row as u32 + 1,
                    exported: c_public(node, source),
                    is_async: false,
                    anonymous: false,
                    test: false,
                    children: Vec::new(),
                });
                return;
            }
        }
        if let (Some((name, receiver)), Some(owner)) = (call_of(self.language, node, container, source), owner) {
            self.reading.calls.push(Call { from: owner.to_owned(), name, receiver });
        }
        // Ruby `total` alone: a call when it is not a local variable of its method.
        if self.language == Language::Ruby && node.kind() == "identifier" {
            if let Some(owner) = owner {
                if let Some(name) = self.ruby_bare_call(node) {
                    self.reading.calls.push(Call { from: owner.to_owned(), name, receiver: Receiver::Plain });
                }
            }
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

    /// A Ruby identifier that calls a method: written as a statement or an expression (not a parameter, an assignment's
    /// target or a call's method name) and not a local variable of its method, a block's parameters included.
    fn ruby_bare_call(&mut self, node: Node) -> Option<String> {
        let source = self.source;
        let name = text(Some(node), source);
        if !ruby_expression(node)
            || matches!(name, "private" | "protected" | "public" | "module_function" | "it")
            || (name.len() == 2 && name.starts_with('_') && name.ends_with(|c: char| c.is_ascii_digit()))
        {
            return None;
        }
        let mut scope = node.parent()?;
        while !RUBY_SCOPES.contains(&scope.kind()) {
            scope = scope.parent()?;
        }
        let locals = self.ruby_locals.entry(scope.id()).or_insert_with(|| ruby_locals(scope, source));
        (!locals.contains(name)).then(|| name.to_owned())
    }
}

/// Where Ruby starts a new set of local variables: a method, a class or module body, the file. A block or a lambda sees
/// its method's.
const RUBY_SCOPES: [&str; 6] = ["method", "singleton_method", "class", "module", "singleton_class", "program"];

/// Whether a Ruby identifier stands where a value is read: a statement, an argument, an operand, a condition, an
/// assignment's value, a call's receiver.
fn ruby_expression(node: Node) -> bool {
    let Some(parent) = node.parent() else { return false };
    let is = |name: &str| field(parent, name).is_some_and(|value| value.id() == node.id());
    // `defined?(total)` asks about the name without calling it.
    let asks_defined =
        |unary: Node| unary.kind() == "unary" && unary.child(0).is_some_and(|op| op.kind() == "defined?");
    if asks_defined(parent)
        || (parent.kind() == "parenthesized_statements" && parent.parent().is_some_and(asks_defined))
    {
        return false;
    }
    match parent.kind() {
        "body_statement"
        | "block_body"
        | "then"
        | "else"
        | "begin"
        | "ensure"
        | "parenthesized_statements"
        | "do"
        | "argument_list"
        | "binary"
        | "interpolation"
        | "array"
        | "conditional"
        | "if"
        | "unless"
        | "while"
        | "until"
        | "if_modifier"
        | "unless_modifier"
        | "while_modifier"
        | "until_modifier"
        | "elsif"
        | "case"
        | "element_reference"
        | "splat_argument"
        | "hash_splat_argument"
        | "block_argument"
        | "range"
        | "right_assignment_list"
        | "unary" => true,
        "pair" => is("value"),
        "assignment" | "operator_assignment" => is("right"),
        "call" => is("receiver"),
        _ => false,
    }
}

/// The names a Ruby scope makes local: parameters (a block's too), assignments' targets, `rescue => e`, a `for`'s
/// variable and pattern variables; nested methods and classes have their own.
fn ruby_locals(scope: Node, source: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut stack = named_children(scope);
    while let Some(node) = stack.pop() {
        if RUBY_SCOPES.contains(&node.kind()) {
            continue;
        }
        if node.kind() == "identifier" && ruby_declares(node) {
            out.insert(text(Some(node), source).to_owned());
        }
        stack.extend(named_children(node));
    }
    out
}

fn ruby_declares(node: Node) -> bool {
    let Some(parent) = node.parent() else { return false };
    let is = |name: &str| field(parent, name).is_some_and(|value| value.id() == node.id());
    match parent.kind() {
        "method_parameters"
        | "block_parameters"
        | "lambda_parameters"
        | "destructured_parameter"
        | "splat_parameter"
        | "hash_splat_parameter"
        | "block_parameter"
        | "left_assignment_list"
        | "destructured_left_assignment"
        | "rest_assignment"
        | "exception_variable" => true,
        "optional_parameter" | "keyword_parameter" => is("name"),
        "assignment" | "operator_assignment" => is("left"),
        "for" => is("pattern"),
        kind => kind.ends_with("_pattern"),
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
    let mut reader = Reader {
        language,
        source,
        path: ctx.path,
        reading: FileReading::default(),
        namespace: String::new(),
        ruby_locals: HashMap::new(),
        declared: Vec::new(),
    };
    let mut top = Vec::new();
    reader.visit(tree.root_node(), &mut top, None, None);
    if !reader.declared.is_empty() {
        // A method declared in its class and defined in the same file (`int Order::total() {…}`) is listed once, as
        // its definition.
        let mut defined = HashSet::new();
        defined_outside(&top, &mut defined);
        let gone: HashSet<String> = reader
            .declared
            .iter()
            .filter(|(_, class, name)| defined.contains(&(class.clone(), name.clone())))
            .map(|(id, _, _)| id.clone())
            .collect();
        drop_ids(&mut top, &gone);
        reader.reading.declarations =
            reader.declared.into_iter().map(|(id, _, _)| id).filter(|id| !gone.contains(id)).collect();
    }
    reader.reading.tree = top;
    reader.reading.imports = imports_of(language, tree.root_node(), ctx, source);
    reader.reading
}

/// C++ methods written outside their class, as (class, method): `Order::total` is (Order, total).
fn defined_outside(items: &[FunctionNode], out: &mut HashSet<(String, String)>) {
    for item in items {
        if let Some(class) = scope_of(&item.name) {
            let method = item.name.rsplit("::").next().unwrap_or(&item.name);
            out.insert((class.to_owned(), method.to_owned()));
        }
        defined_outside(&item.children, out);
    }
}

fn drop_ids(items: &mut Vec<FunctionNode>, ids: &HashSet<String>) {
    items.retain(|item| !ids.contains(&item.id));
    for item in items {
        drop_ids(&mut item.children, ids);
    }
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
        // Java: f(), this.m(), Order.make(), order.total(), new Order()
        Language::Java => match node.kind() {
            "method_invocation" => {
                let receiver = match field(node, "object") {
                    Some(object) if object.kind() == "this" => own(),
                    Some(object) if object.kind() == "identifier" => on_name(text(Some(object), source)),
                    _ => Receiver::Plain,
                };
                Some((text(field(node, "name"), source).to_owned(), receiver))
            }
            "object_creation_expression" => created(field(node, "type"), source),
            _ => None,
        },
        // C#: F(), this.M(), Order.Make(), order.Total(), F<T>(), new Order()
        Language::Csharp => match node.kind() {
            "invocation_expression" => {
                let callee = field(node, "function")?;
                match callee.kind() {
                    "member_access_expression" => {
                        let object = text(field(callee, "expression"), source);
                        let receiver = if object == "this" {
                            own()
                        } else if object.chars().all(|c| c.is_alphanumeric() || c == '_') {
                            on_name(object)
                        } else {
                            Receiver::Plain
                        };
                        Some((without_generics(text(field(callee, "name"), source)), receiver))
                    }
                    "identifier" | "generic_name" => {
                        Some((without_generics(text(Some(callee), source)), Receiver::Plain))
                    }
                    _ => None,
                }
            }
            "object_creation_expression" => created(field(node, "type"), source),
            _ => None,
        },
        // Ruby: f(1), self.m, Order.make, order.total, Order.new; a bare `f` is read by `Reader::ruby_bare_call`.
        Language::Ruby => {
            if node.kind() != "call" {
                return None;
            }
            let name = text(field(node, "method"), source);
            let receiver = match field(node, "receiver") {
                None if matches!(name, "require" | "require_relative" | "private" | "protected" | "public") => {
                    return None
                }
                None => Receiver::Plain,
                Some(receiver) if receiver.kind() == "self" => own(),
                Some(receiver) if matches!(receiver.kind(), "constant" | "scope_resolution") => {
                    let type_ = last(text(Some(receiver), source)).to_owned();
                    if name == "new" {
                        return Some((type_.clone(), Receiver::Type(type_)));
                    }
                    Receiver::Type(type_)
                }
                Some(_) => Receiver::Plain,
            };
            (!name.is_empty()).then(|| (name.to_owned(), receiver))
        }
        // C and C++: f(), o->f(), o.f(), this->m(), Order::make(), std::sort(), new Order()
        Language::C | Language::Cpp => match node.kind() {
            "call_expression" => {
                let callee = field(node, "function")?;
                match callee.kind() {
                    "identifier" => Some((text(Some(callee), source).to_owned(), Receiver::Plain)),
                    "template_function" => Some((text(field(callee, "name"), source).to_owned(), Receiver::Plain)),
                    "field_expression" => {
                        let receiver =
                            if text(field(callee, "argument"), source) == "this" { own() } else { Receiver::Plain };
                        let member = field(callee, "field")?;
                        let name =
                            if member.kind() == "template_method" { field(member, "name") } else { Some(member) };
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
        },
        Language::Typescript => None,
    }
}

/// A call on a name: on a type when it is capitalised (`Order.make()`), else on a value or a namespace.
fn on_name(name: &str) -> Receiver {
    if name.starts_with(|c: char| c.is_ascii_uppercase()) {
        Receiver::Type(name.to_owned())
    } else {
        Receiver::Plain
    }
}

/// `List<int>` is List.
fn without_generics(name: &str) -> String {
    name.split('<').next().unwrap_or(name).trim().to_owned()
}

/// `new Order(…)` as a call to the class Order, as PHP's `new` is.
fn created(type_: Option<Node>, source: &str) -> Option<(String, Receiver)> {
    let name = last(&without_generics(text(type_, source))).to_owned();
    (!name.is_empty()).then(|| (name.clone(), Receiver::Type(name)))
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
                add(package_file(ctx.root, &folder, ".go"), false);
            }
        }
        // Java: import a.b.C is a/b/C.java in the source folder the file's own package sits in (src/main/java/…); a
        // static import or a nested class names a member of a/b.java. `import a.b.*` is left out.
        Language::Java => {
            let package = named_children(root_node)
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
            for import in named_children(root_node).into_iter().filter(|child| child.kind() == "import_declaration") {
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
        // C#: using A.B is the namespace's folder (A/B, or B), to its first file; `using X = A.B.C` and `using static`
        // name a type, so A/B/C.cs first.
        Language::Csharp => {
            for directive in descendants(root_node, &["using_directive"]) {
                let alias = field(directive, "name");
                let Some(name) = named_children(directive)
                    .into_iter()
                    .find(|child| matches!(child.kind(), "qualified_name" | "identifier") && Some(*child) != alias)
                else {
                    continue;
                };
                let segments: Vec<&str> = text(Some(name), source).split('.').map(str::trim).collect();
                // At least two segments of a longer namespace: a lone `Logging` folder is too likely someone else's.
                let shortest = segments.len().min(2);
                let found = (0..=segments.len() - shortest).find_map(|start| {
                    let rest = segments[start..].join("/");
                    [rest.clone(), format!("src/{rest}")].iter().find_map(|folder| {
                        file_in(ctx.root, &[format!("{folder}.cs")]).or_else(|| package_file(ctx.root, folder, ".cs"))
                    })
                });
                add(found, true);
            }
        }
        // Ruby: require_relative 'x' from the file's folder; require 'x' from lib/ or the top, gems left out.
        Language::Ruby => {
            for call in descendants(root_node, &["call"]) {
                let method = text(field(call, "method"), source);
                if field(call, "receiver").is_some() || !matches!(method, "require" | "require_relative") {
                    continue;
                }
                let Some(name) = string_value(field(call, "arguments"), source) else { continue };
                let file = if name.ends_with(".rb") { name } else { format!("{name}.rb") };
                let candidates = if method == "require_relative" {
                    vec![join(&dir, &file)]
                } else {
                    vec![format!("lib/{file}"), file.clone(), join(&dir, &file)]
                };
                add(file_in(ctx.root, &candidates), false);
            }
        }
        // C and C++: #include "x.h" from the file's folder, the top, include/ or src/; <x.h> is the system's.
        Language::C | Language::Cpp => {
            for include in descendants(root_node, &["preproc_include"]) {
                let Some(path) = field(include, "path").filter(|path| path.kind() == "string_literal") else {
                    continue;
                };
                let Some(name) = string_value(Some(path), source) else { continue };
                let candidates = [join(&dir, &name), name.clone(), format!("include/{name}"), format!("src/{name}")];
                add(file_in(ctx.root, &candidates), false);
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

/// A Go package's or C# namespace's first file by name, Go's tests left out: the file its importers lean on stands for
/// the folder.
fn package_file(root: &str, folder: &str, extension: &str) -> Option<String> {
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

    fn calls(reading: &FileReading) -> Vec<(String, Receiver)> {
        reading.calls.iter().map(|call| (call.name.clone(), call.receiver.clone())).collect()
    }

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

    #[test]
    fn reads_csharp_namespaces_records_structs_and_calls() {
        let (tree, reading) = read(
            Language::Csharp,
            "Shop/Order.cs",
            "namespace Shop {\n  public record Line(int X);\n  public class Order {\n    public async Task<int> Total() { return this.Sum() + Rate.Of(1) + Helper<int>(); }\n    private int Sum() {\n      int Local() => 2;\n      Func<int> f = () => Local();\n      return new Pricing().Rate();\n    }\n  }\n  internal struct Point { public int Y() { return 1; } }\n  interface IShown { void Show(); }\n}\n",
        );
        assert_eq!(
            tree,
            [
                "Class Shop exp :1",
                "  Class Line exp :2",
                "  Class Order exp :3",
                "    Method Total exp :4",
                "    Method Sum :5",
                "      Function Local :6",
                "      Arrow f :7",
                "  Class Point :11",
                "    Method Y exp :11",
                "  Class IShown :12",
                "    Method Show :12"
            ]
        );
        assert!(reading.tree[0].children[1].children[0].is_async);
        let own = || Receiver::SelfOf(Some("Order".into()));
        assert_eq!(
            calls(&reading),
            [
                ("Sum".into(), own()),
                ("Of".into(), Receiver::Type("Rate".into())),
                ("Helper".into(), Receiver::Plain),
                ("Local".into(), Receiver::Plain),
                ("Rate".into(), Receiver::Plain),
                ("Pricing".into(), Receiver::Type("Pricing".into()))
            ]
        );
    }

    #[test]
    fn reads_ruby_modules_classes_private_methods_and_calls() {
        let (tree, reading) = read(
            Language::Ruby,
            "lib/shop/order.rb",
            "module Shop\n  class Order\n    def self.make\n      Order.new(1)\n    end\n\n    def total\n      self.sum + Pricing.rate(1) + helper(2)\n    end\n\n    private\n\n    def sum\n      items.map { |x| x.price }\n    end\n  end\nend\n\ndef helper(x)\n  x\nend\n\ndouble = ->(x) { x * 2 }\n",
        );
        assert_eq!(
            tree,
            [
                "Class Shop exp :1",
                "  Class Order exp :2",
                "    Method self.make exp :3",
                "    Method total exp :7",
                "    Method sum :13",
                "Function helper exp :19",
                "Arrow double :23"
            ]
        );
        let own = || Receiver::SelfOf(Some("Order".into()));
        assert_eq!(
            calls(&reading),
            [
                ("Order".into(), Receiver::Type("Order".into())),
                ("sum".into(), own()),
                ("rate".into(), Receiver::Type("Pricing".into())),
                ("helper".into(), Receiver::Plain),
                ("map".into(), Receiver::Plain),
                ("items".into(), Receiver::Plain),
                ("price".into(), Receiver::Plain)
            ]
        );
    }

    #[test]
    fn reads_ruby_names_alone_as_calls_unless_they_are_local() {
        let (_, reading) = read(
            Language::Ruby,
            "lib/order.rb",
            r##"class Order
  private
  def total(rate, *rest, key: 1, &blk)
    count = items
    count += 1
    tax
    sum = count * rate + discount
    lines.each do |line, (a, b)|
      line.price
      a + b
      shipping
    end
    lines.map { |l| l + fee }
    first, second = pair
    puts first, second, rest, key, blk, sum
    self.name
    defined?(cache)
    "#{label}"
  rescue => e
    e
  end

  def self.make
    count
  end
end
"##,
        );
        let names: Vec<&str> = reading.calls.iter().map(|call| call.name.as_str()).collect();
        // Parameters, assignments, a block's parameters and `rescue => e` are locals; `count` is local to total only.
        assert_eq!(
            names,
            [
                "items", "tax", "discount", "each", "lines", "price", "shipping", "map", "lines", "fee", "pair",
                "puts", "name", "label", "count"
            ]
        );
        assert_eq!(reading.calls.iter().filter(|call| call.from.ends_with(":3:3")).count(), 14);
    }

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
    fn reads_cpp_namespaces_classes_and_methods_written_outside_them() {
        let (tree, reading) = read(
            Language::Cpp,
            "src/order.cpp",
            "namespace shop {\nclass Order {\npublic:\n  int total() { return this->sum() + Pricing::rate() + std::max(1, 2); }\nprivate:\n  int sum() {\n    auto twice = [](int x) { return x * 2; };\n    return twice(1);\n  }\n};\n\nvoid Order::print() { this->total(); helper(); }\n}\n\nstatic void helper() { auto o = new shop::Order(); o->total(); }\n",
        );
        assert_eq!(
            tree,
            [
                "Class shop exp :1",
                "  Class Order exp :2",
                "    Method total exp :4",
                "    Method sum :6",
                "      Arrow twice :7",
                "  Method Order::print exp :12",
                "Function helper :15"
            ]
        );
        let own = || Receiver::SelfOf(Some("Order".into()));
        assert_eq!(
            calls(&reading),
            [
                ("sum".into(), own()),
                ("rate".into(), Receiver::Type("Pricing".into())),
                ("max".into(), Receiver::Plain),
                ("twice".into(), Receiver::Plain),
                ("total".into(), own()),
                ("helper".into(), Receiver::Plain),
                ("Order".into(), Receiver::Type("Order".into())),
                ("total".into(), Receiver::Plain)
            ]
        );
    }

    #[test]
    fn lists_cpp_methods_only_declared_in_their_class_once() {
        let (tree, reading) = read(
            Language::Cpp,
            "src/order.hpp",
            "namespace shop {
class Order {
public:
  Order();
  ~Order();
  int total() const;
  virtual void print() = 0;
  static Order make(int count);
  Order& operator+=(const Order& other);
  template <typename T> T get();
  int count() const { return count_; }
  int (*hook)(int);
  friend void audit(Order&);
private:
  int sum();
  int count_;
};

int Order::total() const { return sum(); }
}
",
        );
        // total is defined in the same file, so it is listed once, as its definition; a function pointer field and a
        // friend are no methods of the class.
        assert_eq!(
            tree,
            [
                "Class shop exp :1",
                "  Class Order exp :2",
                "    Method Order exp :4",
                "    Method ~Order exp :5",
                "    Method print exp :7",
                "    Method make exp :8",
                "    Method operator+= exp :9",
                "    Method get exp :10",
                "    Method count exp :11",
                "    Method sum :15",
                "  Method Order::total exp :19"
            ]
        );
        let order = &reading.tree[0].children[0].children;
        assert_eq!((order[2].line, order[2].end_line), (7, 7));
        let declared: Vec<&str> = order
            .iter()
            .filter(|item| reading.declarations.contains(&item.id))
            .map(|item| item.name.as_str())
            .collect();
        assert_eq!(declared, ["Order", "~Order", "print", "make", "operator+=", "get", "sum"]);
        assert_eq!(calls(&reading), [("sum".into(), Receiver::Plain)]);
    }

    #[test]
    fn reads_csharp_properties_indexers_and_operators_as_methods() {
        let (tree, reading) = read(
            Language::Csharp,
            "Shop/Money.cs",
            "public class Money {
  public int Cents { get => Read(); set { Write(value); } }
  public int Auto { get; set; }
  public int Half => Divide(2);
  public int Initial { get; } = Start();
  public static Money operator +(Money a, Money b) { return Add(a, b); }
  public static implicit operator Money(int cents) => new Money(cents);
  static explicit operator int(Money m) { return m.Cents; }
  public int this[int i] => Digit(i);
}
",
        );
        // An auto-property holds no code and is left out, as a field is; its initializer's call counts for the class.
        assert_eq!(
            tree,
            [
                "Class Money exp :1",
                "  Method Cents exp :2",
                "  Method Half exp :4",
                "  Method operator + exp :6",
                "  Method implicit operator Money exp :7",
                "  Method explicit operator int :8",
                "  Method this[] exp :9"
            ]
        );
        let names: Vec<&str> = reading.calls.iter().map(|call| call.name.as_str()).collect();
        assert_eq!(names, ["Read", "Write", "Divide", "Start", "Add", "Money", "Digit"]);
        let cents = &reading.tree[0].children[0].id;
        assert_eq!(reading.calls.iter().filter(|call| &call.from == cents).count(), 2);
    }

    #[test]
    fn a_header_is_read_as_c() {
        use super::super::languages::language_of;
        assert_eq!(language_of("include/order.h"), Some(Language::C));
        assert_eq!(language_of("include/order.hpp"), Some(Language::Cpp));
        // A declaration is no function; a static inline one written in the header is.
        let (tree, _) =
            read(Language::C, "include/order.h", "int total(void);\nstatic inline int sum(int x) { return x; }\n");
        assert_eq!(tree, ["Function sum :2"]);
    }

    #[test]
    fn a_use_path_without_its_group_or_alias() {
        assert_eq!(use_path("crate::a::{b, c}"), ["crate", "a"]);
        assert_eq!(use_path("super::util::*"), ["super", "util"]);
        assert_eq!(use_path("std::io::Read as R"), ["std", "io", "Read"]);
    }
}
