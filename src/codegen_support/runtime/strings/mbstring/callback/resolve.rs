//! Purpose:
//! Validates replacement callbacks and transfers one owned callback box to the Rust arena.
//!
//! Called from:
//! - The protected mbregex callback-resolution entry.
//!
//! Key details:
//! - Eval retains its original callable; native calls normalize to the shared descriptor ABI.
//! - Associative method pairs receive an owned indexed copy guarded through descriptor selection.

use super::*;

/// Emits callback resolution with a live eval-context branch only when that bridge is linked.
pub(super) fn emit(emitter: &mut Emitter, eval: bool) {
    emitter.label_shared("__rt_mbstring_callback_resolve_body");
    enter(emitter, 96);
    if eval {
        load_result(emitter, 0);
        branch_zero(emitter, "__rt_mbstring_callback_resolve_native");
        load_arg(emitter, 0, 0);
        load_arg(emitter, 1, 8);
        emitter.bl_c("__elephc_eval_is_callable");
        branch_zero(emitter, "__rt_mbstring_callback_resolve_invalid");
        load_result(emitter, 8);
        abi::emit_call_label(emitter, "__rt_mixed_clone");
        publish(emitter);
        abi::emit_jump(emitter, "__rt_mbstring_callback_resolve_return");
    }
    emitter.label("__rt_mbstring_callback_resolve_native");
    load_result(emitter, 8);
    if arm(emitter) {
        emitter.instruction("ldr x9, [x0]");                                    // inspect the concrete copied callback tag
        emitter.instruction("cmp x9, #1");                                      // string names use the program's case-insensitive descriptor table
        emitter.instruction("b.eq __rt_mbstring_callback_resolve_ready");       // let descriptor resolution diagnose missing string targets
    } else {
        emitter.instruction("cmp QWORD PTR [rax], 1");                          // route string names through the authoritative descriptor table
        emitter.instruction("je __rt_mbstring_callback_resolve_ready");         // preserve case-insensitive function-name matching
    }
    load_arg(emitter, 0, 8);
    abi::emit_call_label(emitter, "__rt_is_callable_mixed");
    branch_zero(emitter, "__rt_mbstring_callback_resolve_invalid");
    emitter.label("__rt_mbstring_callback_resolve_ready");
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
    save_result(emitter, 24);
    load_result(emitter, 8);
    if arm(emitter) {
        emitter.instruction("ldr x9, [x0]");                                    // distinguish an associative callable pair
        emitter.instruction("cmp x9, #5");                                      // the descriptor selector consumes indexed method pairs
        emitter.instruction("b.ne __rt_mbstring_callback_resolve_guard");       // ordinary strings, objects, and descriptors need no array copy
    } else {
        emitter.instruction("cmp QWORD PTR [rax], 5");                          // identify an associative method pair
        emitter.instruction("jne __rt_mbstring_callback_resolve_guard");        // preserve every other callable representation
    }
    load_arg(emitter, 0, 8);
    abi::emit_call_label(emitter, "__rt_mbstring_callback_pair");
    save_result(emitter, 24);
    save_result(emitter, 8);
    emitter.label("__rt_mbstring_callback_resolve_guard");
    load_result(emitter, 24);
    guards::guard(emitter, 48, 48);
    load_result(emitter, 8);
    abi::emit_call_label(emitter, "_eir_shared_mbstring_callable");
    save_result(emitter, 32);
    if arm(emitter) {
        emitter.instruction("mov x1, x0");                                      // borrow the independently retained descriptor for boxing
        emitter.instruction("mov x0, #10");                                     // identify a callable descriptor payload
        emitter.instruction("mov x2, #0");                                      // descriptors carry no high payload word
    } else {
        emitter.instruction("mov rdi, rax");                                    // borrow the descriptor through the Mixed payload convention
        emitter.instruction("mov eax, 10");                                     // identify the callable runtime tag
        emitter.instruction("xor edx, edx");                                    // clear the unused payload word
    }
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    publish(emitter);
    load_result(emitter, 32);
    abi::emit_call_label(emitter, "__rt_callable_descriptor_release");
    guards::unguard(emitter, 48, 48);
    load_result(emitter, 24);
    abi::emit_call_label(emitter, "__rt_decref_any");
    abi::emit_jump(emitter, "__rt_mbstring_callback_resolve_return");
    emitter.label("__rt_mbstring_callback_resolve_invalid");
    abi::emit_call_label(emitter, "_eir_mbstring_callback_invalid");
    emitter.label("__rt_mbstring_callback_resolve_return");
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
    leave(emitter, 96);
}

/// Publishes the callback owner before any later release can propagate a PHP exception.
fn publish(emitter: &mut Emitter) {
    load(emitter, if arm(emitter) { "x9" } else { "r10" }, 16);
    if arm(emitter) { emitter.instruction("str x0, [x9]"); }                    // transfer callback ownership to the Rust arena
    else { emitter.instruction("mov QWORD PTR [r10], rax"); }                   // transfer the same owned callback cell
}
