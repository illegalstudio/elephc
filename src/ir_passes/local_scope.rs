//! Purpose:
//! Identifies EIR calls that can inspect PHP locals without naming their slots.
//!
//! Called from:
//! - Scalar local promotion, immutable-load discovery, and dead-store elimination.
//!
//! Key details:
//! - Dynamic eval uses a profiled language-construct call, not an `eval_*` opcode.
//! - Its scope inventory can include locals declared after the call in lowering order.

use crate::ir::{Function, Immediate, Op};

/// Returns whether dynamic eval can read or write this function's PHP local frame.
pub(super) fn has_dynamic_eval(function: &Function) -> bool {
    function.instructions.iter().any(|inst| {
        inst.op == Op::LanguageConstructCall
            && matches!(inst.immediate, Some(Immediate::ProfiledData { .. }))
    })
}
