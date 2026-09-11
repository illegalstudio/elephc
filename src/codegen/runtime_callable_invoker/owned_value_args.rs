//! Purpose:
//! Acquires stable string buffers and normalized callable descriptors for invoker arguments.
//!
//! Called from:
//! - The indexed, associative, and default argument builders in the parent module.
//!
//! Key details:
//! - Every resulting string has one owner recorded by `InvokerArgumentOwners`.
//! - Boxed strings bypass the allocating cast so persistence happens exactly once.
//! - Other scalar casts are persisted before later arguments can overwrite shared scratch.
//! - Callable conversions return an owned descriptor and root temporary boxing across TypeError.

use super::{abi, DataSection, Emitter, InvokerEmitContext, PhpType};

/// Coerces a borrowed argument and detaches any resulting string for invocation cleanup.
pub(super) fn coerce(
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
    source_ty: &PhpType,
    target_ty: Option<&PhpType>,
) -> (PhpType, bool) {
    if source_ty.codegen_repr() == PhpType::Mixed
        && target_ty.is_some_and(|ty| ty.codegen_repr() == PhpType::Str)
    {
        let string_label = ctx.next_label("owned_string_argument");
        let persist_label = ctx.next_label("persist_string_argument");
        abi::emit_owned_mixed_string(emitter, &string_label, &persist_label);
        return (PhpType::Str, false);
    }
    let coerced = super::coerce_current_value_to_target(emitter, ctx, data, source_ty, target_ty);
    if coerced.0 == PhpType::Str {
        abi::emit_call_label(emitter, "__rt_str_persist");
    }
    coerced
}

/// Converts supported PHP callback values to owned native descriptors without leaking source boxes.
pub(super) fn normalize_callable(
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    source_ty: &PhpType,
) {
    let boxes_source = source_ty.codegen_repr() != PhpType::Mixed;
    if boxes_source {
        super::emit_box_current_value_as_mixed(emitter, source_ty);
        ctx.argument_owners.record_coercion_source(emitter);
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    }
    abi::emit_call_label(emitter, crate::codegen::lower_inst::CALLABLE_ARGUMENT_NORMALIZER);
    if boxes_source {
        ctx.argument_owners.clear_coercion_source(emitter);
        super::release_preserved_mixed_after_arg_coercion(emitter, &PhpType::Callable);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Target;

    /// Every target passes an owned raw descriptor to the native callable parameter.
    #[test]
    fn invoker_callable_coercions_normalize_before_native_abi_materialization() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for source in [PhpType::Mixed, PhpType::Str, PhpType::Int, PhpType::php_array()] {
                let mut emitter = Emitter::new(Target::parse(target).unwrap());
                let owners = super::super::InvokerArgumentOwners::new(super::super::INVOKER_BOUNDARY_FRAME_SIZE, 1);
                let mut ctx = InvokerEmitContext::new(
                    "callable_argument",
                    owners,
                    false,
                );
                let result = coerce(&mut emitter, &mut ctx, &mut DataSection::new(), &source, Some(&PhpType::Callable));
                assert_eq!(result, (PhpType::Callable, true), "{target}: {source:?}");
                let asm = emitter.output();
                assert_eq!(asm.matches(crate::codegen::lower_inst::CALLABLE_ARGUMENT_NORMALIZER).count(), 1,
                    "{target}: {source:?}");
                assert_eq!(asm.contains("__rt_decref_mixed"), source.codegen_repr() != PhpType::Mixed,
                    "{target}: {source:?}");
            }
        }
    }
}
