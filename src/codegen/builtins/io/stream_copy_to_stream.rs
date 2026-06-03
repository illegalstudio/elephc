//! Purpose:
//! Emits PHP `stream_copy_to_stream` calls.
//! Copies bytes from one stream resource to another, honoring the optional
//! `$length` (maximum bytes) and `$offset` (seek the source first) arguments.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Both arguments are unboxed to file descriptors; the source is preserved on
//!   the stack while the destination expression is evaluated.
//! - With no `$length`/`$offset`: when EITHER side is a synthetic user-wrapper
//!   descriptor (`>= 0x40000000`) a COMPILED, feof-gated loop reads each chunk
//!   through `__rt_fread` and writes it through `__rt_fwrite` (both dispatch
//!   normal vs wrapper fds); when both sides are real descriptors the efficient
//!   `__rt_stream_copy_to_stream` syscall helper is used.
//! - With a `$length`/`$offset`: a single capped, feof-gated `__rt_fread` /
//!   `__rt_fwrite` loop runs for every fd combination (wrapper-aware). It seeks
//!   the source by `$offset >= 0` first (lseek for a normal fd, the wrapper's
//!   `stream_seek` for a synthetic fd) and stops once `$length` bytes are
//!   copied or the source reaches EOF. A `null`/negative `$length` copies to
//!   EOF; a negative/omitted `$offset` does not seek. feof is checked BEFORE
//!   each read so the loop never makes the EOF read whose empty `substr` result
//!   corrupts the source resource cell (see `stream_get_contents`).

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;
use super::stream_get_contents::is_read_all_or_no_seek;

/// Emits codegen for PHP `stream_copy_to_stream()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_copy_to_stream()");
    emit_stream_fd_arg("stream_copy_to_stream", &args[0], emitter, ctx, data);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter)); // preserve the source descriptor while the destination is evaluated
    emit_stream_fd_arg("stream_copy_to_stream", &args[1], emitter, ctx, data);

    let has_len = args.len() >= 3 && !is_read_all_or_no_seek(&args[2]);
    let has_off = args.len() >= 4 && !is_read_all_or_no_seek(&args[3]);
    if has_len || has_off {
        return emit_bounded_copy(args, has_len, has_off, emitter, ctx, data);
    }

    let wrapper_label = ctx.next_label("scs_wrapper");
    let loop_label = ctx.next_label("scs_loop");
    let release_eof_label = ctx.next_label("scs_release_eof");
    let wdone_label = ctx.next_label("scs_wrap_done");
    let done_label = ctx.next_label("scs_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, x0");                                  // destination descriptor becomes the second helper argument
            abi::emit_pop_reg(emitter, "x0"); // restore the source descriptor into the first helper argument
            emitter.instruction("mov w9, #0x4000");                             // high half of USER_WRAPPER_FD_BASE
            emitter.instruction("lsl w9, w9, #16");                             // form 0x40000000 in w9
            emitter.instruction("cmp x0, x9");                                  // is the source a synthetic user-wrapper fd?
            emitter.instruction(&format!("b.ge {}", wrapper_label));            // wrapper source: use the compiled copy loop
            emitter.instruction("cmp x1, x9");                                  // is the destination a synthetic user-wrapper fd?
            emitter.instruction(&format!("b.ge {}", wrapper_label));            // wrapper destination: use the compiled copy loop
            abi::emit_call_label(emitter, "__rt_stream_copy_to_stream");        // both real fds: efficient syscall copy helper
            emitter.instruction(&format!("b {}", done_label));                  // skip the wrapper loop on the all-real path

            emitter.label(&wrapper_label);
            emitter.instruction("sub sp, sp, #32");                             // scratch: [sp,#0]=src, [sp,#8]=dst, [sp,#16]=total, [sp,#24]=chunk
            emitter.instruction("str x0, [sp, #0]");                            // save the source descriptor
            emitter.instruction("str x1, [sp, #8]");                            // save the destination descriptor
            emitter.instruction("str xzr, [sp, #16]");                          // bytes-copied total = 0
            emitter.label(&loop_label);
            emitter.instruction("ldr x0, [sp, #0]");                            // reload the source descriptor
            abi::emit_call_label(emitter, "__rt_feof");                         // check the source's EOF FIRST (x0 = 1 at EOF)
            emitter.instruction(&format!("cbnz x0, {}", wdone_label));          // at EOF: stop without reading
            emitter.instruction("ldr x0, [sp, #0]");                            // reload the source descriptor
            emitter.instruction("mov x1, #4096");                               // request up to 4096 bytes
            abi::emit_call_label(emitter, "__rt_fread");                        // x1=chunk ptr, x2=len
            emitter.instruction(&format!("cbz x2, {}", release_eof_label));     // defensive: empty read also stops
            emitter.instruction("str x1, [sp, #24]");                           // save the chunk ptr for the later release
            emitter.instruction("ldr x9, [sp, #16]");                           // current total
            emitter.instruction("add x9, x9, x2");                              // add this chunk's length
            emitter.instruction("str x9, [sp, #16]");                           // store the updated total
            emitter.instruction("ldr x0, [sp, #8]");                            // destination fd (x1=ptr, x2=len already in place)
            abi::emit_call_label(emitter, "__rt_fwrite");                       // write the chunk to the destination (dispatches wrapper vs fd)
            emitter.instruction("ldr x0, [sp, #24]");                           // reload the chunk ptr
            abi::emit_call_label(emitter, "__rt_decref_any");                   // release the owned chunk, then loop
            emitter.instruction(&format!("b {}", loop_label));                  // copy the next chunk
            emitter.label(&release_eof_label);
            emitter.instruction("mov x0, x1");                                  // the final (empty) owned chunk
            abi::emit_call_label(emitter, "__rt_decref_any");                   // release it (heap freed; non-heap skipped)
            emitter.label(&wdone_label);
            emitter.instruction("ldr x0, [sp, #16]");                           // return the total bytes copied
            emitter.instruction("add sp, sp, #32");                             // release the scratch frame
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("mov rsi, rax");                                // destination descriptor becomes the second SysV argument
            abi::emit_pop_reg(emitter, "rdi"); // restore the source descriptor into the first SysV argument
            emitter.instruction("mov r9d, 0x40000000");                         // USER_WRAPPER_FD_BASE
            emitter.instruction("cmp rdi, r9");                                 // is the source a synthetic user-wrapper fd?
            emitter.instruction(&format!("jge {}", wrapper_label));             // wrapper source: use the compiled copy loop
            emitter.instruction("cmp rsi, r9");                                 // is the destination a synthetic user-wrapper fd?
            emitter.instruction(&format!("jge {}", wrapper_label));             // wrapper destination: use the compiled copy loop
            abi::emit_call_label(emitter, "__rt_stream_copy_to_stream");        // both real fds: efficient syscall copy helper
            emitter.instruction(&format!("jmp {}", done_label));                // skip the wrapper loop on the all-real path

            emitter.label(&wrapper_label);
            emitter.instruction("sub rsp, 32");                                 // scratch: [rsp+0]=src, [rsp+8]=dst, [rsp+16]=total, [rsp+24]=chunk
            emitter.instruction("mov QWORD PTR [rsp + 0], rdi");                // save the source descriptor
            emitter.instruction("mov QWORD PTR [rsp + 8], rsi");                // save the destination descriptor
            emitter.instruction("mov QWORD PTR [rsp + 16], 0");                 // bytes-copied total = 0
            emitter.label(&loop_label);
            emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                // reload the source descriptor
            abi::emit_call_label(emitter, "__rt_feof");                         // check the source's EOF FIRST (rax = 1 at EOF)
            emitter.instruction("test rax, rax");                               // at EOF?
            emitter.instruction(&format!("jnz {}", wdone_label));               // at EOF: stop without reading
            emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                // reload the source descriptor
            emitter.instruction("mov rsi, 4096");                               // request up to 4096 bytes
            abi::emit_call_label(emitter, "__rt_fread");                        // rax=chunk ptr, rdx=len
            emitter.instruction("test rdx, rdx");                               // zero-length read?
            emitter.instruction(&format!("jz {}", release_eof_label));          // defensive: empty read also stops
            emitter.instruction("mov QWORD PTR [rsp + 24], rax");               // save the chunk ptr for the later release
            emitter.instruction("mov r8, QWORD PTR [rsp + 16]");                // current total
            emitter.instruction("add r8, rdx");                                 // add this chunk's length
            emitter.instruction("mov QWORD PTR [rsp + 16], r8");                // store the updated total
            emitter.instruction("mov rsi, rax");                                // chunk ptr → second fwrite argument
            emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");                // destination fd → first argument (rdx=len already in place)
            abi::emit_call_label(emitter, "__rt_fwrite");                       // write the chunk to the destination (dispatches wrapper vs fd)
            emitter.instruction("mov rax, QWORD PTR [rsp + 24]");               // reload the chunk ptr
            abi::emit_call_label(emitter, "__rt_decref_any");                   // release the owned chunk, then loop
            emitter.instruction(&format!("jmp {}", loop_label));                // copy the next chunk
            emitter.label(&release_eof_label);
            abi::emit_call_label(emitter, "__rt_decref_any");                   // release the final (empty) chunk (rax=ptr)
            emitter.label(&wdone_label);
            emitter.instruction("mov rax, QWORD PTR [rsp + 16]");               // return the total bytes copied
            emitter.instruction("add rsp, 32");                                 // release the scratch frame
            emitter.label(&done_label);
        }
    }
    Some(PhpType::Int)
}

/// Emits the bounded `stream_copy_to_stream($from, $to, $length, $offset)` path:
/// a single capped, feof-gated `__rt_fread`/`__rt_fwrite` loop that works for any
/// real/wrapper fd combination. On entry the source descriptor sits on the stack
/// (pushed by the caller) and the destination is in the int-result register.
/// Evaluates `$length` then `$offset` (PHP source order), seeks the source when
/// `$offset >= 0`, and copies until `$length` bytes are written or the source
/// hits EOF. Returns the byte count in the int-result register.
fn emit_bounded_copy(
    args: &[Expr],
    has_len: bool,
    has_off: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let skip_seek = ctx.next_label("scs_skip_seek");
    let wrap_seek = ctx.next_label("scs_wrap_seek");
    let loop_label = ctx.next_label("scs_b_loop");
    let release_eof_label = ctx.next_label("scs_b_release_eof");
    let done_label = ctx.next_label("scs_b_done");
    // Frame: [0]=src, [8]=dst, [16]=total, [24]=chunk_ptr, [32]=max_len (48 = 0 mod 16).
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, x0");                                  // destination → temp
            abi::emit_pop_reg(emitter, "x0");                                   // restore the source descriptor
            emitter.instruction("sub sp, sp, #48");                             // bounded-copy frame (16-aligned)
            emitter.instruction("str x0, [sp, #0]");                            // save the source descriptor
            emitter.instruction("str x1, [sp, #8]");                            // save the destination descriptor
            emitter.instruction("str xzr, [sp, #16]");                          // bytes-copied total = 0
        }
        Arch::X86_64 => {
            emitter.instruction("mov rsi, rax");                                // destination → temp
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the source descriptor
            emitter.instruction("sub rsp, 48");                                 // bounded-copy frame (16-aligned)
            emitter.instruction("mov QWORD PTR [rsp + 0], rdi");                // save the source descriptor
            emitter.instruction("mov QWORD PTR [rsp + 8], rsi");                // save the destination descriptor
            emitter.instruction("mov QWORD PTR [rsp + 16], 0");                 // bytes-copied total = 0
        }
    }
    // $length → max_len (or -1 when omitted/unlimited).
    if has_len {
        emit_expr(&args[2], emitter, ctx, data); // evaluate $length first (source order)
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("str x0, [sp, #32]"),          // save the requested max byte count
            Arch::X86_64 => emitter.instruction("mov QWORD PTR [rsp + 32], rax"),// save the requested max byte count
        }
    }
    // $offset: seek the source before copying.
    if has_off {
        emit_expr(&args[3], emitter, ctx, data); // evaluate $offset after $length
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("cmp x0, #0");                              // a negative offset means "do not seek"
                emitter.instruction(&format!("b.lt {}", skip_seek));            // skip the seek on a negative offset
                emitter.instruction("mov x1, x0");                              // offset → seek arg1
                emitter.instruction("mov x2, #0");                              // whence = SEEK_SET
                emitter.instruction("ldr x0, [sp, #0]");                        // reload the source fd → seek arg0
                emitter.instruction("mov w9, #0x4000");                         // high half of USER_WRAPPER_FD_BASE
                emitter.instruction("lsl w9, w9, #16");                         // form 0x40000000
                emitter.instruction("cmp x0, x9");                              // synthetic user-wrapper fd?
                emitter.instruction(&format!("b.ge {}", wrap_seek));            // wrapper: dispatch stream_seek
                emitter.syscall(199);                                           // lseek(src, offset, SEEK_SET); result ignored (best effort)
                emitter.instruction(&format!("b {}", skip_seek));               // normal fd seeked
                emitter.label(&wrap_seek);
                abi::emit_call_label(emitter, "__rt_user_wrapper_fseek");       // wrapper stream_seek(offset, SEEK_SET)
                emitter.label(&skip_seek);
            }
            Arch::X86_64 => {
                emitter.instruction("cmp rax, 0");                              // a negative offset means "do not seek"
                emitter.instruction(&format!("jl {}", skip_seek));              // skip the seek on a negative offset
                emitter.instruction("mov rsi, rax");                            // offset → seek arg1
                emitter.instruction("mov rdx, 0");                              // whence = SEEK_SET
                emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // reload the source fd → seek arg0
                emitter.instruction("mov r9d, 0x40000000");                     // USER_WRAPPER_FD_BASE
                emitter.instruction("cmp rdi, r9");                             // synthetic user-wrapper fd?
                emitter.instruction(&format!("jge {}", wrap_seek));             // wrapper: dispatch stream_seek
                emitter.instruction("call lseek");                              // lseek(src, offset, SEEK_SET); result ignored (best effort)
                emitter.instruction(&format!("jmp {}", skip_seek));             // normal fd seeked
                emitter.label(&wrap_seek);
                abi::emit_call_label(emitter, "__rt_user_wrapper_fseek");       // wrapper stream_seek(offset, SEEK_SET)
                emitter.label(&skip_seek);
            }
        }
    }
    // Capped feof-gated copy loop.
    emitter.label(&loop_label);
    match emitter.target.arch {
        Arch::AArch64 => {
            if has_len {
                emitter.instruction("ldr x9, [sp, #16]");                       // bytes copied so far
                emitter.instruction("ldr x10, [sp, #32]");                      // requested max byte count
                emitter.instruction("cmp x9, x10");                             // reached the requested length?
                emitter.instruction(&format!("b.ge {}", done_label));           // stop once $length bytes are copied
            }
            emitter.instruction("ldr x0, [sp, #0]");                            // reload the source descriptor
            abi::emit_call_label(emitter, "__rt_feof");                         // check the source's EOF FIRST (x0 = 1 at EOF)
            emitter.instruction(&format!("cbnz x0, {}", done_label));           // at EOF: stop without reading
            emitter.instruction("ldr x0, [sp, #0]");                            // reload the source descriptor
            emitter.instruction("mov x1, #4096");                               // default request: up to 4096 bytes
            if has_len {
                emitter.instruction("ldr x10, [sp, #32]");                      // max byte count
                emitter.instruction("ldr x9, [sp, #16]");                       // bytes copied so far
                emitter.instruction("sub x10, x10, x9");                        // remaining = max - total (>= 1 here)
                emitter.instruction("cmp x10, x1");                             // is the remainder smaller than 4096?
                emitter.instruction("csel x1, x10, x1, lt");                    // clamp the request to the remainder
            }
            abi::emit_call_label(emitter, "__rt_fread");                        // x1=chunk ptr, x2=len
            emitter.instruction(&format!("cbz x2, {}", release_eof_label));     // defensive: empty read also stops
            emitter.instruction("str x1, [sp, #24]");                           // save the chunk ptr for the later release
            emitter.instruction("ldr x9, [sp, #16]");                           // current total
            emitter.instruction("add x9, x9, x2");                              // add this chunk's length
            emitter.instruction("str x9, [sp, #16]");                           // store the updated total
            emitter.instruction("ldr x0, [sp, #8]");                            // destination fd (x1=ptr, x2=len already in place)
            abi::emit_call_label(emitter, "__rt_fwrite");                       // write the chunk (dispatches wrapper vs fd)
            emitter.instruction("ldr x0, [sp, #24]");                           // reload the chunk ptr
            abi::emit_call_label(emitter, "__rt_decref_any");                   // release the owned chunk, then loop
            emitter.instruction(&format!("b {}", loop_label));                  // copy the next chunk
            emitter.label(&release_eof_label);
            emitter.instruction("mov x0, x1");                                  // the final (empty) owned chunk
            abi::emit_call_label(emitter, "__rt_decref_any");                   // release it (heap freed; non-heap skipped)
            emitter.label(&done_label);
            emitter.instruction("ldr x0, [sp, #16]");                           // return the total bytes copied
            emitter.instruction("add sp, sp, #48");                             // release the bounded-copy frame
        }
        Arch::X86_64 => {
            if has_len {
                emitter.instruction("mov r8, QWORD PTR [rsp + 16]");            // bytes copied so far
                emitter.instruction("mov r9, QWORD PTR [rsp + 32]");            // requested max byte count
                emitter.instruction("cmp r8, r9");                              // reached the requested length?
                emitter.instruction(&format!("jge {}", done_label));            // stop once $length bytes are copied
            }
            emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                // reload the source descriptor
            abi::emit_call_label(emitter, "__rt_feof");                         // check the source's EOF FIRST (rax = 1 at EOF)
            emitter.instruction("test rax, rax");                               // at EOF?
            emitter.instruction(&format!("jnz {}", done_label));                // at EOF: stop without reading
            emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                // reload the source descriptor
            emitter.instruction("mov rsi, 4096");                               // default request: up to 4096 bytes
            if has_len {
                emitter.instruction("mov r9, QWORD PTR [rsp + 32]");            // max byte count
                emitter.instruction("mov r8, QWORD PTR [rsp + 16]");            // bytes copied so far
                emitter.instruction("sub r9, r8");                              // remaining = max - total (>= 1 here)
                emitter.instruction("cmp r9, rsi");                             // is the remainder smaller than 4096?
                emitter.instruction("cmovl rsi, r9");                           // clamp the request to the remainder
            }
            abi::emit_call_label(emitter, "__rt_fread");                        // rax=chunk ptr, rdx=len
            emitter.instruction("test rdx, rdx");                               // zero-length read?
            emitter.instruction(&format!("jz {}", release_eof_label));          // defensive: empty read also stops
            emitter.instruction("mov QWORD PTR [rsp + 24], rax");               // save the chunk ptr for the later release
            emitter.instruction("mov r8, QWORD PTR [rsp + 16]");                // current total
            emitter.instruction("add r8, rdx");                                 // add this chunk's length
            emitter.instruction("mov QWORD PTR [rsp + 16], r8");                // store the updated total
            emitter.instruction("mov rsi, rax");                                // chunk ptr → second fwrite argument
            emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");                // destination fd → first argument (rdx=len already in place)
            abi::emit_call_label(emitter, "__rt_fwrite");                       // write the chunk (dispatches wrapper vs fd)
            emitter.instruction("mov rax, QWORD PTR [rsp + 24]");               // reload the chunk ptr
            abi::emit_call_label(emitter, "__rt_decref_any");                   // release the owned chunk, then loop
            emitter.instruction(&format!("jmp {}", loop_label));                // copy the next chunk
            emitter.label(&release_eof_label);
            abi::emit_call_label(emitter, "__rt_decref_any");                   // release the final (empty) chunk (rax=ptr)
            emitter.label(&done_label);
            emitter.instruction("mov rax, QWORD PTR [rsp + 16]");               // return the total bytes copied
            emitter.instruction("add rsp, 48");                                 // release the bounded-copy frame
        }
    }
    Some(PhpType::Int)
}
