//! C++: what C reads (`c.rs`), and namespaces, classes and structs, lambdas, methods written outside their class
//! (`void Order::total() {}`), and methods only declared in their class (conversion operators too), listed without
//! calls unless the file defines them.

use std::collections::HashSet;

use tree_sitter::Node;

use super::c;
use super::parts::{field, text};
use super::{id_of, Add, Context, FileReading, Grammar, Receiver};
use crate::code::types::{FunctionKind, FunctionNode};

/// The methods only declared in their class, as (id, class, name).
#[derive(Default)]
pub struct Cpp {
    declared: Vec<(String, String, String)>,
}

impl Grammar for Cpp {
    fn grammar(&self) -> tree_sitter::Language {
        tree_sitter_cpp::LANGUAGE.into()
    }

    fn kind(&self, kind: &str) -> Option<FunctionKind> {
        use FunctionKind::*;
        Some(match kind {
            "function_definition" => Function,
            "class_specifier" | "struct_specifier" | "namespace_definition" => Class,
            "lambda_expression" => Arrow,
            _ => return None,
        })
    }

    /// `class Order;` (or `struct order *o`) names a type and holds nothing.
    fn is_container(&self, node: Node) -> bool {
        match node.kind() {
            "class_specifier" | "struct_specifier" => field(node, "body").is_some(),
            kind => self.kind(kind).is_some(),
        }
    }

    fn name(&self, node: Node, source: &str) -> Option<String> {
        c::name(node, source)
    }

    fn exported(&self, node: Node, source: &str) -> bool {
        c::exported(node, source)
    }

    fn method_in_type(&self, kind: &str) -> bool {
        kind == "function_definition"
    }

    fn written_outside(&self, name: &str) -> Option<String> {
        scope_of(name).map(str::to_owned)
    }

    /// `int total();` in its class: a method with no calls, so the class shows its whole interface.
    fn declared(&mut self, node: Node, container: Option<&str>, path: &str, source: &str) -> Option<FunctionNode> {
        let class = container?;
        let name = declared_method(node, source)?;
        let id = id_of(path, node);
        self.declared.push((id.clone(), class.to_owned(), name.clone()));
        Some(FunctionNode {
            id,
            name,
            kind: FunctionKind::Method,
            line: node.start_position().row as u32 + 1,
            end_line: node.end_position().row as u32 + 1,
            exported: c::public(node, source),
            is_async: false,
            anonymous: false,
            test: false,
            children: Vec::new(),
        })
    }

    fn call(&self, node: Node, container: Option<&str>, source: &str) -> Option<(String, Receiver)> {
        c::call(node, container, source)
    }

    /// A method declared in its class and defined in the same file (`int Order::total() {…}`) is listed once, as its
    /// definition.
    fn finish(&mut self, tree: &mut Vec<FunctionNode>, reading: &mut FileReading) {
        if self.declared.is_empty() {
            return;
        }
        let mut defined = HashSet::new();
        defined_outside(tree, &mut defined);
        let gone: HashSet<String> = self
            .declared
            .iter()
            .filter(|(_, class, name)| defined.contains(&(class.clone(), name.clone())))
            .map(|(id, _, _)| id.clone())
            .collect();
        drop_ids(tree, &gone);
        reading.declarations = std::mem::take(&mut self.declared)
            .into_iter()
            .map(|(id, _, _)| id)
            .filter(|id| !gone.contains(id))
            .collect();
    }

    fn imports(&self, root: Node, ctx: &Context, source: &str, add: &mut Add) {
        c::imports(root, ctx, source, add)
    }
}

/// The type a C++ method written outside its class belongs to: `Order::total` is Order, `shop::Box<T>::get` is Box.
pub fn scope_of(name: &str) -> Option<&str> {
    let (scope, _) = name.rsplit_once("::")?;
    let scope = scope.rsplit("::").next().unwrap_or(scope);
    let scope = scope.split('<').next().unwrap_or(scope);
    (!scope.is_empty()).then_some(scope)
}

/// A method only declared in its class (`int total() const;`, `virtual void f() = 0;`, `Order();`,
/// `operator int() const;`), by its name; not a field holding a function pointer (`int (*f)(int);`) nor a friend.
fn declared_method(node: Node, source: &str) -> Option<String> {
    if !matches!(node.kind(), "field_declaration" | "declaration") {
        return None;
    }
    let at = node.parent().filter(|parent| parent.kind() == "template_declaration").unwrap_or(node);
    at.parent().filter(|parent| parent.kind() == "field_declaration_list")?;
    if let Some(name) = field(node, "declarator").filter(|declarator| declarator.kind() == "operator_cast") {
        return c::conversion_name(name, source);
    }
    let name = field(c::function_declarator(node)?, "declarator")?;
    if !matches!(name.kind(), "field_identifier" | "identifier" | "destructor_name" | "operator_name") {
        return None;
    }
    let name: String = text(Some(name), source).split_whitespace().collect();
    (!name.is_empty()).then_some(name)
}

/// Methods written outside their class, as (class, method): `Order::total` is (Order, total).
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

#[cfg(test)]
mod tests {
    use super::super::testing::{calls, read};
    use super::*;
    use crate::code::languages::Language;

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
}
