//! The walk every language shares: functions and types into a tree, each call recorded for the function it is written
//! in, and an unnamed function dropped (its calls going to the function around it) unless a named one sits inside it.

use tree_sitter::Node;

use super::parts::named_children;
use super::{id_of, Call, FileReading, Grammar, Target};
use crate::code::types::{FunctionKind, FunctionNode};

struct Walk<'a> {
    grammar: &'a mut dyn Grammar,
    source: &'a str,
    path: &'a str,
    reading: FileReading,
}

/// Walks a file's tree from its root: its functions (in `tree`) and its calls.
pub fn walk(grammar: &mut dyn Grammar, root: Node, path: &str, source: &str) -> FileReading {
    let mut walk = Walk { grammar, source, path, reading: FileReading::default() };
    let mut top = Vec::new();
    walk.visit(root, &mut top, None, None);
    walk.reading.tree = top;
    walk.reading
}

impl Walk<'_> {
    fn visit(&mut self, node: Node, into: &mut Vec<FunctionNode>, owner: Option<&str>, container: Option<&str>) {
        let source = self.source;
        self.grammar.enter(node, source);
        if self.grammar.is_container(node) {
            let name = self.grammar.name(node, source);
            let mut kind = self.grammar.kind(node.kind()).expect("a container");
            if self.grammar.method_in_type(node.kind()) && container.is_some() {
                kind = FunctionKind::Method;
            }
            // C++ `void Order::total() {}`: a method of Order, written outside it.
            let outside = name.as_deref().and_then(|name| self.grammar.written_outside(name));
            if outside.is_some() && kind == FunctionKind::Function {
                kind = FunctionKind::Method;
            }
            let start = node.start_position();
            let mut item = FunctionNode {
                id: id_of(self.path, node),
                name: name.clone().unwrap_or_else(|| "(anonymous)".into()),
                kind,
                line: start.row as u32 + 1,
                end_line: node.end_position().row as u32 + 1,
                exported: self.grammar.exported(node, source),
                is_async: self.grammar.is_async(node, source),
                anonymous: name.is_none(),
                test: false,
                children: Vec::new(),
            };
            self.grammar.listed(node, &item, &mut self.reading);
            let is_type = kind == FunctionKind::Class;
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
        if let Some(item) = self.grammar.declared(node, container, self.path, source) {
            into.push(item);
            return;
        }
        if let Some(owner) = owner {
            if let Some((name, receiver)) = self.grammar.call(node, container, source) {
                self.reading.calls.push(Call { from: owner.to_owned(), name, receiver, target: Target::Any });
            }
            if let Some((name, receiver, target)) = self.grammar.other_call(node, container, source) {
                self.reading.calls.push(Call { from: owner.to_owned(), name, receiver, target });
            }
        }
        for child in named_children(node) {
            self.visit(child, into, owner, container);
        }
    }
}
