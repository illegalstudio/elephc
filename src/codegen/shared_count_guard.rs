//! Purpose:
//! Emits the shared `count()` countable check, so the tag ladder that raises PHP's
//! `TypeError` exists once per program instead of once per `count()` on a boxed value.
//!
//! Called from:
//! - `crate::codegen::block_emit::emit_module()` before the module's own functions.
//! - `crate::codegen::lower_inst::builtins::count_empty` at every Mixed/Union `count()`.
//!
//! Key details:
//! - The guard is seven tag comparisons and seven distinct raises. Inlined, it measured 292
//!   lines of assembly PER SITE — 5 sites of `count($mixed)` went from 3 998 to 5 460 lines,
//!   which is why the first version of this fix was reverted rather than kept.
//! - The check is MOVED, not replaced: the same comparisons in the same order, so what can go
//!   wrong is structural — a bad register, a missing symbol — and not a differently classified
//!   value.
//! - The helper returns normally when the value is countable and never returns otherwise, so
//!   the call site keeps the shape it had: guard, then `__rt_mixed_count`.

use crate::codegen::context::FunctionContext;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::shared_state::SharedCodegenState;
use crate::ir::{Function, Immediate, Module, RuntimeCallTarget, RuntimeFnId};
use crate::types::PhpType;

use super::lower_inst::emit_count_countable_guard_from_result;
use super::shared_helper::emit_shared_helper;
use super::Result;

/// Label of the helper that raises unless a boxed `Mixed` is countable.
pub(super) const COUNT_GUARD_LABEL: &str = "_eir_shared_count_guard";

/// How many boxed `count()` sites a module needs before the guard is worth a helper body.
///
/// Measured on a probe of five `count($mixed)`, in lines of emitted assembly: no guard at all
/// 4 102, guard inlined 5 563 (+1 461, i.e. 292 per site), guard shared 4 457 (+355). Sharing
/// removes three quarters of what the guard costs.
///
/// The second direction is why this is not 1. On a ONE-site probe: no guard 3 854, inlined
/// 4 191, shared 4 201 — the single site pays for a whole body to remove one copy of itself
/// and comes out 10 lines BEHIND. The `__toString` ladder has the same threshold for the same
/// measured reason, and there the wrong answer made `fizzbuzz` grow outright.
const MIN_SITES_TO_SHARE: usize = 2;

/// Returns whether this module routes its `count()` guards through the shared helper.
///
/// The emitter and the call sites both ask THIS function, so they cannot disagree about
/// whether the helper exists — a disagreement would either leave an unresolved label or emit
/// a body nothing calls.
pub(super) fn module_shares_count_guard(
    module: &Module,
    shared: &mut SharedCodegenState,
) -> bool {
    if let Some(cached) = shared.count_guard_sharing() {
        return cached;
    }
    let shares = boxed_count_site_count(module) >= MIN_SITES_TO_SHARE;
    shared.set_count_guard_sharing(shares);
    shares
}

/// Counts the `count()` calls on a boxed receiver across every body this module emits.
fn boxed_count_site_count(module: &Module) -> usize {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .map(|function| {
            function
                .instructions
                .iter()
                .filter(|inst| instruction_is_boxed_count_site(function, inst))
                .count()
        })
        .sum()
}

/// Returns whether one instruction is a `count()` whose argument is a boxed value.
fn instruction_is_boxed_count_site(function: &Function, inst: &crate::ir::Instruction) -> bool {
    let is_count = matches!(
        inst.immediate,
        Some(Immediate::RuntimeCall(
            RuntimeCallTarget::Function(RuntimeFnId::Count)
                | RuntimeCallTarget::ProfiledFunction {
                    target: RuntimeFnId::Count,
                    ..
                }
        ))
    );
    if !is_count {
        return false;
    }
    inst.operands.first().is_some_and(|operand| {
        function.value(*operand).is_some_and(|value| {
            matches!(
                value.php_type.codegen_repr(),
                PhpType::Mixed | PhpType::Union(_)
            )
        })
    })
}

/// Returns the helper label a `count()` guard in `ctx` should call, if any.
///
/// `None` inside the helper itself, which is what stops the body from calling the label it
/// is defining.
pub(in crate::codegen) fn shared_guard_label(ctx: &mut FunctionContext<'_>) -> Option<&'static str> {
    if ctx.function.name == COUNT_GUARD_LABEL {
        return None;
    }
    let module = ctx.module;
    if !module_shares_count_guard(module, ctx.shared) {
        return None;
    }
    Some(COUNT_GUARD_LABEL)
}

/// Emits the shared guard when the module uses it.
pub(super) fn emit_shared_count_guard(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
    regalloc_linear: bool,
) -> Result<()> {
    if !module_shares_count_guard(module, shared) {
        return Ok(());
    }
    emit_shared_helper(
        module,
        emitter,
        data,
        shared,
        regalloc_linear,
        COUNT_GUARD_LABEL,
        PhpType::Void,
        &format!("--- shared count() countable guard: {} ---", COUNT_GUARD_LABEL),
        // The boxed pointer already sits in the int result register, which is where the
        // inlined sequence expected it.
        emit_count_countable_guard_from_result,
    )
}
