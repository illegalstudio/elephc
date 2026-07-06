//! Purpose:
//! Emits PHP `is_file` filesystem metadata builtin calls.
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

/// Emits a call to the `is_file` builtin.
///
/// A `scheme://...` path whose scheme matches a registered userspace wrapper is
/// routed through `__rt_user_wrapper_url_stat_field` (field selector 1 =
/// `'mode'`): the path is a regular file when the wrapper's `url_stat()` reports
/// a `'mode'` whose `S_IFMT` bits equal `S_IFREG` (0o100000). Any other path
/// falls through to the platform-aware `__rt_is_file`. Returns `PhpType::Bool`.
///
/// # Arguments
/// - `args[0]`: the path expression to check
/// - `_name`: unused; matches the dispatcher signature
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("is_file()");
    emit_expr(&args[0], emitter, ctx, data);
    let fallback = ctx.next_label("is_file_fs");
    let done = ctx.next_label("is_file_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            // -- path string: x1 = ptr, x2 = len --
            emitter.instruction("sub sp, sp, #16");                             // scratch: [sp,#0] path ptr, [sp,#8] path len
            emitter.instruction("str x1, [sp, #0]");                            // save path ptr for the filesystem fallback
            emitter.instruction("str x2, [sp, #8]");                            // save path len for the filesystem fallback
            emitter.instruction("mov x0, x1");                                  // field helper arg0 = path ptr
            emitter.instruction("mov x1, x2");                                  // field helper arg1 = path len
            emitter.instruction("mov x2, #1");                                  // field selector 1 = 'mode'
            abi::emit_call_label(emitter, "__rt_user_wrapper_url_stat_field");  // x0 = wrapper 'mode' (or -1)
            abi::emit_symbol_address(emitter, "x9", "_url_stat_matched");
            emitter.instruction("ldrb w9, [x9]");                               // did a registered wrapper scheme match?
            emitter.instruction(&format!("cbz w9, {}", fallback));              // no → real filesystem is_file
            emitter.instruction("and x0, x0, #0xF000");                         // isolate the S_IFMT file-type bits of the mode
            emitter.instruction("mov x9, #0x8000");                             // S_IFREG = 0o100000 (regular file)
            emitter.instruction("cmp x0, x9");                                  // is it a regular file?
            emitter.instruction("cset x0, eq");                                 // is_file = (S_IFMT == S_IFREG)
            emitter.instruction(&format!("b {}", done));                        // skip the filesystem path
            emitter.label(&fallback);
            emitter.instruction("ldr x1, [sp, #0]");                            // restore path ptr for the filesystem helper
            emitter.instruction("ldr x2, [sp, #8]");                            // restore path len for the filesystem helper
            abi::emit_call_label(emitter, "__rt_is_file");                      // real filesystem regular-file check
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
            emitter.instruction("mov edx, 1");                                  // field selector 1 = 'mode'
            abi::emit_call_label(emitter, "__rt_user_wrapper_url_stat_field");  // rax = wrapper 'mode' (or -1)
            abi::emit_symbol_address(emitter, "r9", "_url_stat_matched");       // load runtime data address
            emitter.instruction("movzx r9d, BYTE PTR [r9]");                    // did a registered wrapper scheme match?
            emitter.instruction("test r9d, r9d");                               // matched flag set?
            emitter.instruction(&format!("jz {}", fallback));                   // no → real filesystem is_file
            emitter.instruction("and eax, 0xF000");                             // isolate the S_IFMT file-type bits of the mode
            emitter.instruction("cmp eax, 0x8000");                             // S_IFREG = 0o100000 (regular file)?
            emitter.instruction("sete al");                                     // is_file = (S_IFMT == S_IFREG)
            emitter.instruction("movzx eax, al");                               // widen the bool into the canonical result register
            emitter.instruction(&format!("jmp {}", done));                      // skip the filesystem path
            emitter.label(&fallback);
            emitter.instruction("mov rax, QWORD PTR [rsp + 0]");                // restore path ptr for the filesystem helper
            emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                // restore path len for the filesystem helper
            abi::emit_call_label(emitter, "__rt_is_file");                      // real filesystem regular-file check
            emitter.label(&done);
            emitter.instruction("add rsp, 16");                                 // release the scratch frame
        }
    }
    Some(PhpType::Bool)
}
