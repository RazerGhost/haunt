//! The code, beside the sidecar's TypeScript scan: PHP, Rust, Lua, Python and Go through the native tree-sitter
//! grammars (`read.rs`, with calls matched by name in `guess.rs`), which the sidecar asks for while it scans
//! (`others.rs`); the whole scan of those languages when no JavaScript runtime is found (`scan.rs`); and the HTTP
//! routes the Routes view lists (`routes.rs`).

pub mod architecture;
pub mod changes;
mod guess;
pub mod languages;
pub mod others;
mod project;
mod read;
pub mod routes;
pub mod scan;
pub mod types;

#[cfg(test)]
mod tests;
