//! Purpose:
//! Emits PHP `realpath` path-oriented builtin calls.
//! Marshals path strings into runtime helpers that normalize, split, or enumerate filesystem paths.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Returned strings and arrays must use runtime allocation/layout compatible with PHP false-on-failure behavior.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// x86_64 heap marker: high 32 bits of the magic `0x454C5048` signature baked into
/// mixed-cell heap kind words on this platform to distinguish allocated buffers from
/// inline/special values during runtime verification.
const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Emits a call to the `realpath` builtin.
///
/// Evaluates the path argument, calls `__rt_realpath` to resolve it via libc,
/// then boxes the result into a `Mixed` cell: `String` on success or `Bool(false)`
/// on failure (matching PHP semantics). The returned type is always `PhpType::Mixed`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("realpath()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_realpath");                             // call the target-aware runtime helper that canonicalizes the path through libc realpath()
    box_realpath_result(emitter, ctx);
    Some(PhpType::Mixed)
}

/// Box the realpath runtime result into a Mixed cell.
///
/// The runtime helper returns either `(ptr, len)` in registers or `(0, 0)` on failure.
/// On success, allocates a heap mixed cell, stamps it as kind 5, tags it as a string (tag 1),
/// and stores the path pointer/length directly without copying the owned realpath buffer.
/// On failure, calls `__rt_mixed_from_value` to box `false`. Caller is responsible for
/// preserving any caller-saved registers required by the ABI before this call.
fn box_realpath_result(emitter: &mut Emitter, ctx: &mut Context) {
    let false_label = ctx.next_label("realpath_false");
    let done_label = ctx.next_label("realpath_done");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x1, {}", false_label));           // a null runtime string pointer means realpath() failed
            abi::emit_push_reg_pair(emitter, "x1", "x2");                       // preserve the canonical path while we allocate the mixed box
            emitter.instruction("mov x0, #24");                                 // mixed cells store tag plus two payload words
            abi::emit_call_label(emitter, "__rt_heap_alloc");                   // allocate the mixed result cell for a successful string payload
            emitter.instruction("mov x9, #5");                                  // heap kind 5 = mixed cell
            emitter.instruction("str x9, [x0, #-8]");                           // stamp the allocated payload as a mixed cell
            emitter.instruction("mov x9, #1");                                  // runtime tag 1 = string
            emitter.instruction("str x9, [x0]");                                // store the string tag in the mixed result
            abi::emit_pop_reg_pair(emitter, "x10", "x11");                      // reload the owned canonical path pointer and length
            emitter.instruction("stp x10, x11, [x0, #8]");                      // store the string payload words without copying the owned realpath buffer
            emitter.instruction(&format!("b {}", done_label));                  // skip the false-boxing path after a successful resolve
            emitter.label(&false_label);
            emitter.instruction("mov x1, #0");                                  // false payload = 0 for realpath() failure
            emitter.instruction("mov x2, #0");                                  // bool mixed payloads do not use a high word
            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box false for PHP-compatible failure semantics
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // a null runtime string pointer means realpath() failed
            emitter.instruction(&format!("jz {}", false_label));                // box false when the runtime helper reports failure
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the canonical path while we allocate the mixed box
            emitter.instruction("mov rax, 24");                                 // mixed cells store tag plus two payload words
            abi::emit_call_label(emitter, "__rt_heap_alloc");                   // allocate the mixed result cell for a successful string payload
            emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 5)); //materialize the mixed-cell heap kind word with the x86_64 heap marker
            emitter.instruction("mov QWORD PTR [rax - 8], r10");                // stamp the allocated payload as a mixed cell
            emitter.instruction("mov r10, 1");                                  // runtime tag 1 = string
            emitter.instruction("mov QWORD PTR [rax], r10");                    // store the string tag in the mixed result
            abi::emit_pop_reg_pair(emitter, "r10", "r11");                      // reload the owned canonical path pointer and length
            emitter.instruction("mov QWORD PTR [rax + 8], r10");                // store the string pointer without copying the owned realpath buffer
            emitter.instruction("mov QWORD PTR [rax + 16], r11");               // store the string length without copying the owned realpath buffer
            emitter.instruction(&format!("jmp {}", done_label));                // skip the false-boxing path after a successful resolve
            emitter.label(&false_label);
            emitter.instruction("xor edi, edi");                                // false payload = 0 for realpath() failure
            emitter.instruction("xor esi, esi");                                // bool mixed payloads do not use a high word
            emitter.instruction("mov eax, 3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box false for PHP-compatible failure semantics
            emitter.label(&done_label);
        }
    }
}
