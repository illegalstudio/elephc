//! Purpose:
//! Emits `__rt_stream_write_chain_close_flush`, PHP's closing flush for a stream's WRITE filter
//! chain: the last `filter($in, $out, &$consumed, $closing = true)` call every attached filter is
//! owed before the descriptor goes away, plus the write of whatever that call finally emits.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`.
//! - `lower_fclose` in `crate::codegen::lower_inst::builtins::io::stream_file_ops`, BEFORE
//!   `__rt_stream_close_filter_chains` detaches the chains and while the descriptor is still open.
//!
//! Key details:
//! - A filter is allowed to ACCUMULATE: answering `PSFS_FEED_ME` keeps the bytes and emits
//!   nothing. php then gives it one last dispatch with `$closing = true`, and that dispatch is the
//!   only chance those bytes have to reach the stream. Measured with `php -n` (8.5.6): a filter
//!   that buffers every write and appends one bucket at `$closing` leaves the file EMPTY until
//!   `fclose()`, which makes it hold the whole payload.
//! - The flush walks the CHAIN, not the per-descriptor filter tables. A user filter attached with
//!   `stream_filter_append()` lives as a chain node carrying its own `php_user_filter`
//!   (`lower_user_stream_filter_attach_node` runs the attach helper in "node mode", which
//!   deliberately registers nothing in `_user_filter_instances`), so a per-fd lookup finds
//!   NOTHING and the flush silently does nothing — measured.
//! - `_user_filter_closing` is the flag `__rt_user_filter_brigade_invoke` reads to fill the
//!   `$closing` parameter. The read path already raises it at end of input and
//!   `stream_filter_remove()` raises it for its own flush; the close path never did, so a
//!   buffering write filter's bytes were dropped on the floor. It is raised around exactly one
//!   chain walk and lowered immediately, so an ordinary write still sees `$closing = false`.
//! - The chain is walked with an EMPTY input, exactly as `__rt_filter_node_closing_flush` does for
//!   `stream_filter_remove()`. Feeding bytes here would append them a second time.
//! - The flushed bytes are written with the raw descriptor write, NOT through `__rt_fwrite`:
//!   `__rt_fwrite` would run the same write chain again and re-enter the dispatch.
//! - Synthetic descriptors (user wrappers at `0x40000000`, phar writes at `0x50000000`) are not
//!   real file descriptors, so the write is skipped for them; their own close paths own the bytes.

use crate::codegen_support::runtime::resources::layout::STREAM_WRITE_FILTER_HEAD_OFFSET;
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// The first synthetic descriptor value: user-wrapper streams start here, phar writes above.
///
/// Everything below is a real descriptor the raw write syscall can reach.
const SYNTHETIC_FD_BASE: i64 = 0x4000_0000;

/// Emits `__rt_stream_write_chain_close_flush(handle)`: run and write PHP's closing flush.
///
/// Input: AArch64 `x0` = stream handle; x86_64 `rdi` = stream handle.
/// Output: none. The caller keeps its own copy of the handle.
/// Does nothing when the stream has no write chain, when the closing walk emits no bytes, or when
/// the descriptor is synthetic.
pub fn emit_stream_write_chain_close_flush(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_write_chain_close_flush_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: closing flush for a stream's write filter chain ---");
    emitter.label_global("__rt_stream_write_chain_close_flush");
    emitter.instruction("sub sp, sp, #32");                                     // frame holding the handle and its descriptor
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the helper frame pointer
    emitter.instruction("cbz x0, __rt_swccf_done");                             // a null handle owns no chain
    emitter.instruction("str x0, [sp, #0]");                                    // the chain walk needs the handle, not the descriptor

    // -- an unfiltered stream is the common case and must cost nothing --
    emitter.instruction("bl __rt_stream_state");                                // x0 = the owning state, zero for a raw descriptor
    emitter.instruction("cbz x0, __rt_swccf_done");                             // no state: no chain either
    emitter.instruction(&format!(
        "ldr x9, [x0, #{STREAM_WRITE_FILTER_HEAD_OFFSET}]"
    ));                                                                         // the head of the write chain
    emitter.instruction("cbz x9, __rt_swccf_done");                             // nothing attached on the write side

    // -- resolve the descriptor before the walk: the dispatch clobbers everything --
    emitter.instruction("ldr x0, [sp, #0]");                                    // the handle
    emitter.instruction("bl __rt_stream_fd");                                   // x0 = the backend descriptor
    emitter.instruction("str x0, [sp, #8]");                                    // the flushed bytes are written back here

    // -- raise $closing for exactly one chain walk --
    abi::emit_symbol_address(emitter, "x9", "_user_filter_closing");
    emitter.instruction("mov x10, #1");
    emitter.instruction("str x10, [x9]");                                       // this dispatch is the closing one
    emitter.instruction("ldr x0, [sp, #0]");                                    // the handle the chain walk resolves
    abi::emit_symbol_address(emitter, "x1", "_stream_filter_buf");              // an empty input: the flush feeds no bytes
    emitter.instruction("mov x2, #0");                                          // length 0
    emitter.instruction(&format!("mov x3, #{STREAM_WRITE_FILTER_HEAD_OFFSET}")); // select the write chain
    emitter.instruction("bl __rt_stream_apply_filter_chain");                   // x1/x2 = the bytes the chain finally emits
    abi::emit_symbol_address(emitter, "x9", "_user_filter_closing");
    emitter.instruction("str xzr, [x9]");                                       // lower the flag again immediately

    // -- write whatever the closing walk finally emitted --
    emitter.instruction("cbz x2, __rt_swccf_done");                             // PSFS_FEED_ME and an empty flush both write nothing
    emitter.instruction("ldr x0, [sp, #8]");                                    // the descriptor, still open at this point
    emitter.instruction("cmp x0, #0");
    emitter.instruction("b.lt __rt_swccf_done");                                // a failed open left no descriptor to write to
    emitter.instruction("mov w9, #0x4000");                                     // high half of the first synthetic descriptor
    emitter.instruction("lsl w9, w9, #16");                                     // form the synthetic descriptor base
    emitter.instruction("cmp x0, x9");
    emitter.instruction("b.ge __rt_swccf_done");                                // synthetic descriptors own their own close path
    emitter.syscall(4);                                                         // write(fd, ptr, len); a short write is the caller's own risk

    emitter.label("__rt_swccf_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to the close path
}

/// Emits the Linux x86_64 variant of [`emit_stream_write_chain_close_flush`].
fn emit_stream_write_chain_close_flush_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: closing flush for a stream's write filter chain ---");
    emitter.label_global("__rt_stream_write_chain_close_flush");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the helper frame
    emitter.instruction("sub rsp, 48");                                         // slots for the handle, its descriptor, and the flushed pair
    emitter.instruction("test rdi, rdi");
    emitter.instruction("jz __rt_swccf_done_x");                                // a null handle owns no chain
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // the chain walk needs the handle, not the descriptor

    // -- an unfiltered stream is the common case and must cost nothing --
    emitter.instruction("call __rt_stream_state");                              // rax = the owning state, zero for a raw descriptor
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_swccf_done_x");                                // no state: no chain either
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rax + {STREAM_WRITE_FILTER_HEAD_OFFSET}]"
    ));                                                                         // the head of the write chain
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_swccf_done_x");                                // nothing attached on the write side

    // -- resolve the descriptor before the walk: the dispatch clobbers everything --
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the handle
    emitter.instruction("call __rt_stream_fd");                                 // rax = the backend descriptor
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // the flushed bytes are written back here

    // -- raise $closing for exactly one chain walk --
    abi::emit_symbol_address(emitter, "r10", "_user_filter_closing");
    emitter.instruction("mov QWORD PTR [r10], 1");                              // this dispatch is the closing one
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the handle the chain walk resolves
    abi::emit_symbol_address(emitter, "rax", "_stream_filter_buf");             // an empty input: the flush feeds no bytes
    emitter.instruction("xor edx, edx");                                        // length 0
    emitter.instruction(&format!("mov rsi, {STREAM_WRITE_FILTER_HEAD_OFFSET}")); // select the write chain
    emitter.instruction("call __rt_stream_apply_filter_chain");                 // rax/rdx = the bytes the chain finally emits
    // The pair goes to the frame, not to r14/r15: those are callee-saved under SysV and this
    // helper never spilled them, so borrowing them would corrupt the fclose lowering's own state.
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the flushed payload pointer
    emitter.instruction("mov QWORD PTR [rbp - 32], rdx");                       // preserve the flushed payload length
    abi::emit_symbol_address(emitter, "r10", "_user_filter_closing");
    emitter.instruction("mov QWORD PTR [r10], 0");                              // lower the flag again immediately

    // -- write whatever the closing walk finally emitted --
    emitter.instruction("mov rdx, QWORD PTR [rbp - 32]");                       // the flushed payload length
    emitter.instruction("test rdx, rdx");
    emitter.instruction("jz __rt_swccf_done_x");                                // PSFS_FEED_ME and an empty flush both write nothing
    emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                       // the descriptor, still open at this point
    emitter.instruction("cmp rdi, 0");
    emitter.instruction("jl __rt_swccf_done_x");                                // a failed open left no descriptor to write to
    emitter.instruction(&format!("mov r10, {SYNTHETIC_FD_BASE}"));              // the first synthetic descriptor value
    emitter.instruction("cmp rdi, r10");
    emitter.instruction("jge __rt_swccf_done_x");                               // synthetic descriptors own their own close path
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // the flushed payload pointer
    emitter.instruction("call write");                                          // write(fd, ptr, len) through libc

    emitter.label("__rt_swccf_done_x");
    emitter.instruction("mov rsp, rbp");                                        // release the frame from rbp so its size lives in one place
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to the close path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Arch, Platform, Target};

    /// Verifies the ARM64 closing flush raises `_user_filter_closing` around exactly one write
    /// chain walk, skips unfiltered streams, and balances its frame.
    #[test]
    fn test_stream_write_chain_close_flush_arm64_shape() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_stream_write_chain_close_flush(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("__rt_stream_write_chain_close_flush:\n"));
        assert!(asm.contains(&format!(
            "ldr x9, [x0, #{STREAM_WRITE_FILTER_HEAD_OFFSET}]"
        )));
        assert!(asm.contains("bl __rt_stream_apply_filter_chain"));
        assert!(asm.contains("cbz x2, __rt_swccf_done"));
        // the flag is raised once and lowered once
        assert_eq!(asm.matches("str x10, [x9]").count(), 1);
        assert_eq!(asm.matches("str xzr, [x9]").count(), 1);
        assert_eq!(asm.matches("sub sp, sp, #32").count(), 1);
        assert_eq!(asm.matches("add sp, sp, #32").count(), 1);
    }

    /// Verifies the x86_64 closing flush selects the write chain, preserves the flushed pair
    /// across the flag reset, and writes it through libc `write`.
    #[test]
    fn test_stream_write_chain_close_flush_x86_64_shape() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_stream_write_chain_close_flush(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("__rt_stream_write_chain_close_flush:\n"));
        assert!(asm.contains(&format!("mov rsi, {STREAM_WRITE_FILTER_HEAD_OFFSET}")));
        assert!(asm.contains("call __rt_stream_apply_filter_chain"));
        // the flushed pair is spilled to the frame, never to callee-saved r14/r15
        assert!(asm.contains("mov QWORD PTR [rbp - 24], rax"));
        assert!(asm.contains("mov QWORD PTR [rbp - 32], rdx"));
        assert!(!asm.contains("r14"));
        assert!(!asm.contains("r15"));
        assert!(asm.contains("mov QWORD PTR [r10], 0"));
        assert!(asm.contains("call write"));
    }
}
