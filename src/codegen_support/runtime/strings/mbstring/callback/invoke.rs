//! Purpose:
//! Calls one resolved mbregex callback with an owned capture-array argument.
//!
//! Called from:
//! - The protected replacement callback invocation entry.
//!
//! Key details:
//! - The argument container has an exception activation before PHP runs.
//! - Returned values publish to Rust ownership before ordinary argument cleanup.
//! - Eval Throwable owners enter the native unwinder only after the Rust call returns.

use super::*;

/// Emits the shared native/eval callback invocation and balanced normal ownership cleanup.
pub(super) fn emit(emitter: &mut Emitter, eval: bool) {
    emitter.label("__rt_mbstring_callback_call_body");
    enter(emitter, 128);
    load_result(emitter, 16);
    if arm(emitter) { emitter.instruction("ldp x0, x1, [x0]"); }                // borrow the capture graph and its complete byte length
    else {
        emitter.instruction("mov rdi, QWORD PTR [rax]");                        // borrow the encoded capture graph
        emitter.instruction("mov rsi, QWORD PTR [rax + 8]");                    // retain its complete byte length
    }
    abi::emit_call_label(emitter, "__rt_mbstring_callback_arguments");
    save_result(emitter, 40);
    guards::guard(emitter, 48, 80);
    if eval {
        load_result(emitter, 0);
        branch_zero(emitter, "__rt_mbstring_callback_call_native");
        load_arg(emitter, 0, 0);
        load_arg(emitter, 1, 8);
        load_arg(emitter, 2, 40);
        if arm(emitter) { emitter.instruction("add x3, sp, #80"); }             // provide separate eval result and Throwable ownership slots
        else { emitter.instruction("lea rcx, [rsp + 80]"); }                    // borrow exclusive eval result storage
        emitter.bl_c("__elephc_eval_callable_call_array");
        branch_zero(emitter, "__rt_mbstring_callback_call_eval_value");
        if arm(emitter) {
            emitter.instruction("cmp x0, #3");                                  // recognize the eval PendingThrowable status
            emitter.instruction("b.ne __rt_mbstring_callback_call_fatal");      // preserve non-PHP failures as a fatal callback status
        } else {
            emitter.instruction("cmp eax, 3");                                  // distinguish pending PHP exceptions from fatal eval failures
            emitter.instruction("jne __rt_mbstring_callback_call_fatal");       // retire argument ownership before returning fatal status
        }
        load_result(emitter, 96);
        branch_zero(emitter, "__rt_mbstring_callback_call_fatal");
        abi::emit_call_label(emitter, "__rt_destructor_throw_mixed");
        emitter.label("__rt_mbstring_callback_call_eval_value");
        load_result(emitter, 88);
        abi::emit_jump(emitter, "__rt_mbstring_callback_call_publish");
    }
    emitter.label("__rt_mbstring_callback_call_native");
    load_result(emitter, 8);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    if arm(emitter) {
        emitter.instruction("mov x0, x1");                                      // pass the resolved native descriptor as the first invoker input
        emitter.instruction("ldr x9, [x0, #56]");                               // load the uniform boxed-array invoker
        load_arg(emitter, 1, 40);
        emitter.instruction("blr x9");                                          // execute the PHP callback inside the enclosing handler
    } else {
        emitter.instruction("mov r10, QWORD PTR [rdi + 56]");                   // load the descriptor's uniform invoker address
        load_arg(emitter, 1, 40);
        emitter.instruction("call r10");                                        // call with descriptor and owned boxed argument container
    }
    emitter.label("__rt_mbstring_callback_call_publish");
    branch_zero(emitter, "__rt_mbstring_callback_call_fatal");
    if arm(emitter) {
        load(emitter, "x9", 16);
        emitter.instruction("str x0, [x9, #16]");                               // publish result ownership before retiring callback arguments
    } else {
        load(emitter, "r10", 16);
        emitter.instruction("mov QWORD PTR [r10 + 16], rax");                   // transfer the independent result to the Rust cleanup arena
    }
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
    abi::emit_jump(emitter, "__rt_mbstring_callback_call_cleanup");
    emitter.label("__rt_mbstring_callback_call_fatal");
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 1);
    emitter.label("__rt_mbstring_callback_call_cleanup");
    save_result(emitter, 104);
    guards::unguard(emitter, 48, 80);
    load_result(emitter, 40);
    abi::emit_call_label(emitter, "__rt_decref_any");
    load_result(emitter, 104);
    leave(emitter, 128);
}
