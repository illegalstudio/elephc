//! Purpose:
//! Owns temporary reference arguments created by descriptor invokers.
//!
//! Called from:
//! - Regular, default and variadic reference argument staging in the parent module.
//!
//! Key details:
//! - Managed cells retain their mutable payload and may outlive the invocation through captures.
//! - The invoker retires its cell lease on both normal return and exception escape.

use super::{abi, DataSection, Emitter, InvokerEmitContext, PhpType};

/// Roots a newly boxed hash value through coercion, then retires it after the cell owns its value.
pub(super) fn push_owned_boxed_value(
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
    owner_index: usize,
    target_ty: Option<&PhpType>,
) {
    let result = abi::int_result_reg(emitter);
    abi::emit_push_reg(emitter, result);
    ctx.argument_owners.record_pushed_reference(owner_index, emitter);
    super::push_current_result_ref_arg_address(
        &PhpType::Mixed, target_ty, owner_index, emitter, ctx, data,
    );
    // The managed cell has replaced the temporary box in the cleanup ledger.
    abi::emit_load_temporary_stack_slot(emitter, result, 16);
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    abi::emit_pop_reg(emitter, result);
    abi::emit_release_temporary_stack(emitter, 16);
    abi::emit_push_reg(emitter, result);
}

/// Transfers a pushed value into a managed reference cell and records the invocation's cell lease.
pub(super) fn push_owned_cell(
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    owner_index: usize,
    ty: &PhpType,
) {
    abi::emit_load_int_immediate(
        emitter, abi::int_arg_reg_name(emitter.target, 0),
        crate::codegen_support::runtime::reference_cells::payload_tag(ty),
    );
    abi::emit_call_label(emitter, "__rt_reference_cell_new");
    let cell = abi::symbol_scratch_reg(emitter);
    abi::emit_reg_move(emitter, cell, abi::int_result_reg(emitter));
    super::store_pushed_value_to_ref_cell(emitter, cell, ty);
    abi::emit_push_reg(emitter, cell);
    ctx.argument_owners.record_pushed_reference(owner_index, emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Target;

    /// Every ABI stamps the stored type and records a managed cell instead of leaking raw storage.
    #[test]
    fn temporary_reference_cells_register_typed_cleanup_on_every_target() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for ty in [PhpType::Int, PhpType::Str, PhpType::php_array(), PhpType::Object("Value".into())] {
                let mut emitter = Emitter::new(Target::parse(target).unwrap());
                let owners = super::super::InvokerArgumentOwners::new(super::super::INVOKER_BOUNDARY_FRAME_SIZE, 1);
                let mut ctx = InvokerEmitContext::new(
                    "ref_argument",
                    owners,
                    false,
                    Vec::new(),
                );
                push_owned_cell(&mut emitter, &mut ctx, 0, &ty);
                let asm = emitter.output();
                assert_eq!(asm.matches("__rt_reference_cell_new").count(), 1, "{target}: {ty:?}");
                assert!(!asm.contains("__rt_heap_alloc"), "{target}: {ty:?}");
                let mut recorded = Emitter::new(Target::parse(target).unwrap());
                ctx.argument_owners.record_pushed_reference(0, &mut recorded);
                assert!(asm.ends_with(&recorded.output()), "{target}: {ty:?}");
            }
        }
    }

    /// Hash argument boxing keeps one temporary owner until a managed reference cell replaces it.
    #[test]
    fn boxed_hash_reference_sources_retire_after_cell_publication() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            let owners = super::super::InvokerArgumentOwners::new(super::super::INVOKER_BOUNDARY_FRAME_SIZE, 1);
            let mut ctx = InvokerEmitContext::new(
                "ref_hash_argument",
                owners,
                false,
                Vec::new(),
            );
            push_owned_boxed_value(&mut emitter, &mut ctx, &mut DataSection::new(), 0, Some(&PhpType::Mixed));
            let asm = emitter.output();
            assert!(asm.find("__rt_reference_cell_new").unwrap() < asm.find("__rt_decref_mixed").unwrap(), "{target}");
            let mut recorded = Emitter::new(Target::parse(target).unwrap());
            ctx.argument_owners.record_pushed_reference(0, &mut recorded);
            assert_eq!(asm.matches(&recorded.output()).count(), 2, "{target}");
        }
    }
}
