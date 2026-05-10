//! Purpose:
//! Re-exports program scans that answer codegen-only reachability and variable-use questions.
//! Keeps required-class and variable analysis isolated from emission modules.
//!
//! Called from:
//! - `crate::codegen::generate()` and lowering helpers that need whole-program facts
//!
//! Key details:
//! - These scans must be side-effect free and follow AST recursion whenever new nodes are added.

mod required_classes;
mod variables;

pub(super) use required_classes::{collect_required_class_names, program_has_dynamic_instanceof};
pub(super) use variables::program_uses_variable;
