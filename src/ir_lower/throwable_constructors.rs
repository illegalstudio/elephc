//! Purpose:
//! Lowers the shared Error and Exception constructor bodies to a layout-aware EIR operation.
//!
//! Called from:
//! - Function-body lowering after normal parameter ownership and local setup.
//!
//! Key details:
//! - Inherited constructors must work on both compact builtins and ordinary subclass objects.
//! - The native operation borrows parameters; ordinary function cleanup owns their shadows.

use crate::ir::{Effects, Immediate, Op, RuntimeCallTarget};
use super::context::LoweringContext;

/// Replaces only the two checker-provided constructor bodies, leaving user overrides untouched.
pub(super) fn lower(ctx: &mut LoweringContext<'_, '_>) -> bool {
    let Some((class, method)) = ctx.owner_name().rsplit_once("::") else {
        return false;
    };
    if !matches!(class, "Error" | "Exception") || !method.eq_ignore_ascii_case("__construct") {
        return false;
    }
    let operands = ["this", "message", "code", "previous"].into_iter()
        .map(|name| ctx.load_local(name, None).value).collect();
    ctx.emit_void(
        Op::RuntimeCall,
        operands,
        Some(Immediate::RuntimeCall(RuntimeCallTarget::ThrowableInitialize)),
        Effects::all(),
        None,
    );
    true
}
