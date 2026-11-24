//! Purpose:
//! Normalizes the lowered operand count of `preg_match()` / `preg_match_all()` calls to
//! the form the source selected, so the backend chooses its helper from the operand
//! count alone.
//!
//! Called from:
//! - `crate::ir_lower::expr::array_builtin_args::lower_builtin_call_args()`.
//!
//! Key details:
//! - `uses_extended_capture()` is the single source-level predicate: when `$flags` or
//!   `$offset` is passed, `$matches` is `array<mixed>` and the call carries all five
//!   operands; otherwise it is `array<string>` and the call carries at most three.
//! - Named-argument planning may materialize the `$flags`/`$offset` defaults for a call
//!   that never passed them; those trailing constants are dropped here. A spread that
//!   selected the extended form but supplied fewer operands is padded with PHP's `0`
//!   defaults.
//! - A `preg_match_all()` that never passed `$offset` drops a materialized default so
//!   count-only calls keep the count helper.

use super::*;

/// Returns `preg_match()` / `preg_match_all()` operands trimmed or padded to the source form.
pub(super) fn normalize_operands(
    ctx: &mut LoweringContext<'_, '_>,
    canonical: &str,
    args: &[Expr],
    mut operands: Vec<crate::ir::ValueId>,
) -> Vec<crate::ir::ValueId> {
    if canonical == "preg_match_all" {
        if !crate::builtins::preg_passes_parameter_from(args, &["offset"], 4) {
            operands.truncate(4);
        }
        return operands;
    }
    if canonical != "preg_match" || operands.len() < 2 {
        return operands;
    }
    if !crate::builtins::preg_match_uses_extended_capture(args) {
        operands.truncate(3);
        return operands;
    }
    let span = args.first().map_or_else(Span::dummy, |arg| arg.span);
    if operands.len() == 2 {
        let matches = ctx
            .builder
            .emit_with_effects(
                Op::ConstNull,
                Vec::new(),
                None,
                IrType::I64,
                PhpType::Void,
                Ownership::NonHeap,
                Op::ConstNull.default_effects(),
                Some(span),
            )
            .expect("const_null produces a value");
        operands.push(matches);
    }
    while operands.len() < 5 {
        let zero = ctx
            .builder
            .emit_with_effects(
                Op::ConstI64,
                Vec::new(),
                Some(Immediate::I64(0)),
                IrType::I64,
                PhpType::Int,
                Ownership::NonHeap,
                Op::ConstI64.default_effects(),
                Some(span),
            )
            .expect("const_i64 produces a value");
        operands.push(zero);
    }
    operands
}
