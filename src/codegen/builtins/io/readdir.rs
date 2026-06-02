//! Purpose:
//! Emits PHP `readdir` calls.
//! Reads the next entry name from a directory handle opened by `opendir()`.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - The `__rt_readdir` helper returns a null pointer at end-of-directory; that
//!   case is boxed as PHP false, an entry name as a boxed string.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("readdir()");
    emit_stream_fd_arg("readdir", &args[0], emitter, ctx, data);
    // -- dispatch: synthetic wrapper fd -> dir_readdir, else libc readdir --
    let wrapper = ctx.next_label("readdir_wrapper");
    let after = ctx.next_label("readdir_after");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov w9, #0x4000");                             // high half of USER_WRAPPER_FD_BASE
            emitter.instruction("lsl w9, w9, #16");                             // form 0x40000000
            emitter.instruction("cmp x0, x9");                                  // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("b.ge {}", wrapper));                  // dispatch into dir_readdir
            abi::emit_call_label(emitter, "__rt_readdir");                      // libc readdir -> name in x1/x2
            emitter.instruction(&format!("b {}", after));                       // skip the wrapper path
            emitter.label(&wrapper);
            abi::emit_call_label(emitter, "__rt_user_wrapper_dir_readdir");     // wrapper dir_readdir
            emitter.label(&after);
        }
        Arch::X86_64 => {
            emitter.instruction("mov r9d, 0x40000000");                         // USER_WRAPPER_FD_BASE
            emitter.instruction("cmp rax, r9");                                 // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("jge {}", wrapper));                   // dispatch into dir_readdir
            emitter.instruction("mov rdi, rax");                                // descriptor into the runtime-helper argument register
            abi::emit_call_label(emitter, "__rt_readdir");                      // libc readdir -> name in rax/rdx
            emitter.instruction(&format!("jmp {}", after));                     // skip the wrapper path
            emitter.label(&wrapper);
            emitter.instruction("mov rdi, rax");                                // descriptor into the runtime-helper argument register
            abi::emit_call_label(emitter, "__rt_user_wrapper_dir_readdir");     // wrapper dir_readdir
            emitter.label(&after);
        }
    }
    box_string_or_false(emitter, ctx);
    Some(PhpType::Mixed)
}

/// Boxes the helper result: a null pointer becomes PHP `false`, a non-null
/// pointer/length pair becomes a boxed string without copying the buffer.
fn box_string_or_false(emitter: &mut Emitter, ctx: &mut Context) {
    let false_label = ctx.next_label("readdir_false");
    let done_label = ctx.next_label("readdir_done");

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x1, {}", false_label));           // a null pointer means end of directory
            abi::emit_push_reg_pair(emitter, "x1", "x2"); // preserve the string payload across the allocation
            emitter.instruction("mov x0, #24");                                 // mixed cells store a tag plus two payload words
            abi::emit_call_label(emitter, "__rt_heap_alloc");
            emitter.instruction("mov x9, #5");                                  // heap kind 5 = mixed cell
            emitter.instruction("str x9, [x0, #-8]");                           // stamp the allocation as a mixed cell
            emitter.instruction("mov x9, #1");                                  // runtime tag 1 = string
            emitter.instruction("str x9, [x0]");                                // store the string tag
            abi::emit_pop_reg_pair(emitter, "x10", "x11"); // reload the string pointer and length
            emitter.instruction("stp x10, x11, [x0, #8]");                      // store the string payload words
            emitter.instruction(&format!("b {}", done_label));                  // skip the false path after a valid result
            emitter.label(&false_label);
            emitter.instruction("mov x1, #0");                                  // false payload = 0
            emitter.instruction("mov x2, #0");                                  // bool mixed payloads have no high word
            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("test rax, rax");                               // a null pointer means end of directory
            emitter.instruction(&format!("jz {}", false_label));                // box false at end of directory
            abi::emit_push_reg_pair(emitter, "rax", "rdx"); // preserve the string payload across the allocation
            emitter.instruction("mov rax, 24");                                 // mixed cells store a tag plus two payload words
            abi::emit_call_label(emitter, "__rt_heap_alloc");
            emitter.instruction(&format!(
                "mov r10, 0x{:x}",
                (X86_64_HEAP_MAGIC_HI32 << 32) | 5
            )); // mixed-cell heap-kind word with the x86_64 heap marker
            emitter.instruction("mov QWORD PTR [rax - 8], r10");                // stamp the allocation as a mixed cell
            emitter.instruction("mov r10, 1");                                  // runtime tag 1 = string
            emitter.instruction("mov QWORD PTR [rax], r10");                    // store the string tag
            abi::emit_pop_reg_pair(emitter, "r10", "r11"); // reload the string pointer and length
            emitter.instruction("mov QWORD PTR [rax + 8], r10");                // store the string pointer
            emitter.instruction("mov QWORD PTR [rax + 16], r11");               // store the string length
            emitter.instruction(&format!("jmp {}", done_label));                // skip the false path after a valid result
            emitter.label(&false_label);
            emitter.instruction("xor edi, edi");                                // false payload = 0
            emitter.instruction("xor esi, esi");                                // bool mixed payloads have no high word
            emitter.instruction("mov eax, 3");                                  // runtime tag 3 = bool false
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.label(&done_label);
        }
    }
}
