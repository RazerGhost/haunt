//! What the code readings are made of, as the window's `shared/types.ts` has them: the tree of functions of a file, the
//! calls between functions and the imports between files.

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum FunctionKind {
    Function,
    Arrow,
    Method,
    Class,
}

/// A function, or what holds functions (a class, an impl, a trait): the ones written inside it are its children.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FunctionNode {
    /// `path:line:column` of where it starts.
    pub id: String,
    pub name: String,
    pub kind: FunctionKind,
    pub line: u32,
    pub end_line: u32,
    pub exported: bool,
    #[serde(rename = "async")]
    pub is_async: bool,
    /// It has no name of its own (`(anonymous)`); it is kept only when a named function sits inside it.
    pub anonymous: bool,
    /// A test's callback in a test file (`it('…', () => …)`), named by the test's title.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub test: bool,
    pub children: Vec<FunctionNode>,
}

/// A call from one function to another, counted by call site.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct CallEdge {
    pub from: String,
    pub to: String,
    pub count: u32,
    /// Matched by name, without a type checker.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub guessed: bool,
}

/// An import of one file by another, both relative to the repository.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FileImport {
    pub to: String,
    pub type_only: bool,
}
