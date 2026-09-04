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
            emit_restore_error_handler_aarch64(module, emitter);
            emit_dispatch_error_handler_aarch64(module, emitter);
        }
        Arch::X86_64 => {
            emit_error_reporting_x86_64(module, emitter);
            emit_set_error_handler_x86_64(module, emitter, descriptor_label);
            emit_restore_error_handler_x86_64(module, emitter);
            emit_dispatch_error_handler_x86_64(module, emitter);
        }
    }
}

/// Returns the byte offset for one capture in an eval dynamic callable descriptor.
const fn dynamic_capture_offset(index: usize) -> usize {
    callable_descriptor::CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET + index * 16
}

/// Emits the ARM64 C wrapper for reading or replacing the reporting mask.
fn emit_error_reporting_aarch64(module: &Module, emitter: &mut Emitter) {
    let done = "__elephc_eval_error_reporting_done";
    label_c_global(module, emitter, "__elephc_eval_error_reporting");
    abi::emit_load_symbol_to_reg(emitter, "x9", "_php_error_reporting", 0);
    emitter.instruction(&format!("cbz x1, {done}"));                            // a zero replace flag makes this a read-only query
    abi::emit_store_reg_to_symbol(emitter, "x0", "_php_error_reporting", 0);
    emitter.label(done);
    emitter.instruction("mov x0, x9");                                          // return the reporting mask that was active on entry
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

    emitter.instruction("ldr x9, [sp, #16]");                                   // reload the selected handler error-level mask
    abi::emit_store_reg_to_symbol(emitter, "x9", "_php_error_handler_mask", 0);
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the nullable replacement callback
    emitter.instruction(&format!("cbz x0, {no_callback}"));                     // PHP null clears the active handler after stacking it
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

    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the selected handler error-level mask
    abi::emit_store_reg_to_symbol(emitter, "r10", "_php_error_handler_mask", 0);
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the nullable replacement callback
    emitter.instruction("test rax, rax");                                       // distinguish callback installation from PHP null
    emitter.instruction(&format!("jz {no_callback}"));                          // PHP null clears the active handler after stacking it
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

/// Emits the ARM64 C wrapper for restoring the prior native user error handler.
fn emit_restore_error_handler_aarch64(module: &Module, emitter: &mut Emitter) {
    let done = "__elephc_eval_error_handler_restore_done";
    label_c_global(module, emitter, "__elephc_eval_error_handler_restore");
    emitter.instruction("sub sp, sp, #32");                                     // reserve the node pointer and a standard frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve the C caller frame
    emitter.instruction("add x29, sp, #16");                                    // establish a stable wrapper frame pointer
    abi::emit_load_symbol_to_reg(emitter, "x9", "_php_error_handler_stack", 0);
    emitter.instruction(&format!("cbz x9, {done}"));                            // an empty stack leaves the current state unchanged
    emitter.instruction("str x9, [sp, #0]");                                    // preserve the node across release calls
    release_active_error_handler_aarch64(emitter);
    for (offset, symbol) in error_handler_fields() {
        emitter.instruction("ldr x9, [sp, #0]");                                // reload the node because symbol stores borrow x9
        emitter.instruction(&format!("ldr x10, [x9, #{offset}]"));              // load one prior handler field from the node
        abi::emit_store_reg_to_symbol(emitter, "x10", symbol, 0);
    }
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the consumed node for deallocation
    emitter.instruction("mov x0, x9");                                          // pass the consumed node to the heap allocator
    emitter.instruction("bl __rt_heap_free");                                   // free the restored stack node
    emitter.label(done);
    emitter.instruction("mov w0, #0");                                          // report EvalStatus::Ok to magician
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the C caller frame
    emitter.instruction("add sp, sp, #32");                                     // release wrapper scratch storage
    emitter.instruction("ret");                                                 // return to the Rust runtime adapter
}

/// Emits the x86_64 C wrapper for restoring the prior native user error handler.
fn emit_restore_error_handler_x86_64(module: &Module, emitter: &mut Emitter) {
    let done = "__elephc_eval_error_handler_restore_done_x";
    label_c_global(module, emitter, "__elephc_eval_error_handler_restore");
    emitter.instruction("push rbp");                                            // preserve the C caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 16");                                         // reserve the previous-state node pointer
    abi::emit_load_symbol_to_reg(emitter, "r10", "_php_error_handler_stack", 0);
    emitter.instruction("test r10, r10");                                       // check whether a previous registration exists
    emitter.instruction(&format!("jz {done}"));                                 // an empty stack leaves the current state unchanged
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                        // preserve the node across release calls
    release_active_error_handler_x86_64(emitter);
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the previous-state node
    for (offset, symbol) in error_handler_fields() {
        emitter.instruction(&format!("mov r11, QWORD PTR [r10 + {offset}]"));   // load one prior handler field from the node
        abi::emit_store_reg_to_symbol(emitter, "r11", symbol, 0);
    }
    emitter.instruction("mov rax, r10");                                        // pass the consumed node to the heap allocator
    emitter.instruction("call __rt_heap_free");                                 // free the restored stack node
    emitter.label(done);
    emitter.instruction("xor eax, eax");                                        // report EvalStatus::Ok to magician
    emitter.instruction("mov rsp, rbp");                                        // release wrapper scratch storage
    emitter.instruction("pop rbp");                                             // restore the C caller frame pointer
    emitter.instruction("ret");                                                 // return to the Rust runtime adapter
}

/// Emits the ARM64 C wrapper for invoking the active native user error handler.
fn emit_dispatch_error_handler_aarch64(module: &Module, emitter: &mut Emitter) {
    let done = "__elephc_eval_error_handler_dispatch_done";
    let fatal = "__elephc_eval_error_handler_dispatch_fatal";
    label_c_global(module, emitter, "__elephc_eval_error_handler_dispatch");
    emitter.instruction("sub sp, sp, #64");                                     // reserve inputs, descriptor state, and a standard frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // preserve the C caller frame
    emitter.instruction("add x29, sp, #48");                                    // establish a stable wrapper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the emitted PHP error level
    emitter.instruction("str x1, [sp, #8]");                                    // save the boxed Mixed callback argument array
    emitter.instruction("str x2, [sp, #16]");                                   // save writable callback-result output storage
    emitter.instruction("str x3, [sp, #24]");                                   // save writable invocation-state output storage
    emitter.instruction("str xzr, [x2]");                                       // clear the callback result before probing the active handler
    emitter.instruction("str xzr, [x3]");                                       // report no invocation unless the descriptor call completes
    abi::emit_load_symbol_to_reg(emitter, "x9", "_php_error_handler_callable", 0);
    emitter.instruction(&format!("cbz x9, {done}"));                            // no active descriptor means default error handling
    abi::emit_load_symbol_to_reg(emitter, "x10", "_php_error_handler_mask", 0);
    emitter.instruction("ldr x11, [sp, #0]");                                   // reload the emitted PHP error level
    emitter.instruction("tst x10, x11");                                        // check whether the handler accepts this error category
    emitter.instruction(&format!("b.eq {done}"));                               // a masked handler delegates to the default path
    emitter.instruction("str x9, [sp, #32]");                                   // preserve the active descriptor across invocation setup
    emitter.instruction(&format!(                                               // load the descriptor's uniform invoker entry
        "ldr x12, [x9, #{}]",
        callable_descriptor::CALLABLE_DESC_INVOKER_OFFSET
    ));
    emitter.instruction(&format!("cbz x12, {fatal}"));                          // reject malformed handler descriptors safely
    emitter.instruction("mov x0, x9");                                          // invoker argument 0 is the callable descriptor
    emitter.instruction("ldr x1, [sp, #8]");                                    // invoker argument 1 is the boxed argument array
    emitter.instruction("blr x12");                                             // execute the selected user error handler once
    emitter.instruction("ldr x9, [sp, #16]");                                   // reload callback-result output storage
    emitter.instruction("str x0, [x9]");                                        // transfer the owned boxed callback result to magician
    emitter.instruction("ldr x9, [sp, #24]");                                   // reload invocation-state output storage
    emitter.instruction("mov x10, #1");                                         // mark that a matching handler was invoked
    emitter.instruction("str x10, [x9]");                                       // publish the successful invocation marker
    emitter.label(done);
    emitter.instruction("mov w0, #0");                                          // report EvalStatus::Ok to magician
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the C caller frame
    emitter.instruction("add sp, sp, #64");                                     // release wrapper scratch storage
    emitter.instruction("ret");                                                 // return to the Rust runtime adapter
    emitter.label(fatal);
    emitter.instruction("mov w0, #2");                                          // report EvalStatus::RuntimeFatal for a malformed descriptor
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore the C caller frame after failed dispatch
    emitter.instruction("add sp, sp, #64");                                     // release wrapper scratch storage after failed dispatch
    emitter.instruction("ret");                                                 // return the failure status to magician
}

/// Emits the x86_64 C wrapper for invoking the active native user error handler.
fn emit_dispatch_error_handler_x86_64(module: &Module, emitter: &mut Emitter) {
    let done = "__elephc_eval_error_handler_dispatch_done_x";
    let fatal = "__elephc_eval_error_handler_dispatch_fatal_x";
    label_c_global(module, emitter, "__elephc_eval_error_handler_dispatch");
    emitter.instruction("push rbp");                                            // preserve the C caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish a stable wrapper frame pointer
    emitter.instruction("sub rsp, 32");                                         // reserve arguments and the active descriptor pointer
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the boxed Mixed callback argument array
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save writable callback-result output storage
    emitter.instruction("mov QWORD PTR [rbp - 24], rcx");                       // save writable invocation-state output storage
    emitter.instruction("mov QWORD PTR [rdx], 0");                              // clear the callback result before probing the active handler
    emitter.instruction("mov QWORD PTR [rcx], 0");                              // report no invocation unless the descriptor call completes
    abi::emit_load_symbol_to_reg(emitter, "r10", "_php_error_handler_callable", 0);
    emitter.instruction("test r10, r10");                                       // check whether a user error handler is active
    emitter.instruction(&format!("jz {done}"));                                 // no active descriptor means default error handling
    abi::emit_load_symbol_to_reg(emitter, "r11", "_php_error_handler_mask", 0);
    emitter.instruction("test r11, rdi");                                       // check whether the handler accepts this error category
    emitter.instruction(&format!("jz {done}"));                                 // a masked handler delegates to the default path
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // preserve the active descriptor across invocation setup
    emitter.instruction(&format!(                                               // load the descriptor's uniform invoker entry
        "mov r11, QWORD PTR [r10 + {}]",
        callable_descriptor::CALLABLE_DESC_INVOKER_OFFSET
    ));
    emitter.instruction("test r11, r11");                                       // validate the descriptor invoker pointer
    emitter.instruction(&format!("jz {fatal}"));                                // reject malformed handler descriptors safely
    emitter.instruction("mov rdi, r10");                                        // invoker argument 0 is the callable descriptor
    emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                        // invoker argument 1 is the boxed argument array
    emitter.instruction("call r11");                                            // execute the selected user error handler once
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload callback-result output storage
    emitter.instruction("mov QWORD PTR [r10], rax");                            // transfer the owned boxed callback result to magician
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload invocation-state output storage
    emitter.instruction("mov QWORD PTR [r10], 1");                              // publish the successful invocation marker
    emitter.label(done);
    emitter.instruction("xor eax, eax");                                        // report EvalStatus::Ok to magician
    emitter.instruction("mov rsp, rbp");                                        // release wrapper scratch storage
    emitter.instruction("pop rbp");                                             // restore the C caller frame pointer
    emitter.instruction("ret");                                                 // return to the Rust runtime adapter
    emitter.label(fatal);
    emitter.instruction("mov eax, 2");                                          // report EvalStatus::RuntimeFatal for a malformed descriptor
    emitter.instruction("mov rsp, rbp");                                        // release wrapper scratch storage after failed dispatch
    emitter.instruction("pop rbp");                                             // restore the C caller frame after failed dispatch
    emitter.instruction("ret");                                                 // return the failure status to magician
}

/// Releases the currently active ARM64 error-handler owners before restoration.
fn release_active_error_handler_aarch64(emitter: &mut Emitter) {
    abi::emit_load_symbol_to_reg(emitter, "x0", "_php_error_handler_value", 0);
    emitter.instruction("cbz x0, 1f");                                          // skip release when PHP null is active
    emitter.instruction("bl __rt_decref_mixed");                                // release the current PHP-visible callback owner
    emitter.label("1");
    abi::emit_load_symbol_to_reg(emitter, "x0", "_php_error_handler_callable", 0);
    emitter.instruction("bl __rt_callable_descriptor_release");                 // release the current normalized descriptor owner
    abi::emit_load_symbol_to_reg(emitter, "x10", "_php_error_handler_context_release", 0);
    emitter.instruction("cbz x10, 2f");                                         // native handlers carry no eval context owner
    abi::emit_load_symbol_to_reg(emitter, "x0", "_php_error_handler_context", 0);
    emitter.instruction("blr x10");                                             // release the active handler's retained eval context
    emitter.label("2");
}

/// Releases the currently active x86_64 error-handler owners before restoration.
fn release_active_error_handler_x86_64(emitter: &mut Emitter) {
    abi::emit_load_symbol_to_reg(emitter, "rax", "_php_error_handler_value", 0);
    emitter.instruction("test rax, rax");                                       // skip release when PHP null is active
    emitter.instruction("jz 1f");                                               // branch around the Mixed release
    emitter.instruction("call __rt_decref_mixed");                              // release the current PHP-visible callback owner
    emitter.label("1");
    abi::emit_load_symbol_to_reg(emitter, "rax", "_php_error_handler_callable", 0);
    emitter.instruction("call __rt_callable_descriptor_release");               // release the current normalized descriptor owner
    abi::emit_load_symbol_to_reg(emitter, "r10", "_php_error_handler_context_release", 0);
    emitter.instruction("test r10, r10");                                       // native handlers carry no eval context owner
    emitter.instruction("jz 2f");                                               // skip the absent eval-context release callback
    abi::emit_load_symbol_to_reg(emitter, "rdi", "_php_error_handler_context", 0);
    emitter.instruction("call r10");                                            // release the active handler's retained eval context
    emitter.label("2");
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
