//! Purpose:
//! Lowers vtable lookup and class/interface slot calculations.
//! Shares receiver preparation and ABI call conventions with the object call dispatcher.
//!
//! Called from:
//! - `crate::codegen::expr::objects::dispatch`
//!
//! Key details:
//! - Receiver ownership, late/static binding, and vtable slot layout must match class metadata emission.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::emit::Emitter;
use crate::intrinsics::IntrinsicCall;
use crate::types::PhpType;

use super::super::super::{
    restore_concat_offset_after_nested_call, restore_concat_offset_after_owned_string_call,
    save_concat_offset_before_nested_call,
};
use super::intrinsic::emit_instance_intrinsic_with_loaded_args;
use super::prep::resolve_instance_method_dispatch;

pub(crate) fn emit_dispatch_instance_method(
    class_name: &str,
    method: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    if let Some(intrinsic) = IntrinsicCall::instance_method(class_name, method) {
        return emit_instance_intrinsic_with_loaded_args(intrinsic, &[], 0, emitter, ctx);
    }

    let (ret_ty, slot, direct_private_label) =
        resolve_instance_method_dispatch(ctx, class_name, method);

    save_concat_offset_before_nested_call(emitter, ctx);
    if let Some(slot) = slot {
        let class_id_reg = abi::temp_int_reg(emitter.target);
        let dispatch_reg = abi::symbol_scratch_reg(emitter);
        abi::emit_load_from_address(
            emitter,
            class_id_reg,
            abi::int_arg_reg_name(emitter.target, 0),
            0,
        ); // load the dynamic class id from the receiver object header
        abi::emit_symbol_address(emitter, dispatch_reg, "_class_vtable_ptrs");
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction(&format!("ldr {}, [{}, {}, lsl #3]", dispatch_reg, dispatch_reg, class_id_reg)); // load the class-specific instance-vtable pointer from the global table
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction(&format!("mov {}, QWORD PTR [{} + {} * 8]", dispatch_reg, dispatch_reg, class_id_reg)); // load the class-specific instance-vtable pointer from the global table
            }
        }
        abi::emit_load_from_address(emitter, dispatch_reg, dispatch_reg, slot * 8); // load the selected method entry from the class-specific instance vtable
        abi::emit_call_reg(emitter, dispatch_reg);                              // call the resolved virtual method implementation
    } else if let Some(label) = direct_private_label {
        abi::emit_call_label(emitter, &label);                                  // call lexically-resolved private method directly
    } else {
        emitter.comment(&format!(
            "WARNING: missing vtable slot for {}::{}",
            class_name, method
        ));
    }
    restore_concat_offset_after_user_call(emitter, ctx, &ret_ty);

    ret_ty
}

fn restore_concat_offset_after_user_call(emitter: &mut Emitter, ctx: &Context, ret_ty: &PhpType) {
    if ret_ty == &PhpType::Str {
        restore_concat_offset_after_owned_string_call(emitter, ctx);
    } else {
        restore_concat_offset_after_nested_call(emitter, ctx, ret_ty);
    }
}
