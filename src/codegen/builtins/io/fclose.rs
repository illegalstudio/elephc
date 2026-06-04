//! Purpose:
//! Emits PHP `fclose` stream builtin calls over runtime file handles.
//! Uses shared stream unboxing before invoking file descriptor runtime helpers.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - Stream resources must be validated and failure results must follow PHP false/null conventions.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

use super::stream_arg::emit_stream_fd_arg;

/// Emits PHP `fclose(stream)` by extracting the file descriptor from the stream
/// resource, closing it via the target syscall/libc, and returning a bool indicating
/// success (true) or failure (false). Consumes the stream resource in `args[0]`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fclose()");
    emit_stream_fd_arg("fclose", &args[0], emitter, ctx, data);
    let success = ctx.next_label("fclose_ok");
    let done = ctx.next_label("fclose_done");
    let user_wrapper_label = ctx.next_label("fclose_user_wrapper");
    let after_dispatch = ctx.next_label("fclose_after_dispatch");
    let phar_label = ctx.next_label("fclose_phar");

    // -- phar:// write stream synthetic fd (exact 0x50000000): finalize the
    //    buffered archive to disk instead of the normal close path. --
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov w9, #0x5000");                             // low half of the phar-write descriptor 0x50000000
            emitter.instruction("lsl w9, w9, #16");                             // form the full 0x50000000 phar-write descriptor
            emitter.instruction("cmp x0, x9");                                  // is this the phar-write synthetic descriptor?
            emitter.instruction(&format!("b.eq {}", phar_label));               // finalize the buffered phar archive to disk
        }
        Arch::X86_64 => {
            emitter.instruction("mov r9d, 0x50000000");                         // the phar-write synthetic descriptor
            emitter.instruction("cmp rax, r9");                                 // is this the phar-write synthetic descriptor?
            emitter.instruction(&format!("je {}", phar_label));                 // finalize the buffered phar archive to disk
        }
    }

    // -- user-wrapper synthetic fd path (Phase 10 step 4) --
    //    fopen() returns descriptors >= 0x40000000 for user-defined wrappers
    //    so the inline _stream_*_filters/_zstream_handles tables (indexed by
    //    fd) and the close syscall do not apply to those handles.
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov w9, #0x4000");                             // load the high half of USER_WRAPPER_FD_BASE = 0x40000000
            emitter.instruction("lsl w9, w9, #16");                             // shift into bits 30..16 to form 0x40000000
            emitter.instruction("cmp x0, x9");                                  // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("b.ge {}", user_wrapper_label));       // branch into the wrapper-aware close helper
        }
        Arch::X86_64 => {
            emitter.instruction("mov r9d, 0x40000000");                         // USER_WRAPPER_FD_BASE
            emitter.instruction("cmp rax, r9");                                 // is this a synthetic user-wrapper fd?
            emitter.instruction(&format!("jge {}", user_wrapper_label));        // branch into the wrapper-aware close helper
        }
    }

    emit_zlib_flush_on_close(emitter, ctx);
    emit_bz2_flush_on_close(emitter, ctx);
    emit_iconv_flush_on_close(emitter, ctx);
    emit_tls_session_teardown(emitter, ctx);
    emit_user_filter_on_close(emitter, ctx);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x9", "_stream_read_filters");
            emitter.instruction("strb wzr, [x9, x0]");                          // drop any read filter so a reused descriptor starts clean
            abi::emit_symbol_address(emitter, "x9", "_stream_write_filters");
            emitter.instruction("strb wzr, [x9, x0]");                          // drop any write filter so a reused descriptor starts clean
            emitter.syscall(6);                                                 // close the requested file descriptor through the platform syscall path
            emitter.instruction("cmp x0, #0");                                  // did the close syscall report success?
            emitter.instruction(&format!("b.eq {}", success));                  // branch to the success result when the close syscall returns zero
            emitter.instruction("mov x0, #0");                                  // return false when the close syscall reports an error
            emitter.instruction(&format!("b {}", done));                        // skip the success result write on the error path
            emitter.label(&success);
            emitter.instruction("mov x0, #1");                                  // return true when the close syscall succeeds
        }
        Arch::X86_64 => {
            emitter.instruction("lea r9, [rip + _stream_read_filters]");        // read-filter table base
            emitter.instruction("mov BYTE PTR [r9 + rax], 0");                  // drop any read filter so a reused descriptor starts clean
            emitter.instruction("lea r9, [rip + _stream_write_filters]");       // write-filter table base
            emitter.instruction("mov BYTE PTR [r9 + rax], 0");                  // drop any write filter so a reused descriptor starts clean
            emitter.instruction("mov rdi, rax");                                // move the file descriptor into the first SysV libc close() argument register
            emitter.instruction("call close");                                  // close the requested file descriptor through libc close()
            emitter.instruction("cmp rax, 0");                                  // did libc close() report success?
            emitter.instruction(&format!("je {}", success));                    // branch to the success result when libc close() returns zero
            emitter.instruction("xor eax, eax");                                // return false when libc close() reports an error
            emitter.instruction(&format!("jmp {}", done));                      // skip the success result write on the error path
            emitter.label(&success);
            emitter.instruction("mov rax, 1");                                  // return true when libc close() succeeds
        }
    }
    emitter.label(&done);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("b {}", after_dispatch)), // skip the user-wrapper close path on the normal-fd success/failure
        Arch::X86_64 => emitter.instruction(&format!("jmp {}", after_dispatch)), // skip the user-wrapper close path on the normal-fd success/failure
    }

    // -- user-wrapper dispatch: call __rt_user_wrapper_fclose with fd --
    emitter.label(&user_wrapper_label);
    match emitter.target.arch {
        Arch::AArch64 => {
            // x0 already holds the synthetic fd; matches the helper's first arg.
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // move the synthetic fd into the first SysV arg register for the wrapper helper
        }
    }
    abi::emit_call_label(emitter, "__rt_user_wrapper_fclose");                  // dispatch into the wrapper's stream_close and free the handle slot

    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("b {}", after_dispatch)), // skip the phar finalize block on the user-wrapper path
        Arch::X86_64 => emitter.instruction(&format!("jmp {}", after_dispatch)), // skip the phar finalize block on the user-wrapper path
    }

    // -- phar:// write finalize: flush the buffered archive to disk --
    emitter.label(&phar_label);
    abi::emit_call_label(emitter, "__rt_phar_write_finalize");

    emitter.label(&after_dispatch);
    Some(PhpType::Bool)
}

/// Calls `onClose()` on any user-filter instance attached to this fd
/// (read or write direction) before the fd is closed, then clears the
/// instances-table slots so a reused descriptor starts clean. The
/// runtime helper `__rt_user_filter_release_fd` carries the same logic
/// for `stream_filter_remove`.
fn emit_user_filter_on_close(emitter: &mut Emitter, _ctx: &mut Context) {
    // The helper takes fd in x0/rdi (its SysV first arg). After
    // emit_stream_fd_arg the fd lives in x0/rax (the standard
    // int-result register), so on x86_64 it must be moved into rdi.
    if matches!(emitter.target.arch, Arch::X86_64) {
        emitter.instruction("mov rdi, rax");                                    // fd → SysV first arg for the helper
    }
    abi::emit_call_label(emitter, "__rt_user_filter_release_fd");
}

/// Closes the TLS session attached to `fd` (if any), sending `close_notify`
/// via `_elephc_tls_close_fn` and zeroing `_tls_sessions[fd]` so the descriptor
/// can be reused for a plain TCP connection. `fd` must already be in the
/// int-result register; the helper is a no-op when no session is attached.
/// Shared by `fclose()` and `stream_socket_enable_crypto($s, false)` (the
/// mid-stream crypto-shutdown path).
pub(super) fn emit_tls_session_teardown(emitter: &mut Emitter, ctx: &mut Context) {
    let skip = ctx.next_label("fclose_tls_skip");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x9", "_tls_sessions");
            emitter.instruction("ldr x10, [x9, x0, lsl #3]");                   // _tls_sessions[fd] handle
            emitter.instruction(&format!("cbz x10, {}", skip));                 // no TLS attached → nothing to close
            abi::emit_push_reg(emitter, "x0");                                  // preserve fd across the close call
            emitter.instruction("mov x0, x10");                                 // handle as the close helper's first arg
            abi::emit_symbol_address(emitter, "x9", "_elephc_tls_close_fn");
            emitter.instruction("ldr x9, [x9]");                                // load runtime value
            emitter.instruction("blr x9");                                      // send close_notify, drop the session
            abi::emit_pop_reg(emitter, "x0");                                   // restore fd
            abi::emit_symbol_address(emitter, "x9", "_tls_sessions");
            emitter.instruction("str xzr, [x9, x0, lsl #3]");                   // clear the slot so the fd is reusable
            emitter.label(&skip);
        }
        Arch::X86_64 => {
            emitter.instruction("lea r9, [rip + _tls_sessions]");               // load runtime data address
            emitter.instruction("mov r10, QWORD PTR [r9 + rax*8]");             // _tls_sessions[fd] handle
            emitter.instruction("test r10, r10");                               // check whether the runtime value is zero
            emitter.instruction(&format!("je {}", skip));                       // no TLS attached → skip
            abi::emit_push_reg(emitter, "rax");                                 // preserve fd across the close call
            emitter.instruction("mov rdi, r10");                                // handle as first arg
            emitter.instruction("mov r9, QWORD PTR [rip + _elephc_tls_close_fn]"); // prepare SysV call argument
            emitter.instruction("call r9");                                     // call selected function pointer
            abi::emit_pop_reg(emitter, "rax");                                  // restore fd
            emitter.instruction("lea r9, [rip + _tls_sessions]");               // load runtime data address
            emitter.instruction("mov QWORD PTR [r9 + rax*8], 0");               // clear the slot
            emitter.label(&skip);
        }
    }
}

/// Flushes a `zlib.deflate` write filter before the descriptor is closed.
/// When `_zstream_handles[fd]` is non-zero the descriptor has an attached
/// deflate stream, so the compressed tail is flushed through the per-program
/// `_zlib_close_fn` helper. The descriptor is preserved across the call so the
/// caller's close logic still runs. Only an indirect call is emitted here — no
/// libz symbol is named, so non-zlib programs stay free of `-lz`.
fn emit_zlib_flush_on_close(emitter: &mut Emitter, ctx: &mut Context) {
    let skip = ctx.next_label("fclose_zlib_skip");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x9", "_zstream_handles");
            emitter.instruction("ldr x10, [x9, x0, lsl #3]");                   // load this descriptor's deflate stream handle
            emitter.instruction(&format!("cbz x10, {}", skip));                 // no zlib filter attached: nothing to flush
            abi::emit_push_reg(emitter, "x0"); // preserve the descriptor across the flush helper
            abi::emit_symbol_address(emitter, "x9", "_zlib_close_fn");
            emitter.instruction("ldr x9, [x9]");                                // load the deflate close helper pointer
            emitter.instruction("blr x9");                                      // flush the compressed tail and end the stream
            abi::emit_pop_reg(emitter, "x0"); // restore the descriptor for the close path
            emitter.label(&skip);
        }
        Arch::X86_64 => {
            emitter.instruction("lea r9, [rip + _zstream_handles]");            // deflate stream handle table base
            emitter.instruction("mov r10, QWORD PTR [r9 + rax*8]");             // load this descriptor's deflate stream handle
            emitter.instruction("test r10, r10");                               // is a zlib deflate filter attached?
            emitter.instruction(&format!("je {}", skip));                       // no zlib filter attached: nothing to flush
            abi::emit_push_reg(emitter, "rax"); // preserve the descriptor across the flush helper
            emitter.instruction("mov rdi, rax");                                // fd argument for the deflate close helper
            emitter.instruction("mov r9, QWORD PTR [rip + _zlib_close_fn]");    // load the deflate close helper pointer
            emitter.instruction("call r9");                                     // flush the compressed tail and end the stream
            abi::emit_pop_reg(emitter, "rax"); // restore the descriptor for the close path
            emitter.label(&skip);
        }
    }
}

/// Closes a `convert.iconv` WRITE filter before the descriptor is closed. When
/// `_iconv_handles[fd]` is non-zero the descriptor has an attached iconv
/// transcoder, so the per-program `_iconv_close_fn` helper `iconv_close`s it and
/// clears the handle. The descriptor is preserved across the call. Only an
/// indirect call is emitted here — no iconv symbol is named, so non-iconv
/// programs stay free of the macOS `-liconv` dependency.
fn emit_iconv_flush_on_close(emitter: &mut Emitter, ctx: &mut Context) {
    let skip = ctx.next_label("fclose_iconv_skip");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x9", "_iconv_handles");
            emitter.instruction("ldr x10, [x9, x0, lsl #3]");                   // load this descriptor's iconv transcoder handle
            emitter.instruction(&format!("cbz x10, {}", skip));                 // no iconv write filter attached: nothing to close
            abi::emit_push_reg(emitter, "x0"); // preserve the descriptor across the close helper
            abi::emit_symbol_address(emitter, "x9", "_iconv_close_fn");
            emitter.instruction("ldr x9, [x9]");                                // load the iconv close helper pointer
            emitter.instruction("blr x9");                                      // iconv_close the descriptor and clear the handle
            abi::emit_pop_reg(emitter, "x0"); // restore the descriptor for the close path
            emitter.label(&skip);
        }
        Arch::X86_64 => {
            emitter.instruction("lea r9, [rip + _iconv_handles]");              // iconv transcoder handle table base
            emitter.instruction("mov r10, QWORD PTR [r9 + rax*8]");             // load this descriptor's iconv transcoder handle
            emitter.instruction("test r10, r10");                               // is an iconv write filter attached?
            emitter.instruction(&format!("je {}", skip));                       // no iconv write filter attached: nothing to close
            abi::emit_push_reg(emitter, "rax"); // preserve the descriptor across the close helper
            emitter.instruction("mov rdi, rax");                                // fd argument for the iconv close helper
            emitter.instruction("mov r9, QWORD PTR [rip + _iconv_close_fn]");   // load the iconv close helper pointer
            emitter.instruction("call r9");                                     // iconv_close the descriptor and clear the handle
            abi::emit_pop_reg(emitter, "rax"); // restore the descriptor for the close path
            emitter.label(&skip);
        }
    }
}

/// Flushes a `bzip2.compress` write filter before the descriptor is closed.
/// When `_bzstream_handles[fd]` is non-zero the descriptor has an attached
/// bzip2 compress stream, so the compressed tail is flushed through the
/// per-program `_bz2_close_fn` helper. The descriptor is preserved across the
/// call so the caller's close logic still runs. Only an indirect call is
/// emitted here — no libbz2 symbol is named, so non-bzip2 programs stay free of
/// `-lbz2`.
fn emit_bz2_flush_on_close(emitter: &mut Emitter, ctx: &mut Context) {
    let skip = ctx.next_label("fclose_bz2_skip");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x9", "_bzstream_handles");
            emitter.instruction("ldr x10, [x9, x0, lsl #3]");                   // load this descriptor's bzip2 stream handle
            emitter.instruction(&format!("cbz x10, {}", skip));                 // no bzip2 filter attached: nothing to flush
            abi::emit_push_reg(emitter, "x0"); // preserve the descriptor across the flush helper
            abi::emit_symbol_address(emitter, "x9", "_bz2_close_fn");
            emitter.instruction("ldr x9, [x9]");                                // load the bzip2 close helper pointer
            emitter.instruction("blr x9");                                      // flush the compressed tail and end the stream
            abi::emit_pop_reg(emitter, "x0"); // restore the descriptor for the close path
            emitter.label(&skip);
        }
        Arch::X86_64 => {
            emitter.instruction("lea r9, [rip + _bzstream_handles]");           // bzip2 stream handle table base
            emitter.instruction("mov r10, QWORD PTR [r9 + rax*8]");             // load this descriptor's bzip2 stream handle
            emitter.instruction("test r10, r10");                               // is a bzip2 compress filter attached?
            emitter.instruction(&format!("je {}", skip));                       // no bzip2 filter attached: nothing to flush
            abi::emit_push_reg(emitter, "rax"); // preserve the descriptor across the flush helper
            emitter.instruction("mov rdi, rax");                                // fd argument for the bzip2 close helper
            emitter.instruction("mov r9, QWORD PTR [rip + _bz2_close_fn]");     // load the bzip2 close helper pointer
            emitter.instruction("call r9");                                     // flush the compressed tail and end the stream
            abi::emit_pop_reg(emitter, "rax"); // restore the descriptor for the close path
            emitter.label(&skip);
        }
    }
}
