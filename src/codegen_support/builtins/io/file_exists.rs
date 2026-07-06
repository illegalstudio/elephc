//! Purpose:
//! Emits PHP `file_exists` filesystem metadata builtin calls.
//! Delegates platform stat work to runtime helpers and boxes PHP false-or-value results.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - Filesystem state is observable, so emitters must preserve call order and failure sentinels.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits a `file_exists` filesystem check for a single path argument.
///
/// Evaluates the path expression (argument 0). A `scheme://...` path whose
/// scheme matches a registered userspace wrapper is routed through
/// `__rt_user_wrapper_url_stat`, which instantiates the wrapper and calls its
/// `url_stat()`; the path exists iff that returns a stat array (not `false`).
/// Any other path falls through to `__rt_file_exists` for a real filesystem
/// stat. Returns `PhpType::Bool`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("file_exists()");
    emit_expr(&args[0], emitter, ctx, data);
    let fallback = ctx.next_label("file_exists_fs");
    let done = ctx.next_label("file_exists_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            // -- path string: x1 = ptr, x2 = len (the elephc string ABI) --
            emitter.instruction("sub sp, sp, #16");                             // scratch: [sp,#0] path ptr / result bool, [sp,#8] path len
            emitter.instruction("str x1, [sp, #0]");                            // save path ptr for the filesystem fallback
            emitter.instruction("str x2, [sp, #8]");                            // save path len for the filesystem fallback
            emitter.instruction("mov x0, x1");                                  // url_stat helper arg0 = path ptr
            emitter.instruction("mov x1, x2");                                  // url_stat helper arg1 = path len
            emitter.instruction("mov x2, #0");                                  // url_stat helper arg2 = flags (0)
            abi::emit_call_label(emitter, "__rt_user_wrapper_url_stat");        // x0 = boxed Mixed when a wrapper scheme matched
            abi::emit_symbol_address(emitter, "x9", "_url_stat_matched");
            emitter.instruction("ldrb w9, [x9]");                               // did a registered wrapper scheme match?
            emitter.instruction(&format!("cbz w9, {}", fallback));              // no → real filesystem stat
            emitter.instruction("ldr x10, [x0]");                               // boxed Mixed runtime tag (url_stat result)
            emitter.instruction("cmp x10, #3");                                 // tag 3 = bool false (wrapper reported the path absent)?
            emitter.instruction("cset x10, ne");                                // exists = (tag != 3, i.e. url_stat returned a stat array)
            emitter.instruction("str x10, [sp, #0]");                           // stash the exists bool across the result release
            abi::emit_call_label(emitter, "__rt_decref_any");                   // x0 still = boxed Mixed result; release it
            emitter.instruction("ldr x0, [sp, #0]");                            // reload the exists bool as the builtin result
            emitter.instruction(&format!("b {}", done));                        // skip the filesystem path
            emitter.label(&fallback);
            emitter.instruction("ldr x1, [sp, #0]");                            // restore path ptr for the filesystem helper
            emitter.instruction("ldr x2, [sp, #8]");                            // restore path len for the filesystem helper
            abi::emit_call_label(emitter, "__rt_file_exists");                  // real filesystem existence check
            emitter.label(&done);
            emitter.instruction("add sp, sp, #16");                             // release the scratch frame
        }
        Arch::X86_64 => {
            // -- path string: rax = ptr, rdx = len (the elephc string ABI) --
            emitter.instruction("sub rsp, 16");                                 // scratch: [rsp+0] path ptr / result bool, [rsp+8] path len
            emitter.instruction("mov QWORD PTR [rsp + 0], rax");                // save path ptr for the filesystem fallback
            emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                // save path len for the filesystem fallback
            emitter.instruction("mov rdi, rax");                                // url_stat helper arg0 = path ptr
            emitter.instruction("mov rsi, rdx");                                // url_stat helper arg1 = path len
            emitter.instruction("xor edx, edx");                                // url_stat helper arg2 = flags (0)
            abi::emit_call_label(emitter, "__rt_user_wrapper_url_stat");        // rax = boxed Mixed when a wrapper scheme matched
            abi::emit_symbol_address(emitter, "r9", "_url_stat_matched");       // load runtime data address
            emitter.instruction("movzx r9d, BYTE PTR [r9]");                    // did a registered wrapper scheme match?
            emitter.instruction("test r9d, r9d");                               // matched flag set?
            emitter.instruction(&format!("jz {}", fallback));                   // no → real filesystem stat
            emitter.instruction("mov r10, QWORD PTR [rax]");                    // boxed Mixed runtime tag (url_stat result)
            emitter.instruction("mov rdi, rax");                                // preserve the boxed result pointer for release
            emitter.instruction("cmp r10, 3");                                  // tag 3 = bool false (wrapper reported the path absent)?
            emitter.instruction("setne al");                                    // exists = (tag != 3, i.e. url_stat returned a stat array)
            emitter.instruction("movzx eax, al");                               // widen the bool into the canonical result register
            emitter.instruction("mov QWORD PTR [rsp + 0], rax");                // stash the exists bool across the result release
            emitter.instruction("mov rax, rdi");                                // __rt_decref_any reads the pointer in rax
            abi::emit_call_label(emitter, "__rt_decref_any");                   // release the boxed Mixed result
            emitter.instruction("mov rax, QWORD PTR [rsp + 0]");                // reload the exists bool as the builtin result
            emitter.instruction(&format!("jmp {}", done));                      // skip the filesystem path
            emitter.label(&fallback);
            emitter.instruction("mov rax, QWORD PTR [rsp + 0]");                // restore path ptr for the filesystem helper
            emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                // restore path len for the filesystem helper
            abi::emit_call_label(emitter, "__rt_file_exists");                  // real filesystem existence check
            emitter.label(&done);
            emitter.instruction("add rsp, 16");                                 // release the scratch frame
        }
    }
    Some(PhpType::Bool)
}
