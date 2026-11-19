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
use crate::types::PhpType;

/// Returns whether dynamic eval can read or write this function's PHP local frame.
///
/// A profiled language construct is only dynamic eval when it produces the `mixed` result
/// eval does; `exit()`/`die()` also carry a source profile now, so that a boxed status can
/// enforce the call site's strict signature at run time, but they always yield void and never
/// inspect the local frame.
pub(super) fn has_dynamic_eval(function: &Function) -> bool {
    function.instructions.iter().any(|inst| {
        inst.op == Op::LanguageConstructCall
            && inst.result_php_type != PhpType::Void
            && matches!(inst.immediate, Some(Immediate::ProfiledData { .. }))
    })
}
