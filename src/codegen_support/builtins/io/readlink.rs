//! Purpose:
//! Emits PHP `readlink` builtin calls.
//! Returns the canonical link target boxed as `Mixed` (`string|false`).
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - The runtime helper returns either an owned string pointer/length pair or
//!   `(0, 0)` on failure; this wrapper boxes both shapes into a Mixed cell so
//!   `=== false` and string echo behave PHP-compatibly.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Magic high 32 bits of the x86_64 heap-cell marker word, forming
/// `(X86_64_HEAP_MAGIC_HI32 << 32) | kind` together with the runtime kind.
const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Lowers a PHP `readlink()` call into target assembly.
///
/// Evaluates the path argument, calls the `__rt_readlink` runtime helper,
/// then boxes the raw result (owned string pointer/length or 0/0 on failure)
/// into a `Mixed` cell so PHP's `=== false` and string-echo semantics work
/// correctly. Returns `PhpType::Mixed`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("readlink()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_readlink");                             // libc readlink wrapper that returns an owned heap string (or 0/0 on failure)
    box_readlink_result(emitter, ctx);
    Some(PhpType::Mixed)
}

/// Boxes the raw `__rt_readlink` result into a `Mixed` cell.
///
/// On success the runtime helper returns `(ptr, len)` in registers (x1/x2 on
/// ARM64, rax/rdx on x86_64); this function allocates a heap cell, stamps it
/// with the string tag, and stores the pointer/length words without copying
/// the owned buffer. On failure the helper returns a null pointer; this path
/// jumps to `__rt_mixed_from_value` to box PHP's `false` value. Both paths
/// converge at `done_label`.
fn box_readlink_result(emitter: &mut Emitter, ctx: &mut Context) {
    let false_label = ctx.next_label("readlink_false");
    let done_label = ctx.next_label("readlink_done");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x1, {}", false_label));           // a null pointer means readlink() failed
            abi::emit_push_reg_pair(emitter, "x1", "x2");                       // preserve the successful link target while we allocate the mixed box
            emitter.instruction("mov x0, #24");                                 // mixed cells store tag plus two payload words
            abi::emit_call_label(emitter, "__rt_heap_alloc");                   // allocate the mixed result cell for a successful string payload
            emitter.instruction("mov x9, #5");                                  // heap kind 5 = mixed cell
            emitter.instruction("str x9, [x0, #-8]");                           // stamp the allocated payload as a mixed cell
            emitter.instruction("mov x9, #1");                                  // runtime tag 1 = string
            emitter.instruction("str x9, [x0]");                                // store the string tag in the mixed result
            abi::emit_pop_reg_pair(emitter, "x10", "x11");                      // reload the owned link target pointer and length
            emitter.instruction("stp x10, x11, [x0, #8]");                      // store the string payload words without copying the owned readlink buffer
            emitter.instruction(&format!("b {}", done_label));                  // skip the false-boxing path after a successful read
            emitter.label(&false_label);
            emitter.instruction("mov x1, #0");                                  // false payload = 0 for readlink() failure
            emitter.instruction("mov x2, #0");                                  // bool mixed payloads do not use a high word
            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box false for PHP-compatible failure semantics
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // null pointer means readlink() failed
            emitter.instruction(&format!("jz {}", false_label));                // box false when the runtime helper reports failure
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                     // preserve the successful link target while we allocate the mixed box
            emitter.instruction("mov rax, 24");                                 // mixed cells store tag plus two payload words
            abi::emit_call_label(emitter, "__rt_heap_alloc");                   // allocate the mixed result cell for a successful string payload
            emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 5)); // materialize the mixed-cell heap kind word with the x86_64 heap marker
            emitter.instruction("mov QWORD PTR [rax - 8], r10");                // stamp the allocated payload as a mixed cell
            emitter.instruction("mov r10, 1");                                  // runtime tag 1 = string
            emitter.instruction("mov QWORD PTR [rax], r10");                    // store the string tag in the mixed result
            abi::emit_pop_reg_pair(emitter, "r10", "r11");                      // reload the owned link target pointer and length
            emitter.instruction("mov QWORD PTR [rax + 8], r10");                // store the string pointer without copying the owned readlink buffer
            emitter.instruction("mov QWORD PTR [rax + 16], r11");               // store the string length without copying the owned readlink buffer
            emitter.instruction(&format!("jmp {}", done_label));                // skip the false-boxing path after a successful read
            emitter.label(&false_label);
            emitter.instruction("xor edi, edi");                                // false payload = 0 for readlink() failure
            emitter.instruction("xor esi, esi");                                // bool mixed payloads do not use a high word
            emitter.instruction("mov eax, 3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box false for PHP-compatible failure semantics
            emitter.label(&done_label);
        }
    }
}
