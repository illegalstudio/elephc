//! Purpose:
//! Invokes PHP error handlers with exception-safe suspension of the active registration.
//!
//! Called from:
//! - AOT trigger lowering and the eval error-handler bridge.
//!
//! Key details:
//! - A native cleanup activation restores suspended ownership during unwinding.
//! - A callback-installed replacement survives; otherwise the old handler is restored.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

const SAVED_FIELDS: [(usize, &str); 4] = [
    (32, "_php_error_handler_value"),
    (40, "_php_error_handler_callable"),
    (48, "_php_error_handler_context"),
    (56, "_php_error_handler_context_release"),
];

/// Emits a C-ABI descriptor invocation wrapper and its unwind cleanup callback.
pub(super) fn emit_error_handler_invoke(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    let stack = if arm { "sp" } else { "rsp" };
    let scratch = if arm { "x10" } else { "r10" };
    let result = if arm { "x0" } else { "rax" };
    let arg0 = if arm { "x0" } else { "rdi" };
    let arg1 = if arm { "x1" } else { "rsi" };
    emitter.blank();
    emitter.label_global("__rt_error_handler_invoke");
    emit_frame_enter(emitter, 96);
    abi::emit_store_to_address(emitter, arg1, stack, 64);
    for (offset, symbol) in SAVED_FIELDS {
        abi::emit_load_symbol_to_reg(emitter, scratch, symbol, 0);
        abi::emit_store_to_address(emitter, scratch, stack, offset);
        abi::emit_store_zero_to_symbol(emitter, symbol, 0);
    }
    abi::emit_load_symbol_to_reg(emitter, scratch, "_exc_call_frame_top", 0);
    abi::emit_store_to_address(emitter, scratch, stack, 0);
    abi::emit_symbol_address(emitter, scratch, "__rt_error_handler_restore");
    abi::emit_store_to_address(emitter, scratch, stack, 8);
    abi::emit_reg_move(emitter, scratch, stack);
    abi::emit_store_to_address(emitter, scratch, stack, 16);
    abi::emit_store_reg_to_symbol(emitter, scratch, "_exc_call_frame_top", 0);
    abi::emit_load_int_immediate(emitter, scratch, 0);
    abi::emit_store_to_address(emitter, scratch, stack, 24);
    abi::emit_load_from_address(emitter, arg0, stack, 40);
    abi::emit_load_from_address(emitter, arg1, stack, 64);
    abi::emit_load_from_address(
        emitter, scratch, arg0,
        crate::codegen_support::callable_descriptor::CALLABLE_DESC_INVOKER_OFFSET,
    );
    abi::emit_call_reg(emitter, scratch);
    abi::emit_store_to_address(emitter, result, stack, 72);
    abi::emit_reg_move(emitter, arg0, stack);
    abi::emit_call_label(emitter, "__rt_error_handler_restore");
    abi::emit_load_from_address(emitter, scratch, stack, 0);
    abi::emit_store_reg_to_symbol(emitter, scratch, "_exc_call_frame_top", 0);
    abi::emit_load_from_address(emitter, result, stack, 72);
    emit_frame_leave(emitter, 96);
    emit_restore(emitter);
}

/// Restores suspended owners or releases them when PHP installed a replacement handler.
fn emit_restore(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    let stack = if arm { "sp" } else { "rsp" };
    let scratch = if arm { "x10" } else { "r10" };
    let record = if arm { "x11" } else { "r11" };
    let result = if arm { "x0" } else { "rax" };
    let arg0 = if arm { "x0" } else { "rdi" };
    emitter.blank();
    emitter.label_global("__rt_error_handler_restore");
    emit_frame_enter(emitter, 32);
    abi::emit_store_to_address(emitter, arg0, stack, 0);
    abi::emit_load_symbol_to_reg(emitter, scratch, "_php_error_handler_callable", 0);
    if arm {
        emitter.instruction("cbnz x10, __rt_error_handler_discard");            // keep any replacement installed by the callback
    } else {
        emitter.instruction("test r10, r10");                                   // inspect the post-callback active registration
        emitter.instruction("jnz __rt_error_handler_discard");                  // keep any replacement installed by the callback
    }
    for (offset, symbol) in SAVED_FIELDS {
        abi::emit_load_from_address(emitter, record, stack, 0);
        abi::emit_load_from_address(emitter, scratch, record, offset);
        abi::emit_store_reg_to_symbol(emitter, scratch, symbol, 0);
    }
    emit_frame_leave(emitter, 32);

    emitter.label("__rt_error_handler_discard");
    for (offset, helper) in [(32, "__rt_decref_mixed"), (40, "__rt_callable_descriptor_release")] {
        abi::emit_load_from_address(emitter, record, stack, 0);
        abi::emit_load_from_address(emitter, result, record, offset);
        abi::emit_call_label(emitter, helper);
    }
    abi::emit_load_from_address(emitter, record, stack, 0);
    abi::emit_load_from_address(emitter, scratch, record, 56);
    if arm {
        emitter.instruction("cbz x10, __rt_error_handler_restore_done");        // native registrations have no eval context release callback
    } else {
        emitter.instruction("test r10, r10");                                   // inspect the optional eval context release callback
        emitter.instruction("jz __rt_error_handler_restore_done");              // native registrations have no eval context owner
    }
    abi::emit_load_from_address(emitter, arg0, record, 48);
    abi::emit_call_reg(emitter, scratch);
    emitter.label("__rt_error_handler_restore_done");
    emit_frame_leave(emitter, 32);
}

/// Creates aligned storage with standard frame linkage on every supported target.
fn emit_frame_enter(emitter: &mut Emitter, size: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("sub sp, sp, #{size}"));               // reserve aligned helper storage
            emitter.instruction(&format!("stp x29, x30, [sp, #{}]", size - 16)); // preserve caller frame linkage
            emitter.instruction(&format!("add x29, sp, #{}", size - 16));       // establish the helper frame pointer
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                    // preserve caller linkage and align nested calls
            emitter.instruction("mov rbp, rsp");                                // establish stable helper frame linkage
            emitter.instruction(&format!("sub rsp, {size}"));                   // reserve aligned helper storage
        }
    }
}

/// Releases helper storage and returns without changing the result register.
fn emit_frame_leave(emitter: &mut Emitter, size: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", size - 16)); // restore caller frame linkage
            emitter.instruction(&format!("add sp, sp, #{size}"));               // release aligned helper storage
        }
        Arch::X86_64 => {
            emitter.instruction("leave");                                       // restore the caller frame and stack pointer
        }
    }
    emitter.instruction("ret");                                                 // return the callback result or finish unwind cleanup
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{AppleVariant, Platform, Target};

    /// Every target publishes a cleanup activation and restores suspended owners.
    #[test]
    fn error_handler_invocation_has_unwind_cleanup_on_every_target() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new_apple(Arch::AArch64, AppleVariant::IOS),
            Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_error_handler_invoke(&mut emitter);
            let asm = emitter.output();
            for symbol in [
                "__rt_error_handler_invoke:", "__rt_error_handler_restore:",
                "_exc_call_frame_top", "__rt_callable_descriptor_release",
                "_php_error_handler_context_release",
            ] {
                assert!(asm.contains(symbol), "missing {symbol} for {target:?}");
            }
        }
    }
}
