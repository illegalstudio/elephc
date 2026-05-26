//! Purpose:
//! Boxes filesystem stat runtime results into PHP arrays, strings, ints, or false.
//! Centralizes result-shape handling shared by stat-family builtin emitters.
//!
//! Called from:
//! - `crate::codegen::builtins::io::*::emit() for stat-family builtins`.
//!
//! Key details:
//! - False sentinels and Mixed array payloads must match PHP failure semantics and runtime GC layout.

use crate::codegen::context::Context;
use crate::codegen::emit::Emitter;
use crate::codegen::{abi, platform::Arch};

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Boxes a stat-family integer result into a Mixed cell, or returns PHP `false` on failure.
///
/// ## ARM64 register contract
/// - `x0`: stat integer payload on entry; holds box result on exit
/// - `x1`: success flag (non-zero = success, zero = failure)
/// - `x2`: unused for integers (high word = 0)
/// - Calls `__rt_mixed_from_value` to box int (tag 0) or bool false (tag 3)
///
/// ## x86_64 register contract
/// - `rax`: stat integer payload on entry; holds box result on exit
/// - `rdx`: success flag (non-zero = success, zero = failure)
/// - `rdi`: payload for `__rt_mixed_from_value`
/// - `esi`: 0 (high word unused for integers)
/// - `eax`: runtime tag (0 = int, 3 = bool false)
///
/// ## Ownership
/// Neither path retains the input registers as owners — the callee helper takes ownership.
pub(super) fn box_stat_int_or_false_result(emitter: &mut Emitter, ctx: &mut Context) {
    let false_label = ctx.next_label("stat_int_false");
    let done_label = ctx.next_label("stat_int_done");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x1, {}", false_label));           // runtime success flag unset: box PHP false
            emitter.instruction("mov x2, xzr");                                 // integer mixed payloads do not use a high word
            emitter.instruction("mov x1, x0");                                  // move the stat integer payload into the mixed helper low word
            emitter.instruction("mov x0, #0");                                  // runtime tag 0 = int
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box the successful integer result
            emitter.instruction(&format!("b {}", done_label));                  // skip the false-boxing path
            emitter.label(&false_label);
            emitter.instruction("mov x1, #0");                                  // false payload = 0 for stat failure
            emitter.instruction("mov x2, #0");                                  // bool mixed payloads do not use a high word
            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box false for PHP-compatible failure semantics
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("test rdx, rdx");                               // runtime success flag unset: box PHP false
            emitter.instruction(&format!("jz {}", false_label));                // jump to false boxing when stat failed
            emitter.instruction("mov rdi, rax");                                // move the stat integer payload into the mixed helper low word
            emitter.instruction("xor esi, esi");                                // integer mixed payloads do not use a high word
            emitter.instruction("xor eax, eax");                                // runtime tag 0 = int
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box the successful integer result
            emitter.instruction(&format!("jmp {}", done_label));                // skip the false-boxing path
            emitter.label(&false_label);
            emitter.instruction("xor edi, edi");                                // false payload = 0 for stat failure
            emitter.instruction("xor esi, esi");                                // bool mixed payloads do not use a high word
            emitter.instruction("mov eax, 3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box false for PHP-compatible failure semantics
            emitter.label(&done_label);
        }
    }
}

/// Boxes a stat-family string result into a Mixed cell, or returns PHP `false` on failure.
///
/// The string is assumed to be null-terminated with length in a separate register.
///
/// ## ARM64 register contract
/// - `x0`: null pointer means failure; otherwise holds box result
/// - `x1`: success flag (non-zero = success, zero = failure)
/// - Calls `__rt_mixed_from_value` to box string (tag 1) or bool false (tag 3)
///
/// ## x86_64 register contract
/// - `rax`: null pointer means failure; otherwise holds the string pointer
/// - `rdx`: string length
/// - `rdi`: string pointer for `__rt_mixed_from_value`
/// - `rsi`: string length for `__rt_mixed_from_value`
/// - `eax`: runtime tag (1 = string, 3 = bool false)
///
/// ## Ownership
/// The callee helper takes ownership of the string pointer.
pub(super) fn box_stat_string_or_false_result(emitter: &mut Emitter, ctx: &mut Context) {
    let false_label = ctx.next_label("stat_string_false");
    let done_label = ctx.next_label("stat_string_done");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x1, {}", false_label));           // null string pointer means filetype() failed
            emitter.instruction("mov x0, #1");                                  // runtime tag 1 = string
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // persist and box the successful filetype string
            emitter.instruction(&format!("b {}", done_label));                  // skip the false-boxing path
            emitter.label(&false_label);
            emitter.instruction("mov x1, #0");                                  // false payload = 0 for filetype() failure
            emitter.instruction("mov x2, #0");                                  // bool mixed payloads do not use a high word
            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box false for PHP-compatible failure semantics
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // null string pointer means filetype() failed
            emitter.instruction(&format!("jz {}", false_label));                // jump to false boxing when lstat failed
            emitter.instruction("mov rdi, rax");                                // move the filetype string pointer into the mixed helper low word
            emitter.instruction("mov rsi, rdx");                                // move the filetype string length into the mixed helper high word
            emitter.instruction("mov eax, 1");                                  // runtime tag 1 = string
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // persist and box the successful filetype string
            emitter.instruction(&format!("jmp {}", done_label));                // skip the false-boxing path
            emitter.label(&false_label);
            emitter.instruction("xor edi, edi");                                // false payload = 0 for filetype() failure
            emitter.instruction("xor esi, esi");                                // bool mixed payloads do not use a high word
            emitter.instruction("mov eax, 3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box false for PHP-compatible failure semantics
            emitter.label(&done_label);
        }
    }
}

/// Boxes a stat-family associative-array result into a Mixed cell, or returns PHP `false` on failure.
///
/// Allocates a heap mixed cell with tag 5 (associative array) and stores the hash pointer as the payload.
/// The false path uses `__rt_mixed_from_value` (tag 3) to box a bool false.
///
/// ## ARM64 register contract
/// - `x0`: null pointer means failure; otherwise holds the freshly built hash pointer
/// - `x9`: scratch used to stamp heap kind and runtime tag
/// - `x10`: scratch for reloading the hash pointer after allocation
/// - Calls `__rt_heap_alloc` to allocate 24-byte mixed cell, then `__rt_mixed_from_value` for false
///
/// ## x86_64 register contract
/// - `rax`: null pointer means failure; otherwise holds the freshly built hash pointer
/// - `r10`: scratch for heap kind stamping and reloading hash pointer
/// - Uses `X86_64_HEAP_MAGIC_HI32` marker in the heap kind stamp
/// - Calls `__rt_heap_alloc` to allocate 24-byte mixed cell, then `__rt_mixed_from_value` for false
///
/// ## Ownership
/// The allocated mixed cell owns the hash payload. The false path transfers no ownership.
pub(super) fn box_stat_array_or_false_result(emitter: &mut Emitter, ctx: &mut Context) {
    let false_label = ctx.next_label("stat_array_false");
    let done_label = ctx.next_label("stat_array_done");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x0, {}", false_label));           // null hash pointer means stat()/lstat()/fstat() failed
            abi::emit_push_reg(emitter, "x0");                                  // preserve the freshly built hash while allocating the mixed cell
            emitter.instruction("mov x0, #24");                                 // mixed cells store tag plus two payload words
            abi::emit_call_label(emitter, "__rt_heap_alloc");                   // allocate the mixed result cell for a successful stat array
            emitter.instruction("mov x9, #5");                                  // heap kind 5 = mixed cell
            emitter.instruction("str x9, [x0, #-8]");                           // stamp the allocated payload as a mixed cell
            emitter.instruction("mov x9, #5");                                  // runtime tag 5 = associative array
            emitter.instruction("str x9, [x0]");                                // store the associative-array tag in the mixed result
            abi::emit_pop_reg(emitter, "x10");                                  // reload the newly built stat hash pointer
            emitter.instruction("str x10, [x0, #8]");                           // store the hash pointer without retaining the new owner twice
            emitter.instruction("str xzr, [x0, #16]");                          // associative-array payloads do not use a high word
            emitter.instruction(&format!("b {}", done_label));                  // skip the false-boxing path
            emitter.label(&false_label);
            emitter.instruction("mov x1, #0");                                  // false payload = 0 for stat-array failure
            emitter.instruction("mov x2, #0");                                  // bool mixed payloads do not use a high word
            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box false for PHP-compatible failure semantics
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // null hash pointer means stat()/lstat()/fstat() failed
            emitter.instruction(&format!("jz {}", false_label));                // jump to false boxing when the runtime stat call failed
            abi::emit_push_reg(emitter, "rax");                                 // preserve the freshly built hash while allocating the mixed cell
            emitter.instruction("mov rax, 24");                                 // mixed cells store tag plus two payload words
            abi::emit_call_label(emitter, "__rt_heap_alloc");                   // allocate the mixed result cell for a successful stat array
            emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 5)); // materialize the mixed-cell heap kind word with the x86_64 heap marker
            emitter.instruction("mov QWORD PTR [rax - 8], r10");                // stamp the allocated payload as a mixed cell
            emitter.instruction("mov QWORD PTR [rax], 5");                      // runtime tag 5 = associative array
            abi::emit_pop_reg(emitter, "r10");                                  // reload the newly built stat hash pointer
            emitter.instruction("mov QWORD PTR [rax + 8], r10");                // store the hash pointer without retaining the new owner twice
            emitter.instruction("mov QWORD PTR [rax + 16], 0");                 // associative-array payloads do not use a high word
            emitter.instruction(&format!("jmp {}", done_label));                // skip the false-boxing path
            emitter.label(&false_label);
            emitter.instruction("xor edi, edi");                                // false payload = 0 for stat-array failure
            emitter.instruction("xor esi, esi");                                // bool mixed payloads do not use a high word
            emitter.instruction("mov eax, 3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box false for PHP-compatible failure semantics
            emitter.label(&done_label);
        }
    }
}
