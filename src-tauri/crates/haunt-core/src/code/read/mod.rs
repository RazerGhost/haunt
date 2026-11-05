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
//!
//! Each language is one file with a `Grammar`, listed in `GRAMMARS`; `walk.rs` walks the tree the same way for all of
//! them, and `parts.rs` holds what they share.

mod c;
mod cpp;
mod csharp;
mod go;
mod java;
mod lua;
mod parts;
mod php;
mod python;
mod ruby;
mod rust;
#[cfg(test)]
mod testing;
mod walk;

use std::collections::HashMap;

use tree_sitter::{Node, Parser};

use super::languages::Language;
use super::types::{FileImport, FunctionKind, FunctionNode};

pub use cpp::scope_of;

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

/// Records an import: the file it resolves to (None when it does not), and whether it only names types.
type Add<'a> = dyn FnMut(Option<String>, bool) + 'a;

/// How one language reads: its tree-sitter grammar, which nodes are functions and what they are called, its calls and
/// its imports. A fresh one is made for each file, so it can keep what it learns while reading it.
trait Grammar {
    fn grammar(&self) -> tree_sitter::Language;

    /// The kind of function a node of this kind is, if it is one.
    fn kind(&self, kind: &str) -> Option<FunctionKind>;

    /// Whether this node is a function or type that is listed; some kinds only are with a body.
    fn is_container(&self, node: Node) -> bool {
        self.kind(node.kind()).is_some()
    }

    fn name(&self, node: Node, source: &str) -> Option<String>;

    fn exported(&self, node: Node, source: &str) -> bool;

    fn is_async(&self, _node: Node, _source: &str) -> bool {
        false
    }

    /// Whether a function of this node kind written in a type is its method (Python's `def` in a class).
    fn method_in_type(&self, _kind: &str) -> bool {
        false
    }

    /// The type a function written outside it belongs to, by the function's name (C++ `Order::total`).
    fn written_outside(&self, _name: &str) -> Option<String> {
        None
    }

    /// Each node, before anything else is done with it.
    fn enter(&mut self, _node: Node, _source: &str) {}

    /// A function or type just listed, before its children are read.
    fn listed(&mut self, _node: Node, _item: &FunctionNode, _reading: &mut FileReading) {}

    /// A node that is no function but is listed as one (C++ methods only declared in their class), with no children.
    fn declared(&mut self, _node: Node, _container: Option<&str>, _path: &str, _source: &str) -> Option<FunctionNode> {
        None
    }

    /// The callee of a call node by name, and how it is called; None when the node is not a call.
    fn call(&self, node: Node, container: Option<&str>, source: &str) -> Option<(String, Receiver)>;

    /// A call made some other way than a call node: a Ruby name alone, a C# property read or operator, a PHP hook.
    fn other_call(
        &mut self,
        _node: Node,
        _container: Option<&str>,
        _source: &str,
    ) -> Option<(String, Receiver, Target)> {
        None
    }

    /// After the walk: the file's whole tree, to settle what only the whole file shows.
    fn finish(&mut self, _tree: &mut Vec<FunctionNode>, _reading: &mut FileReading) {}

    fn imports(&self, root: Node, ctx: &Context, source: &str, add: &mut Add);
}

/// Makes a language's grammar for one file.
type Make = fn() -> Box<dyn Grammar>;

/// The languages read here, each with its grammar for one file.
const GRAMMARS: &[(Language, Make)] = &[
    (Language::Php, || Box::<php::Php>::default()),
    (Language::Rust, || Box::new(rust::Rust)),
    (Language::Lua, || Box::new(lua::Lua)),
    (Language::Python, || Box::new(python::Python)),
    (Language::Go, || Box::new(go::Go)),
    (Language::Java, || Box::new(java::Java)),
    (Language::Csharp, || Box::<csharp::Csharp>::default()),
    (Language::Ruby, || Box::<ruby::Ruby>::default()),
    (Language::C, || Box::new(c::C)),
    (Language::Cpp, || Box::<cpp::Cpp>::default()),
];

/// A function's id: its file, line and column.
fn id_of(path: &str, node: Node) -> String {
    let start = node.start_position();
    format!("{}:{}:{}", path, start.row + 1, start.column + 1)
}

/// Reads one file: its functions, its call sites by name, and its imports resolved to files.
pub fn read_file(language: Language, ctx: &Context, source: &str) -> FileReading {
    let Some((_, make)) = GRAMMARS.iter().find(|(listed, _)| *listed == language) else {
        return FileReading::default();
    };
    let mut grammar = make();
    let mut parser = Parser::new();
    if parser.set_language(&grammar.grammar()).is_err() {
        return FileReading::default();
    }
    let Some(tree) = parser.parse(source, None) else { return FileReading::default() };
    let mut reading = walk::walk(grammar.as_mut(), tree.root_node(), ctx.path, source);
    let mut top = std::mem::take(&mut reading.tree);
    grammar.finish(&mut top, &mut reading);
    reading.tree = top;
    let mut imports = Vec::new();
    grammar.imports(tree.root_node(), ctx, source, &mut |to: Option<String>, type_only: bool| {
        if let Some(to) = to.filter(|to| to != ctx.path) {
            imports.push(FileImport { to, type_only });
        }
    });
    reading.imports = imports;
    reading
}
