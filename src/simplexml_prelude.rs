//! Purpose:
//! Constructs the pay-for-use SimpleXML prelude on the existing XML bridge.
//!
//! Called from:
//! - XML injection and shared prelude parity audits.
//!
//! Key details:
//! - Parsing is cached; declarations compile through the ordinary EIR pipeline.

mod surface;
use crate::parser::ast::Program;
use std::sync::OnceLock;

/// Returns the cached SimpleXML externs, classes, and three PHP functions.
pub(crate) fn declarations() -> Program {
    static DECLARATIONS: OnceLock<Program> = OnceLock::new();
    DECLARATIONS.get_or_init(|| {
        let tokens = crate::lexer::tokenize(surface::SOURCE).expect("SimpleXML prelude tokenizes");
        crate::parser::parse_internal(&tokens).expect("SimpleXML prelude parses")
    }).clone()
}
