//! Purpose:
//! Emits PHP `filesize` filesystem metadata builtin calls.
//! Delegates platform stat work to runtime helpers and boxes PHP false-or-value results.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Filesystem state is observable, so emitters must preserve call order and failure sentinels.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits PHP `filesize()` call. A `scheme://...` path whose scheme matches a
/// registered userspace wrapper is routed through
/// `__rt_user_wrapper_url_stat_field` (field selector 0 = `'size'`), which calls
/// the wrapper's `url_stat()` and extracts the integer `'size'` entry. Any other
/// path falls through to the platform-aware `__rt_filesize`. Returns
/// `PhpType::Int` (PHP's `false`-on-error is represented via the existing scalar
/// convention, which `__rt_filesize` itself models only approximately).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("filesize()");
    emit_expr(&args[0], emitter, ctx, data);
    let fallback = ctx.next_label("filesize_fs");
    let done = ctx.next_label("filesize_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            // -- path string: x1 = ptr, x2 = len --
            emitter.instruction("sub sp, sp, #16");                             // scratch: [sp,#0] path ptr, [sp,#8] path len
            emitter.instruction("str x1, [sp, #0]");                            // save path ptr for the filesystem fallback
            emitter.instruction("str x2, [sp, #8]");                            // save path len for the filesystem fallback
            emitter.instruction("mov x0, x1");                                  // field helper arg0 = path ptr
            emitter.instruction("mov x1, x2");                                  // field helper arg1 = path len
            emitter.instruction("mov x2, #0");                                  // field selector 0 = 'size'
            abi::emit_call_label(emitter, "__rt_user_wrapper_url_stat_field");  // x0 = wrapper 'size' (or -1)
            abi::emit_symbol_address(emitter, "x9", "_url_stat_matched");
            emitter.instruction("ldrb w9, [x9]");                               // did a registered wrapper scheme match?
            emitter.instruction(&format!("cbz w9, {}", fallback));              // no → real filesystem filesize
            emitter.instruction(&format!("b {}", done));                        // matched: x0 already holds the wrapper 'size'
            emitter.label(&fallback);
            emitter.instruction("ldr x1, [sp, #0]");                            // restore path ptr for the filesystem helper
            emitter.instruction("ldr x2, [sp, #8]");                            // restore path len for the filesystem helper
            abi::emit_call_label(emitter, "__rt_filesize");                     // real filesystem size
            emitter.label(&done);
            emitter.instruction("add sp, sp, #16");                             // release the scratch frame
        }
        Arch::X86_64 => {
            // -- path string: rax = ptr, rdx = len --
            emitter.instruction("sub rsp, 16");                                 // scratch: [rsp+0] path ptr, [rsp+8] path len
            emitter.instruction("mov QWORD PTR [rsp + 0], rax");                // save path ptr for the filesystem fallback
            emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                // save path len for the filesystem fallback
            emitter.instruction("mov rdi, rax");                                // field helper arg0 = path ptr
            emitter.instruction("mov rsi, rdx");                                // field helper arg1 = path len
            emitter.instruction("xor edx, edx");                                // field selector 0 = 'size'
            abi::emit_call_label(emitter, "__rt_user_wrapper_url_stat_field");  // rax = wrapper 'size' (or -1)
            emitter.instruction("lea r9, [rip + _url_stat_matched]");
            emitter.instruction("movzx r9d, BYTE PTR [r9]");                    // did a registered wrapper scheme match?
            emitter.instruction("test r9d, r9d");                               // matched flag set?
            emitter.instruction(&format!("jz {}", fallback));                   // no → real filesystem filesize
            emitter.instruction(&format!("jmp {}", done));                      // matched: rax already holds the wrapper 'size'
            emitter.label(&fallback);
            emitter.instruction("mov rax, QWORD PTR [rsp + 0]");                // restore path ptr for the filesystem helper
            emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                // restore path len for the filesystem helper
            abi::emit_call_label(emitter, "__rt_filesize");                     // real filesystem size
            emitter.label(&done);
            emitter.instruction("add rsp, 16");                                 // release the scratch frame
        }
    }
    Some(PhpType::Int)
}
