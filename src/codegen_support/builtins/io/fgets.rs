//! Purpose:
//! Emits PHP `fgets` stream builtin calls over runtime file handles.
//! Uses shared stream unboxing before invoking file descriptor runtime helpers.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - Stream resources must be validated and failure results must follow PHP false/null conventions.
//! - Normal descriptors delegate to `__rt_fgets`. Synthetic user-wrapper
//!   descriptors (`>= 0x40000000`) read one line through a COMPILED, feof-gated
//!   loop emitted here: it checks `stream_eof` before each 1-byte `stream_read`,
//!   so it never makes the EOF read whose empty `substr` result corrupts the
//!   caller's resource cell (see `stream_get_contents`). Bytes accumulate into
//!   `_user_wrapper_drain_buf`; the line ends at `\n` (kept) or EOF. The boxed
//!   result copies the bytes out via `__rt_str_persist`, so reusing the shared
//!   buffer is safe.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

/// Emits a call to the `fgets` builtin.
///
/// Unboxes the stream resource in `args[0]` to extract a raw file descriptor,
/// then reads one line. Normal descriptors invoke `__rt_fgets`; synthetic
/// user-wrapper descriptors run the feof-gated byte loop emitted below. Both
/// paths converge on `(ptr, len)` and the shared false/string boxing.
///
/// # Arguments
/// * `args[0]` — must be a valid stream resource; validated by `emit_stream_fd_arg`.
/// * `emitter` — target-aware instruction emitter.
/// * `ctx` — codegen context carrying stream/FD metadata.
///
/// # Returns
/// `Some(PhpType::Mixed)` — a boxed string on success, boxed `false` at EOF.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fgets()");
    emit_stream_fd_arg("fgets", &args[0], emitter, ctx, data);
    let wrapper_label = ctx.next_label("fgets_wrapper");
    let box_label = ctx.next_label("fgets_box");
    let wloop_label = ctx.next_label("fgets_wrap_loop");
    let wlast_label = ctx.next_label("fgets_wrap_last");
    let wrelease_label = ctx.next_label("fgets_wrap_release");
    let wdone_label = ctx.next_label("fgets_wrap_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov w9, #0x4000");                             // high half of USER_WRAPPER_FD_BASE
            emitter.instruction("lsl w9, w9, #16");                             // form 0x40000000 in w9
            emitter.instruction("cmp x0, x9");                                  // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("b.ge {}", wrapper_label));            // wrappers read the line via the feof-gated loop below
            abi::emit_call_label(emitter, "__rt_fgets");                        // normal fd: runtime helper reads one line (x1=ptr, x2=len)
            emitter.instruction(&format!("b {}", box_label));                   // converge on the shared boxing

            emitter.label(&wrapper_label);
            emitter.instruction("sub sp, sp, #16");                             // scratch: [sp,#0]=fd, [sp,#8]=line length
            emitter.instruction("str x0, [sp, #0]");                            // save the synthetic wrapper fd
            emitter.instruction("str xzr, [sp, #8]");                           // line length = 0
            emitter.label(&wloop_label);
            emitter.instruction("ldr x0, [sp, #0]");                            // reload the wrapper fd
            abi::emit_call_label(emitter, "__rt_feof");                         // check stream_eof FIRST (x0 = 1 at EOF)
            emitter.instruction(&format!("cbnz x0, {}", wdone_label));          // at EOF: return the bytes gathered so far
            emitter.instruction("ldr x0, [sp, #0]");                            // reload the wrapper fd
            emitter.instruction("mov x1, #1");                                  // read exactly one byte
            abi::emit_call_label(emitter, "__rt_fread");                        // x1=chunk ptr, x2=len
            emitter.instruction(&format!("cbz x2, {}", wdone_label));           // defensive: empty read also ends the line
            emitter.instruction("ldrb w13, [x1]");                              // load the read byte
            emitter.instruction("ldr x10, [sp, #8]");                           // current line length
            emitter.instruction("movz x11, #0x10, lsl #16");                    // line buffer capacity = 1 MiB
            emitter.instruction("cmp x10, x11");                                // is the buffer full?
            emitter.instruction(&format!("b.ge {}", wrelease_label));           // full: release the chunk and stop
            abi::emit_symbol_address(emitter, "x12", "_user_wrapper_drain_buf");
            emitter.instruction("strb w13, [x12, x10]");                        // append the byte to the line buffer
            emitter.instruction("add x10, x10, #1");                            // advance the line length
            emitter.instruction("str x10, [sp, #8]");                           // store the updated line length
            emitter.instruction("cmp w13, #10");                                // is the byte a newline?
            emitter.instruction("mov x0, x1");                                  // chunk ptr for release (flags preserved)
            emitter.instruction(&format!("b.eq {}", wlast_label));              // newline: release this chunk, then finish the line
            abi::emit_call_label(emitter, "__rt_decref_any");                   // not newline: release the chunk and keep reading
            emitter.instruction(&format!("b {}", wloop_label));                 // read the next byte
            emitter.label(&wlast_label);
            abi::emit_call_label(emitter, "__rt_decref_any");                   // release the newline chunk
            emitter.instruction(&format!("b {}", wdone_label));                 // line complete
            emitter.label(&wrelease_label);
            emitter.instruction("mov x0, x1");                                  // chunk ptr for release
            abi::emit_call_label(emitter, "__rt_decref_any");                   // release the dropped chunk (buffer full)
            emitter.label(&wdone_label);
            abi::emit_symbol_address(emitter, "x1", "_user_wrapper_drain_buf"); // line pointer
            emitter.instruction("ldr x2, [sp, #8]");                            // line length
            emitter.instruction("add sp, sp, #16");                             // release the scratch frame
            emitter.label(&box_label);
        }
        Arch::X86_64 => {
            emitter.instruction("mov r9d, 0x40000000");                         // USER_WRAPPER_FD_BASE
            emitter.instruction("cmp rax, r9");                                 // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("jge {}", wrapper_label));             // wrappers read the line via the feof-gated loop below
            emitter.instruction("mov rdi, rax");                                // normal fd: pass the descriptor to the helper
            abi::emit_call_label(emitter, "__rt_fgets");                        // runtime helper reads one line (rax=ptr, rdx=len)
            emitter.instruction(&format!("jmp {}", box_label));                 // converge on the shared boxing

            emitter.label(&wrapper_label);
            emitter.instruction("sub rsp, 16");                                 // scratch: [rsp+0]=fd, [rsp+8]=line length
            emitter.instruction("mov QWORD PTR [rsp + 0], rax");                // save the synthetic wrapper fd
            emitter.instruction("mov QWORD PTR [rsp + 8], 0");                  // line length = 0
            emitter.label(&wloop_label);
            emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                // reload the wrapper fd
            abi::emit_call_label(emitter, "__rt_feof");                         // check stream_eof FIRST (rax = 1 at EOF)
            emitter.instruction("test rax, rax");                               // at EOF?
            emitter.instruction(&format!("jnz {}", wdone_label));               // at EOF: return the bytes gathered so far
            emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                // reload the wrapper fd
            emitter.instruction("mov rsi, 1");                                  // read exactly one byte
            abi::emit_call_label(emitter, "__rt_fread");                        // rax=chunk ptr, rdx=len
            emitter.instruction("test rdx, rdx");                               // zero-length read?
            emitter.instruction(&format!("jz {}", wdone_label));                // defensive: empty read also ends the line
            emitter.instruction("movzx r10d, BYTE PTR [rax]");                  // load the read byte
            emitter.instruction("mov r8, QWORD PTR [rsp + 8]");                 // current line length
            emitter.instruction("cmp r8, 0x100000");                            // is the buffer full (1 MiB)?
            emitter.instruction(&format!("jge {}", wrelease_label));            // full: release the chunk and stop
            abi::emit_symbol_address(emitter, "r11", "_user_wrapper_drain_buf"); // line buffer base
            emitter.instruction("mov BYTE PTR [r11 + r8], r10b");               // append the byte to the line buffer
            emitter.instruction("inc r8");                                      // advance the line length
            emitter.instruction("mov QWORD PTR [rsp + 8], r8");                 // store the updated line length
            emitter.instruction("cmp r10b, 10");                                // is the byte a newline? (rax still = chunk ptr)
            emitter.instruction(&format!("je {}", wlast_label));                // newline: release this chunk, then finish the line
            abi::emit_call_label(emitter, "__rt_decref_any");                   // not newline: release the chunk (rax=ptr) and keep reading
            emitter.instruction(&format!("jmp {}", wloop_label));               // read the next byte
            emitter.label(&wlast_label);
            abi::emit_call_label(emitter, "__rt_decref_any");                   // release the newline chunk (rax=ptr)
            emitter.instruction(&format!("jmp {}", wdone_label));               // line complete
            emitter.label(&wrelease_label);
            abi::emit_call_label(emitter, "__rt_decref_any");                   // release the dropped chunk (rax=ptr, buffer full)
            emitter.label(&wdone_label);
            abi::emit_symbol_address(emitter, "rax", "_user_wrapper_drain_buf"); // line pointer
            emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                // line length
            emitter.instruction("add rsp, 16");                                 // release the scratch frame
            emitter.label(&box_label);
        }
    }
    // The (ptr, len) result is shared by both paths. PHP fgets distinguishes EOF
    // (no bytes accumulated) from a successful read: len == 0 means false. Box
    // the result as a Mixed cell so `($l = fgets($f)) !== false` actually
    // terminates when EOF is reached.
    let false_label = ctx.next_label("fgets_false");
    let done_label = ctx.next_label("fgets_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz x2, {}", false_label));           // zero-length read → PHP false
            emitter.instruction("mov x0, #1");                                  // runtime tag 1 = string
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box the line as a Mixed string
            emitter.instruction(&format!("b {}", done_label));                  // continue at target label
            emitter.label(&false_label);
            emitter.instruction("mov x1, #0");                                  // bool payload = 0 (false)
            emitter.instruction("mov x2, #0");                                  // bool mixed payloads have no high word
            emitter.instruction("mov x0, #3");                                  // runtime tag 3 = bool
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box PHP false so `!== false` short-circuits
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("test rdx, rdx");                               // zero-length read → PHP false
            emitter.instruction(&format!("jz {}", false_label));                // branch when the checked value is zero or equal
            emitter.instruction("mov rdi, rax");                                // string ptr → mixed_from_value's payload-lo register
            emitter.instruction("mov rsi, rdx");                                // string len → mixed_from_value's payload-hi register
            emitter.instruction("mov eax, 1");                                  // runtime tag 1 = string
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box the line as a Mixed string
            emitter.instruction(&format!("jmp {}", done_label));                // continue at target label
            emitter.label(&false_label);
            emitter.instruction("xor edi, edi");                                // bool payload = 0 (false)
            emitter.instruction("xor esi, esi");                                // bool mixed payloads have no high word
            emitter.instruction("mov eax, 3");                                  // runtime tag 3 = bool
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // box PHP false so `!== false` short-circuits
            emitter.label(&done_label);
        }
    }
    Some(PhpType::Mixed)
}
