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
use crate::types::PhpType;

use super::prep::resolve_instance_method_dispatch;
use super::super::super::{
    restore_concat_offset_after_nested_call, save_concat_offset_before_nested_call,
};

pub(crate) fn emit_dispatch_instance_method(
    class_name: &str,
    method: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    let (ret_ty, slot, direct_private_label) =
        resolve_instance_method_dispatch(ctx, class_name, method);

    // Generator's Iterator surface and send/throw/getReturn are intrinsics —
    // call directly into the runtime helpers instead of going through the
    // user-PHP stub bodies that the type system synthesised for the class.
    if class_name == "Generator" {
        if let Some(rt_label) = generator_runtime_label_for(method) {
            save_concat_offset_before_nested_call(emitter, ctx);
            abi::emit_call_label(emitter, rt_label);                            // direct call into the generator runtime helper
            restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
            return ret_ty;
        }
    }

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
    restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);

    ret_ty
}

/// Map a Generator-class method name to the corresponding runtime helper
/// symbol. The name resolver lowercases all method tokens before they
/// reach codegen (PHP class methods are case-insensitive), so the match
/// arms below are the lowercase forms of `current`, `getReturn`, etc.
/// Returns `None` for methods elephc doesn't intercept (so dispatch falls
/// back to the regular vtable path).
pub(super) fn generator_runtime_label_for(method: &str) -> Option<&'static str> {
    match method {
        "current" => Some("__rt_gen_current"),
        "key" => Some("__rt_gen_key"),
        "next" => Some("__rt_gen_next"),
        "valid" => Some("__rt_gen_valid"),
        "rewind" => Some("__rt_gen_rewind"),
        "send" => Some("__rt_gen_send"),
        "throw" => Some("__rt_gen_throw"),
        "getreturn" => Some("__rt_gen_get_return"),
        _ => None,
    }
}
