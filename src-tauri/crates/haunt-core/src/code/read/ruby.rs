//! Ruby: methods (`def self.make` too), classes and modules, lambdas assigned to a name; calls with a receiver or
//! arguments, and a name alone unless it is a local variable made before it; imports by `require` and
//! `require_relative`.

use std::collections::HashMap;

use tree_sitter::Node;

use super::parts::{assigned_name, descendants, field, last, named_children, string_value, text};
use super::{Add, Context, Grammar, Receiver, Target};
use crate::code::project::{dirname, file_in, join};
use crate::code::types::FunctionKind;

/// The local variables of each method (or class body, or file), by the scope node's id.
#[derive(Default)]
pub struct Ruby {
    locals: HashMap<usize, Locals>,
}

impl Grammar for Ruby {
    fn grammar(&self) -> tree_sitter::Language {
        tree_sitter_ruby::LANGUAGE.into()
    }

    fn kind(&self, kind: &str) -> Option<FunctionKind> {
        use FunctionKind::*;
        Some(match kind {
            "method" => Function,
            "singleton_method" => Method,
            "class" | "module" => Class,
            "lambda" => Arrow,
            _ => return None,
        })
    }

    /// `def self.make` is self.make.
    fn name(&self, node: Node, source: &str) -> Option<String> {
        let own = || {
            let name = text(field(node, "name"), source);
            (!name.is_empty()).then(|| name.to_owned())
        };
        match node.kind() {
            "singleton_method" => Some(format!("{}.{}", text(field(node, "object"), source), own()?)),
            "lambda" => assigned_name(node, source),
            _ => own(),
        }
    }

    fn exported(&self, node: Node, source: &str) -> bool {
        node.kind() != "lambda" && (node.kind() != "method" || public(node, source))
    }

    fn method_in_type(&self, kind: &str) -> bool {
        kind == "method"
    }

    /// f(1), self.m, Order.make, order.total, Order.new; a name alone is read by `other_call`.
    fn call(&self, node: Node, container: Option<&str>, source: &str) -> Option<(String, Receiver)> {
        if node.kind() != "call" {
            return None;
        }
        let name = text(field(node, "method"), source);
        let receiver = match field(node, "receiver") {
            None if matches!(name, "require" | "require_relative" | "private" | "protected" | "public") => return None,
            None => Receiver::Plain,
            Some(receiver) if receiver.kind() == "self" => Receiver::SelfOf(container.map(str::to_owned)),
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

    /// `total` alone: a call when it is not a local variable of its method.
    fn other_call(&mut self, node: Node, _container: Option<&str>, source: &str) -> Option<(String, Receiver, Target)> {
        if node.kind() != "identifier" {
            return None;
        }
        Some((self.bare_call(node, source)?, Receiver::Plain, Target::Any))
    }

    /// require_relative 'x' from the file's folder; require 'x' from lib/ or the top, gems left out.
    fn imports(&self, root: Node, ctx: &Context, source: &str, add: &mut Add) {
        let dir = dirname(ctx.path);
        for call in descendants(root, &["call"]) {
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
}

impl Ruby {
    /// An identifier that calls a method: written as a statement or an expression (not a parameter, an assignment's
    /// target or a call's method name) and not a local variable made before it, as Ruby decides when it parses: an
    /// assignment or a parameter earlier in the method, or in the block it is in.
    fn bare_call(&mut self, node: Node, source: &str) -> Option<String> {
        let name = text(Some(node), source);
        if !is_expression(node)
            || matches!(name, "private" | "protected" | "public" | "module_function" | "it")
            || (name.len() == 2 && name.starts_with('_') && name.ends_with(|c: char| c.is_ascii_digit()))
        {
            return None;
        }
        let mut scope = node.parent()?;
        while !SCOPES.contains(&scope.kind()) {
            scope = scope.parent()?;
        }
        let locals = self.locals.entry(scope.id()).or_insert_with(|| locals(scope, source));
        let at = node.start_byte();
        let local = locals.get(name).is_some_and(|made| {
            made.iter().any(|&(from, start, end)| from < at && start <= at && node.end_byte() <= end)
        });
        (!local).then(|| name.to_owned())
    }
}

/// A method after `private` or `protected` in its class, or written as `private def x`, is the class's own.
fn public(node: Node, source: &str) -> bool {
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

/// Where Ruby starts a new set of local variables: a method, a class or module body, the file. A block or a lambda sees
/// its method's.
const SCOPES: [&str; 6] = ["method", "singleton_method", "class", "module", "singleton_class", "program"];

/// Whether an identifier stands where a value is read: a statement, an argument, an operand, a condition, an
/// assignment's value, a call's receiver.
fn is_expression(node: Node) -> bool {
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

/// Where a scope makes each local, by name: (where it is made, the start and end of where it lives), all byte offsets.
type Locals = HashMap<String, Vec<(usize, usize, usize)>>;

/// The names a scope makes local: parameters (a block's too), assignments' targets, `rescue => e`, a `for`'s variable
/// and pattern variables; nested methods and classes have their own. Each lives from where it is made to the end of
/// the block or lambda it is made in, or else of the scope.
fn locals(scope: Node, source: &str) -> Locals {
    let mut out = Locals::new();
    let mut stack = named_children(scope);
    while let Some(node) = stack.pop() {
        if SCOPES.contains(&node.kind()) {
            continue;
        }
        if node.kind() == "identifier" && declares(node) {
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

fn declares(node: Node) -> bool {
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

#[cfg(test)]
mod tests {
    use super::super::testing::{calls, read};
    use super::*;
    use crate::code::languages::Language;

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
}
