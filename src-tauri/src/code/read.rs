//! One PHP, Rust, Lua, Python, Go, Java, C#, Ruby, C or C++ file through tree-sitter (the native grammars):
//!
//! - Functions: the same tree as the sidecar builds for TypeScript: a function written inside another is its child, and
//!   an unnamed function only stays when a named one sits inside it.
//! - Imports: resolved to files per language (PHP `use` through Composer's PSR-4 map and `require`/`include`; Rust `mod`
//!   and `use`; Lua `require`; Python `import` and `from … import`; Go `import` of the module's own packages, through
//!   go.mod, to a file of that package; Java `import` from the file's source folder; C# `using` of a namespace, to its
//!   folder's first file; Ruby `require` and `require_relative`; C and C++ `#include "…"`, not `<…>`).
//! - Calls: by the callee's name and how it is called; `guess.rs` matches them to functions across files. A Ruby name
//!   alone (`total`) is a call unless a local variable of that name is made before it; a block's calls count for its
//!   method.
//! - C++ methods only declared in their class (conversion operators too) are listed as methods without calls, unless
//!   the file defines them.
//! - C# properties, indexers and events with code, and operators (`operator +`, `implicit operator Money`), are
//!   methods. Reading or writing a property calls it, and an operator or a cast whose operand has a known type of its
//!   own calls that type's operator; implicit conversions do not show in the source and are left out.

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

/// What a call can reach: any function, or (C#) only a property's or event's accessors, or only an operator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target {
    Any,
    Property,
    Operator,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    /// The id of the calling function.
    pub from: String,
    pub name: String,
    pub receiver: Receiver,
    pub target: Target,
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
    /// For C#: the ids of the properties and events with code, which reading or writing them calls.
    pub properties: Vec<String>,
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
            "property_declaration"
            | "indexer_declaration"
            | "event_declaration"
            | "operator_declaration"
            | "conversion_operator_declaration",
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
/// (or `struct order *o`) names a type; they hold nothing. A C# property, indexer or event counts when it has code:
/// `=> …` or an accessor with a body; an auto-property (`{ get; set; }`) is a field, as a field-like event is.
fn is_container(language: Language, node: Node) -> bool {
    if kind_of(language, node.kind()).is_none() {
        return false;
    }
    match node.kind() {
        "mod_item" | "class_specifier" | "struct_specifier" => field(node, "body").is_some(),
        "property_declaration" | "indexer_declaration" | "event_declaration" => {
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
/// Order::total, `Money::operator int()` is Money::operator int.
fn c_function_name(node: Node, source: &str) -> Option<String> {
    if let Some(name) = field(node, "declarator").and_then(|declarator| cpp_conversion_name(declarator, source)) {
        return Some(name);
    }
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

/// C++: a conversion operator by the type it converts to, without its namespace: `operator int() const` is operator int,
/// `Money::operator std::string()` is Money::operator string. It has no function_declarator of its own to name it.
fn cpp_conversion_name(declarator: Node, source: &str) -> Option<String> {
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
            let name = cpp_conversion_name(field(declarator, "name")?, source)?;
            let scope: String = text(field(declarator, "scope"), source).split_whitespace().collect();
            Some(format!("{scope}::{name}"))
        }
        _ => None,
    }
}

/// C++: a method only declared in its class (`int total() const;`, `virtual void f() = 0;`, `Order();`,
/// `operator int() const;`), by its name; not a field holding a function pointer (`int (*f)(int);`) nor a friend.
fn cpp_declared_method(node: Node, source: &str) -> Option<String> {
    if !matches!(node.kind(), "field_declaration" | "declaration") {
        return None;
    }
    let at = node.parent().filter(|parent| parent.kind() == "template_declaration").unwrap_or(node);
    at.parent().filter(|parent| parent.kind() == "field_declaration_list")?;
    if let Some(name) = field(node, "declarator").filter(|declarator| declarator.kind() == "operator_cast") {
        return cpp_conversion_name(name, source);
    }
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
                Some(format!("{way}operator {}", csharp_short_type(text(field(node, "type"), source))))
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
    ruby_locals: HashMap<usize, RubyLocals>,
    /// C#: the names a member declares, and those its type declares, with their types, by the member's or type body's
    /// node id.
    csharp_names: HashMap<usize, HashMap<String, String>>,
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
            if self.language == Language::Csharp && matches!(node.kind(), "property_declaration" | "event_declaration")
            {
                self.reading.properties.push(item.id.clone());
            }
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
            self.reading.calls.push(Call { from: owner.to_owned(), name, receiver, target: Target::Any });
        }
        // Ruby `total` alone: a call when it is not a local variable of its method.
        if self.language == Language::Ruby && node.kind() == "identifier" {
            if let Some(owner) = owner {
                if let Some(name) = self.ruby_bare_call(node) {
                    let receiver = Receiver::Plain;
                    self.reading.calls.push(Call { from: owner.to_owned(), name, receiver, target: Target::Any });
                }
            }
        }
        if self.language == Language::Csharp {
            if let Some(owner) = owner {
                if let Some((name, receiver, target)) = self.csharp_use(node, container) {
                    self.reading.calls.push(Call { from: owner.to_owned(), name, receiver, target });
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
    /// target or a call's method name) and not a local variable made before it, as Ruby decides when it parses: an
    /// assignment or a parameter earlier in the method, or in the block it is in.
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
        let at = node.start_byte();
        let local = locals.get(name).is_some_and(|made| {
            made.iter().any(|&(from, start, end)| from < at && start <= at && node.end_byte() <= end)
        });
        (!local).then(|| name.to_owned())
    }

    /// C#: a property read or written (`order.Amount`, `this.Amount`, `Amount`, `new Order { Amount = 1 }`) as a call
    /// to it, and an operator used (`a + b`, `-a`, `a += b`) or a cast (`(Money)x`) as a call to the operator of the
    /// operand's type, only when that type is known and is not a built-in one: numbers and strings use no operator of
    /// ours, and without types every `+` would look like a call to the one `operator +` there is.
    fn csharp_use(&mut self, node: Node, container: Option<&str>) -> Option<(String, Receiver, Target)> {
        let source = self.source;
        let own = || Receiver::SelfOf(container.map(str::to_owned));
        match node.kind() {
            // `order.Amount`, and `order?.Amount` unless it is called.
            "member_access_expression" | "member_binding_expression" => {
                let name = field(node, "name").filter(|name| name.kind() == "identifier")?;
                let object = if node.kind() == "member_access_expression" {
                    if !csharp_read(node, source) {
                        return None;
                    }
                    field(node, "expression")?
                } else {
                    let access = node.parent().filter(|parent| parent.kind() == "conditional_access_expression")?;
                    if access.parent().is_some_and(|call| is_field(call, "function", access)) {
                        return None;
                    }
                    field(access, "condition")?
                };
                let receiver = if object.kind() == "this" {
                    own()
                } else {
                    match self.csharp_type_of(object) {
                        Some(type_) if csharp_built_in(&type_) => return None,
                        Some(type_) => Receiver::Type(type_),
                        None if object.kind() == "identifier" => on_name(text(Some(object), source)),
                        None => Receiver::Plain,
                    }
                };
                Some((text(Some(name), source).to_owned(), receiver, Target::Property))
            }
            "identifier" => {
                if !csharp_read(node, source) {
                    return None;
                }
                let name = text(Some(node), source);
                // `new Order { Amount = 1 }` sets the created type's property.
                let created = node
                    .parent()
                    .filter(|set| set.kind() == "assignment_expression" && is_field(*set, "left", node))
                    .and_then(|set| set.parent())
                    .filter(|list| list.kind() == "initializer_expression")
                    .and_then(|list| list.parent())
                    .filter(|new| new.kind() == "object_creation_expression");
                if let Some(created) = created {
                    let type_ = csharp_type_name(text(field(created, "type"), source));
                    return Some((name.to_owned(), Receiver::Type(type_), Target::Property));
                }
                // An accessor's `value` is its parameter.
                let accessor =
                    || std::iter::successors(node.parent(), Node::parent).any(|up| up.kind() == "accessor_declaration");
                if self.csharp_local(node, name).is_some() || (name == "value" && accessor()) {
                    return None;
                }
                Some((name.to_owned(), own(), Target::Property))
            }
            "binary_expression" => {
                let operator = text(field(node, "operator"), source);
                if !CSHARP_BINARY.contains(&operator) {
                    return None;
                }
                let type_ = self.csharp_own_type(&[field(node, "left")?, field(node, "right")?])?;
                Some((format!("operator {operator}"), Receiver::Type(type_), Target::Operator))
            }
            "prefix_unary_expression" | "postfix_unary_expression" => {
                let (operator, operand) = csharp_unary(node, source)?;
                if !matches!(operator, "+" | "-" | "!" | "~" | "++" | "--") {
                    return None;
                }
                let type_ = self.csharp_own_type(&[operand])?;
                Some((format!("operator {operator}"), Receiver::Type(type_), Target::Operator))
            }
            // `a += b` uses `operator +`.
            "assignment_expression" => {
                let operator = text(field(node, "operator"), source).strip_suffix('=')?;
                if !CSHARP_BINARY.contains(&operator) {
                    return None;
                }
                let type_ = self.csharp_own_type(&[field(node, "left")?, field(node, "right")?])?;
                Some((format!("operator {operator}"), Receiver::Type(type_), Target::Operator))
            }
            // `(Money)x` is `explicit operator Money`, in Money or in x's type; to a built-in type, in x's.
            "cast_expression" => {
                let written = text(field(node, "type"), source);
                let to = csharp_type_name(written);
                let from = self.csharp_type_of(field(node, "value")?).filter(|from| !csharp_built_in(from));
                let receiver = match (csharp_built_in(&to), from) {
                    (false, Some(_)) => Receiver::Plain,
                    (false, None) => Receiver::Type(to),
                    (true, Some(from)) => Receiver::Type(from),
                    (true, None) => return None,
                };
                Some((format!("explicit operator {}", csharp_short_type(written)), receiver, Target::Operator))
            }
            _ => None,
        }
    }

    /// The first of these operands whose type is known and not built in.
    fn csharp_own_type(&mut self, operands: &[Node]) -> Option<String> {
        operands.iter().find_map(|operand| self.csharp_type_of(*operand).filter(|type_| !csharp_built_in(type_)))
    }

    /// C#: the type of an expression, when the source shows it: a literal; a local, a parameter or a member of the
    /// type it is written in, by its declared type; `new T()` and `(T)x`; a call of the type's own method, by what it
    /// returns; and operators on those.
    fn csharp_type_of(&mut self, node: Node) -> Option<String> {
        let source = self.source;
        let known = |type_: String| (!type_.is_empty()).then_some(type_);
        let on_this = |access: Node| field(access, "expression").is_some_and(|object| object.kind() == "this");
        match node.kind() {
            "integer_literal" | "real_literal" => Some("int".into()),
            "string_literal"
            | "verbatim_string_literal"
            | "raw_string_literal"
            | "interpolated_string_expression"
            | "character_literal" => Some("string".into()),
            "boolean_literal" => Some("bool".into()),
            "null_literal" => Some("null".into()),
            "parenthesized_expression" => self.csharp_type_of(named_children(node).into_iter().next()?),
            "object_creation_expression" | "cast_expression" => {
                known(csharp_type_name(text(field(node, "type"), source)))
            }
            "identifier" => {
                let name = text(Some(node), source);
                match self.csharp_local(node, name) {
                    Some(type_) => known(type_),
                    None => known(self.csharp_member_of_type(node, name)?),
                }
            }
            "member_access_expression" if on_this(node) => {
                known(self.csharp_member_of_type(node, text(field(node, "name"), source))?)
            }
            "invocation_expression" => {
                let callee = field(node, "function")?;
                let name = match callee.kind() {
                    "identifier" => text(Some(callee), source),
                    "member_access_expression" if on_this(callee) => text(field(callee, "name"), source),
                    _ => return None,
                };
                known(self.csharp_member_of_type(node, name)?)
            }
            "prefix_unary_expression" | "postfix_unary_expression" => {
                let (operator, operand) = csharp_unary(node, source)?;
                let type_ = self.csharp_type_of(operand)?;
                match operator {
                    "+" | "-" | "~" | "++" | "--" => Some(type_),
                    "!" if csharp_built_in(&type_) => Some("bool".into()),
                    _ => None,
                }
            }
            "binary_expression" => {
                let operator = text(field(node, "operator"), source);
                if matches!(operator, "==" | "!=" | "<" | ">" | "<=" | ">=" | "&&" | "||" | "is") {
                    return Some("bool".into());
                }
                let operands = [field(node, "left")?, field(node, "right")?];
                let types: Vec<String> = operands.iter().filter_map(|operand| self.csharp_type_of(*operand)).collect();
                types.iter().find(|type_| !csharp_built_in(type_)).or(types.first()).cloned()
            }
            _ => None,
        }
    }

    /// A local or parameter of the member `node` is written in, by name: its type, or "" when it is not written.
    fn csharp_local(&mut self, node: Node, name: &str) -> Option<String> {
        let member = csharp_member(node)?;
        let source = self.source;
        self.csharp_names.entry(member.id()).or_insert_with(|| csharp_locals(member, source)).get(name).cloned()
    }

    /// A field, property, event or method of the type `node` is written in, by name: its type, or what it returns.
    fn csharp_member_of_type(&mut self, node: Node, name: &str) -> Option<String> {
        let mut holder = node.parent()?;
        while !matches!(
            holder.kind(),
            "class_declaration" | "struct_declaration" | "record_declaration" | "interface_declaration"
        ) {
            holder = holder.parent()?;
        }
        let body = field(holder, "body")?;
        let source = self.source;
        self.csharp_names.entry(body.id()).or_insert_with(|| csharp_members(body, source)).get(name).cloned()
    }
}

/// The C# operators of two operands a type can declare (`&&` and `||` go through `&`, `|`, `true` and `false`).
const CSHARP_BINARY: [&str; 17] =
    ["+", "-", "*", "/", "%", "&", "|", "^", "<<", ">>", ">>>", "==", "!=", "<", ">", "<=", ">="];

/// The built-in C# types, whose operators are the language's own.
fn csharp_built_in(type_: &str) -> bool {
    matches!(
        type_,
        "int"
            | "long"
            | "short"
            | "byte"
            | "sbyte"
            | "uint"
            | "ulong"
            | "ushort"
            | "nint"
            | "nuint"
            | "float"
            | "double"
            | "decimal"
            | "bool"
            | "char"
            | "string"
            | "object"
            | "dynamic"
            | "void"
            | "null"
    )
}

fn is_field(parent: Node, name: &str, node: Node) -> bool {
    field(parent, name).is_some_and(|value| value.id() == node.id())
}

/// A C# unary expression's operator and operand: `-a`, `a++`.
fn csharp_unary<'t>(node: Node<'t>, source: &'t str) -> Option<(&'t str, Node<'t>)> {
    let mut cursor = node.walk();
    let operator = node.children(&mut cursor).find(|child| !child.is_named())?;
    Some((text(Some(operator), source), named_children(node).into_iter().next()?))
}

/// A C# type's name as types are matched: `Shop.Money?` and `Money<T>` are Money.
fn csharp_type_name(written: &str) -> String {
    last(&without_generics(written.trim_end_matches('?'))).to_owned()
}

/// A C# type as a conversion operator is named after it: without its namespace (`Shop.Money` is Money).
fn csharp_short_type(written: &str) -> String {
    let (head, generics) = written.split_at(written.find('<').unwrap_or(written.len()));
    format!("{}{generics}", head.rsplit('.').next().unwrap_or(head).trim())
}

/// The member of its type (a method, property, field…) a C# node is written in.
fn csharp_member(node: Node) -> Option<Node> {
    let mut member = node;
    while member.parent()?.kind() != "declaration_list" {
        member = member.parent()?;
    }
    Some(member)
}

/// The names a C# member declares (parameters, locals, `foreach`, `catch`, `out var` and pattern variables, a
/// lambda's parameters) with their written types; `var` takes the type of `new T()` or `(T)x`, else none.
fn csharp_locals(member: Node, source: &str) -> HashMap<String, String> {
    let kinds = [
        "parameter",
        "implicit_parameter",
        "variable_declarator",
        "foreach_statement",
        "catch_declaration",
        "declaration_expression",
        "declaration_pattern",
    ];
    let mut out = HashMap::new();
    for node in descendants(member, &kinds) {
        let (name, type_) = match node.kind() {
            "implicit_parameter" => (Some(node), None),
            "foreach_statement" => (field(node, "left"), field(node, "type")),
            "variable_declarator" => {
                let declared = node.parent().and_then(|declaration| field(declaration, "type"));
                let type_ = match declared {
                    Some(declared) if declared.kind() == "implicit_type" => named_children(node)
                        .into_iter()
                        .find(|value| matches!(value.kind(), "object_creation_expression" | "cast_expression"))
                        .and_then(|value| field(value, "type")),
                    declared => declared,
                };
                (field(node, "name"), type_)
            }
            _ => (field(node, "name"), field(node, "type")),
        };
        let Some(name) = name else { continue };
        let type_ = type_.filter(|type_| type_.kind() != "implicit_type");
        let type_ = type_.map(|type_| csharp_type_name(text(Some(type_), source))).unwrap_or_default();
        out.insert(text(Some(name), source).to_owned(), type_);
    }
    out
}

/// The fields, properties and events of a C# type with their types, and its methods with what they return.
fn csharp_members(body: Node, source: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for member in named_children(body) {
        match member.kind() {
            "field_declaration" | "event_field_declaration" => {
                let declarations = named_children(member).into_iter().filter(|d| d.kind() == "variable_declaration");
                for declaration in declarations {
                    let type_ = csharp_type_name(text(field(declaration, "type"), source));
                    for declarator in named_children(declaration) {
                        if declarator.kind() == "variable_declarator" {
                            out.insert(text(field(declarator, "name"), source).to_owned(), type_.clone());
                        }
                    }
                }
            }
            "property_declaration" | "event_declaration" | "method_declaration" => {
                let type_ = field(member, "type").or_else(|| field(member, "returns"));
                out.insert(text(field(member, "name"), source).to_owned(), csharp_type_name(text(type_, source)));
            }
            _ => {}
        }
    }
    out
}

/// C#: whether a name or member access stands where a value is read or written, not where it is declared, called (a
/// member access that is the function of a call is a call already), named as a type, or asked about with `nameof`.
fn csharp_read(node: Node, source: &str) -> bool {
    let Some(parent) = node.parent() else { return false };
    let is = |name: &str| is_field(parent, name, node);
    let in_nameof = parent.kind() == "argument"
        && parent.parent().and_then(|list| list.parent()).is_some_and(|call| {
            call.kind() == "invocation_expression" && text(field(call, "function"), source) == "nameof"
        });
    if in_nameof {
        return false;
    }
    match parent.kind() {
        "binary_expression"
        | "prefix_unary_expression"
        | "postfix_unary_expression"
        | "parenthesized_expression"
        | "argument"
        | "arrow_expression_clause"
        | "return_statement"
        | "yield_statement"
        | "throw_statement"
        | "throw_expression"
        | "await_expression"
        | "interpolation"
        | "conditional_expression"
        | "initializer_expression"
        | "assignment_expression"
        | "equals_value_clause" => true,
        "member_access_expression" | "element_access_expression" | "is_pattern_expression" => is("expression"),
        "conditional_access_expression" => is("condition"),
        "cast_expression" => is("value"),
        "variable_declarator" => !is("name"),
        "if_statement" | "while_statement" | "do_statement" => is("condition"),
        "foreach_statement" => is("right"),
        "lambda_expression" => is("body"),
        _ => false,
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
        // `for row in rows`, `case input`
        "in" => true,
        "case" | "case_match" => is("value"),
        _ => false,
    }
}

/// Where a Ruby scope makes each local, by name: (where it is made, the start and end of where it lives), all byte
/// offsets.
type RubyLocals = HashMap<String, Vec<(usize, usize, usize)>>;

/// The names a Ruby scope makes local: parameters (a block's too), assignments' targets, `rescue => e`, a `for`'s
/// variable and pattern variables; nested methods and classes have their own. Each lives from where it is made to the
/// end of the block or lambda it is made in, or else of the scope.
fn ruby_locals(scope: Node, source: &str) -> RubyLocals {
    let mut out = RubyLocals::new();
    let mut stack = named_children(scope);
    while let Some(node) = stack.pop() {
        if RUBY_SCOPES.contains(&node.kind()) {
            continue;
        }
        if node.kind() == "identifier" && ruby_declares(node) {
            let mut lives = scope;
            let mut at = node;
            while let Some(parent) = at.parent().filter(|parent| parent.id() != scope.id()) {
                if matches!(parent.kind(), "block" | "do_block" | "lambda") {
                    lives = parent;
                    break;
                }
                at = parent;
            }
            let made = (node.start_byte(), lives.start_byte(), lives.end_byte());
            out.entry(text(Some(node), source).to_owned()).or_default().push(made);
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
        csharp_names: HashMap::new(),
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
                    // `nameof(x)` names x without calling anything.
                    "identifier" if text(Some(callee), source) == "nameof" => None,
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
            (!name.is_empty()).then(|| Call {
                from: owner.to_owned(),
                name,
                receiver: Receiver::Plain,
                target: Target::Any,
            })
        }
        "array_creation_expression" => {
            let elements: Vec<&str> =
                named_children(value).into_iter().map(|element| text(Some(element), source)).collect();
            let method = unquote(elements.get(1)?).to_owned();
            let receiver = if elements[0].contains("$this") { Receiver::SelfOf(None) } else { Receiver::Plain };
            (!method.is_empty()).then(|| Call { from: owner.to_owned(), name: method, receiver, target: Target::Any })
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
                // Rate could be a property of Order's: a read that only a property can answer.
                ("Rate".into(), own()),
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
        // `m.Cents` reads the property; the setter's `value` is its parameter.
        assert_eq!(names, ["Read", "Write", "Divide", "Start", "Add", "Money", "Cents", "Digit"]);
        let cents = &reading.tree[0].children[0].id;
        assert_eq!(reading.calls.iter().filter(|call| &call.from == cents).count(), 2);
    }

    fn targeted(reading: &FileReading) -> Vec<(String, Receiver, Target)> {
        reading.calls.iter().map(|call| (call.name.clone(), call.receiver.clone(), call.target)).collect()
    }

    #[test]
    fn reads_csharp_property_reads_and_writes_as_calls_to_properties() {
        let (_, reading) = read(
            Language::Csharp,
            "Shop/Order.cs",
            "public class Order {
  private int _count;
  public int Amount { get => _count; set { _count = value; } }
  public int Total() { return Amount + this.Amount; }
  public void Copy(Order other, Line line) {
    other.Amount = 5;
    var made = new Order { Amount = other.Total() };
    line.Text.Trim();
    Use(nameof(Amount), other?.Amount);
  }
  public void Shadow() { int Amount = 3; Use(Amount); }
}
",
        );
        let own = || Receiver::SelfOf(Some("Order".into()));
        let order = || Receiver::Type("Order".into());
        let property = |name: &str, receiver: Receiver| (name.to_owned(), receiver, Target::Property);
        // `_count` is read too, but only a property answers a property read and a field is none. A local named like
        // the property hides it, `nameof` reads nothing, and a call (`other.Total()`, `Trim()`) is a call already.
        assert_eq!(
            targeted(&reading),
            [
                property("_count", own()),
                property("_count", own()),
                property("Amount", own()),
                property("Amount", own()),
                property("Amount", order()),
                ("Order".into(), order(), Target::Any),
                property("Amount", order()),
                ("Total".into(), Receiver::Plain, Target::Any),
                ("Trim".into(), Receiver::Plain, Target::Any),
                property("Text", Receiver::Type("Line".into())),
                ("Use".into(), Receiver::Plain, Target::Any),
                property("Amount", order()),
                ("Use".into(), Receiver::Plain, Target::Any)
            ]
        );
        assert_eq!(reading.properties, [reading.tree[0].children[0].id.clone()]);
    }

    #[test]
    fn reads_csharp_operators_and_casts_on_known_types_as_calls_to_operators() {
        let (_, reading) = read(
            Language::Csharp,
            "Shop/Money.cs",
            r#"public struct Money {
  public static Money operator +(Money a, Money b) => a;
  int Use(Money a, Money b, int i, Shop.Money? maybe) {
    var c = a + b + a;
    var n = 1 + 2 + i;
    var s = "x" + i;
    c += a;
    var d = -a;
    var e = (Money)i;
    int f = (int)a;
    int g = (int)3.5;
    bool same = maybe == null;
    var h = new Money() * 2;
    return Count() * Total();
  }
  int Count() => 1;
}
"#,
        );
        let operators: Vec<(String, Receiver)> = targeted(&reading)
            .into_iter()
            .filter(|(_, _, target)| *target == Target::Operator)
            .map(|(name, receiver, _)| (name, receiver))
            .collect();
        let money = || Receiver::Type("Money".into());
        // Numbers, strings and calls of unknown type use no operator of ours; `Count()` returns an int.
        assert_eq!(
            operators,
            [
                ("operator +".into(), money()),
                ("operator +".into(), money()),
                ("operator +".into(), money()),
                ("operator -".into(), money()),
                ("explicit operator Money".into(), money()),
                ("explicit operator int".into(), money()),
                ("operator ==".into(), money()),
                ("operator *".into(), money())
            ]
        );
    }

    #[test]
    fn reads_csharp_event_accessors_as_methods() {
        let (tree, reading) = read(
            Language::Csharp,
            "Shop/Shop.cs",
            "public class Shop {
  public event EventHandler Changed { add { Attach(value); } remove { Detach(value); } }
  public event EventHandler Plain;
  void Fire() { Changed += OnChange; }
}
",
        );
        // A field-like event holds no code and is left out, as an auto-property is.
        assert_eq!(tree, ["Class Shop exp :1", "  Method Changed exp :2", "  Method Fire :4"]);
        let changed = &reading.tree[0].children[0].id;
        let names: Vec<&str> =
            reading.calls.iter().filter(|call| &call.from == changed).map(|call| call.name.as_str()).collect();
        assert_eq!(names, ["Attach", "Detach"]);
        assert_eq!(reading.properties, std::slice::from_ref(changed));
    }

    #[test]
    fn lists_cpp_conversion_operators_like_other_methods() {
        let (tree, reading) = read(
            Language::Cpp,
            "src/money.hpp",
            "namespace shop {
class Money {
public:
  operator int() const;
  explicit operator bool() const;
  operator std::string() const { return text(); }
};

Money::operator int() const { return cents(); }
}
",
        );
        // operator int is defined in the same file, so it is listed once, as its definition.
        assert_eq!(
            tree,
            [
                "Class shop exp :1",
                "  Class Money exp :2",
                "    Method operator bool exp :5",
                "    Method operator string exp :6",
                "  Method Money::operator int exp :9"
            ]
        );
        let declared: Vec<&str> = reading.tree[0].children[0]
            .children
            .iter()
            .filter(|item| reading.declarations.contains(&item.id))
            .map(|item| item.name.as_str())
            .collect();
        assert_eq!(declared, ["operator bool"]);
        let names: Vec<&str> = reading.calls.iter().map(|call| call.name.as_str()).collect();
        assert_eq!(names, ["text", "cents"]);
    }

    #[test]
    fn a_ruby_local_hides_a_call_only_after_it_is_made() {
        let (_, reading) = read(
            Language::Ruby,
            "lib/order.rb",
            "def total
  tax
  tax = 1
  tax
  items.each { |item| price = item; price }
  price
  [1].map do |x|
    fee = x
    fee
  end
  fee
  x
  shipping = rate { shipping }
  for row in rows
    row
  end
  row
  begin
    risky
  rescue => err
    err
  end
  err
  case input
  in [first, *]
    first
  end
  first
  count += 1
  count
  a, b = pair
  a + b
  late
  late = 2
  ->(y) { y }
  y
end
",
        );
        let names: Vec<&str> = reading.calls.iter().map(|call| call.name.as_str()).collect();
        // A block's parameters and the locals first assigned in a block end with it; `for`, `rescue =>`, patterns,
        // `+=` and multiple assignment make locals from there on. An assignment's own value already sees its local
        // (`shipping = rate { shipping }`), as Ruby reads it.
        assert_eq!(
            names,
            ["tax", "each", "items", "price", "map", "fee", "x", "rate", "rows", "risky", "input", "pair", "late", "y"]
        );
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
