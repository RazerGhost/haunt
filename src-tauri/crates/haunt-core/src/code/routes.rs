//! The HTTP routes in the code, for the Routes view: the TypeScript family's Hono and Express routes come from the
//! scanner (`sidecar/routes.ts`); WordPress's REST routes (`register_rest_route`) are read here from the PHP files with
//! tree-sitter. Nothing runs and nothing leaves the machine.

use std::path::Path;

use serde::{Deserialize, Serialize};
use specta::Type;
use tree_sitter::{Node, Parser};

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    pub method: String,
    pub path: String,
    pub file: String,
    pub line: u32,
    /// `hono-express` or `wordpress`.
    pub framework: String,
    /// The handler's function id in the scan, when the scanner knows it; else the window finds it by name.
    pub handler: Option<String>,
    /// The handler as written: a name, or `(inline)`.
    pub handler_name: String,
    /// The functions an inline handler calls, by id.
    pub calls: Vec<String>,
    /// What runs before the handler: middleware, or WordPress's permission callback.
    pub middleware: Vec<String>,
}

fn text<'s>(node: Option<Node>, source: &'s str) -> &'s str {
    node.and_then(|node| node.utf8_text(source.as_bytes()).ok()).unwrap_or("")
}

fn named_children<'t>(node: Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

/// A string literal's content; None for anything else.
fn string(node: Node, source: &str) -> Option<String> {
    let node = if node.kind() == "argument" { named_children(node).into_iter().next()? } else { node };
    if !matches!(node.kind(), "string" | "encapsed_string") {
        return None;
    }
    let whole = text(Some(node), source);
    (whole.len() >= 2).then(|| whole[1..whole.len() - 1].to_owned())
}

/// The value of `name` in a PHP array literal (`['methods' => 'GET', …]`).
fn entry<'t>(array: Node<'t>, name: &str, source: &str) -> Option<Node<'t>> {
    named_children(array).into_iter().find_map(|element| {
        let parts = named_children(element);
        match parts.as_slice() {
            [key, value] if string(*key, source).as_deref() == Some(name) => Some(*value),
            _ => None,
        }
    })
}

/// A callback as people name it: `'my_function'`, `[$this, 'get_item']`, `[Items::class, 'list']`.
fn callback(value: Node, source: &str) -> String {
    if let Some(name) = string(value, source) {
        return name.rsplit("::").next().unwrap_or(&name).to_owned();
    }
    if value.kind() == "array_creation_expression" {
        let items: Vec<Node> = named_children(value)
            .into_iter()
            .filter_map(|element| named_children(element).into_iter().last())
            .collect();
        if let Some(method) = items.get(1).and_then(|item| string(*item, source)) {
            return method;
        }
    }
    let raw = text(Some(value), source).split_whitespace().collect::<Vec<_>>().join(" ");
    if raw.starts_with("function") || raw.starts_with("fn") {
        "(inline)".into()
    } else {
        raw.chars().take(40).collect()
    }
}

/// `'GET'`, `'GET, POST'`, `WP_REST_Server::READABLE` and the others, `['GET', 'POST']`.
fn methods(value: Option<Node>, source: &str) -> String {
    let Some(value) = value else { return "GET".into() };
    if let Some(listed) = string(value, source) {
        return listed.split(',').map(|method| method.trim().to_uppercase()).collect::<Vec<_>>().join(", ");
    }
    let raw = text(Some(value), source);
    match raw.rsplit("::").next().unwrap_or(raw) {
        "READABLE" => "GET".into(),
        "CREATABLE" => "POST".into(),
        "EDITABLE" => "POST, PUT, PATCH".into(),
        "DELETABLE" => "DELETE".into(),
        "ALLMETHODS" => "ALL".into(),
        _ if value.kind() == "array_creation_expression" => named_children(value)
            .into_iter()
            .filter_map(|element| named_children(element).into_iter().last().and_then(|item| string(item, source)))
            .map(|method| method.to_uppercase())
            .collect::<Vec<_>>()
            .join(", "),
        other => other.to_owned(),
    }
}

/// The REST routes a PHP file registers with `register_rest_route($namespace, $route, $args)`: one per endpoint, with
/// its methods, callback and permission callback, under `/wp-json/`.
pub fn wordpress_routes(path: &str, source: &str) -> Vec<Route> {
    if !source.contains("register_rest_route") {
        return Vec::new();
    }
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_php::LANGUAGE_PHP.into()).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else { return Vec::new() };
    let mut out = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        stack.extend(named_children(node));
        if node.kind() != "function_call_expression"
            || text(node.child_by_field_name("function"), source) != "register_rest_route"
        {
            continue;
        }
        let Some(arguments) = node.child_by_field_name("arguments") else { continue };
        let arguments = named_children(arguments);
        let (Some(namespace), Some(route)) = (
            arguments.first().and_then(|node| string(*node, source)),
            arguments.get(1).and_then(|node| string(*node, source)),
        ) else {
            continue;
        };
        let full = format!("/wp-json/{}/{}", namespace.trim_matches('/'), route.trim_start_matches('/'));
        let Some(args) =
            arguments.get(2).map(|argument| named_children(*argument).into_iter().next().unwrap_or(*argument))
        else {
            continue;
        };
        if args.kind() != "array_creation_expression" {
            continue;
        }
        // One endpoint, or a list of them (`[['methods' => 'GET', …], ['methods' => 'POST', …]]`).
        let first =
            named_children(args).into_iter().next().and_then(|element| named_children(element).into_iter().last());
        let endpoints: Vec<Node> = if first.is_some_and(|value| value.kind() == "array_creation_expression")
            && entry(args, "callback", source).is_none()
        {
            named_children(args).into_iter().filter_map(|element| named_children(element).into_iter().last()).collect()
        } else {
            vec![args]
        };
        for endpoint in endpoints {
            let Some(handler) = entry(endpoint, "callback", source) else { continue };
            let permission = entry(endpoint, "permission_callback", source)
                .map(|value| format!("permission: {}", callback(value, source)));
            out.push(Route {
                method: methods(entry(endpoint, "methods", source), source),
                path: full.clone(),
                file: path.to_owned(),
                line: node.start_position().row as u32 + 1,
                framework: "wordpress".into(),
                handler: None,
                handler_name: callback(handler, source),
                calls: Vec::new(),
                middleware: permission.into_iter().collect(),
            });
        }
    }
    out.sort_by_key(|route| route.line);
    out
}

/// The WordPress routes of the scan's PHP files.
pub fn php_routes(root: &str, paths: &[String]) -> Vec<Route> {
    paths
        .iter()
        .filter(|path| path.ends_with(".php"))
        .filter_map(|path| {
            std::fs::read_to_string(Path::new(root).join(path)).ok().map(|source| wordpress_routes(path, &source))
        })
        .flatten()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_wordpress_rest_routes() {
        let source = r#"<?php
add_action('rest_api_init', function () {
    register_rest_route('shop/v1', '/items', [
        'methods' => WP_REST_Server::READABLE,
        'callback' => 'shop_list_items',
        'permission_callback' => '__return_true',
    ]);
    register_rest_route('shop/v1', '/items/(?P<id>\d+)', [
        ['methods' => 'GET', 'callback' => [$this, 'get_item']],
        ['methods' => 'POST, PUT', 'callback' => [Items::class, 'update'], 'permission_callback' => [$this, 'can_edit']],
    ]);
});
"#;
        let routes: Vec<(String, String, String, Vec<String>)> = wordpress_routes("plugin.php", source)
            .into_iter()
            .map(|route| (route.method, route.path, route.handler_name, route.middleware))
            .collect();
        assert_eq!(
            routes,
            [
                (
                    "GET".into(),
                    "/wp-json/shop/v1/items".into(),
                    "shop_list_items".into(),
                    vec!["permission: __return_true".to_owned()]
                ),
                ("GET".into(), r"/wp-json/shop/v1/items/(?P<id>\d+)".into(), "get_item".into(), vec![]),
                (
                    "POST, PUT".into(),
                    r"/wp-json/shop/v1/items/(?P<id>\d+)".into(),
                    "update".into(),
                    vec!["permission: can_edit".to_owned()]
                ),
            ]
        );
    }
}
