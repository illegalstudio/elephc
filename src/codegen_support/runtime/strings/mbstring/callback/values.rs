//! Purpose:
//! Builds independent capture arguments and normalizes associative callable pairs.
//!
//! Called from:
//! - Replacement callback invocation and native callable resolution.
//!
//! Key details:
//! - No PHP code runs while these fresh arrays are constructed.
//! - Every acquired child transfers through the standard refcounted array and Mixed helpers.

use super::*;

/// Emits the one-argument capture container and the two-element callable-array adapter.
pub(super) fn emit(emitter: &mut Emitter) {
    array(emitter, "__rt_mbstring_callback_arguments", 1, captures);
    array(emitter, "__rt_mbstring_callback_pair", 2, pair_element);
}

/// Builds and boxes a fresh indexed array whose acquired children are retained exactly once.
fn array(emitter: &mut Emitter, label: &str, count: usize, element: fn(&mut Emitter, usize)) {
    emitter.label(label);
    enter(emitter, 64);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 0), count as i64);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 1), 8);
    abi::emit_call_label(emitter, "__rt_array_new");
    save_result(emitter, 24);
    for index in 0..count {
        element(emitter, index);
        save_result(emitter, 32);
        load_arg(emitter, 0, 24);
        load_arg(emitter, 1, 32);
        abi::emit_call_label(emitter, "__rt_array_push_refcounted");
        save_result(emitter, 24);
        load_result(emitter, 32);
        abi::emit_call_label(emitter, "__rt_decref_any");
    }
    load_result(emitter, 24);
    box_container(emitter, false);
    save_result(emitter, 40);
    load_result(emitter, 24);
    abi::emit_call_label(emitter, "__rt_decref_any");
    load_result(emitter, 40);
    leave(emitter, 64);
}

/// Restores and boxes numeric and named captures from the shared ordered graph format.
fn captures(emitter: &mut Emitter, _index: usize) {
    load_arg(emitter, 0, 0);
    load_arg(emitter, 1, 8);
    abi::emit_call_label(emitter, "__rt_mbstring_restore_array");
    save_result(emitter, 48);
    box_container(emitter, true);
    save_result(emitter, 56);
    load_result(emitter, 48);
    abi::emit_call_label(emitter, "__rt_decref_any");
    load_result(emitter, 56);
}

/// Reads one present numeric key from an already validated associative method pair.
fn pair_element(emitter: &mut Emitter, index: usize) {
    load_arg(emitter, 0, 0);
    for (arg, value) in [(1, index as i64), (2, -1), (3, 0)] {
        abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, arg), value);
    }
    abi::emit_call_label(emitter, "__rt_mixed_array_get");
}

/// Retains a raw array in a new Mixed cell, deriving hash versus list shape for capture graphs.
fn box_container(emitter: &mut Emitter, dynamic: bool) {
    if dynamic { abi::emit_push_reg(emitter, abi::int_result_reg(emitter)); abi::emit_call_label(emitter, "__rt_heap_kind"); }
    if arm(emitter) {
        if dynamic {
            emitter.instruction("cmp x0, #2");                                  // distinguish indexed captures from a named-capture hash
            emitter.instruction("mov x9, #4");                                  // use the indexed-array runtime tag
            emitter.instruction("mov x10, #5");                                 // use the associative-array runtime tag
            emitter.instruction("csel x0, x9, x10, eq");                        // select the Mixed representation from the actual heap kind
            abi::emit_pop_reg(emitter, "x1");
        } else {
            emitter.instruction("mov x1, x0");                                  // borrow the completed indexed array
            emitter.instruction("mov x0, #4");                                  // box the indexed-array runtime representation
        }
        emitter.instruction("mov x2, #0");                                      // array payloads have no high word
    } else {
        if dynamic {
            emitter.instruction("cmp rax, 2");                                  // distinguish indexed and associative capture storage
            emitter.instruction("mov eax, 4");                                  // prepare the indexed-array tag
            emitter.instruction("mov r10d, 5");                                 // prepare the associative-array tag
            emitter.instruction("cmovne rax, r10");                             // select the tag without changing the retained container
            abi::emit_pop_reg(emitter, "rdi");
        } else {
            emitter.instruction("mov rdi, rax");                                // borrow the completed indexed array
            emitter.instruction("mov eax, 4");                                  // select the indexed-array Mixed tag
        }
        emitter.instruction("xor edx, edx");                                    // clear the unused payload word
    }
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
}
