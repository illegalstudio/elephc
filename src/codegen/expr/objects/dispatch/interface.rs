//! Purpose:
//! Lowers interface method dispatch through vtable-compatible wrapper targets.
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

use super::super::super::{
    restore_concat_offset_after_nested_call, save_concat_offset_before_nested_call,
};

pub(crate) fn emit_dispatch_interface_method(
    interface_name: &str,
    method: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    let Some(interface_info) = ctx.interfaces.get(interface_name).cloned() else {
        emitter.comment(&format!(
            "WARNING: missing interface metadata for {}::{}",
            interface_name, method
        ));
        return PhpType::Int;
    };
    let ret_ty = interface_info
        .methods
        .get(method)
        .map(|sig| sig.return_type.clone())
        .unwrap_or(PhpType::Int);
    let Some(slot) = interface_info.method_slots.get(method).copied() else {
        emitter.comment(&format!(
            "WARNING: missing interface slot for {}::{}",
            interface_name, method
        ));
        return ret_ty;
    };

    let interface_id = interface_info.interface_id as i64;
    let scan_loop = ctx.next_label("interface_dispatch_scan");
    let found = ctx.next_label("interface_dispatch_found");
    let done = ctx.next_label("interface_dispatch_done");
    let missing = ctx.next_label("interface_dispatch_missing");

    save_concat_offset_before_nested_call(emitter, ctx);
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            emitter.instruction("ldr x10, [x0]");                               // load the receiver object's runtime class id without consuming x0
            abi::emit_symbol_address(emitter, "x11", "_class_interface_ptrs");
            emitter.instruction("ldr x11, [x11, x10, lsl #3]");                 // select the receiver class's emitted interface metadata block
            emitter.instruction("ldr x10, [x11]");                              // load the number of implemented interface entries to scan
            emitter.instruction("add x11, x11, #8");                            // advance to the first [interface_id, impl_table] pair
            abi::emit_load_int_immediate(emitter, "x13", interface_id);

            emitter.label(&scan_loop);
            emitter.instruction(&format!("cbz x10, {}", missing));              // stop scanning if no implemented interface matched the target id
            emitter.instruction("ldr x12, [x11]");                              // load the current implemented interface id
            emitter.instruction("cmp x12, x13");                                // compare the current interface id with the dispatch target
            emitter.instruction(&format!("b.eq {}", found));                    // use this implementation table when the interface id matches
            emitter.instruction("add x11, x11, #16");                           // advance to the next [interface_id, impl_table] pair
            emitter.instruction("sub x10, x10, #1");                            // consume one implemented interface metadata entry
            emitter.instruction(&format!("b {}", scan_loop));                   // continue scanning the receiver's implemented interfaces

            emitter.label(&found);
            emitter.instruction("ldr x11, [x11, #8]");                          // load the implementation table pointer for the matched interface
            if slot == 0 {
                emitter.instruction("ldr x11, [x11]");                          // load the first method implementation pointer from the interface table
            } else {
                emitter.instruction(&format!("ldr x11, [x11, #{}]", slot * 8)); // load the selected method implementation pointer from the interface table
            }
            emitter.instruction("blr x11");                                     // call the resolved interface method implementation
            emitter.instruction(&format!("b {}", done));                        // skip the defensive missing-interface fallback

            emitter.label(&missing);
            emitter.instruction("mov x0, #0");                                  // defensive fallback for invalid runtime metadata; valid programs never take this path
            emitter.label(&done);
        }
        crate::codegen::platform::Arch::X86_64 => {
            emitter.instruction("mov r10, QWORD PTR [rdi]");                    // load the receiver object's runtime class id without consuming rdi
            abi::emit_symbol_address(emitter, "r11", "_class_interface_ptrs");
            emitter.instruction("mov r11, QWORD PTR [r11 + r10 * 8]");          // select the receiver class's emitted interface metadata block
            emitter.instruction("mov r10, QWORD PTR [r11]");                    // load the number of implemented interface entries to scan
            emitter.instruction("add r11, 8");                                  // advance to the first [interface_id, impl_table] pair
            abi::emit_load_int_immediate(emitter, "r9", interface_id);

            emitter.label(&scan_loop);
            emitter.instruction("test r10, r10");                               // check whether any implemented interface entries remain
            emitter.instruction(&format!("je {}", missing));                    // stop scanning if no implemented interface matched the target id
            emitter.instruction("mov r8, QWORD PTR [r11]");                     // load the current implemented interface id
            emitter.instruction("cmp r8, r9");                                  // compare the current interface id with the dispatch target
            emitter.instruction(&format!("je {}", found));                      // use this implementation table when the interface id matches
            emitter.instruction("add r11, 16");                                 // advance to the next [interface_id, impl_table] pair
            emitter.instruction("sub r10, 1");                                  // consume one implemented interface metadata entry
            emitter.instruction(&format!("jmp {}", scan_loop));                 // continue scanning the receiver's implemented interfaces

            emitter.label(&found);
            emitter.instruction("mov r11, QWORD PTR [r11 + 8]");                // load the implementation table pointer for the matched interface
            if slot == 0 {
                emitter.instruction("mov r11, QWORD PTR [r11]");                // load the first method implementation pointer from the interface table
            } else {
                emitter.instruction(&format!("mov r11, QWORD PTR [r11 + {}]", slot * 8)); // load the selected method implementation pointer from the interface table
            }
            emitter.instruction("call r11");                                    // call the resolved interface method implementation
            emitter.instruction(&format!("jmp {}", done));                      // skip the defensive missing-interface fallback

            emitter.label(&missing);
            emitter.instruction("xor eax, eax");                                // defensive fallback for invalid runtime metadata; valid programs never take this path
            emitter.label(&done);
        }
    }
    restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);

    ret_ty
}
