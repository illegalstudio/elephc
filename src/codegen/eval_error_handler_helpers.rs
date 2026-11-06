//! Purpose:
//! Emits generated C-ABI helpers that synchronize eval error state with the
//! process-wide native PHP reporting mask and user-error-handler stack.
//!
//! Called from:
//! - `crate::codegen::finalize_user_asm()` when the eval bridge is active.
//!
//! Key details:
//! - Eval callbacks become runtime callable descriptors with retained context ownership.
//! - AOT and eval calls observe one reporting mask, handler stack, and dispatch path.
//! - The getter wrapper hands eval a retained callback cell without exposing the descriptor.
//! - Handler throws remain inside a guarded C boundary and return as pending eval exceptions.

use super::eval_callable_helpers::EvalCallableDescriptorSupport;
use crate::codegen::abi;
use crate::codegen::callable_descriptor;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::ir::Module;

const ERROR_HANDLER_NODE_BYTES: i64 = 48;
const EVAL_DYNAMIC_CONTEXT_CAPTURE: usize = 0;
const EVAL_DYNAMIC_CALLBACK_CAPTURE: usize = 1;
const EVAL_DYNAMIC_CALLABLE_CAPTURE_BYTES: usize = 32;

/// Emits eval-facing error-reporting, handler registration, restore, and dispatch wrappers.
pub(super) fn emit_eval_error_handler_helpers(
    module: &Module,
    emitter: &mut Emitter,
    support: &EvalCallableDescriptorSupport,
) {
    let Some(descriptor_label) = support.dynamic_descriptor_label() else {
        return;
    };
    emitter.blank();
    emitter.comment("--- eval bridge: error handler synchronization ---");
    match module.target.arch {
        Arch::AArch64 => {
            emit_error_reporting_aarch64(module, emitter);
            emit_set_error_handler_aarch64(module, emitter, descriptor_label);
            emit_restore_error_handler(module, emitter);

        }
        Arch::X86_64 => {
            emit_error_reporting_x86_64(module, emitter);
            emit_set_error_handler_x86_64(module, emitter, descriptor_label);
            emit_restore_error_handler(module, emitter);

        }
    }
    emit_dispatch_error_handler(module, emitter);
    emit_get_error_handler(module, emitter);
}

/// Returns the byte offset for one capture in an eval dynamic callable descriptor.
const fn dynamic_capture_offset(index: usize) -> usize {
    callable_descriptor::CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET + index * 16
}

/// Emits the ARM64 C wrapper for reading or replacing the reporting mask.
fn emit_error_reporting_aarch64(module: &Module, emitter: &mut Emitter) {
    let done = "__elephc_eval_error_reporting_done";
    label_c_global(module, emitter, "__elephc_eval_error_reporting");
    abi::emit_load_symbol_to_reg(emitter, "x10", "_php_error_reporting", 0);
    emitter.instruction(&format!("cbz x1, {done}"));                            // a zero replace flag makes this a read-only query
    abi::emit_store_reg_to_symbol(emitter, "x0", "_php_error_reporting", 0);
    emitter.label(done);
    emitter.instruction("mov x0, x10");                                         // return the reporting mask that was active on entry
    emitter.instruction("ret");                                                 // return to the magician runtime adapter
}

/// Emits the x86_64 C wrapper for reading or replacing the reporting mask.
fn emit_error_reporting_x86_64(module: &Module, emitter: &mut Emitter) {
    let done = "__elephc_eval_error_reporting_done_x";
    label_c_global(module, emitter, "__elephc_eval_error_reporting");
    abi::emit_load_symbol_to_reg(emitter, "rax", "_php_error_reporting", 0);
    emitter.instruction("test rsi, rsi");                                       // a zero replace flag makes this a read-only query
    emitter.instruction(&format!("jz {done}"));                                 // preserve the global mask for query calls
    abi::emit_store_reg_to_symbol(emitter, "rdi", "_php_error_reporting", 0);
    emitter.label(done);
    emitter.instruction("ret");                                                 // return the prior reporting mask in rax
}

/// Emits the ARM64 C wrapper for installing an eval user error handler.
fn emit_set_error_handler_aarch64(
    module: &Module,
    emitter: &mut Emitter,
    descriptor_label: &str,
) {
    let no_previous = "__elephc_eval_error_handler_set_no_previous";
    let no_callback = "__elephc_eval_error_handler_set_no_callback";
    let done = "__elephc_eval_error_handler_set_done";
    label_c_global(module, emitter, "__elephc_eval_error_handler_set");
    emitter.instruction("sub sp, sp, #112");                                    // reserve inputs, node state, captures, and a standard frame
    emitter.instruction("stp x29, x30, [sp, #96]");                             // preserve the C caller frame
    emitter.instruction("add x29, sp, #96");                                    // establish a stable wrapper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the active eval context pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the nullable boxed callback value
    emitter.instruction("str x2, [sp, #16]");                                   // save the selected PHP error-level mask
    emitter.instruction("str x3, [sp, #24]");                                   // save writable previous-callback output storage
    emitter.instruction(&format!("mov x0, #{ERROR_HANDLER_NODE_BYTES}"));       // allocate one native error-handler stack node
    emitter.instruction("bl __rt_heap_alloc");                                  // create storage for the previous process-wide state
    emitter.instruction("str x0, [sp, #32]");                                   // preserve the node across symbol loads and calls
    for (offset, symbol) in error_handler_fields() {
        abi::emit_load_symbol_to_reg(emitter, "x9", symbol, 0);
        emitter.instruction(&format!("str x9, [x0, #{offset}]"));               // transfer one prior handler field into the stack node
    }
    abi::emit_store_reg_to_symbol(emitter, "x0", "_php_error_handler_stack", 0);
    abi::emit_load_symbol_to_reg(emitter, "x0", "_php_error_handler_value", 0);
    emitter.instruction(&format!("cbz x0, {no_previous}"));                     // an empty prior registration returns a null raw pointer
    emitter.instruction("bl __rt_incref");                                      // give the PHP return value an independent owner
    emitter.label(no_previous);
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload previous-callback output storage
    emitter.instruction("str x0, [x9]");                                        // publish the retained prior callback or null

    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the nullable replacement callback
    emitter.instruction(&format!("cbz x0, {no_callback}"));                     // PHP null clears the active handler after stacking it
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the selected handler error-level mask
    abi::emit_store_reg_to_symbol(emitter, "x9", "_php_error_handler_mask", 0);
    emitter.instruction("bl __rt_incref");                                      // retain the PHP-visible callback in the native global
    abi::emit_store_reg_to_symbol(emitter, "x0", "_php_error_handler_value", 0);
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the callback for descriptor capture ownership
    emitter.instruction("bl __rt_incref");                                      // retain the callback owned by the runtime descriptor capture
    emitter.instruction("str x0, [sp, #40]");                                   // keep the capture owner while allocating the descriptor
    emitter.instruction(&format!(                                               // allocate the fixed descriptor plus two capture slots
        "mov x0, #{}",
        callable_descriptor::CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET
            + EVAL_DYNAMIC_CALLABLE_CAPTURE_BYTES
    ));
    emitter.instruction("bl __rt_heap_alloc");                                  // create the runtime descriptor block
    callable_descriptor::emit_copy_static_descriptor_to_runtime(
        emitter,
        "x0",
        descriptor_label,
    );
    emitter.instruction("ldr x9, [sp, #0]");                                    // load the eval context capture
    abi::emit_store_to_address(
        emitter,
        "x9",
        "x0",
        dynamic_capture_offset(EVAL_DYNAMIC_CONTEXT_CAPTURE),
    );
    emitter.instruction("ldr x9, [sp, #40]");                                   // load the retained boxed callback capture
    abi::emit_store_to_address(
        emitter,
        "x9",
        "x0",
        dynamic_capture_offset(EVAL_DYNAMIC_CALLBACK_CAPTURE),
    );
    abi::emit_store_reg_to_symbol(emitter, "x0", "_php_error_handler_callable", 0);
    emitter.instruction("ldr x0, [sp, #0]");                                    // retain the eval context for the persistent handler descriptor
    let retain_context = module.target.extern_symbol("__elephc_eval_context_retain");
    abi::emit_call_label(emitter, &retain_context);
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the retained eval context pointer
    abi::emit_store_reg_to_symbol(emitter, "x9", "_php_error_handler_context", 0);
    let release_context = module.target.extern_symbol("__elephc_eval_context_free");
    abi::emit_extern_symbol_address(emitter, "x10", &release_context);
    abi::emit_store_reg_to_symbol(
        emitter,
        "x10",
        "_php_error_handler_context_release",
        0,
    );
    emitter.instruction(&format!("b {done}"));                                  // leave the installed handler active

    emitter.label(no_callback);
    clear_error_handler_owner_globals(emitter);
    emitter.label(done);
    emitter.instruction("mov w0, #0");                                          // report EvalStatus::Ok to magician
    emitter.instruction("ldp x29, x30, [sp, #96]");                             // restore the C caller frame
    emitter.instruction("add sp, sp, #112");                                    // release wrapper scratch storage
    emitter.instruction("ret");                                                 // return to the Rust runtime adapter
}

/// Emits the x86_64 C wrapper for installing an eval user error handler.
fn emit_set_error_handler_x86_64(
    module: &Module,
    emitter: &mut Emitter,
    descriptor_label: &str,
) {
    let no_previous = "__elephc_eval_error_handler_set_no_previous_x";
    let no_callback = "__elephc_eval_error_handler_set_no_callback_x";
    let done = "__elephc_eval_error_handler_set_done_x";
    label_c_global(module, emitter, "__elephc_eval_error_handler_set");
    emitter.instruction("push rbp");                                            // preserve the C caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 64");                                         // reserve inputs, node state, and descriptor capture storage
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the active eval context pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the nullable boxed callback value
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save the selected PHP error-level mask
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save writable previous-callback output storage
    emitter.instruction(&format!("mov rax, {ERROR_HANDLER_NODE_BYTES}"));       // allocate one native error-handler stack node
    emitter.instruction("call __rt_heap_alloc");                                // create storage for the previous process-wide state
    emitter.instruction("mov QWORD PTR [rbp - 40], rax");                       // preserve the node across symbol loads and calls
    for (offset, symbol) in error_handler_fields() {
        abi::emit_load_symbol_to_reg(emitter, "r10", symbol, 0);
        emitter.instruction(&format!("mov QWORD PTR [rax + {offset}], r10"));   // transfer one prior handler field into the stack node
    }
    abi::emit_store_reg_to_symbol(emitter, "rax", "_php_error_handler_stack", 0);
    abi::emit_load_symbol_to_reg(emitter, "rax", "_php_error_handler_value", 0);
    emitter.instruction("test rax, rax");                                       // check for an existing callback value
    emitter.instruction(&format!("jz {no_previous}"));                          // an empty prior registration returns a null raw pointer
    emitter.instruction("call __rt_incref");                                    // give the PHP return value an independent owner
    emitter.label(no_previous);
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload previous-callback output storage
    emitter.instruction("mov QWORD PTR [r10], rax");                            // publish the retained prior callback or null

    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the nullable replacement callback
    emitter.instruction("test rax, rax");                                       // distinguish callback installation from PHP null
    emitter.instruction(&format!("jz {no_callback}"));                          // PHP null clears the active handler after stacking it
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the selected handler error-level mask
    abi::emit_store_reg_to_symbol(emitter, "r10", "_php_error_handler_mask", 0);
    emitter.instruction("call __rt_incref");                                    // retain the PHP-visible callback in the native global
    abi::emit_store_reg_to_symbol(emitter, "rax", "_php_error_handler_value", 0);
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the callback for descriptor capture ownership
    emitter.instruction("call __rt_incref");                                    // retain the callback owned by the runtime descriptor capture
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");                       // keep the capture owner while allocating the descriptor
    emitter.instruction(&format!(                                               // allocate the fixed descriptor plus two capture slots
        "mov rax, {}",
        callable_descriptor::CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET
            + EVAL_DYNAMIC_CALLABLE_CAPTURE_BYTES
    ));
    emitter.instruction("call __rt_heap_alloc");                                // create the runtime descriptor block
    callable_descriptor::emit_copy_static_descriptor_to_runtime(
        emitter,
        "rax",
        descriptor_label,
    );
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // load the eval context capture
    abi::emit_store_to_address(
        emitter,
        "r10",
        "rax",
        dynamic_capture_offset(EVAL_DYNAMIC_CONTEXT_CAPTURE),
    );
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // load the retained boxed callback capture
    abi::emit_store_to_address(
        emitter,
        "r10",
        "rax",
        dynamic_capture_offset(EVAL_DYNAMIC_CALLBACK_CAPTURE),
    );
    abi::emit_store_reg_to_symbol(emitter, "rax", "_php_error_handler_callable", 0);
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // retain the eval context for the persistent handler descriptor
    let retain_context = module.target.extern_symbol("__elephc_eval_context_retain");
    abi::emit_call_label(emitter, &retain_context);
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the retained eval context pointer
    abi::emit_store_reg_to_symbol(emitter, "r10", "_php_error_handler_context", 0);
    let release_context = module.target.extern_symbol("__elephc_eval_context_free");
    abi::emit_extern_symbol_address(emitter, "r10", &release_context);
    abi::emit_store_reg_to_symbol(
        emitter,
        "r10",
        "_php_error_handler_context_release",
        0,
    );
    emitter.instruction(&format!("jmp {done}"));                                // leave the installed handler active

    emitter.label(no_callback);
    clear_error_handler_owner_globals(emitter);
    emitter.label(done);
    emitter.instruction("xor eax, eax");                                        // report EvalStatus::Ok to magician
    emitter.instruction("mov rsp, rbp");                                        // release wrapper scratch storage
    emitter.instruction("pop rbp");                                             // restore the C caller frame pointer
    emitter.instruction("ret");                                                 // return to the Rust runtime adapter
}

/// Emits the target-aware C wrapper for restoring the prior native error handler.
fn emit_restore_error_handler(module: &Module, emitter: &mut Emitter) {
    label_c_global(module, emitter, "__elephc_eval_error_handler_restore");
    abi::emit_frame_prologue(emitter, 16);
    abi::emit_call_label(emitter, "__rt_core_error_handler_pop");
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
    abi::emit_frame_restore(emitter, 16);
    abi::emit_return(emitter);
}


/// Emits the target-aware C wrapper that reads the active native error handler for eval.
///
/// Returns the retained PHP-visible callback cell, or a null pointer when no handler is
/// active. The normalized descriptor and its context owner are never exposed.
fn emit_get_error_handler(module: &Module, emitter: &mut Emitter) {
    let done = "__elephc_eval_error_handler_get_done";
    label_c_global(module, emitter, "__elephc_eval_error_handler_get");
    abi::emit_frame_prologue(emitter, 16);
    abi::emit_load_symbol_to_reg(
        emitter,
        abi::int_result_reg(emitter),
        "_php_error_handler_value",
        0,
    );
    abi::emit_branch_if_int_result_zero(emitter, done);
    callable_descriptor::emit_retain_current_descriptor(emitter);
    emitter.label(done);
    abi::emit_frame_restore(emitter, 16);
    abi::emit_return(emitter);
}

/// Protects native handler invocation so exceptions return to eval before reaching its catch clauses.
fn emit_dispatch_error_handler(module: &Module, emitter: &mut Emitter) {
    use crate::codegen_support::runtime::emit_guarded_cleanup_call;
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let done = "__rt_eval_error_dispatch_done";
    let fatal = "__rt_eval_error_dispatch_fatal";
    label_c_global(module, emitter, "__elephc_eval_error_handler_dispatch");
    // -- stage borrowed inputs and clear the caller's result ownership outputs --
    abi::emit_frame_prologue(emitter, 96);
    for (index, offset) in [48, 40, 32, 24].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::emit_load_int_immediate(emitter, result, 0);
    for offset in [32, 24] {
        abi::load_at_offset(emitter, scratch, offset);
        abi::emit_store_to_address(emitter, result, scratch, 0);
    }
    abi::emit_store_zero_to_local_slot(emitter, 8);
    abi::emit_load_symbol_to_reg(emitter, result, "_php_error_handler_callable", 0);
    abi::store_at_offset(emitter, result, 56);
    abi::emit_branch_if_int_result_zero(emitter, done);
    abi::emit_load_symbol_to_reg(emitter, scratch, "_php_error_handler_mask", 0);
    abi::load_at_offset(emitter, result, 48);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("tst {result}, {scratch}"));           // honor the registered PHP diagnostic mask
            emitter.instruction(&format!("b.eq {done}"));                       // leave masked diagnostics to default reporting
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {result}, {scratch}"));          // honor the registered PHP diagnostic mask
            emitter.instruction(&format!("jz {done}"));                         // leave masked diagnostics to default reporting
        }
    }
    abi::load_at_offset(emitter, result, 56);
    abi::emit_load_from_address(emitter, result, result, callable_descriptor::CALLABLE_DESC_INVOKER_OFFSET);
    abi::emit_branch_if_int_result_zero(emitter, fatal);
    // -- retain the exception inside the C boundary instead of unwinding through Rust --
    abi::emit_frame_slot_address(emitter, result, 56);
    emit_guarded_cleanup_call(emitter, "__rt_eval_error_dispatch_callback", result, 8);
    abi::load_at_offset(emitter, result, 8);
    abi::emit_branch_if_int_result_zero(emitter, done);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    abi::emit_load_int_immediate(emitter, result, 3);
    abi::emit_frame_restore(emitter, 96);
    abi::emit_return(emitter);
    emitter.label(fatal);
    abi::emit_load_int_immediate(emitter, result, 2);
    abi::emit_frame_restore(emitter, 96);
    abi::emit_return(emitter);
    emitter.label(done);
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::emit_frame_restore(emitter, 96);
    abi::emit_return(emitter);

    // -- adapt the protected unary record to the existing descriptor invocation helper --
    emitter.label_global("__rt_eval_error_dispatch_callback");
    abi::emit_frame_prologue(emitter, 32);
    let arg0 = abi::int_arg_reg_name(emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(emitter.target, 1);
    abi::store_at_offset(emitter, arg0, 8);
    abi::emit_load_from_address(emitter, arg1, arg0, 16);
    abi::emit_load_from_address(emitter, arg0, arg0, 0);
    abi::emit_call_label(emitter, "__rt_error_handler_invoke");
    abi::load_at_offset(emitter, scratch, 8);
    abi::emit_load_from_address(emitter, arg1, scratch, 24);
    abi::emit_store_to_address(emitter, result, arg1, 0);
    abi::emit_load_from_address(emitter, arg1, scratch, 32);
    abi::emit_load_int_immediate(emitter, result, 1);
    abi::emit_store_to_address(emitter, result, arg1, 0);
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);
}

/// Clears the globals owned only by an active user error handler.
fn clear_error_handler_owner_globals(emitter: &mut Emitter) {
    abi::emit_store_zero_to_symbol(emitter, "_php_error_handler_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_php_error_handler_callable", 0);
    abi::emit_store_zero_to_symbol(emitter, "_php_error_handler_context", 0);
    abi::emit_store_zero_to_symbol(emitter, "_php_error_handler_context_release", 0);
}

/// Returns the linked-node field layout shared by registration and restoration.
fn error_handler_fields() -> [(usize, &'static str); 6] {
    [
        (0, "_php_error_handler_stack"),
        (8, "_php_error_handler_value"),
        (16, "_php_error_handler_callable"),
        (24, "_php_error_handler_mask"),
        (32, "_php_error_handler_context"),
        (40, "_php_error_handler_context_release"),
    ]
}

/// Emits a platform-C global label for one generated eval wrapper.
fn label_c_global(module: &Module, emitter: &mut Emitter, name: &str) {
    emitter.label_global(&module.target.extern_symbol(name));
}
