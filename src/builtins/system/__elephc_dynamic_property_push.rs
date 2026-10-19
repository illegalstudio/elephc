//! Purpose:
//! Declares the internal runtime-named property append the parser emits for
//! `$obj->{$name}[] = $value`.
//!
//! Called from:
//! - `crate::parser::stmt::assign::postfix`, which desugars the append into
//!   `__elephc_dynamic_property_push($obj, $name, $value)`.
//!
//! Key details:
//! - The check hook refines every array slot of the receiver's class as a statically named push
//!   would (`refine_array_slots_for_dynamic_push`), because the lowering pushes onto the slot the
//!   name selects with that slot's own static type. Other slots, and classes without such a push,
//!   keep their types.
//! - Direct calls are expanded in `crate::ir_lower::expr::dynamic_property_push`; the registry
//!   lowering hook below only guards paths that bypass that rewrite.
//! - `internal: true` keeps this compiler primitive out of PHP-visible builtin catalogs.

use crate::builtins::semantics::{
    internal_eir_semantics, BuiltinLoweringContext, BuiltinLoweringError,
    BuiltinResultOwnership, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::ir::Effects;
use crate::types::PhpType;

builtin! {
    contract: "__elephc_dynamic_property_push",
    check: check_dynamic_property_push,
    semantics: internal_eir_semantics(
        lower,
        Effects::from_bits_retain(
            Effects::READS_GLOBAL.bits()
                | Effects::READS_HEAP.bits()
                | Effects::WRITES_HEAP.bits()
                | Effects::ALLOC_HEAP.bits()
                | Effects::REFCOUNT_OP.bits()
                | Effects::MAY_THROW.bits(),
        ),
        BuiltinResultOwnership::Fresh,
    ),
}

/// Infers the operands and refines the array slots the runtime name may select.
fn check_dynamic_property_push(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let object_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    cx.checker.infer_type(&cx.args[1], cx.env)?;
    let value_ty = cx.checker.infer_type(&cx.args[2], cx.env)?;
    let class_name = match &object_ty {
        PhpType::Object(class_name) => Some(class_name.clone()),
        _ => cx.checker.union_single_object_class(&object_ty),
    };
    if let Some(class_name) = class_name {
        crate::types::checker::stmt_check::assignments::refine_array_slots_for_dynamic_push(
            cx.checker,
            &class_name,
            &value_ty,
            cx.span,
        );
    }
    Ok(PhpType::Void)
}

/// Rejects a runtime-named property append that reached registry lowering.
fn lower(
    _ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    Err(BuiltinLoweringError::new(format!(
        "{}() must be expanded by the dynamic property push lowering",
        call.name,
    )))
}
