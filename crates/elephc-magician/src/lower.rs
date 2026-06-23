//! Purpose:
//! Placeholder for lowering parsed eval fragments into EvalIR.
//! Keeps parse, lowering, and interpretation responsibilities split from the
//! start of the eval bridge crate.
//!
//! Called from:
//! - Future `crate::interpreter` execution flow.
//!
//! Key details:
//! - EvalIR will use by-name scope operations rather than static local slots.

/// Validates that the lowering stub is intentionally unavailable.
pub fn lowering_is_stubbed() -> bool {
    true
}
