//! Purpose:
//! Emits `__rt_mixed_spread_array`, which hands the spread lowering a REAL indexed array for a
//! boxed value that may be a callable descriptor rather than an array.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::callables`.
//! - `narrow_gradual_indexed_spread_source` in `crate::ir_lower::expr::indexed_array_literals`.
//!
//! Key details:
//! - A `callable` slot holds a descriptor, not the `[$object, 'method']` array the caller wrote,
//!   so `...$callable` had nothing to unpack: the narrowing unboxed runtime tag 4 and read a
//!   descriptor as an array. Materializing the two elements once, here, lets every downstream
//!   step — the length guard, the element reads, the argument planner — stay unchanged.
//! - Mixed IN, Mixed OUT. Only a callable descriptor is replaced — by a boxed two-slot array
//!   of Mixed cells holding the bound receiver and the bare method name. EVERY other cell is
//!   returned untouched, so the ordinary `MixedUnbox` that runs after it keeps its tag guard,
//!   its "Only arrays and Traversables can be unpacked" message, and the invoker clone it
//!   makes for an `array<mixed>` result.

use crate::codegen_support::callable_descriptor::{
    CALLABLE_DESC_KIND_ARRAY, CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET,
};
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits the `__rt_mixed_spread_array` runtime helper for the active target.
///
/// ## ARM64 ABI
/// - **Input**: `x0` = boxed Mixed cell
/// - **Output**: `x0` = a boxed Mixed cell: the input itself, or a fresh boxed array
///
/// ## x86_64 ABI
/// - **Input**: `rax` = boxed Mixed cell
/// - **Output**: `rax` = the same
pub(crate) fn emit_mixed_spread_array(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_spread_array_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed spread array ---");
    emitter.label_global("__rt_mixed_spread_array");

    emitter.instruction("cbz x0, __rt_mixed_spread_array_passthrough");         // a null cell is the caller's problem, not ours
    emitter.instruction("ldr x9, [x0]");                                        // load the boxed runtime tag
    emitter.instruction("cmp x9, #10");                                         // tag 10 is a callable descriptor
    emitter.instruction("b.eq __rt_mixed_spread_array_callable");
    emitter.label("__rt_mixed_spread_array_passthrough");
    emitter.instruction("ret");                                                 // every other cell is returned untouched

    emitter.label("__rt_mixed_spread_array_callable");
    emitter.instruction("ldr x9, [x0, #8]");                                    // load the callable descriptor
    emitter.instruction("ldr x10, [x9]");                                       // load its source-shape kind
    emitter.instruction(&format!("cmp x10, #{}", CALLABLE_DESC_KIND_ARRAY));
    emitter.instruction("b.ne __rt_mixed_spread_array_passthrough");            // a closure has no array form; let the unbox refuse it

    emitter.instruction("sub sp, sp, #32");                                     // reserve slots for the descriptor and the result array
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");
    emitter.instruction("str x9, [sp, #0]");                                    // preserve the descriptor across the allocations

    emitter.instruction("mov x0, #2");                                          // a callable array is exactly two elements
    emitter.instruction("mov x1, #8");                                          // each slot holds one boxed Mixed pointer
    emitter.instruction("bl __rt_array_new");
    emitter.instruction("ldr x11, [x0, #-8]");                                  // load the packed indexed-array kind word
    emitter.instruction("mov x12, #0x700");                                     // runtime value_type tag 7 = boxed Mixed cells
    emitter.instruction("orr x11, x11, x12");
    emitter.instruction("str x11, [x0, #-8]");                                  // persist the Mixed slot tag on the new array
    emitter.instruction("mov x12, #2");
    emitter.instruction("str x12, [x0]");                                       // header[0]: length = 2
    emitter.instruction("str x0, [sp, #8]");                                    // preserve the array across the boxing calls

    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the descriptor
    emitter.instruction(&format!(
        "ldr x1, [x9, #{}]",
        CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET
    ));                                                                          // capture 0 is the bound receiver
    emitter.instruction("mov x0, #6");                                          // runtime tag 6 identifies an object payload
    emitter.instruction("mov x2, #0");                                          // object payloads do not use a high word
    emitter.instruction("bl __rt_mixed_from_value");                            // box it, retaining the receiver
    emitter.instruction("ldr x9, [sp, #8]");
    emitter.instruction("str x0, [x9, #24]");                                   // payload slot 0, past the 24-byte header

    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the descriptor
    emitter.instruction("bl __rt_callable_descriptor_method_name");             // already answers a boxed Mixed string
    emitter.instruction("ldr x9, [sp, #8]");
    emitter.instruction("str x0, [x9, #32]");                                   // payload slot 1

    emitter.instruction("mov x1, x9");                                          // box the materialized array back up
    emitter.instruction("mov x0, #4");                                          // runtime tag 4 identifies an indexed array
    emitter.instruction("mov x2, #0");                                          // array payloads do not use a high word
    emitter.instruction("bl __rt_mixed_from_value");
    emitter.instruction("ldp x29, x30, [sp, #16]");
    emitter.instruction("add sp, sp, #32");
    emitter.instruction("ret");                                                 // a boxed cell, exactly like the input
}

/// Emits the x86_64 variant of `__rt_mixed_spread_array`.
fn emit_mixed_spread_array_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed spread array ---");
    emitter.label_global("__rt_mixed_spread_array");

    emitter.instruction("test rax, rax");                                       // a null cell is the caller's problem, not ours
    emitter.instruction("jz __rt_mixed_spread_array_passthrough");
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load the boxed runtime tag
    emitter.instruction("cmp r10, 10");                                         // tag 10 is a callable descriptor
    emitter.instruction("je __rt_mixed_spread_array_callable");
    emitter.label("__rt_mixed_spread_array_passthrough");
    emitter.instruction("ret");                                                 // every other cell is returned untouched

    emitter.label("__rt_mixed_spread_array_callable");
    emitter.instruction("mov r10, QWORD PTR [rax + 8]");                        // load the callable descriptor
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // load its source-shape kind
    emitter.instruction(&format!("cmp r11, {}", CALLABLE_DESC_KIND_ARRAY));
    emitter.instruction("jne __rt_mixed_spread_array_passthrough");             // a closure has no array form; let the unbox refuse it

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 16");                                         // slots for the descriptor and the result array
    emitter.instruction("mov QWORD PTR [rbp - 8], r10");                        // preserve the descriptor across the allocations

    emitter.instruction("mov rdi, 2");                                          // a callable array is exactly two elements
    emitter.instruction("mov rsi, 8");                                          // each slot holds one boxed Mixed pointer
    emitter.instruction("call __rt_array_new");
    emitter.instruction("mov r11, QWORD PTR [rax - 8]");                        // load the packed indexed-array kind word
    emitter.instruction("or r11, 0x700");                                       // runtime value_type tag 7 = boxed Mixed cells
    emitter.instruction("mov QWORD PTR [rax - 8], r11");                        // persist the Mixed slot tag on the new array
    emitter.instruction("mov QWORD PTR [rax], 2");                              // header[0]: length = 2
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the array across the boxing calls

    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the descriptor
    emitter.instruction(&format!(
        "mov rdi, QWORD PTR [r10 + {}]",
        CALLABLE_DESC_RUNTIME_CAPTURE_OFFSET
    ));                                                                          // capture 0 is the bound receiver
    emitter.instruction("xor esi, esi");                                        // object payloads do not use a length word
    emitter.instruction("mov rax, 6");                                          // runtime tag 6 identifies an object payload
    emitter.instruction("call __rt_mixed_from_value");                          // box it, retaining the receiver
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");
    emitter.instruction("mov QWORD PTR [r10 + 24], rax");                       // payload slot 0, past the 24-byte header

    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the descriptor
    emitter.instruction("call __rt_callable_descriptor_method_name");           // already answers a boxed Mixed string
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");
    emitter.instruction("mov QWORD PTR [r10 + 32], rax");                       // payload slot 1

    emitter.instruction("mov rdi, r10");                                        // box the materialized array back up
    emitter.instruction("xor esi, esi");                                        // array payloads do not use a length word
    emitter.instruction("mov rax, 4");                                          // runtime tag 4 identifies an indexed array
    emitter.instruction("call __rt_mixed_from_value");
    emitter.instruction("add rsp, 16");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");                                                 // a boxed cell, exactly like the input
}
