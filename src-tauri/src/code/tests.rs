//! The languages read with tree-sitter on the sidecar's fixture projects: each file's functions, its imports resolved per language, and
//! the calls matched by name. How the sidecar puts them into a scan (parts, imports between parts) is tested end to end
//! in `api.rs`.

use std::collections::HashMap;
use std::path::Path;

use super::others::{read, Request, Wanted};
use super::types::FunctionNode;

fn fixture(name: &str) -> String {
    format!("{}/../sidecar/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn files(root: &str, dir: &str, out: &mut Vec<String>) {
    let mut entries: Vec<_> = std::fs::read_dir(Path::new(root).join(dir)).unwrap().flatten().collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = if dir.is_empty() { name } else { format!("{dir}/{name}") };
        if entry.path().is_dir() {
            files(root, &path, out);
        } else {
            out.push(path);
        }
    }
}

struct Seen {
    outline: HashMap<String, Vec<String>>,
    calls: Vec<String>,
    imports: Vec<String>,
}

fn outline(items: &[FunctionNode], depth: usize, out: &mut Vec<String>) {
    for item in items {
        let kind = format!("{:?}", item.kind).to_lowercase();
        let exported = if item.exported { " exp" } else { "" };
        out.push(format!("{}{kind} {}{exported} :{}", "  ".repeat(depth), item.name, item.line));
        outline(&item.children, depth + 1, out);
    }
}

fn names(items: &[FunctionNode], out: &mut HashMap<String, String>) {
    for item in items {
        out.insert(item.id.clone(), item.name.clone());
        names(&item.children, out);
    }
}

fn seen(name: &str, manifests: &[&str]) -> Seen {
    let root = fixture(name);
    let mut paths = Vec::new();
    files(&root, "", &mut paths);
    let request = Request {
        files: paths.iter().map(|path| Wanted { path: path.clone(), test: path.starts_with("tests/") }).collect(),
        cargo_manifests: manifests.iter().map(|manifest| manifest.to_string()).collect(),
    };
    let answer = read(&root, &request);
    let mut by_id = HashMap::new();
    let mut outlines = HashMap::new();
    let mut imports = Vec::new();
    for (path, file) in &answer.files {
        names(&file.tree, &mut by_id);
        let mut lines = Vec::new();
        outline(&file.tree, 0, &mut lines);
        outlines.insert(path.clone(), lines);
        imports.extend(
            file.imports.iter().map(|to| format!("{path} → {}{}", to.to, if to.type_only { " (types)" } else { "" })),
        );
    }
    let mut calls: Vec<String> =
        answer.calls.iter().map(|edge| format!("{} → {}", by_id[&edge.from], by_id[&edge.to])).collect();
    assert!(answer.calls.iter().all(|edge| edge.guessed));
    calls.sort();
    imports.sort();
    Seen { outline: outlines, calls, imports }
}

#[test]
fn a_laravel_app_classes_methods_psr4_and_calls_by_name() {
    let seen = seen("laravel", &[]);
    assert_eq!(
        seen.outline["app/Models/Order.php"],
        ["class Order exp :5", "  method total exp :7", "  method sum :12"]
    );
    assert_eq!(
        seen.calls,
        [
            "format → helper_round",
            "show → Order",
            "show → format",
            "show → total",
            "test_total → Order",
            "test_total → total",
            "total → sum",
        ]
    );
    assert!(seen
        .imports
        .contains(&"app/Http/Controllers/OrderController.php → app/Models/Order.php (types)".to_owned()));
    assert!(seen
        .imports
        .contains(&"app/Http/Controllers/OrderController.php → app/Services/Pricing.php (types)".to_owned()));
}

#[test]
fn a_wordpress_plugin_require_once_and_hooks_as_calls() {
    let seen = seen("wp-plugin", &[]);
    assert_eq!(
        seen.outline["includes/class-loader.php"],
        ["class My_Loader exp :3", "  method run exp :5", "  method register exp :11"]
    );
    assert_eq!(
        seen.calls,
        [
            "my_plugin_boot → My_Loader",
            "my_plugin_boot → run",
            "register → my_plugin_log",
            "run → my_plugin_log",
            "run → register",
        ]
    );
    assert!(seen.imports.iter().any(|import| import.starts_with("my-plugin.php → includes/")));
}

#[test]
fn a_cargo_workspace_mod_and_use_across_crates() {
    let seen = seen("rust-workspace", &["crates/cli/Cargo.toml", "crates/core/Cargo.toml"]);
    assert_eq!(
        seen.outline["crates/core/src/math.rs"],
        [
            "function add exp :1",
            "class impl Calc :7",
            "  method new exp :8",
            "  method sum exp :12",
            "  method double :16"
        ]
    );
    assert_eq!(seen.outline["crates/cli/src/commands.rs"], ["function run exp :3", "  arrow shift :4"]);
    assert_eq!(
        seen.calls,
        ["main → new", "main → run", "main → sum", "run → shift", "shift → version", "sum → add", "sum → double"]
    );
    assert!(seen.imports.contains(&"crates/cli/src/commands.rs → crates/core/src/lib.rs (types)".to_owned()));
}

#[test]
fn a_lua_project_require_and_methods() {
    let seen = seen("lua-app", &[]);
    assert_eq!(
        seen.outline["app/player.lua"],
        ["function Player.new exp :4", "function Player:greet exp :8", "function Player:format exp :12"]
    );
    assert_eq!(seen.outline["app/util.lua"], ["function M.clamp exp :3"]);
    assert_eq!(seen.outline["main.lua"], ["function start :4"]);
    assert_eq!(
        seen.calls,
        ["Player:greet → Player:format", "start → M.clamp", "start → Player.new", "start → Player:greet"]
    );
    assert!(seen.imports.iter().all(|import| import.starts_with("main.lua → app/")));
}

#[test]
fn a_python_app_imports_classes_and_calls_by_name() {
    let seen = seen("python-app", &[]);
    assert_eq!(seen.outline["shop/order.py"], ["class Order exp :4", "  method total exp :5", "  method _sum :8"]);
    assert_eq!(seen.outline["main.py"], ["function main exp :5"]);
    assert_eq!(seen.calls, ["main → Order", "main → rate", "main → total", "total → _sum", "total → rate"]);
    assert_eq!(
        seen.imports,
        ["main.py → shop/order.py", "main.py → shop/pricing.py", "shop/order.py → shop/pricing.py"]
    );
}

#[test]
fn a_go_module_packages_methods_and_calls_by_name() {
    let seen = seen("go-app", &[]);
    assert_eq!(seen.outline["order/order.go"], ["function New exp :5", "method Order.Total exp :9"]);
    assert_eq!(seen.calls, ["Order.Total → rate", "main → New", "main → Order.Total"]);
    // A package is a folder: an import leads to its first file.
    assert_eq!(seen.imports, ["main.go → order/order.go"]);
}

#[test]
fn a_java_project_imports_from_the_source_folder_and_calls_by_name() {
    let seen = seen("java-app", &[]);
    assert_eq!(
        seen.outline["src/main/java/com/shop/order/Order.java"],
        ["class Order exp :3", "  method Order exp :6", "  method create exp :10", "  method total exp :14"]
    );
    assert_eq!(seen.outline["src/main/java/com/shop/order/Pricing.java"], ["class Pricing :3", "  method rate :4"]);
    assert_eq!(seen.calls, ["create → Order", "main → create", "main → total", "total → rate"]);
    assert_eq!(seen.imports, ["src/main/java/com/shop/Main.java → src/main/java/com/shop/order/Order.java (types)"]);
}

#[test]
fn a_csharp_project_namespaces_to_folders_and_calls_by_name() {
    let seen = seen("csharp-app", &[]);
    assert_eq!(
        seen.outline["Shop/Orders/Order.cs"],
        ["class Order exp :3", "  method Order exp :7", "  method Create exp :9", "  method Total exp :11"]
    );
    // Properties with code and operators are methods with their calls; the auto-property Lines is left out.
    assert_eq!(
        seen.outline["Shop/Orders/Receipt.cs"],
        [
            "class Receipt exp :3",
            "  method Receipt exp :7",
            "  method Amount exp :11",
            "  method Text exp :13",
            "  method operator + exp :18",
            "  method implicit operator int exp :20",
            "  method Fee :22",
            "  method Format :24",
            "  method Merge :26"
        ]
    );
    assert_eq!(
        seen.calls,
        [
            "Amount → Fee",
            "Amount → Total",
            "Create → Order",
            "Fee → Rate",
            "Main → Create",
            "Main → Total",
            "Merge → Receipt",
            "Text → Format",
            "Total → Rate",
            "operator + → Merge"
        ]
    );
    // A namespace is a folder: a using leads to its first file.
    assert_eq!(seen.imports, ["Shop/Program.cs → Shop/Orders/Order.cs (types)"]);
}

#[test]
fn a_ruby_project_require_relative_modules_and_private_methods() {
    let seen = seen("ruby-app", &[]);
    assert_eq!(
        seen.outline["lib/shop/order.rb"],
        [
            "class Shop exp :4",
            "  class Order exp :5",
            "    method self.make exp :6",
            "    method initialize exp :10",
            "    method total exp :14",
            "    method round :20"
        ]
    );
    // Names alone (`lines`, `footer`, `header`) are calls unless they are locals; a block's calls count for its method.
    assert_eq!(
        seen.calls,
        [
            "main → self.make",
            "main → total",
            "text → footer",
            "text → format_line",
            "text → header",
            "text → lines",
            "total → round",
            "total → self.rate"
        ]
    );
    assert_eq!(
        seen.imports,
        [
            "lib/shop/order.rb → lib/shop/pricing.rb",
            "lib/shop/receipt.rb → lib/shop/order.rb",
            "main.rb → lib/shop/order.rb"
        ]
    );
}

#[test]
fn a_c_project_headers_static_functions_and_calls_by_name() {
    let seen = seen("c-app", &[]);
    assert_eq!(
        seen.outline["order.c"],
        ["function rate :3", "function order_new exp :7", "function order_total exp :12"]
    );
    // A header's declarations are no functions; the definitions in order.c are.
    assert!(seen.outline["order.h"].is_empty());
    assert_eq!(seen.calls, ["main → order_new", "main → order_total", "order_total → rate"]);
    assert_eq!(seen.imports, ["main.c → order.h", "order.c → order.h"]);
}

#[test]
fn a_cpp_project_namespaces_classes_and_methods_written_outside_them() {
    let seen = seen("cpp-app", &[]);
    assert_eq!(
        seen.outline["src/shop/order.hpp"],
        [
            "class shop exp :3",
            "  class Order exp :5",
            "    method make exp :7",
            "    method total exp :8",
            "    method Order :11"
        ]
    );
    // Declared and defined in one file: listed once, as the definition; count is only declared.
    assert_eq!(
        seen.outline["src/shop/basket.hpp"],
        [
            "class shop exp :5",
            "  class Basket exp :7",
            "    method count exp :9",
            "    method print exp :11",
            "  method Basket::sum exp :14"
        ]
    );
    assert_eq!(
        seen.outline["src/shop/order.cpp"],
        ["class shop exp :4", "  method Order::make exp :6", "  method Order::total exp :10"]
    );
    // The header declares make and total and order.cpp defines them: calls go to the definitions, where their calls
    // are; a method only declared (count) is called as its declaration.
    assert_eq!(seen.calls, ["Basket::sum → count", "Order::total → rate", "main → Order::make", "main → Order::total"]);
    assert_eq!(
        seen.imports,
        [
            "src/main.cpp → src/shop/order.hpp",
            "src/shop/basket.hpp → src/shop/order.hpp",
            "src/shop/order.cpp → src/shop/order.hpp",
            "src/shop/order.cpp → src/shop/pricing.hpp"
        ]
    );
}

/* ---------- the changes by function ---------- */

mod changes {
    use std::collections::HashMap;

    use crate::code::changes::{changes_between, changes_of, follow_moves, staged_changes, ChangedFile, ScannedFile};
    use crate::code::others::functions_of;
    use crate::git::diff::Wanted;
    use crate::git::tests::Repo;

    const A1: &str = "pub fn keep() -> u32 { 1 }\npub fn edit() -> u32 {\n    1\n}\npub fn drop() -> u32 { 3 }\nimpl Order {\n    fn total(&self) -> u32 { 0 }\n}\n";
    const A2: &str = "pub fn keep() -> u32 { 1 }\npub fn edit() -> u32 {\n    2\n}\nimpl Order {\n    fn total(&self) -> u32 { 0 }\n}\npub fn fresh() -> u32 { keep() }\n";

    /// main: a.rs, old.rs, gone.rs, other.rs; feature (two commits): a.rs edited, old.rs renamed; main moves on; then
    /// uncommitted work: a new file git does not know yet, and a deleted one.
    fn feature_branch() -> Repo {
        let r = Repo::new();
        r.write("a.rs", A1);
        r.write("old.rs", "pub fn moved() -> &'static str {\n    \"a\"\n}\npub fn same() -> u32 { 1 }\n");
        r.write("gone.rs", "pub fn lost() -> u32 { 1 }\n");
        r.write("other.rs", "pub fn elsewhere() -> u32 { 1 }\n");
        r.sh(&["add", "."]);
        r.sh(&["commit", "-q", "-m", "First"]);
        r.sh(&["switch", "-q", "-c", "feature"]);
        r.write("a.rs", A2);
        r.sh(&["commit", "-q", "-am", "Edit a"]);
        r.sh(&["mv", "old.rs", "renamed.rs"]);
        r.write("renamed.rs", "pub fn moved() -> &'static str {\n    \"b\"\n}\npub fn same() -> u32 { 1 }\n");
        r.sh(&["add", "."]);
        r.sh(&["commit", "-q", "-m", "Rename old"]);
        r.sh(&["switch", "-q", "main"]);
        r.write("other.rs", "pub fn elsewhere() -> u32 { 2 }\n");
        r.sh(&["commit", "-q", "-am", "Main moves on"]);
        r.sh(&["switch", "-q", "feature"]);
        r.write("new.rs", "pub fn brand_new() -> u32 { 1 }\n");
        std::fs::remove_file(r.path("gone.rs")).unwrap();
        r
    }

    /// The working tree as a scan has it: every .rs file at the top, in the part "app".
    fn scanned(r: &Repo) -> HashMap<String, ScannedFile> {
        let mut files = HashMap::new();
        for entry in std::fs::read_dir(&r.root).unwrap().flatten() {
            let path = entry.file_name().to_string_lossy().into_owned();
            if !path.ends_with(".rs") {
                continue;
            }
            let text = std::fs::read_to_string(entry.path()).unwrap();
            let functions = functions_of(&r.root, &path, &text);
            files.insert(path.clone(), ScannedFile { path, part: "app".into(), functions });
        }
        files
    }

    fn statuses(files: &[ChangedFile], path: &str) -> Vec<String> {
        let file = files.iter().find(|file| file.path == path).unwrap_or_else(|| panic!("{path} is not changed"));
        file.functions
            .iter()
            .map(|function| format!("{:?}:{}", function.status, function.qualified).to_lowercase())
            .collect()
    }

    #[test]
    fn maps_changes_since_the_merge_base_onto_functions() {
        let r = feature_branch();
        let files = scanned(&r);
        let read = |path: &str, text: &str| functions_of(&r.root, path, text);
        let result = changes_of(&r.root, Wanted::MergeBase(None), &files, &read).unwrap();
        assert_eq!(result.base.reference.as_deref(), Some("main"));
        assert_eq!((result.base.ahead, result.base.behind), (2, 1));
        assert_eq!(result.head.branch.as_deref(), Some("feature"));
        // Main's own commit after the branch left is not the branch's change.
        assert!(!result.files.iter().any(|file| file.path == "other.rs"));
        assert_eq!(statuses(&result.files, "a.rs"), ["changed:edit", "removed:drop", "added:fresh"]);
        let renamed = result.files.iter().find(|file| file.path == "renamed.rs").unwrap();
        assert_eq!((format!("{:?}", renamed.status), renamed.old_path.as_str()), ("Renamed".into(), "old.rs"));
        assert_eq!(statuses(&result.files, "renamed.rs"), ["changed:moved"]);
        assert_eq!(statuses(&result.files, "new.rs"), ["added:brand_new"]);
        let gone = result.files.iter().find(|file| file.path == "gone.rs").unwrap();
        assert_eq!(gone.part, None);
        assert_eq!(statuses(&result.files, "gone.rs"), ["removed:lost"]);
        // Changed and added functions carry their id in the scan; removed ones have none.
        let a = &result.files.iter().find(|file| file.path == "a.rs").unwrap().functions;
        assert!(a.iter().find(|function| function.name == "edit").unwrap().id.is_some());
        assert!(a.iter().find(|function| function.name == "drop").unwrap().id.is_none());
    }

    #[test]
    fn uncommitted_only_and_a_ref_as_the_base() {
        let r = feature_branch();
        let files = scanned(&r);
        let read = |path: &str, text: &str| functions_of(&r.root, path, text);
        let uncommitted = changes_of(&r.root, Wanted::Uncommitted, &files, &read).unwrap();
        let mut paths: Vec<&str> = uncommitted.files.iter().map(|file| file.path.as_str()).collect();
        paths.sort();
        assert_eq!(paths, ["gone.rs", "new.rs"]);

        r.sh(&["tag", "v1", "HEAD~1"]);
        let tagged = changes_of(&r.root, Wanted::Ref("v1"), &files, &read).unwrap();
        assert_eq!(tagged.base.label, "v1");
        assert!(!tagged.files.iter().any(|file| file.path == "a.rs"));
        assert!(tagged.files.iter().any(|file| file.path == "renamed.rs"));
    }

    #[test]
    fn a_renumbered_anonymous_function_is_neither_added_nor_removed() {
        let r = Repo::new();
        let head = "fn step(f: impl Fn()) { f() }\nfn main() {\n";
        r.write(
            "list.rs",
            &format!("{head}    step(|| {{ fn one() {{}} }});\n    step(|| {{ fn two() {{}} }});\n}}\n"),
        );
        r.sh(&["add", "."]);
        r.sh(&["commit", "-q", "-m", "First"]);
        // A new closure above the others: `(anonymous) #2` is now `#3`, and so on.
        r.write(
            "list.rs",
            &format!("{head}    step(|| {{ fn zero() {{}} }});\n    step(|| {{ fn one() {{}} }});\n    step(|| {{ fn two() {{}} }});\n}}\n"),
        );
        let files = scanned(&r);
        let read = |path: &str, text: &str| functions_of(&r.root, path, text);
        let result = changes_of(&r.root, Wanted::Uncommitted, &files, &read).unwrap();
        // The new closure takes the first one's name, so that one reads as changed; one() and two() are only renumbered.
        assert_eq!(
            statuses(&result.files, "list.rs"),
            ["changed:main › (anonymous)", "added:main › (anonymous) › zero"]
        );
    }

    #[test]
    fn maps_a_commit_a_range_and_a_first_commit_with_ids_where_the_working_tree_has_them() {
        let r = feature_branch();
        let shas = r.sh(&["log", "--format=%H", "feature"]);
        let shas: Vec<&str> = shas.lines().collect();
        let (rename, edit, first) = (shas[0], shas[1], shas[2]);
        let files = scanned(&r);
        let read = |path: &str, text: &str| functions_of(&r.root, path, text);

        let one = changes_between(&r.root, Some(first), edit, &files, &read).unwrap();
        assert_eq!(statuses(&one.files, "a.rs"), ["changed:edit", "removed:drop", "added:fresh"]);
        let a = one.files.iter().find(|file| file.path == "a.rs").unwrap();
        assert!(a.functions.iter().find(|function| function.name == "edit").unwrap().id.is_some());
        assert!(a.functions.iter().find(|function| function.name == "drop").unwrap().id.is_none());
        assert_eq!(a.part.as_deref(), Some("app"));

        let range = changes_between(&r.root, Some(first), rename, &files, &read).unwrap();
        let mut seen: Vec<String> = range.files.iter().map(|file| format!("{:?}:{}", file.status, file.path)).collect();
        seen.sort();
        assert_eq!(seen, ["Modified:a.rs", "Renamed:renamed.rs"]);
        assert_eq!(statuses(&range.files, "renamed.rs"), ["changed:moved"]);

        // The first commit: everything it has is added; gone.rs is deleted in the working tree, so lost() has no id now.
        let start = changes_between(&r.root, None, first, &files, &read).unwrap();
        assert_eq!(statuses(&start.files, "gone.rs"), ["added:lost"]);
        assert!(start.files.iter().find(|file| file.path == "gone.rs").unwrap().functions[0].id.is_none());
    }

    #[test]
    fn maps_only_what_is_staged() {
        let r = Repo::new();
        r.write("a.rs", A1);
        r.write("b.rs", "pub fn other() -> u32 { 1 }\n");
        r.sh(&["add", "."]);
        r.sh(&["commit", "-q", "-m", "First"]);
        // a.rs staged as A2; then edited again without staging; b.rs changed but not staged.
        r.write("a.rs", A2);
        r.sh(&["add", "a.rs"]);
        let again =
            A2.replace("    2\n", "    3\n").replace("pub fn keep() -> u32 { 1 }", "pub fn keep() -> u32 { 9 }");
        r.write("a.rs", &again);
        r.write("b.rs", "pub fn other() -> u32 { 2 }\n");
        let files = scanned(&r);
        let read = |path: &str, text: &str| functions_of(&r.root, path, text);
        let staged = staged_changes(&r.root, &files, &read).unwrap();
        assert_eq!(staged.files.iter().map(|file| file.path.as_str()).collect::<Vec<_>>(), ["a.rs"]);
        // keep() is changed only in the working tree, not in what is staged.
        assert_eq!(statuses(&staged.files, "a.rs"), ["changed:edit", "removed:drop", "added:fresh"]);
        assert_eq!(staged.to, "index");
    }

    const HELPER: &str =
        "pub fn helper(x: u32) -> u32 {\n    let y = x + 1;\n    let z = y * 2;\n    z - helper_base()\n}\n";

    /// a.rs has helper(); it is edited, then moved to b.rs and renamed assist() with one line changed.
    fn moved_helper() -> (Repo, Vec<String>) {
        let r = Repo::new();
        r.write("a.rs", &format!("{HELPER}pub fn helper_base() -> u32 {{ 0 }}\n"));
        r.sh(&["add", "."]);
        r.sh(&["commit", "-q", "-m", "First"]);
        r.write("a.rs", &format!("{}pub fn helper_base() -> u32 {{ 0 }}\n", HELPER.replace("x + 1", "x + 2")));
        r.sh(&["commit", "-q", "-am", "Edit helper"]);
        r.write("a.rs", "pub fn helper_base() -> u32 { 0 }\n");
        r.write(
            "b.rs",
            "use crate::a::helper_base;\n\npub fn assist(x: u32) -> u32 {\n    let y = x + 2;\n    let z = y * 3;\n    z - helper_base()\n}\n",
        );
        r.sh(&["add", "."]);
        r.sh(&["commit", "-q", "-m", "Move helper"]);
        let shas = r.sh(&["log", "--format=%H"]).lines().map(str::to_owned).collect();
        (r, shas)
    }

    #[test]
    fn a_function_moved_to_another_file_and_renamed_is_one_move() {
        let (r, shas) = moved_helper();
        let files = scanned(&r);
        let read = |path: &str, text: &str| functions_of(&r.root, path, text);
        let moved = changes_between(&r.root, Some(&shas[1]), &shas[0], &files, &read).unwrap();
        assert_eq!(statuses(&moved.files, "b.rs"), ["moved:assist"]);
        // Not also listed as removed where it was.
        assert!(statuses(&moved.files, "a.rs").is_empty());
        let assist = &moved.files.iter().find(|file| file.path == "b.rs").unwrap().functions[0];
        let from = assist.from.as_ref().unwrap();
        assert_eq!((from.path.as_str(), from.name.as_str(), from.line), ("a.rs", "helper", 1));
        // One line differs between the two bodies (the renamed first line counts as the same).
        assert_eq!((assist.added, assist.removed), (1, 1));

        // Uncommitted, the same way: a short function moved stays an addition and a removal.
        r.write("a.rs", "");
        r.write("c.rs", "pub fn helper_base() -> u32 { 0 }\n");
        let files = scanned(&r);
        let local = changes_of(&r.root, Wanted::Uncommitted, &files, &read).unwrap();
        assert_eq!(statuses(&local.files, "c.rs"), ["added:helper_base"]);
    }

    #[test]
    fn a_function_history_goes_on_past_the_commit_that_moved_it() {
        let (r, shas) = moved_helper();
        let files = scanned(&r);
        let read = |path: &str, text: &str| functions_of(&r.root, path, text);
        let mut history = crate::git::function_history(&r.root, "b.rs", 3, 7);
        assert_eq!(history.commits.iter().map(|commit| commit.sha.as_str()).collect::<Vec<_>>(), [shas[0].as_str()]);
        follow_moves(&r.root, &mut history, "b.rs", 3, &files, &read);
        let subjects: Vec<&str> = history.commits.iter().map(|commit| commit.subject.as_str()).collect();
        assert_eq!(subjects, ["Move helper", "Edit helper", "First"]);
        assert_eq!(history.moves.len(), 1);
        let step = &history.moves[0];
        assert_eq!(
            (step.sha.as_str(), step.from_path.as_str(), step.from_name.as_str(), step.to_name.as_str()),
            (shas[0].as_str(), "a.rs", "helper", "assist")
        );
    }
}
