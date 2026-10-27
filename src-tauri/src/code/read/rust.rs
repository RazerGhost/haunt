//! Rust: functions, `impl`s, traits and inline modules, closures; calls on `self` and on a type (`A::make()`); imports
//! by `mod x;` and `use`, to the deepest module file that exists.

use tree_sitter::Node;

use super::parts::{assigned_name, descendants, field, has_child, last, named_children, text};
use super::{Add, Context, Grammar, Receiver};
use crate::code::project::{dirname, file_in, join, rust_crate_root};
use crate::code::types::FunctionKind;

pub struct Rust;

impl Grammar for Rust {
    fn grammar(&self) -> tree_sitter::Language {
        tree_sitter_rust::LANGUAGE.into()
    }

    fn kind(&self, kind: &str) -> Option<FunctionKind> {
        use FunctionKind::*;
        Some(match kind {
            "function_item" | "function_signature_item" => Function,
            "impl_item" | "trait_item" | "mod_item" => Class,
            "closure_expression" => Arrow,
            _ => return None,
        })
    }

    /// `mod util;` declares a file and holds nothing.
    fn is_container(&self, node: Node) -> bool {
        match node.kind() {
            "mod_item" => field(node, "body").is_some(),
            kind => self.kind(kind).is_some(),
        }
    }

    fn name(&self, node: Node, source: &str) -> Option<String> {
        match node.kind() {
            "impl_item" => {
                let target = text(field(node, "type"), source);
                Some(match field(node, "trait") {
                    Some(trait_) => format!("impl {} for {target}", text(Some(trait_), source)),
                    None => format!("impl {target}"),
                })
            }
            "closure_expression" => assigned_name(node, source),
            _ => {
                let name = text(field(node, "name"), source);
                (!name.is_empty()).then(|| name.to_owned())
            }
        }
    }

    fn exported(&self, node: Node, _source: &str) -> bool {
        has_child(node, "visibility_modifier")
    }

    fn is_async(&self, node: Node, source: &str) -> bool {
        named_children(node).iter().any(|child| {
            child.kind() == "function_modifiers"
                && text(Some(*child), source).split_whitespace().any(|word| word == "async")
        })
    }

    fn method_in_type(&self, kind: &str) -> bool {
        kind == "function_item"
    }

    fn call(&self, node: Node, container: Option<&str>, source: &str) -> Option<(String, Receiver)> {
        if node.kind() != "call_expression" {
            return None;
        }
        let own = || Receiver::SelfOf(container.map(str::to_owned));
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

    fn imports(&self, root: Node, ctx: &Context, source: &str, add: &mut Add) {
        let dir = dirname(ctx.path);
        let file = ctx.path.rsplit('/').next().unwrap_or(ctx.path);
        let is_root_file = matches!(file, "main.rs" | "lib.rs" | "mod.rs");
        // Where this file's submodules live: next to main.rs, lib.rs and mod.rs, else in a folder named after the file.
        let child_dir =
            if is_root_file { dir.clone() } else { ctx.path.strip_suffix(".rs").unwrap_or(ctx.path).to_owned() };
        for item in descendants(root, &["mod_item"]) {
            if field(item, "body").is_some() {
                continue;
            }
            let name = text(field(item, "name"), source);
            add(file_in(ctx.root, &[format!("{child_dir}/{name}.rs"), format!("{child_dir}/{name}/mod.rs")]), true);
        }
        for declaration in descendants(root, &["use_declaration"]) {
            let Some(argument) = field(declaration, "argument") else { continue };
            let path = use_path(text(Some(argument), source));
            add(module_file(ctx, &path, &child_dir, &dir), true);
        }
    }
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

fn module_folder(file: &str) -> String {
    let name = file.rsplit('/').next().unwrap_or(file);
    if matches!(name, "main.rs" | "lib.rs" | "mod.rs") {
        dirname(file)
    } else {
        file.strip_suffix(".rs").unwrap_or(file).to_owned()
    }
}

/// The file of the deepest module a Rust `use` path names that exists: `crate::a::b::Item` is `src/a/b.rs`.
fn module_file(ctx: &Context, path: &[String], child_dir: &str, dir: &str) -> Option<String> {
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
    use super::super::testing::read;
    use super::*;
    use crate::code::languages::Language;

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
    fn a_use_path_without_its_group_or_alias() {
        assert_eq!(use_path("crate::a::{b, c}"), ["crate", "a"]);
        assert_eq!(use_path("super::util::*"), ["super", "util"]);
        assert_eq!(use_path("std::io::Read as R"), ["std", "io", "Read"]);
    }
}
