//! Purpose:
//! Groups required-class scans used to decide which class metadata must be emitted.
//! Combines static class references with dynamic instanceof detection.
//!
//! Called from:
//! - `crate::codegen::program_usage`
//!
//! Key details:
//! - Scans must recurse through new AST nodes so runtime class tables remain complete.

mod collect;
mod dynamic_instanceof;

pub(in crate::codegen) use collect::collect_required_class_names;
pub(in crate::codegen) use dynamic_instanceof::program_has_dynamic_instanceof;
