//! The calls between the functions read with tree-sitter, by name: there is no type checker, so a call goes to a function of
//! that name in the caller's class or impl first, then its file, then the files it imports, then the one function
//! anywhere with that name. Calls found this way are marked `guessed`; an ambiguous name is left out rather than guessed
//! wrong. A C++ method declared in its class and defined elsewhere is called as its definition. A C# property read
//! reaches only a property (or an event with accessors), and an operator used only an operator, so neither makes a
//! method of the same name look called.

use std::collections::{HashMap, HashSet};

use super::read::{scope_of, FileReading, Receiver, Target};
use super::types::{CallEdge, FunctionKind, FunctionNode};

struct Entry<'a> {
    id: &'a str,
    name: &'a str,
    path: &'a str,
    container: Option<&'a str>,
    kind: FunctionKind,
}

/// The name a call uses for a function: Lua's `M.f` and `M:f` as `f`, C++'s `Order::total` as `total`, Ruby's
/// `self.make` as `make`, a Rust `impl Trait for Type` as `Type`.
fn call_name(name: &str) -> &str {
    let name = match name.strip_prefix("impl ") {
        Some(rest) => rest.rfind(" for ").map_or(rest, |at| &rest[at + 5..]),
        None => name,
    };
    name.rsplit(['.', ':']).next().unwrap_or(name)
}

/// A C# operator's name: `operator +`, `implicit operator Money`.
fn is_operator(name: &str) -> bool {
    name.starts_with("operator ") || name.starts_with("implicit operator ") || name.starts_with("explicit operator ")
}

fn entries<'a>(path: &'a str, items: &'a [FunctionNode], container: Option<&'a str>, out: &mut Vec<Entry<'a>>) {
    for item in items {
        if !item.anonymous {
            // A method written outside its type (C++ `void Order::total()`) belongs to the type its name starts with.
            let container =
                if item.kind == FunctionKind::Method { scope_of(&item.name).or(container) } else { container };
            out.push(Entry { id: &item.id, name: &item.name, path, container, kind: item.kind });
        }
        let inner = if item.kind == FunctionKind::Class { Some(item.name.as_str()) } else { container };
        entries(path, &item.children, inner, out);
    }
}

/// The calls between the files' functions; `imports` are (from, to) pairs between the files, tests left out.
pub fn guessed_calls(readings: &[(String, &FileReading)], imports: &[(String, String)]) -> Vec<CallEdge> {
    let mut all = Vec::new();
    for (path, reading) in readings {
        entries(path, &reading.tree, None, &mut all);
    }
    // A C++ method declared in a header and defined in a .cpp is called as its definition, where its calls are.
    let declarations: HashSet<&str> =
        readings.iter().flat_map(|(_, reading)| reading.declarations.iter().map(String::as_str)).collect();
    let defined: HashSet<(Option<&str>, &str)> = all
        .iter()
        .filter(|entry| entry.kind == FunctionKind::Method && !declarations.contains(entry.id))
        .map(|entry| (entry.container, call_name(entry.name)))
        .collect();
    let mut by_name: HashMap<&str, Vec<&Entry>> = HashMap::new();
    for entry in &all {
        if declarations.contains(entry.id) && defined.contains(&(entry.container, call_name(entry.name))) {
            continue;
        }
        by_name.entry(call_name(entry.name)).or_default().push(entry);
    }
    let properties: HashSet<&str> =
        readings.iter().flat_map(|(_, reading)| reading.properties.iter().map(String::as_str)).collect();
    let mut imported: HashMap<&str, HashSet<&str>> = HashMap::new();
    for (from, to) in imports {
        imported.entry(from.as_str()).or_default().insert(to.as_str());
    }

    let mut order: Vec<(String, String)> = Vec::new();
    let mut counts: HashMap<(String, String), u32> = HashMap::new();
    for (path, reading) in readings {
        for call in &reading.calls {
            let mut candidates: Vec<&Entry> = by_name.get(call.name.as_str()).cloned().unwrap_or_default();
            // A cast (`explicit operator Money`) can use an implicit conversion too.
            if candidates.is_empty() {
                if let Some(type_) = call.name.strip_prefix("explicit operator ") {
                    let implicit = format!("implicit operator {type_}");
                    candidates = by_name.get(implicit.as_str()).cloned().unwrap_or_default();
                }
            }
            match call.target {
                Target::Any => {}
                Target::Property => candidates.retain(|entry| properties.contains(entry.id)),
                Target::Operator => candidates.retain(|entry| is_operator(entry.name)),
            }
            match &call.receiver {
                Receiver::Type(type_) => {
                    let suffix = format!(" {type_}");
                    let in_type: Vec<&Entry> = candidates
                        .iter()
                        .copied()
                        .filter(|entry| entry.container.is_some_and(|c| c == type_ || c.ends_with(&suffix)))
                        .collect();
                    candidates = if in_type.is_empty() {
                        candidates
                            .into_iter()
                            .filter(|entry| entry.kind == FunctionKind::Class && entry.name == type_)
                            .collect()
                    } else {
                        in_type
                    };
                }
                Receiver::SelfOf(container) => {
                    let own: Vec<&Entry> = candidates
                        .iter()
                        .copied()
                        .filter(|entry| {
                            entry.path == path && (container.is_none() || entry.container == container.as_deref())
                        })
                        .collect();
                    candidates = if own.is_empty() {
                        candidates.into_iter().filter(|entry| entry.kind == FunctionKind::Method).collect()
                    } else {
                        own
                    };
                }
                Receiver::Plain => {}
            }
            let same_file: Vec<&Entry> = candidates.iter().copied().filter(|entry| entry.path == path).collect();
            let from_imports: Vec<&Entry> = candidates
                .iter()
                .copied()
                .filter(|entry| imported.get(path.as_str()).is_some_and(|files| files.contains(entry.path)))
                .collect();
            let pick = if same_file.len() == 1 {
                same_file[0]
            } else if from_imports.len() == 1 {
                from_imports[0]
            } else if candidates.len() == 1 {
                candidates[0]
            } else {
                continue;
            };
            if pick.id == call.from {
                continue;
            }
            let key = (call.from.clone(), pick.id.to_owned());
            let count = counts.entry(key.clone()).or_insert(0);
            if *count == 0 {
                order.push(key);
            }
            *count += 1;
        }
    }
    order
        .into_iter()
        .map(|key| {
            let count = counts[&key];
            CallEdge { from: key.0, to: key.1, count, guessed: true }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_a_call_uses() {
        assert_eq!(call_name("M.start"), "start");
        assert_eq!(call_name("M:go"), "go");
        assert_eq!(call_name("impl Show for Order"), "Order");
        assert_eq!(call_name("impl Order"), "Order");
        assert_eq!(call_name("total"), "total");
        assert_eq!(call_name("Order::total"), "total");
        assert_eq!(call_name("self.make"), "make");
    }

    #[test]
    fn a_cpp_method_written_outside_its_class() {
        assert_eq!(scope_of("Order::total"), Some("Order"));
        assert_eq!(scope_of("shop::Box<T>::get"), Some("Box"));
        assert_eq!(scope_of("total"), None);
    }

    fn guessed(files: &[(&str, &str)]) -> Vec<String> {
        use super::super::languages::Language;
        use super::super::read::{read_file, Context};
        let crates = HashMap::new();
        let readings: Vec<(String, FileReading)> = files
            .iter()
            .map(|(path, source)| {
                let ctx = Context { root: ".", path, psr4: &[], crates: &crates, go_module: "" };
                (path.to_string(), read_file(Language::Csharp, &ctx, source))
            })
            .collect();
        let mut names = HashMap::new();
        fn walk<'a>(items: &'a [FunctionNode], out: &mut HashMap<&'a str, &'a str>) {
            for item in items {
                out.insert(item.id.as_str(), item.name.as_str());
                walk(&item.children, out);
            }
        }
        for (_, reading) in &readings {
            walk(&reading.tree, &mut names);
        }
        let by_path: Vec<(String, &FileReading)> =
            readings.iter().map(|(path, reading)| (path.clone(), reading)).collect();
        let mut out: Vec<String> = guessed_calls(&by_path, &[])
            .iter()
            .map(|edge| format!("{} → {}", names[edge.from.as_str()], names[edge.to.as_str()]))
            .collect();
        out.sort();
        out
    }

    #[test]
    fn a_csharp_property_read_reaches_only_a_property_and_an_operator_only_an_operator() {
        let calls = guessed(&[
            (
                "Shop/Order.cs",
                "public class Order {
  public int Amount { get { return Sum(); } }
  public int Total() { return 1; }
  public int Sum() { return 2; }
  public int Count;
  public static Order operator +(Order a, Order b) => a;
  public static implicit operator int(Order o) => o.Amount;
}
",
            ),
            (
                "Shop/Use.cs",
                "public class Use {
  public int Run(Order order, Order other) {
    var amount = order.Amount;
    var total = order.Total;
    var count = order.Count;
    var both = order + other;
    var sum = 1 + 2;
    return (int)order;
  }
  public int Sum => 3;
}
",
            ),
        ]);
        // `order.Total` names the method without calling it and Count is a field: neither is a call. `(int)order`
        // uses the implicit conversion. Use's own property Sum is not what Amount's `Sum()` calls.
        assert_eq!(
            calls,
            [
                "Amount → Sum",
                "Run → Amount",
                "Run → implicit operator int",
                "Run → operator +",
                "implicit operator int → Amount"
            ]
        );
    }
}
