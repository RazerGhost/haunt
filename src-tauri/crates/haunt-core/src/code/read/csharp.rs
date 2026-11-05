//! C#: namespaces, classes (structs, interfaces, records), methods, local functions and lambdas, and properties,
//! indexers, events and operators with code as methods; calls on `this`, a type and `new`, property reads and writes,
//! and operators and casts on a known type of its own; imports by `using`, to the namespace's folder's first file.

use std::collections::HashMap;

use tree_sitter::Node;

use super::parts::{
    assigned_name, created, descendants, field, is_field, last, named_children, on_name, package_file, text,
    without_generics,
};
use super::{Add, Context, FileReading, Grammar, Receiver, Target};
use crate::code::project::file_in;
use crate::code::types::{FunctionKind, FunctionNode};

/// The names a member declares, and those its type declares, with their types, by the member's or type body's node id.
#[derive(Default)]
pub struct Csharp {
    names: HashMap<usize, HashMap<String, String>>,
}

impl Grammar for Csharp {
    fn grammar(&self) -> tree_sitter::Language {
        tree_sitter_c_sharp::LANGUAGE.into()
    }

    fn kind(&self, kind: &str) -> Option<FunctionKind> {
        use FunctionKind::*;
        Some(match kind {
            "method_declaration" | "constructor_declaration" => Method,
            "lambda_expression" => Arrow,
            "local_function_statement" | "anonymous_method_expression" => Function,
            "property_declaration"
            | "indexer_declaration"
            | "event_declaration"
            | "operator_declaration"
            | "conversion_operator_declaration" => Method,
            "class_declaration"
            | "struct_declaration"
            | "interface_declaration"
            | "record_declaration"
            | "namespace_declaration" => Class,
            _ => return None,
        })
    }

    /// A property, indexer or event counts when it has code: `=> …` or an accessor with a body; an auto-property
    /// (`{ get; set; }`) is a field, as a field-like event is.
    fn is_container(&self, node: Node) -> bool {
        match node.kind() {
            "property_declaration" | "indexer_declaration" | "event_declaration" => {
                field(node, "value").is_some_and(|value| value.kind() == "arrow_expression_clause")
                    || field(node, "accessors").is_some_and(|accessors| {
                        named_children(accessors).iter().any(|accessor| field(*accessor, "body").is_some())
                    })
            }
            kind => self.kind(kind).is_some(),
        }
    }

    fn name(&self, node: Node, source: &str) -> Option<String> {
        match node.kind() {
            "lambda_expression" | "anonymous_method_expression" => assigned_name(node, source),
            // `operator +`, `implicit operator Money`, and an indexer as `this[]`.
            "operator_declaration" => Some(format!("operator {}", text(field(node, "operator"), source))),
            "conversion_operator_declaration" => {
                let mut cursor = node.walk();
                let way = node
                    .children(&mut cursor)
                    .find(|child| matches!(child.kind(), "implicit" | "explicit"))
                    .map(|child| format!("{} ", child.kind()))
                    .unwrap_or_default();
                Some(format!("{way}operator {}", short_type(text(field(node, "type"), source))))
            }
            "indexer_declaration" => Some("this[]".into()),
            _ => {
                let name = text(field(node, "name"), source);
                (!name.is_empty()).then(|| name.to_owned())
            }
        }
    }

    /// Declared `public`; a namespace is open to all.
    fn exported(&self, node: Node, source: &str) -> bool {
        node.kind() == "namespace_declaration"
            || named_children(node)
                .iter()
                .any(|child| child.kind() == "modifier" && text(Some(*child), source) == "public")
    }

    fn is_async(&self, node: Node, source: &str) -> bool {
        named_children(node).iter().any(|child| child.kind() == "modifier" && text(Some(*child), source) == "async")
    }

    fn listed(&mut self, node: Node, item: &FunctionNode, reading: &mut FileReading) {
        if matches!(node.kind(), "property_declaration" | "event_declaration") {
            reading.properties.push(item.id.clone());
        }
    }

    /// F(), this.M(), Order.Make(), order.Total(), F<T>(), new Order()
    fn call(&self, node: Node, container: Option<&str>, source: &str) -> Option<(String, Receiver)> {
        match node.kind() {
            "invocation_expression" => {
                let callee = field(node, "function")?;
                match callee.kind() {
                    "member_access_expression" => {
                        let object = text(field(callee, "expression"), source);
                        let receiver = if object == "this" {
                            Receiver::SelfOf(container.map(str::to_owned))
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
        }
    }

    /// A property read or written (`order.Amount`, `this.Amount`, `Amount`, `new Order { Amount = 1 }`) as a call to
    /// it, and an operator used (`a + b`, `-a`, `a += b`) or a cast (`(Money)x`) as a call to the operator of the
    /// operand's type, only when that type is known and is not a built-in one: numbers and strings use no operator of
    /// ours, and without types every `+` would look like a call to the one `operator +` there is.
    fn other_call(&mut self, node: Node, container: Option<&str>, source: &str) -> Option<(String, Receiver, Target)> {
        let own = || Receiver::SelfOf(container.map(str::to_owned));
        match node.kind() {
            // `order.Amount`, and `order?.Amount` unless it is called.
            "member_access_expression" | "member_binding_expression" => {
                let name = field(node, "name").filter(|name| name.kind() == "identifier")?;
                let object = if node.kind() == "member_access_expression" {
                    if !is_read(node, source) {
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
                    match self.type_of(object, source) {
                        Some(type_) if built_in(&type_) => return None,
                        Some(type_) => Receiver::Type(type_),
                        None if object.kind() == "identifier" => on_name(text(Some(object), source)),
                        None => Receiver::Plain,
                    }
                };
                Some((text(Some(name), source).to_owned(), receiver, Target::Property))
            }
            "identifier" => {
                if !is_read(node, source) {
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
                    let type_ = type_name(text(field(created, "type"), source));
                    return Some((name.to_owned(), Receiver::Type(type_), Target::Property));
                }
                // An accessor's `value` is its parameter.
                let accessor =
                    || std::iter::successors(node.parent(), Node::parent).any(|up| up.kind() == "accessor_declaration");
                if self.local(node, name, source).is_some() || (name == "value" && accessor()) {
                    return None;
                }
                Some((name.to_owned(), own(), Target::Property))
            }
            "binary_expression" => {
                let operator = text(field(node, "operator"), source);
                if !BINARY.contains(&operator) {
                    return None;
                }
                let type_ = self.own_type(&[field(node, "left")?, field(node, "right")?], source)?;
                Some((format!("operator {operator}"), Receiver::Type(type_), Target::Operator))
            }
            "prefix_unary_expression" | "postfix_unary_expression" => {
                let (operator, operand) = unary(node, source)?;
                if !matches!(operator, "+" | "-" | "!" | "~" | "++" | "--") {
                    return None;
                }
                let type_ = self.own_type(&[operand], source)?;
                Some((format!("operator {operator}"), Receiver::Type(type_), Target::Operator))
            }
            // `a += b` uses `operator +`.
            "assignment_expression" => {
                let operator = text(field(node, "operator"), source).strip_suffix('=')?;
                if !BINARY.contains(&operator) {
                    return None;
                }
                let type_ = self.own_type(&[field(node, "left")?, field(node, "right")?], source)?;
                Some((format!("operator {operator}"), Receiver::Type(type_), Target::Operator))
            }
            // `(Money)x` is `explicit operator Money`, in Money or in x's type; to a built-in type, in x's.
            "cast_expression" => {
                let written = text(field(node, "type"), source);
                let to = type_name(written);
                let from = self.type_of(field(node, "value")?, source).filter(|from| !built_in(from));
                let receiver = match (built_in(&to), from) {
                    (false, Some(_)) => Receiver::Plain,
                    (false, None) => Receiver::Type(to),
                    (true, Some(from)) => Receiver::Type(from),
                    (true, None) => return None,
                };
                Some((format!("explicit operator {}", short_type(written)), receiver, Target::Operator))
            }
            _ => None,
        }
    }

    /// using A.B is the namespace's folder (A/B, or B), to its first file; `using X = A.B.C` and `using static` name a
    /// type, so A/B/C.cs first.
    fn imports(&self, root: Node, ctx: &Context, source: &str, add: &mut Add) {
        for directive in descendants(root, &["using_directive"]) {
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
}

impl Csharp {
    /// The first of these operands whose type is known and not built in.
    fn own_type(&mut self, operands: &[Node], source: &str) -> Option<String> {
        operands.iter().find_map(|operand| self.type_of(*operand, source).filter(|type_| !built_in(type_)))
    }

    /// The type of an expression, when the source shows it: a literal; a local, a parameter or a member of the type it
    /// is written in, by its declared type; `new T()` and `(T)x`; a call of the type's own method, by what it returns;
    /// and operators on those.
    fn type_of(&mut self, node: Node, source: &str) -> Option<String> {
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
            "parenthesized_expression" => self.type_of(named_children(node).into_iter().next()?, source),
            "object_creation_expression" | "cast_expression" => known(type_name(text(field(node, "type"), source))),
            "identifier" => {
                let name = text(Some(node), source);
                match self.local(node, name, source) {
                    Some(type_) => known(type_),
                    None => known(self.member_of_type(node, name, source)?),
                }
            }
            "member_access_expression" if on_this(node) => {
                known(self.member_of_type(node, text(field(node, "name"), source), source)?)
            }
            "invocation_expression" => {
                let callee = field(node, "function")?;
                let name = match callee.kind() {
                    "identifier" => text(Some(callee), source),
                    "member_access_expression" if on_this(callee) => text(field(callee, "name"), source),
                    _ => return None,
                };
                known(self.member_of_type(node, name, source)?)
            }
            "prefix_unary_expression" | "postfix_unary_expression" => {
                let (operator, operand) = unary(node, source)?;
                let type_ = self.type_of(operand, source)?;
                match operator {
                    "+" | "-" | "~" | "++" | "--" => Some(type_),
                    "!" if built_in(&type_) => Some("bool".into()),
                    _ => None,
                }
            }
            "binary_expression" => {
                let operator = text(field(node, "operator"), source);
                if matches!(operator, "==" | "!=" | "<" | ">" | "<=" | ">=" | "&&" | "||" | "is") {
                    return Some("bool".into());
                }
                let operands = [field(node, "left")?, field(node, "right")?];
                let types: Vec<String> = operands.iter().filter_map(|operand| self.type_of(*operand, source)).collect();
                types.iter().find(|type_| !built_in(type_)).or(types.first()).cloned()
            }
            _ => None,
        }
    }

    /// A local or parameter of the member `node` is written in, by name: its type, or "" when it is not written.
    fn local(&mut self, node: Node, name: &str, source: &str) -> Option<String> {
        let member = member(node)?;
        self.names.entry(member.id()).or_insert_with(|| locals(member, source)).get(name).cloned()
    }

    /// A field, property, event or method of the type `node` is written in, by name: its type, or what it returns.
    fn member_of_type(&mut self, node: Node, name: &str, source: &str) -> Option<String> {
        let mut holder = node.parent()?;
        while !matches!(
            holder.kind(),
            "class_declaration" | "struct_declaration" | "record_declaration" | "interface_declaration"
        ) {
            holder = holder.parent()?;
        }
        let body = field(holder, "body")?;
        self.names.entry(body.id()).or_insert_with(|| members(body, source)).get(name).cloned()
    }
}

/// The operators of two operands a type can declare (`&&` and `||` go through `&`, `|`, `true` and `false`).
const BINARY: [&str; 17] =
    ["+", "-", "*", "/", "%", "&", "|", "^", "<<", ">>", ">>>", "==", "!=", "<", ">", "<=", ">="];

/// The built-in types, whose operators are the language's own.
fn built_in(type_: &str) -> bool {
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

/// A unary expression's operator and operand: `-a`, `a++`.
fn unary<'t>(node: Node<'t>, source: &'t str) -> Option<(&'t str, Node<'t>)> {
    let mut cursor = node.walk();
    let operator = node.children(&mut cursor).find(|child| !child.is_named())?;
    Some((text(Some(operator), source), named_children(node).into_iter().next()?))
}

/// A type's name as types are matched: `Shop.Money?` and `Money<T>` are Money.
fn type_name(written: &str) -> String {
    last(&without_generics(written.trim_end_matches('?'))).to_owned()
}

/// A type as a conversion operator is named after it: without its namespace (`Shop.Money` is Money).
fn short_type(written: &str) -> String {
    let (head, generics) = written.split_at(written.find('<').unwrap_or(written.len()));
    format!("{}{generics}", head.rsplit('.').next().unwrap_or(head).trim())
}

/// The member of its type (a method, property, field…) a node is written in.
fn member(node: Node) -> Option<Node> {
    let mut member = node;
    while member.parent()?.kind() != "declaration_list" {
        member = member.parent()?;
    }
    Some(member)
}

/// The names a member declares (parameters, locals, `foreach`, `catch`, `out var` and pattern variables, a lambda's
/// parameters) with their written types; `var` takes the type of `new T()` or `(T)x`, else none.
fn locals(member: Node, source: &str) -> HashMap<String, String> {
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
        let type_ = type_.map(|type_| type_name(text(Some(type_), source))).unwrap_or_default();
        out.insert(text(Some(name), source).to_owned(), type_);
    }
    out
}

/// The fields, properties and events of a type with their types, and its methods with what they return.
fn members(body: Node, source: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for member in named_children(body) {
        match member.kind() {
            "field_declaration" | "event_field_declaration" => {
                let declarations = named_children(member).into_iter().filter(|d| d.kind() == "variable_declaration");
                for declaration in declarations {
                    let type_ = type_name(text(field(declaration, "type"), source));
                    for declarator in named_children(declaration) {
                        if declarator.kind() == "variable_declarator" {
                            out.insert(text(field(declarator, "name"), source).to_owned(), type_.clone());
                        }
                    }
                }
            }
            "property_declaration" | "event_declaration" | "method_declaration" => {
                let type_ = field(member, "type").or_else(|| field(member, "returns"));
                out.insert(text(field(member, "name"), source).to_owned(), type_name(text(type_, source)));
            }
            _ => {}
        }
    }
    out
}

/// Whether a name or member access stands where a value is read or written, not where it is declared, called (a
/// member access that is the function of a call is a call already), named as a type, or asked about with `nameof`.
fn is_read(node: Node, source: &str) -> bool {
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

#[cfg(test)]
mod tests {
    use super::super::testing::{calls, read, targeted};
    use super::*;
    use crate::code::languages::Language;

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
}
