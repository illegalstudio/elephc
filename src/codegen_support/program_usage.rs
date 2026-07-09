//! Purpose:
//! Re-exports program scans that answer required-class reachability questions.
//! Keeps class metadata filtering isolated from runtime feature selection.
//!
//! Called from:
//! - `crate::codegen_support::runtime_features`.
//!
//! Key details:
//! - These scans must be side-effect free and follow AST recursion whenever new nodes are added.

mod required_classes;

pub(super) use required_classes::{
    collect_required_class_names, collect_required_class_names_in_stmts,
    program_has_dynamic_instanceof,
};
