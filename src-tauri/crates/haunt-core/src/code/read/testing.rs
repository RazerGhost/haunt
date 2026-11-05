//! What the languages' tests share: reading a source as one file, and its functions as an outline.

use std::collections::HashMap;

use super::{read_file, Context, FileReading, Receiver, Target};
use crate::code::languages::Language;
use crate::code::types::FunctionNode;

fn outline(items: &[FunctionNode], depth: usize, out: &mut Vec<String>) {
    for item in items {
        let exported = if item.exported { " exp" } else { "" };
        out.push(format!("{}{:?} {}{exported} :{}", "  ".repeat(depth), item.kind, item.name, item.line));
        outline(&item.children, depth + 1, out);
    }
}

/// Reads `source` as the file at `path`: its functions as lines (`Method total exp :4`, indented under their parent),
/// and the whole reading.
pub fn read(language: Language, path: &str, source: &str) -> (Vec<String>, FileReading) {
    let crates = HashMap::new();
    let ctx = Context { root: ".", path, psr4: &[], crates: &crates, go_module: "" };
    let reading = read_file(language, &ctx, source);
    let mut lines = Vec::new();
    outline(&reading.tree, 0, &mut lines);
    (lines, reading)
}

pub fn calls(reading: &FileReading) -> Vec<(String, Receiver)> {
    reading.calls.iter().map(|call| (call.name.clone(), call.receiver.clone())).collect()
}

pub fn targeted(reading: &FileReading) -> Vec<(String, Receiver, Target)> {
    reading.calls.iter().map(|call| (call.name.clone(), call.receiver.clone(), call.target)).collect()
}
