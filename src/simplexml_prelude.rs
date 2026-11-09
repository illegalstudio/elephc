//! Purpose:
//! Constructs the pay-for-use SimpleXML prelude on the existing XML bridge.
//!
//! Called from:
//! - XML injection and shared prelude parity audits.
//!
//! Key details:
//! - Declarations are built directly as AST nodes (no PHP source text is parsed here) and
//!   compile through the ordinary EIR pipeline.

mod generated;
use crate::parser::ast::Program;
use std::sync::OnceLock;

/// Returns the cached SimpleXML externs, classes, and PHP functions as AST declarations.
pub(crate) fn declarations() -> Program {
    static DECLARATIONS: OnceLock<Program> = OnceLock::new();
    DECLARATIONS.get_or_init(generated::declarations).clone()
}
