//! Purpose:
//! Lowers native Core object operations while retaining ordinary PHP method signatures.
//!
//! Called from:
//! - Function-body lowering after parameter and ownership setup.
//!
//! Key details:
//! - WeakReference methods expose no owning edge from the wrapper to its referent.
//! - Backend implementations are selected by typed runtime targets rather than PHP names.

use crate::ir::{Effects, Immediate, Op, RuntimeCallTarget};
use crate::types::PhpType;
use super::context::LoweringContext;

/// Replaces native Core method bodies with typed operations and the normal return cleanup.
pub(super) fn lower(ctx: &mut LoweringContext<'_, '_>, body: &[crate::parser::ast::Stmt]) -> bool {
    if matches!(ctx.owner_name(), "ArrayObject::__elephc_fetch_for_write" | "ArrayIterator::__elephc_fetch_for_write")
        && ctx.current_class.as_ref().and_then(|name| ctx.classes.get(name)).is_some_and(|info| info.is_internal)
    {
        // The ordinary AST keeps the selector's parameter, local and reachability contracts.
        super::stmt::lower_stmt(ctx, &body[0]);
        let receiver = ctx.load_local("this", None);
        let index = ctx.load_local("i", None);
        let property = ctx.intern_string("values");
        let array = ctx.emit_value(Op::PropGetForWrite, vec![receiver.value],
            Some(Immediate::Data(property)), PhpType::Array(Box::new(PhpType::Mixed)),
            Op::PropGetForWrite.default_effects(), None);
        ctx.builder.set_value_ownership(array.value, crate::ir::Ownership::Borrowed);
        let result = ctx.emit_owned_value(Op::ArrayGetForWrite, vec![array.value, index.value],
            None, PhpType::Mixed, Op::ArrayGetForWrite.default_effects(), None);
        super::stmt::terminate_return(ctx, Some(result.value));
        return true;
    }
    if matches!(ctx.owner_name(), "WeakMap::__elephc_key" | "WeakMap::__elephc_array") {
        let array = ctx.owner_name().ends_with("__elephc_array");
        crate::ir_lower::stmt::lower_stmt(ctx, &body[0]);
        let argument = ctx.load_local(if array { "value" } else { "object" }, None);
        let result_type = if array {
            ctx.classes["WeakMap"].static_methods["__elephc_array"].return_type.clone()
        } else { PhpType::Mixed };
        let op = Op::Acquire;
        let result = ctx.emit_owned_value(
            op, vec![argument.value], None, result_type.clone(),
            if op == Op::Acquire { op.default_effects() } else { Op::mixed_unbox_effects(&result_type) }, None,
        );
        super::stmt::terminate_return(ctx, Some(result.value));
        return true;
    }
    if matches!(ctx.owner_name(), "WeakReference::create" | "WeakMap::__elephc_id") {
        super::stmt::lower_stmt(ctx, &body[0]);
    }
    let (target, argument, result) = match ctx.owner_name() {
        "WeakMap::__elephc_id" => (RuntimeCallTarget::WeakObjectId, "object", PhpType::Int),
        "WeakReference::create" => (RuntimeCallTarget::WeakReferenceCreate, "object", PhpType::Object("WeakReference".into())),
        "WeakReference::get" => (RuntimeCallTarget::WeakReferenceGet, "this", PhpType::Mixed),
        _ => return false,
    };
    let argument = ctx.load_local(argument, None);
    let result = if result == PhpType::Int {
        ctx.emit_value(Op::RuntimeCall, vec![argument.value], Some(Immediate::RuntimeCall(target)),
            result, Effects::all(), None)
    } else {
        ctx.emit_owned_value(Op::RuntimeCall, vec![argument.value], Some(Immediate::RuntimeCall(target)),
            result, Effects::all(), None)
    };
    super::stmt::terminate_return(ctx, Some(result.value));
    true
}
