//! Purpose:
//! Emits PHP `stream_get_contents` calls.
//! Reads bytes from a stream resource into an elephc string, honoring the
//! optional `$length` (maximum bytes) and `$offset` (seek-before-read) args.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - With no finite `$length`, a normal descriptor delegates to the TLS-aware
//!   `__rt_stream_get_contents` read-all helper, while a synthetic user-wrapper
//!   descriptor (`>= 0x40000000`) is drained by a feof-gated compiled loop
//!   (see `emit_read_all_from_fd`). Checking feof FIRST avoids the corrupting
//!   empty read at EOF that frees the caller's resource cell.
//! - A finite positive `$length` delegates to `__rt_stream_get_contents_bounded`,
//!   which loops through `__rt_fread` until the requested byte count is filled,
//!   EOF is reached, or an empty read is produced. Dynamic `null` / negative
//!   lengths are checked at run time and fall back to the read-all path,
//!   matching PHP's default `-1` contract.
//! - `$offset >= 0` seeks the descriptor before reading (lseek for a normal fd,
//!   the wrapper's `stream_seek` for a synthetic fd); a failed seek boxes PHP
//!   `false`. Successful reads are also boxed so `string|false` keeps one
//!   runtime representation. A literal `null`/negative `$length` means "read to
//!   EOF" and `$offset < 0`/omitted means "do not seek".

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::driver_support::emit_box_current_value_as_mixed;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

const NULL_SENTINEL: i64 = 0x7fff_ffff_ffff_fffe;

/// Returns true when a `$length`/`$offset` argument is a compile-time literal
/// meaning "read to EOF" / "do not seek" — i.e. `null` or a negative integer
/// literal (`-1`, the PHP default; the parser models `-1` as `Negate(IntLiteral)`).
/// Such literals have no side effects, so the caller can skip evaluating them and
/// treat the parameter as absent. Shared with `stream_copy_to_stream`.
pub(super) fn is_read_all_or_no_seek(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Null => true,
        ExprKind::IntLiteral(n) => *n < 0,
        ExprKind::Negate(inner) => matches!(inner.kind, ExprKind::IntLiteral(n) if n > 0),
        _ => false,
    }
}

/// Branches to `target_label` when a runtime length register means "unlimited":
/// PHP `null` (elephc's null sentinel) or a negative integer such as `-1`.
pub(super) fn emit_branch_if_unlimited_length(
    emitter: &mut Emitter,
    length_reg: &str,
    scratch_reg: &str,
    target_label: &str,
) {
    abi::emit_load_int_immediate(emitter, scratch_reg, NULL_SENTINEL);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, {}", length_reg, scratch_reg)); // is the requested length PHP null?
            emitter.instruction(&format!("b.eq {}", target_label));             // null length means read/copy until EOF
            emitter.instruction(&format!("cmp {}, #0", length_reg));            // is the requested length negative?
            emitter.instruction(&format!("b.lt {}", target_label));             // negative length means read/copy until EOF
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, {}", length_reg, scratch_reg)); // is the requested length PHP null?
            emitter.instruction(&format!("je {}", target_label));               // null length means read/copy until EOF
            emitter.instruction(&format!("cmp {}, 0", length_reg));             // is the requested length negative?
            emitter.instruction(&format!("jl {}", target_label));               // negative length means read/copy until EOF
        }
    }
}

/// Emits codegen for PHP `stream_get_contents()` stream and I/O builtin calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_get_contents()");
    emit_stream_fd_arg("stream_get_contents", &args[0], emitter, ctx, data);

    let has_len = args.len() >= 2 && !is_read_all_or_no_seek(&args[1]);
    let has_off = args.len() >= 3 && !is_read_all_or_no_seek(&args[2]);

    if !has_len && !has_off {
        // Fast path: read every remaining byte from the current position.
        emit_read_all_from_fd(emitter, ctx);
        emit_box_current_value_as_mixed(emitter, &PhpType::Str);
        return Some(PhpType::Mixed);
    }

    // General path: stash the fd, evaluate $length then $offset (PHP source
    // order), optionally seek, then read. The 32-byte frame stays 16-aligned
    // so the x86_64 `call lseek` below lands on an aligned stack.
    let skip_seek = ctx.next_label("sgc_skip_seek");
    let wrap_seek = ctx.next_label("sgc_wrap_seek");
    let seek_failed = ctx.next_label("sgc_seek_failed");
    let read_all = ctx.next_label("sgc_read_all");
    let done = ctx.next_label("sgc_general_done");
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("sub sp, sp, #32"),                // frame: [sp,#0]=fd, [sp,#8]=max_len (16-aligned)
        Arch::X86_64 => emitter.instruction("sub rsp, 32"),                     // frame: [rsp+0]=fd, [rsp+8]=max_len (16-aligned)
    }
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("str x0, [sp, #0]"),               // save the stream fd
        Arch::X86_64 => emitter.instruction("mov QWORD PTR [rsp + 0], rax"),    // save the stream fd
    }
    if has_len {
        emit_expr(&args[1], emitter, ctx, data); // evaluate $length first (source order)
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction("str x0, [sp, #8]"),           // save the requested max byte count
            Arch::X86_64 => emitter.instruction("mov QWORD PTR [rsp + 8], rax"), // save the requested max byte count
        }
    }
    if has_off {
        emit_expr(&args[2], emitter, ctx, data); // evaluate $offset after $length
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("cmp x0, #0");                              // a negative offset means "do not seek"
                emitter.instruction(&format!("b.lt {}", skip_seek));            // skip the seek on a negative offset
                emitter.instruction("mov x1, x0");                              // offset → seek arg1
                emitter.instruction("mov x2, #0");                              // whence = SEEK_SET
                emitter.instruction("ldr x0, [sp, #0]");                        // reload the fd → seek arg0
                emitter.instruction("mov w9, #0x4000");                         // high half of USER_WRAPPER_FD_BASE
                emitter.instruction("lsl w9, w9, #16");                         // form 0x40000000
                emitter.instruction("cmp x0, x9");                              // synthetic user-wrapper fd?
                emitter.instruction(&format!("b.ge {}", wrap_seek));            // wrapper: dispatch stream_seek
                emitter.syscall(199);                                           // lseek(fd, offset, SEEK_SET)
                if emitter.platform.needs_cmp_before_error_branch() {
                    emitter.instruction("cmp x0, #0");                          // Linux reports lseek failure as a negative result
                }
                emitter.instruction(&emitter.platform.branch_on_syscall_success(&skip_seek)); // continue only when lseek succeeded
                emitter.instruction(&format!("b {}", seek_failed));             // seek failure makes stream_get_contents() return false
                emitter.label(&wrap_seek);
                abi::emit_call_label(emitter, "__rt_user_wrapper_fseek");       // wrapper stream_seek(offset, SEEK_SET)
                emitter.instruction("cmp x0, #0");                              // did the wrapper stream_seek report success?
                emitter.instruction(&format!("b.ne {}", seek_failed));          // wrapper seek failure returns PHP false
                emitter.label(&skip_seek);
            }
            Arch::X86_64 => {
                emitter.instruction("cmp rax, 0");                              // a negative offset means "do not seek"
                emitter.instruction(&format!("jl {}", skip_seek));              // skip the seek on a negative offset
                emitter.instruction("mov rsi, rax");                            // offset → seek arg1
                emitter.instruction("mov rdx, 0");                              // whence = SEEK_SET
                emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");            // reload the fd → seek arg0
                emitter.instruction("mov r9d, 0x40000000");                     // USER_WRAPPER_FD_BASE
                emitter.instruction("cmp rdi, r9");                             // synthetic user-wrapper fd?
                emitter.instruction(&format!("jge {}", wrap_seek));             // wrapper: dispatch stream_seek
                emitter.instruction("call lseek");                              // lseek(fd, offset, SEEK_SET)
                emitter.instruction("cmp rax, 0");                              // did libc lseek return a non-negative offset?
                emitter.instruction(&format!("jl {}", seek_failed));            // seek failure makes stream_get_contents() return false
                emitter.instruction(&format!("jmp {}", skip_seek));             // normal fd seeked successfully
                emitter.label(&wrap_seek);
                abi::emit_call_label(emitter, "__rt_user_wrapper_fseek");       // wrapper stream_seek(offset, SEEK_SET)
                emitter.instruction("cmp rax, 0");                              // did the wrapper stream_seek report success?
                emitter.instruction(&format!("jne {}", seek_failed));           // wrapper seek failure returns PHP false
                emitter.label(&skip_seek);
            }
        }
    }
    if has_len {
        // Positive finite length: fill up to $length bytes. Dynamic null/negative
        // lengths take the read-all path below, matching PHP's default `-1`.
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("ldr x9, [sp, #8]");                        // reload max_len for the runtime unlimited check
                emit_branch_if_unlimited_length(emitter, "x9", "x10", &read_all);
                emitter.instruction("ldr x0, [sp, #0]");                        // reload the fd for the bounded read
                emitter.instruction("mov x1, x9");                              // finite byte count for the bounded read
                emitter.instruction("add sp, sp, #32");                         // release the argument-evaluation frame
                abi::emit_call_label(emitter, "__rt_stream_get_contents_bounded"); // loop through fread until the cap is filled or EOF
                emit_box_current_value_as_mixed(emitter, &PhpType::Str);
            }
            Arch::X86_64 => {
                emitter.instruction("mov r9, QWORD PTR [rsp + 8]");             // reload max_len for the runtime unlimited check
                emit_branch_if_unlimited_length(emitter, "r9", "r10", &read_all);
                emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // reload the fd for the bounded read
                emitter.instruction("mov rdi, rax");                            // fd argument for the bounded helper
                emitter.instruction("mov rsi, r9");                             // finite byte count for the bounded read
                emitter.instruction("add rsp, 32");                             // release the argument-evaluation frame
                abi::emit_call_label(emitter, "__rt_stream_get_contents_bounded"); // loop through fread until the cap is filled or EOF
                emit_box_current_value_as_mixed(emitter, &PhpType::Str);
            }
        }
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction(&format!("b {}", done)),       // bounded positive length is complete
            Arch::X86_64 => emitter.instruction(&format!("jmp {}", done)),      // bounded positive length is complete
        }
    } else {
        // $offset only: reload the fd and read every remaining byte.
        emitter.label(&read_all);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("ldr x0, [sp, #0]");                        // reload the fd for the read-all path
                emitter.instruction("add sp, sp, #32");                         // release the frame
            }
            Arch::X86_64 => {
                emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // reload the fd for the read-all path
                emitter.instruction("add rsp, 32");                             // release the frame
            }
        }
        emit_read_all_from_fd(emitter, ctx);
        emit_box_current_value_as_mixed(emitter, &PhpType::Str);
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction(&format!("b {}", done)),       // successful read skips the seek-failure boxing path
            Arch::X86_64 => emitter.instruction(&format!("jmp {}", done)),      // successful read skips the seek-failure boxing path
        }
    }
    if has_len {
        emitter.label(&read_all);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("ldr x0, [sp, #0]");                        // reload the fd for unlimited-length reads
                emitter.instruction("add sp, sp, #32");                         // release the frame before the read-all path
            }
            Arch::X86_64 => {
                emitter.instruction("mov rax, QWORD PTR [rsp + 0]");            // reload the fd for unlimited-length reads
                emitter.instruction("add rsp, 32");                             // release the frame before the read-all path
            }
        }
        emit_read_all_from_fd(emitter, ctx);
        emit_box_current_value_as_mixed(emitter, &PhpType::Str);
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction(&format!("b {}", done)),       // successful read skips the seek-failure boxing path
            Arch::X86_64 => emitter.instruction(&format!("jmp {}", done)),      // successful read skips the seek-failure boxing path
        }
    }
    emitter.label(&seek_failed);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("add sp, sp, #32");                             // release the argument-evaluation frame after a failed seek
            emitter.instruction("mov x0, #0");                                  // false payload = 0
        }
        Arch::X86_64 => {
            emitter.instruction("add rsp, 32");                                 // release the argument-evaluation frame after a failed seek
            emitter.instruction("xor eax, eax");                                // false payload = 0
        }
    }
    emit_box_current_value_as_mixed(emitter, &PhpType::Bool);
    emitter.label(&done);
    Some(PhpType::Mixed)
}

/// Reads every remaining byte from the descriptor in the int-result register
/// (`x0`/`rax`) into an elephc string, returning the pointer/length in the
/// standard string registers (`x1`/`x2` on AArch64, `rax`/`rdx` on x86_64).
///
/// A normal fd delegates to the TLS-aware `__rt_stream_get_contents` read-all
/// loop. A synthetic user-wrapper fd (`>= 0x40000000`) is drained by a
/// **feof-gated** compiled loop: each iteration checks `__rt_feof` first and
/// stops at EOF, then `__rt_fread`s one chunk and copies it into
/// `_user_wrapper_drain_buf`. Checking feof first mirrors the only safe drain
/// form (`while(!feof($f)) $b .= fread($f,N)`); a read-then-check-empty loop
/// forces an extra read at EOF whose empty `substr` result frees the caller's
/// resource cell. Each owned chunk is released via `__rt_decref_any`.
fn emit_read_all_from_fd(emitter: &mut Emitter, ctx: &mut Context) {
    let wrapper_label = ctx.next_label("sgc_wrapper");
    let loop_label = ctx.next_label("sgc_wrap_loop");
    let copy_label = ctx.next_label("sgc_wrap_copy");
    let release_label = ctx.next_label("sgc_wrap_release");
    let release_eof_label = ctx.next_label("sgc_wrap_release_eof");
    let wdone_label = ctx.next_label("sgc_wrap_done");
    let done_label = ctx.next_label("sgc_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov w9, #0x4000");                             // high half of USER_WRAPPER_FD_BASE
            emitter.instruction("lsl w9, w9, #16");                             // form 0x40000000 in w9
            emitter.instruction("cmp x0, x9");                                  // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("b.ge {}", wrapper_label));            // wrappers drain via the feof-gated fread loop below
            abi::emit_call_label(emitter, "__rt_stream_get_contents");          // normal fd: TLS-aware read-all helper (x1=ptr, x2=len)
            emitter.instruction(&format!("b {}", done_label));                  // skip the wrapper loop on the normal path

            emitter.label(&wrapper_label);
            emitter.instruction("sub sp, sp, #16");                             // scratch: [sp,#0]=fd, [sp,#8]=accumulated total
            emitter.instruction("str x0, [sp, #0]");                            // save the synthetic wrapper fd
            emitter.instruction("str xzr, [sp, #8]");                           // accumulated byte total = 0
            emitter.label(&loop_label);
            emitter.instruction("ldr x0, [sp, #0]");                            // reload the wrapper fd
            abi::emit_call_label(emitter, "__rt_feof");                         // check the wrapper's stream_eof FIRST (x0 = 1 at EOF)
            emitter.instruction(&format!("cbnz x0, {}", wdone_label));          // at EOF: stop WITHOUT reading (avoids the corrupting empty read)
            emitter.instruction("ldr x0, [sp, #0]");                            // reload the wrapper fd
            emitter.instruction("mov x1, #4096");                               // request up to 4096 bytes
            abi::emit_call_label(emitter, "__rt_fread");                        // compiled-context fread → x1=chunk ptr, x2=len
            emitter.instruction(&format!("cbz x2, {}", release_eof_label));     // defensive: empty read also stops
            emitter.instruction("ldr x9, [sp, #8]");                            // current accumulated total
            emitter.instruction("movz x10, #0x10, lsl #16");                    // drain buffer capacity = 1 MiB
            emitter.instruction("subs x10, x10, x9");                           // remaining capacity
            emitter.instruction(&format!("b.le {}", release_eof_label));        // buffer full: release the chunk, then finish
            emitter.instruction("cmp x2, x10");                                 // does this chunk exceed the remaining capacity?
            emitter.instruction("csel x2, x2, x10, ls");                        // clamp the chunk to the remaining capacity
            abi::emit_symbol_address(emitter, "x11", "_user_wrapper_drain_buf");
            emitter.instruction("add x11, x11, x9");                            // destination = drain buffer + total
            emitter.instruction("mov x12, #0");                                 // byte-copy index
            emitter.label(&copy_label);
            emitter.instruction("ldrb w13, [x1, x12]");                         // load the next source byte
            emitter.instruction("strb w13, [x11, x12]");                        // store it into the drain buffer
            emitter.instruction("add x12, x12, #1");                            // advance the copy index
            emitter.instruction("cmp x12, x2");                                 // copied the whole chunk yet?
            emitter.instruction(&format!("b.lt {}", copy_label));               // keep copying until the chunk is done
            emitter.instruction("ldr x9, [sp, #8]");                            // reload the accumulated total
            emitter.instruction("add x9, x9, x2");                              // add the copied byte count
            emitter.instruction("str x9, [sp, #8]");                            // store the updated total
            emitter.label(&release_label);
            emitter.instruction("mov x0, x1");                                  // the owned wrapper stream_read result
            abi::emit_call_label(emitter, "__rt_decref_any");                   // release it, then loop back to the feof check
            emitter.instruction(&format!("b {}", loop_label));                  // read the next chunk
            emitter.label(&release_eof_label);
            emitter.instruction("mov x0, x1");                                  // the final (empty/uncopied) owned result
            abi::emit_call_label(emitter, "__rt_decref_any");                   // release it (heap strings freed; non-heap skipped)
            emitter.label(&wdone_label);
            abi::emit_symbol_address(emitter, "x1", "_user_wrapper_drain_buf"); // result string pointer
            emitter.instruction("ldr x2, [sp, #8]");                            // result length = accumulated total
            emitter.instruction("add sp, sp, #16");                             // release the scratch frame
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            emitter.instruction("mov r9d, 0x40000000");                         // USER_WRAPPER_FD_BASE
            emitter.instruction("cmp rax, r9");                                 // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("jge {}", wrapper_label));             // wrappers drain via the feof-gated fread loop below
            emitter.instruction("mov rdi, rax");                                // normal fd: pass the descriptor to the helper
            abi::emit_call_label(emitter, "__rt_stream_get_contents");          // TLS-aware read-all helper (rax=ptr, rdx=len)
            emitter.instruction(&format!("jmp {}", done_label));                // skip the wrapper loop on the normal path

            emitter.label(&wrapper_label);
            emitter.instruction("sub rsp, 16");                                 // scratch: [rsp+0]=fd, [rsp+8]=accumulated total
            emitter.instruction("mov QWORD PTR [rsp + 0], rax");                // save the synthetic wrapper fd
            emitter.instruction("mov QWORD PTR [rsp + 8], 0");                  // accumulated byte total = 0
            emitter.label(&loop_label);
            emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                // reload the wrapper fd
            abi::emit_call_label(emitter, "__rt_feof");                         // check the wrapper's stream_eof FIRST (rax = 1 at EOF)
            emitter.instruction("test rax, rax");                               // at EOF?
            emitter.instruction(&format!("jnz {}", wdone_label));               // at EOF: stop WITHOUT reading (avoids the corrupting empty read)
            emitter.instruction("mov rdi, QWORD PTR [rsp + 0]");                // reload the wrapper fd
            emitter.instruction("mov rsi, 4096");                               // request up to 4096 bytes
            abi::emit_call_label(emitter, "__rt_fread");                        // compiled-context fread → rax=chunk ptr, rdx=len
            emitter.instruction("test rdx, rdx");                               // zero-length read?
            emitter.instruction(&format!("jz {}", release_eof_label));          // defensive: empty read also stops
            emitter.instruction("mov r8, QWORD PTR [rsp + 8]");                 // current accumulated total
            emitter.instruction("mov r9, 0x100000");                            // drain buffer capacity = 1 MiB
            emitter.instruction("sub r9, r8");                                  // remaining capacity
            emitter.instruction(&format!("jle {}", release_eof_label));         // buffer full: release the chunk, then finish
            emitter.instruction("cmp rdx, r9");                                 // does this chunk exceed the remaining capacity?
            emitter.instruction("cmova rdx, r9");                               // clamp the chunk to the remaining capacity
            emitter.instruction("lea r10, [rip + _user_wrapper_drain_buf]");    // drain buffer base
            emitter.instruction("add r10, r8");                                 // destination = drain buffer + total
            emitter.instruction("xor rcx, rcx");                                // byte-copy index
            emitter.label(&copy_label);
            emitter.instruction("mov r11b, BYTE PTR [rax + rcx]");              // load the next source byte
            emitter.instruction("mov BYTE PTR [r10 + rcx], r11b");              // store it into the drain buffer
            emitter.instruction("inc rcx");                                     // advance the copy index
            emitter.instruction("cmp rcx, rdx");                                // copied the whole chunk yet?
            emitter.instruction(&format!("jl {}", copy_label));                 // keep copying until the chunk is done
            emitter.instruction("mov r8, QWORD PTR [rsp + 8]");                 // reload the accumulated total
            emitter.instruction("add r8, rdx");                                 // add the copied byte count
            emitter.instruction("mov QWORD PTR [rsp + 8], r8");                 // store the updated total
            emitter.label(&release_label);
            abi::emit_call_label(emitter, "__rt_decref_any");                   // release the owned chunk (rax=ptr), then loop
            emitter.instruction(&format!("jmp {}", loop_label));                // read the next chunk
            emitter.label(&release_eof_label);
            abi::emit_call_label(emitter, "__rt_decref_any");                   // release the final (empty/uncopied) result (rax=ptr)
            emitter.label(&wdone_label);
            emitter.instruction("lea rax, [rip + _user_wrapper_drain_buf]");    // result string pointer
            emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                // result length = accumulated total
            emitter.instruction("add rsp, 16");                                 // release the scratch frame
            emitter.label(&done_label);
        }
    }
}
