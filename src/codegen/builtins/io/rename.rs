//! Purpose:
//! Emits PHP `rename` filesystem mutation builtin calls.
//! Routes a `scheme://` source path matching a registered userspace wrapper to
//! the wrapper's `rename()` method; all other paths use the libc `__rt_rename`.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - These calls are effectful and must preserve PHP-visible ordering and boolean failure results.
//! - The wrapper split mirrors `readfile()`: a `__rt_path_is_wrapper` probe on
//!   the SOURCE path picks the wrapper branch (`__rt_user_wrapper_rename`, vtable
//!   slot 16) over the libc filesystem branch.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `rename($from, $to)` filesystem function.
///
/// Evaluates the source path first, spills it, then evaluates the destination
/// path and spills it. A `__rt_path_is_wrapper` probe on the source path selects
/// the wrapper branch (`__rt_user_wrapper_rename`) or the libc branch
/// (`__rt_rename`).
///
/// # Arguments
/// - `_name`: Unused; builtin dispatch is handled at the call site.
/// - `args`: Two expressions — the source path and destination path.
/// - `emitter`: Target-aware assembly emitter.
/// - `ctx`: Codegen context (types, locals, class metadata).
/// - `data`: Data section for string literals and constants.
///
/// # Returns
/// Always returns `PhpType::Bool` — PHP's rename returns false on failure, true on success.
///
/// # Implementation notes
/// - String arguments use pointer/length pairs: `x1`/`x2` on AArch64, `rax`/`rdx`
///   on x86_64. Both paths are spilled to a 32-byte scratch frame so the source
///   data survives destination evaluation and the wrapper-scheme probe.
/// - The libc `__rt_rename` takes `from` in `x1`/`x2` and `to` in `x3`/`x4`
///   (AArch64) / `from` in `rax`/`rdx` and `to` in `rdi`/`rsi` (x86_64).
/// - `__rt_user_wrapper_rename` takes `from` then `to` in the SysV-style argument
///   registers (`x0`/`x1`, `x2`/`x3`; `rdi`/`rsi`, `rdx`/`rcx`).
/// - Effectful: observable OS filesystem mutation with PHP-visible ordering.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("rename()");
    emit_expr(&args[0], emitter, ctx, data);
    let wrapper = ctx.next_label("rename_wrapper");
    let after = ctx.next_label("rename_after");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #32");                             // scratch: [sp,#0] from ptr, [sp,#8] from len, [sp,#16] to ptr, [sp,#24] to len
            emitter.instruction("str x1, [sp, #0]");                            // save the source path pointer
            emitter.instruction("str x2, [sp, #8]");                            // save the source path length
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("str x1, [sp, #16]");                           // save the destination path pointer
            emitter.instruction("str x2, [sp, #24]");                           // save the destination path length
            emitter.instruction("ldr x0, [sp, #0]");                            // path_is_wrapper arg0 = source path ptr
            emitter.instruction("ldr x1, [sp, #8]");                            // path_is_wrapper arg1 = source path len
            abi::emit_call_label(emitter, "__rt_path_is_wrapper");              // x0 = 1 when the source scheme matches a registered wrapper
            emitter.instruction(&format!("cbnz x0, {}", wrapper));              // registered wrapper scheme → wrapper rename
            emitter.instruction("ldr x1, [sp, #0]");                            // libc from ptr → x1
            emitter.instruction("ldr x2, [sp, #8]");                            // libc from len → x2
            emitter.instruction("ldr x3, [sp, #16]");                           // libc to ptr → x3
            emitter.instruction("ldr x4, [sp, #24]");                           // libc to len → x4
            abi::emit_call_label(emitter, "__rt_rename");                       // normal path: libc rename(from, to)
            emitter.instruction(&format!("b {}", after));                       // skip the wrapper path
            emitter.label(&wrapper);
            emitter.instruction("ldr x0, [sp, #0]");                            // wrapper from ptr → x0
            emitter.instruction("ldr x1, [sp, #8]");                            // wrapper from len → x1
            emitter.instruction("ldr x2, [sp, #16]");                           // wrapper to ptr → x2
            emitter.instruction("ldr x3, [sp, #24]");                           // wrapper to len → x3
            abi::emit_call_label(emitter, "__rt_user_wrapper_rename");          // dispatch into the wrapper's rename method
            emitter.label(&after);
            emitter.instruction("add sp, sp, #32");                             // release the scratch frame
        }
        Arch::X86_64 => {
            emitter.instruction("sub rsp, 32");                                 // scratch: [rsp+0] from ptr, [rsp+8] from len, [rsp+16] to ptr, [rsp+24] to len
            emitter.instruction("mov QWORD PTR [rsp + 0], rax");                // save the source path pointer
            emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                // save the source path length
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov QWORD PTR [rsp + 16], rax");               // save the destination path pointer
            emitter.instruction("mov QWORD PTR [rsp + 24], rdx");               // save the destination path length
            emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                // path_is_wrapper arg0 = source path ptr
            emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");                // path_is_wrapper arg1 = source path len
            abi::emit_call_label(emitter, "__rt_path_is_wrapper");              // rax = 1 when the source scheme matches a registered wrapper
            emitter.instruction("test rax, rax");                               // matched a registered wrapper scheme?
            emitter.instruction(&format!("jnz {}", wrapper));                   // registered wrapper scheme → wrapper rename
            emitter.instruction("mov rax, QWORD PTR [rsp + 0]");                // libc from ptr → rax
            emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                // libc from len → rdx
            emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");               // libc to ptr → rdi
            emitter.instruction("mov rsi, QWORD PTR [rsp + 24]");               // libc to len → rsi
            abi::emit_call_label(emitter, "__rt_rename");                       // normal path: libc rename(from, to)
            emitter.instruction(&format!("jmp {}", after));                     // skip the wrapper path
            emitter.label(&wrapper);
            emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                // wrapper from ptr → rdi
            emitter.instruction("mov rsi, QWORD PTR [rsp + 8]");                // wrapper from len → rsi
            emitter.instruction("mov rdx, QWORD PTR [rsp + 16]");               // wrapper to ptr → rdx
            emitter.instruction("mov rcx, QWORD PTR [rsp + 24]");               // wrapper to len → rcx
            abi::emit_call_label(emitter, "__rt_user_wrapper_rename");          // dispatch into the wrapper's rename method
            emitter.label(&after);
            emitter.instruction("add rsp, 32");                                 // release the scratch frame
        }
    }
    Some(PhpType::Bool)
}
