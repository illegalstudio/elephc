//! Purpose:
//! Emits PHP `stream_get_contents` calls.
//! Reads all remaining bytes from a stream resource into an elephc string.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Normal descriptors delegate to the efficient `__rt_stream_get_contents`
//!   runtime helper (syscall loop into `_concat_buf`).
//! - Synthetic user-wrapper descriptors (`>= 0x40000000`) are drained by a
//!   COMPILED, **feof-gated** loop emitted here: each iteration calls
//!   `__rt_feof` first and stops at EOF, then `__rt_fread`s one chunk and
//!   copies it into `_user_wrapper_drain_buf`. Checking feof FIRST is what
//!   makes this safe — it mirrors `while(!feof($f)) $b .= fread($f,N)`, the one
//!   draining form that works. A read-then-check-empty loop instead forces an
//!   extra read AT EOF; the wrapper's `stream_read` then returns an empty
//!   `substr` heap value whose handling frees the caller's resource cell
//!   (a core heap/refcount bug). Each owned chunk is released via
//!   `__rt_decref_any` (range-checked, no-ops on non-heap).

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_get_contents()");
    emit_stream_fd_arg("stream_get_contents", &args[0], emitter, ctx, data);
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
            abi::emit_call_label(emitter, "__rt_stream_get_contents");          // normal fd: efficient syscall-loop helper (x1=ptr, x2=len)
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
            abi::emit_call_label(emitter, "__rt_stream_get_contents");          // efficient syscall-loop helper (rax=ptr, rdx=len)
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
    Some(PhpType::Str)
}
