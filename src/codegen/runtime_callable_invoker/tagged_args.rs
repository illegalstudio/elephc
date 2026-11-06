//! Purpose:
//! Coerces nullable integer callable arguments into the two-word tagged scalar ABI.
//!
//! Called from:
//! - Descriptor invoker argument coercion in the parent module.
//!
//! Key details:
//! - Null remains distinct from integer zero before captures and later ABI arguments are placed.
//! - Borrowed Mixed sources remain borrowed; temporary boxes are guarded and released once.

use super::*;
use crate::codegen_support::sentinels::{emit_tagged_scalar_from_int_result, emit_tagged_scalar_null};

/// Places the nullable integer payload and tag in the active target's result registers.
pub(super) fn coerce(emitter: &mut Emitter, ctx: &mut InvokerEmitContext, source: &PhpType) {
    let repr = source.codegen_repr();
    match repr {
        PhpType::TaggedScalar => return,
        PhpType::Int | PhpType::Bool => {
            emit_tagged_scalar_from_int_result(emitter);
            return;
        }
        PhpType::Void | PhpType::Never => {
            emit_tagged_scalar_null(emitter);
            return;
        }
        PhpType::Float => {
            abi::emit_float_result_to_int_result(emitter);
            emit_tagged_scalar_from_int_result(emitter);
            return;
        }
        _ => {}
    }
    let boxed = repr != PhpType::Mixed;
    if boxed {
        emit_box_current_value_as_mixed(emitter, source);
        ctx.argument_owners.record_coercion_source(emitter);
    }
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    let null = ctx.next_label("nullable_argument_null");
    let done = ctx.next_label("nullable_argument_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #8");                                  // distinguish a null box before the integer cast erases its tag
            emitter.instruction(&format!("b.eq {null}"));                       // materialize the canonical nullable integer null representation
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 8");                                  // distinguish a null box before the integer cast erases its tag
            emitter.instruction(&format!("je {null}"));                         // materialize the canonical nullable integer null representation
        }
    }
    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 0);
    abi::emit_call_label(emitter, "__rt_mixed_cast_int");
    emit_tagged_scalar_from_int_result(emitter);
    abi::emit_jump(emitter, &done);
    emitter.label(&null);
    emit_tagged_scalar_null(emitter);
    emitter.label(&done);
    if boxed {
        ctx.argument_owners.clear_coercion_source(emitter);
        super::release_preserved_mixed_after_arg_coercion(emitter, &PhpType::TaggedScalar);
    } else {
        abi::emit_release_temporary_stack(emitter, 16);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every supported target stages two ABI words for boxed, raw, and already-tagged nullable arguments.
    #[test]
    fn nullable_invoker_arguments_preserve_physical_width_on_every_target() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for source in [PhpType::Mixed, PhpType::Int, PhpType::Void, PhpType::Float, PhpType::Str, PhpType::TaggedScalar] {
                let mut emitter = Emitter::new(Target::parse(target).unwrap());
                let owners = InvokerArgumentOwners::new(INVOKER_BOUNDARY_FRAME_SIZE, 1);
                let mut ctx = InvokerEmitContext::new("nullable_argument", owners, false, false, Vec::new(), emitter.target);
                let target_type = PhpType::Union(vec![PhpType::Int, PhpType::Void]);
                assert!(super::super::can_coerce_result_to_type(&source, &target_type), "{target}: {source:?}");
                let result = super::super::coerce_current_value_to_target(
                    &mut emitter, &mut ctx, &mut DataSection::new(), &source, Some(&target_type),
                );
                assert_eq!(result, (PhpType::TaggedScalar, false), "{target}: {source:?}");
                assert_eq!(result.0.stack_size(), 16, "{target}: {source:?}");
            }
        }
    }

    /// Reference-marker boxes acquire conversion cleanup before nullable arguments are pushed.
    #[test]
    fn nullable_owned_marker_boxes_are_retired_on_every_target() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            let owners = InvokerArgumentOwners::new(INVOKER_BOUNDARY_FRAME_SIZE, 1);
            let mut ctx = InvokerEmitContext::new("nullable_owned_marker", owners, false, false, Vec::new(), emitter.target);
            let target_type = PhpType::Union(vec![PhpType::Int, PhpType::Void]);
            let result = super::super::push_materialized_mixed_hash_value_arg(
                Some(&target_type), true, &mut emitter, &mut ctx, &mut DataSection::new(),
            );
            assert_eq!(result, PhpType::TaggedScalar, "{target}");
            let arch = emitter.target.arch;
            let assembly = emitter.output();
            assert_eq!(assembly.matches("__rt_decref_mixed").count(), 1, "{target}");
            let restore = assembly.split_once("__rt_decref_mixed").unwrap().1;
            if arch == Arch::AArch64 {
                assert!(restore.contains("ldp x0, x1, [sp], #16"), "{target}: {restore}");
            } else {
                assert!(restore.contains("mov rax, QWORD PTR [rsp]"), "{target}: {restore}");
                assert!(restore.contains("mov rdx, QWORD PTR [rsp + 8]"), "{target}: {restore}");
            }
        }
    }
}
