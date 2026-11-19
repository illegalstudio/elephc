//! Purpose:
//! Emits generated C-ABI helpers that synchronize eval exception handlers with
//! the process-wide native PHP handler stack.
//!
//! Called from:
//! - `crate::codegen::finalize_user_asm()` when the eval bridge is active.
//!
//! Key details:
//! - Eval callbacks become runtime callable descriptors that retain their boxed value.
//! - The native stack remains authoritative, so outer AOT catches run before the
//!   terminal user exception handler.
//! - The getter wrapper hands eval a retained callback cell without exposing the descriptor.

use super::eval_callable_helpers::EvalCallableDescriptorSupport;
use crate::codegen::callable_descriptor;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::abi;
use crate::ir::Module;
use elephc_memoir::callables::storage::{self, CaptureSlot};

const EXCEPTION_HANDLER_NODE_BYTES: i64 = 40;

/// Emits eval-facing exception-handler install and restore wrappers.
pub(super) fn emit_eval_exception_handler_helpers(
    module: &Module,
    emitter: &mut Emitter,
    support: &EvalCallableDescriptorSupport,
) {
    let Some(descriptor_label) = support.dynamic_descriptor_label() else {
        return;
    };
    emitter.blank();
    emitter.comment("--- eval bridge: exception handler synchronization ---");
    match module.target.arch {
        Arch::AArch64 => {
            emit_set_exception_handler_aarch64(module, emitter, descriptor_label);
            emit_restore_exception_handler(module, emitter);
        }
        Arch::X86_64 => {
            emit_set_exception_handler_x86_64(module, emitter, descriptor_label);
            emit_restore_exception_handler(module, emitter);
        }
    }
    emit_get_exception_handler(module, emitter);
}

/// Emits the ARM64 C wrapper for installing an eval exception handler.
fn emit_set_exception_handler_aarch64(
    module: &Module,
    emitter: &mut Emitter,
    descriptor_label: &str,
) {
    let no_previous = "__elephc_eval_exception_handler_set_no_previous";
    let no_callback = "__elephc_eval_exception_handler_set_no_callback";
    let done = "__elephc_eval_exception_handler_set_done";
    label_c_global(module, emitter, "__elephc_eval_exception_handler_set");
    emitter.instruction("sub sp, sp, #96");                                     // reserve saved inputs, node state, and a standard frame
    emitter.instruction("stp x29, x30, [sp, #80]");                             // preserve the C caller frame
    emitter.instruction("add x29, sp, #80");                                    // establish a stable wrapper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the active eval context pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the nullable boxed callback value
    emitter.instruction("str x2, [sp, #16]");                                   // save writable previous-callback output storage
    emitter.instruction(&format!("mov x0, #{EXCEPTION_HANDLER_NODE_BYTES}"));   // allocate one native exception-handler stack node
    emitter.instruction("bl __rt_heap_alloc");                                  // create storage for the previous process-wide state
    emitter.instruction("str x0, [sp, #24]");                                   // preserve the node across symbol loads and calls
    let fields = [
        "_php_exception_handler_stack",
        "_php_exception_handler_value",
        "_php_exception_handler_callable",
        "_php_exception_handler_context",
        "_php_exception_handler_context_release",
    ];
    elephc_memoir::handler_owners::emit_save_node(emitter, "x0", "x9", &fields, true);
    abi::emit_load_symbol_to_reg(emitter, "x0", "_php_exception_handler_value", 0);
    emitter.instruction(&format!("cbz x0, {no_previous}"));                     // an empty prior registration returns a null raw pointer
    emitter.instruction("bl __rt_incref");                                      // give the PHP return value an independent owner
    emitter.label(no_previous);
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload previous-callback output storage
    emitter.instruction("str x0, [x9]");                                        // publish the retained prior callback or null

    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the nullable replacement callback
    emitter.instruction(&format!("cbz x0, {no_callback}"));                     // PHP null clears the active handler after stacking it
    emitter.instruction("bl __rt_incref");                                      // retain the PHP-visible callback in the native global
    abi::emit_store_reg_to_symbol(emitter, "x0", "_php_exception_handler_value", 0);
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the callback for descriptor capture ownership
    emitter.instruction("bl __rt_incref");                                      // retain the callback owned by the runtime descriptor capture
    emitter.instruction("str x0, [sp, #32]");                                   // keep the capture owner while allocating the descriptor
    storage::emit_callback_adapter(
        emitter,
        descriptor_label,
        "x9",
        CaptureSlot::Stack(0),
        CaptureSlot::Stack(32),
    );
    abi::emit_store_reg_to_symbol(emitter, "x0", "_php_exception_handler_callable", 0);
    emitter.instruction("ldr x0, [sp, #0]");                                    // retain the eval context for the persistent handler descriptor
    let retain_context = module
        .target
        .extern_symbol("__elephc_eval_context_retain");
    abi::emit_call_label(emitter, &retain_context);
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the retained eval context pointer
    abi::emit_store_reg_to_symbol(emitter, "x9", "_php_exception_handler_context", 0);
    let release_context = module.target.extern_symbol("__elephc_eval_context_free");
    abi::emit_extern_symbol_address(emitter, "x10", &release_context);
    abi::emit_store_reg_to_symbol(
        emitter,
        "x10",
        "_php_exception_handler_context_release",
        0,
    );
    emitter.instruction(&format!("b {done}"));                                  // leave the installed handler active

    emitter.label(no_callback);
    abi::emit_store_zero_to_symbol(emitter, "_php_exception_handler_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_php_exception_handler_callable", 0);
    abi::emit_store_zero_to_symbol(emitter, "_php_exception_handler_context", 0);
    abi::emit_store_zero_to_symbol(
        emitter,
        "_php_exception_handler_context_release",
        0,
    );
    emitter.label(done);
    emitter.instruction("mov w0, #0");                                          // report EvalStatus::Ok to magician
    emitter.instruction("ldp x29, x30, [sp, #80]");                             // restore the C caller frame
    emitter.instruction("add sp, sp, #96");                                     // release wrapper scratch storage
    emitter.instruction("ret");                                                 // return to the Rust runtime adapter
}

/// Emits the x86_64 C wrapper for installing an eval exception handler.
fn emit_set_exception_handler_x86_64(
    module: &Module,
    emitter: &mut Emitter,
    descriptor_label: &str,
) {
    let no_previous = "__elephc_eval_exception_handler_set_no_previous_x";
    let no_callback = "__elephc_eval_exception_handler_set_no_callback_x";
    let done = "__elephc_eval_exception_handler_set_done_x";
    label_c_global(module, emitter, "__elephc_eval_exception_handler_set");
    emitter.instruction("push rbp");                                            // preserve the C caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 64");                                         // reserve saved inputs and handler-node scratch storage
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the active eval context pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the nullable boxed callback value
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save writable previous-callback output storage
    emitter.instruction(&format!("mov rax, {EXCEPTION_HANDLER_NODE_BYTES}"));   // allocate one native exception-handler stack node
    emitter.instruction("call __rt_heap_alloc");                                // create storage for the previous process-wide state
    emitter.instruction("mov QWORD PTR [rbp - 32], rax");                       // preserve the node across symbol loads and calls
    let fields = [
        "_php_exception_handler_stack",
        "_php_exception_handler_value",
        "_php_exception_handler_callable",
        "_php_exception_handler_context",
        "_php_exception_handler_context_release",
    ];
    elephc_memoir::handler_owners::emit_save_node(emitter, "rax", "r10", &fields, true);
    abi::emit_load_symbol_to_reg(emitter, "rax", "_php_exception_handler_value", 0);
    emitter.instruction("test rax, rax");                                       // check for an existing callback value
    emitter.instruction(&format!("jz {no_previous}"));                          // an empty prior registration returns a null raw pointer
    emitter.instruction("call __rt_incref");                                    // give the PHP return value an independent owner
    emitter.label(no_previous);
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload previous-callback output storage
    emitter.instruction("mov QWORD PTR [r10], rax");                            // publish the retained prior callback or null

    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the nullable replacement callback
    emitter.instruction("test rax, rax");                                       // distinguish callback installation from PHP null
    emitter.instruction(&format!("jz {no_callback}"));                          // PHP null clears the active handler after stacking it
    emitter.instruction("call __rt_incref");                                    // retain the PHP-visible callback in the native global
    abi::emit_store_reg_to_symbol(emitter, "rax", "_php_exception_handler_value", 0);
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the callback for descriptor capture ownership
    emitter.instruction("call __rt_incref");                                    // retain the callback owned by the runtime descriptor capture
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // keep the capture owner while allocating the descriptor
    storage::emit_callback_adapter(
        emitter,
        descriptor_label,
        "r10",
        CaptureSlot::Frame(8),
        CaptureSlot::Frame(40),
    );
    abi::emit_store_reg_to_symbol(emitter, "rax", "_php_exception_handler_callable", 0);
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // retain the eval context for the persistent handler descriptor
    let retain_context = module
        .target
        .extern_symbol("__elephc_eval_context_retain");
    abi::emit_call_label(emitter, &retain_context);
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the retained eval context pointer
    abi::emit_store_reg_to_symbol(emitter, "r10", "_php_exception_handler_context", 0);
    let release_context = module.target.extern_symbol("__elephc_eval_context_free");
    abi::emit_extern_symbol_address(emitter, "r10", &release_context);
    abi::emit_store_reg_to_symbol(
        emitter,
        "r10",
        "_php_exception_handler_context_release",
        0,
    );
    emitter.instruction(&format!("jmp {done}"));                                // leave the installed handler active

    emitter.label(no_callback);
    abi::emit_store_zero_to_symbol(emitter, "_php_exception_handler_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_php_exception_handler_callable", 0);
    abi::emit_store_zero_to_symbol(emitter, "_php_exception_handler_context", 0);
    abi::emit_store_zero_to_symbol(
        emitter,
        "_php_exception_handler_context_release",
        0,
    );
    emitter.label(done);
    emitter.instruction("xor eax, eax");                                        // report EvalStatus::Ok to magician
    emitter.instruction("mov rsp, rbp");                                        // release wrapper scratch storage
    emitter.instruction("pop rbp");                                             // restore the C caller frame pointer
    emitter.instruction("ret");                                                 // return to the Rust runtime adapter
}

/// Emits the target-aware C wrapper for restoring the prior native exception handler.
fn emit_restore_exception_handler(module: &Module, emitter: &mut Emitter) {
    label_c_global(module, emitter, "__elephc_eval_exception_handler_restore");
    abi::emit_frame_prologue(emitter, 16);
    abi::emit_call_label(emitter, "__rt_core_exception_handler_pop");
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
    abi::emit_frame_restore(emitter, 16);
    abi::emit_return(emitter);
}


/// Emits the target-aware C wrapper that reads the active native exception handler for eval.
///
/// Returns the retained PHP-visible callback cell, or a null pointer when no handler is
/// active. The normalized descriptor and its context owner are never exposed.
fn emit_get_exception_handler(module: &Module, emitter: &mut Emitter) {
    let done = "__elephc_eval_exception_handler_get_done";
    label_c_global(module, emitter, "__elephc_eval_exception_handler_get");
    abi::emit_frame_prologue(emitter, 16);
    abi::emit_load_symbol_to_reg(
        emitter,
        abi::int_result_reg(emitter),
        "_php_exception_handler_value",
        0,
    );
    abi::emit_branch_if_int_result_zero(emitter, done);
    callable_descriptor::emit_retain_current_descriptor(emitter);
    emitter.label(done);
    abi::emit_frame_restore(emitter, 16);
    abi::emit_return(emitter);
}

/// Emits a platform-C global label for one generated eval wrapper.
fn label_c_global(module: &Module, emitter: &mut Emitter, name: &str) {
    emitter.label_global(&module.target.extern_symbol(name));
}
