//! Purpose:
//! Emits `__rt_stream_pending_put` and `__rt_stream_pending_take`, the per-stream holding area for
//! bytes a read consumed but must not hand back yet.
//!
//! Called from:
//! - `crate::codegen_support::runtime::io::stream_get_line`, which fills it and drains it.
//! - `crate::codegen_support::runtime::io::{fread, fgets}`, which drain it before the descriptor.
//!
//! Key details:
//! - php's `stream_get_line()` answers `false` when it finds neither the delimiter nor the length
//!   cap and the stream is not at EOF — and the bytes it read STAY on the stream. Measured on
//!   `php -n` 8.5.6 over a non-blocking socket pair: writing "abc" then calling
//!   `stream_get_line($h, 100, "\n")` answers `false`, and after "def\n" arrives the next call
//!   answers "abcdef". elephc consumed the "abc" and handed it back, so the caller received a line
//!   that php never breaks.
//! - php holds those bytes in the stream's own read buffer, which every read function shares — a
//!   refused `stream_get_line()` is followed by an `fread()` that sees the same "abc". This is that
//!   buffer, narrowed to what makes the difference observable: only `stream_get_line()` fills it,
//!   and the three readers drain it before touching the descriptor. A program that never reaches
//!   the refusal pays one load and a branch per read.
//! - EOF is NOT this case. A blocking file whose last line has no delimiter still answers that
//!   line, because there the loop stopped at a genuine end of input; only the would-block exit
//!   pushes back.

use crate::codegen_support::runtime::resources::layout::{
    STREAM_PENDING_LEN_OFFSET, STREAM_PENDING_POS_OFFSET, STREAM_PENDING_PTR_OFFSET,
    STREAM_URI_LEN_OFFSET, STREAM_URI_PTR_OFFSET, STREAM_WRAPPER_ID_OFFSET,
};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// The `php://` wrapper's StreamState id, whose `temp` sub-wrapper this module singles out.
const WRAPPER_ID_PHP: u64 = 6;

/// The two compress wrappers, which report end of file on the same rule `php://temp` does.
const WRAPPER_ID_COMPRESS_ZLIB: u64 = 8;
const WRAPPER_ID_COMPRESS_BZIP2: u64 = 9;

/// Emits `__rt_stream_pending_put(handle, ptr, len)`, which retains a copy of `len` bytes.
///
/// Replaces whatever was held: the only caller drained the buffer into its own accumulation first,
/// so what it hands back already contains the previous contents.
/// Emits `__rt_stream_pending_append(handle, ptr, len)`: adds to what the holding area holds.
///
/// The put beside it REPLACES, which is right for a caller that has drained the area first. The
/// topping-up in `__rt_fread` has not: php keeps the leftovers and adds after them.
pub fn emit_stream_pending_append(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_append_aarch64(emitter),
        Arch::X86_64 => emit_append_x86_64(emitter),
    }
}

pub fn emit_stream_pending_put(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_put_aarch64(emitter),
        Arch::X86_64 => emit_put_x86_64(emitter),
    }
}

/// Emits `__rt_stream_pending_take(handle, dest, max) -> n`, which moves up to `max` held bytes.
///
/// Advances the read cursor and releases the block once it is drained, so a stream that is not in
/// the middle of a refused `stream_get_line()` holds nothing.
pub fn emit_stream_pending_take(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_take_aarch64(emitter),
        Arch::X86_64 => emit_take_x86_64(emitter),
    }
}

/// The AArch64 writer.
fn emit_put_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: retain unconsumed stream bytes ---");
    emitter.label_global("__rt_stream_pending_put");
    // Frame: [0]=state [8]=source ptr [16]=source len.
    emitter.instruction("sub sp, sp, #48");                                     // reserve the writer frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("str x1, [sp, #8]");                                    // the bytes to retain
    emitter.instruction("str x2, [sp, #16]");                                   // and how many
    emitter.instruction("cbz x2, __rt_spp_done");                               // nothing to retain
    emitter.instruction("bl __rt_stream_state");                                // resolve the owning stream state
    emitter.instruction("cbz x0, __rt_spp_done");                               // a stale handle retains nothing
    emitter.instruction("str x0, [sp, #0]");                                    // the state outlives the allocation

    // -- release anything already held, then take a fresh copy --
    emitter.instruction(&format!("ldr x9, [x0, #{STREAM_PENDING_PTR_OFFSET}]"));
    emitter.instruction("cbz x9, __rt_spp_alloc");
    emitter.instruction("mov x0, x9");
    emitter.instruction("bl __rt_heap_free");                                   // the caller already drained it
    emitter.label("__rt_spp_alloc");
    emitter.instruction("ldr x0, [sp, #16]");                                   // the byte count
    emitter.instruction("bl __rt_heap_alloc");
    emitter.instruction("cbz x0, __rt_spp_done");                               // out of memory: retain nothing
    emitter.instruction("ldr x9, [sp, #0]");                                    // the state again
    emitter.instruction(&format!("str x0, [x9, #{STREAM_PENDING_PTR_OFFSET}]"));
    emitter.instruction("ldr x10, [sp, #16]");
    emitter.instruction(&format!("str x10, [x9, #{STREAM_PENDING_LEN_OFFSET}]"));
    emitter.instruction(&format!("str xzr, [x9, #{STREAM_PENDING_POS_OFFSET}]"));
    emitter.instruction("ldr x11, [sp, #8]");                                   // the source bytes
    emitter.instruction("mov x12, #0");                                         // copy cursor
    emitter.label("__rt_spp_copy");
    emitter.instruction("cmp x12, x10");
    emitter.instruction("b.ge __rt_spp_done");
    emitter.instruction("ldrb w13, [x11, x12]");
    emitter.instruction("strb w13, [x0, x12]");
    emitter.instruction("add x12, x12, #1");
    emitter.instruction("b __rt_spp_copy");

    emitter.label("__rt_spp_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the writer frame
    emitter.instruction("ret");
}

/// The AArch64 appender: keeps what is still held and adds to it.
///
/// `__rt_stream_pending_put` replaces, and says so — it frees the old block because its callers
/// have drained it. Topping the area up has NOT drained it: php keeps the bytes the last read left
/// over and adds the new chunk after them, which is what lets `fread($h, 4)` answer four bytes
/// across a chunk boundary. Replacing there loses exactly the leftovers.
fn emit_append_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: append to retained stream bytes ---");
    emitter.label_global("__rt_stream_pending_append");
    // Frame: [0]=state [8]=source ptr [16]=source len [24]=old block [32]=kept count [40]=new block
    emitter.instruction("sub sp, sp, #64");
    emitter.instruction("stp x29, x30, [sp, #48]");
    emitter.instruction("add x29, sp, #48");
    emitter.instruction("str x1, [sp, #8]");                                    // the bytes to add
    emitter.instruction("str x2, [sp, #16]");                                   // and how many
    emitter.instruction("cbz x2, __rt_spa_done");                               // nothing to add
    emitter.instruction("bl __rt_stream_state");                                // resolve the owning stream state
    emitter.instruction("cbz x0, __rt_spa_done");                               // a stale handle retains nothing
    emitter.instruction("str x0, [sp, #0]");

    emitter.instruction(&format!("ldr x9, [x0, #{STREAM_PENDING_PTR_OFFSET}]"));
    emitter.instruction("str x9, [sp, #24]");                                   // whatever block is there now
    emitter.instruction(&format!("ldr x10, [x0, #{STREAM_PENDING_LEN_OFFSET}]"));
    emitter.instruction(&format!("ldr x11, [x0, #{STREAM_PENDING_POS_OFFSET}]"));
    emitter.instruction("subs x10, x10, x11");                                  // what is still unread
    emitter.instruction("csel x10, x10, xzr, gt");                              // never negative
    emitter.instruction("cbz x9, __rt_spa_kept");                               // no block: nothing is kept
    emitter.instruction("b __rt_spa_have_kept");
    emitter.label("__rt_spa_kept");
    emitter.instruction("mov x10, #0");
    emitter.label("__rt_spa_have_kept");
    emitter.instruction("str x10, [sp, #32]");                                  // how many survive the append

    emitter.instruction("ldr x0, [sp, #16]");
    emitter.instruction("add x0, x0, x10");                                     // the whole area, after
    emitter.instruction("bl __rt_heap_alloc");
    emitter.instruction("cbz x0, __rt_spa_done");                               // out of memory: keep what there is
    emitter.instruction("str x0, [sp, #40]");

    // -- the survivors first, from where the reader had got to --
    emitter.instruction("ldr x9, [sp, #24]");                                   // the old block
    emitter.instruction("cbz x9, __rt_spa_kept_copied");
    emitter.instruction("ldr x11, [sp, #0]");
    emitter.instruction(&format!("ldr x11, [x11, #{STREAM_PENDING_POS_OFFSET}]"));
    emitter.instruction("add x9, x9, x11");                                     // the first unread byte
    emitter.instruction("ldr x10, [sp, #32]");
    emitter.instruction("mov x12, #0");
    emitter.label("__rt_spa_kept_copy");
    emitter.instruction("cmp x12, x10");
    emitter.instruction("b.ge __rt_spa_kept_copied");
    emitter.instruction("ldrb w13, [x9, x12]");
    emitter.instruction("strb w13, [x0, x12]");
    emitter.instruction("add x12, x12, #1");
    emitter.instruction("b __rt_spa_kept_copy");
    emitter.label("__rt_spa_kept_copied");

    // -- then the new bytes, after them --
    emitter.instruction("ldr x0, [sp, #40]");
    emitter.instruction("ldr x10, [sp, #32]");
    emitter.instruction("add x0, x0, x10");                                     // where the new chunk lands
    emitter.instruction("ldr x11, [sp, #8]");
    emitter.instruction("ldr x14, [sp, #16]");
    emitter.instruction("mov x12, #0");
    emitter.label("__rt_spa_new_copy");
    emitter.instruction("cmp x12, x14");
    emitter.instruction("b.ge __rt_spa_new_copied");
    emitter.instruction("ldrb w13, [x11, x12]");
    emitter.instruction("strb w13, [x0, x12]");
    emitter.instruction("add x12, x12, #1");
    emitter.instruction("b __rt_spa_new_copy");
    emitter.label("__rt_spa_new_copied");

    emitter.instruction("ldr x9, [sp, #0]");                                    // the state
    emitter.instruction("ldr x0, [sp, #40]");
    emitter.instruction(&format!("str x0, [x9, #{STREAM_PENDING_PTR_OFFSET}]"));
    emitter.instruction("ldr x10, [sp, #32]");
    emitter.instruction("ldr x14, [sp, #16]");
    emitter.instruction("add x10, x10, x14");
    emitter.instruction(&format!("str x10, [x9, #{STREAM_PENDING_LEN_OFFSET}]"));
    emitter.instruction(&format!("str xzr, [x9, #{STREAM_PENDING_POS_OFFSET}]"));
    emitter.instruction("ldr x0, [sp, #24]");                                   // the block it replaces
    emitter.instruction("cbz x0, __rt_spa_done");
    emitter.instruction("bl __rt_heap_free");

    emitter.label("__rt_spa_done");
    emitter.instruction("ldp x29, x30, [sp, #48]");
    emitter.instruction("add sp, sp, #64");
    emitter.instruction("ret");
}

/// The x86_64 twin of [`emit_append_aarch64`].
fn emit_append_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: append to retained stream bytes ---");
    emitter.label_global("__rt_stream_pending_append");
    emitter.instruction("push rbp");
    emitter.instruction("mov rbp, rsp");
    emitter.instruction("sub rsp, 64");
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // the bytes to add
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // and how many
    emitter.instruction("test rdx, rdx");
    emitter.instruction("jz __rt_spa_done_x86");                                // nothing to add
    emitter.instruction("call __rt_stream_state");
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_spa_done_x86");                                // a stale handle retains nothing
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");

    emitter.instruction(&format!("mov r9, QWORD PTR [rax + {STREAM_PENDING_PTR_OFFSET}]"));
    emitter.instruction("mov QWORD PTR [rbp - 32], r9");                        // whatever block is there now
    emitter.instruction(&format!("mov r10, QWORD PTR [rax + {STREAM_PENDING_LEN_OFFSET}]"));
    emitter.instruction(&format!("mov r11, QWORD PTR [rax + {STREAM_PENDING_POS_OFFSET}]"));
    emitter.instruction("sub r10, r11");                                        // what is still unread
    emitter.instruction("test r9, r9");
    emitter.instruction("jnz __rt_spa_have_kept_x86");
    emitter.instruction("xor r10, r10");                                        // no block: nothing is kept
    emitter.label("__rt_spa_have_kept_x86");
    emitter.instruction("cmp r10, 0");
    emitter.instruction("jg __rt_spa_kept_ok_x86");
    emitter.instruction("xor r10, r10");                                        // never negative
    emitter.label("__rt_spa_kept_ok_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], r10");                       // how many survive the append

    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // __rt_heap_alloc reads its size in rax
    emitter.instruction("add rax, r10");                                        // the whole area, after
    emitter.instruction("call __rt_heap_alloc");
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_spa_done_x86");                                // out of memory: keep what there is
    emitter.instruction("mov QWORD PTR [rbp - 48], rax");

    emitter.instruction("mov r9, QWORD PTR [rbp - 32]");                        // the old block
    emitter.instruction("test r9, r9");
    emitter.instruction("jz __rt_spa_kept_copied_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");
    emitter.instruction(&format!("mov r11, QWORD PTR [r11 + {STREAM_PENDING_POS_OFFSET}]"));
    emitter.instruction("add r9, r11");                                         // the first unread byte
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");
    emitter.instruction("xor rcx, rcx");
    emitter.label("__rt_spa_kept_copy_x86");
    emitter.instruction("cmp rcx, r10");
    emitter.instruction("jge __rt_spa_kept_copied_x86");
    emitter.instruction("movzx edi, BYTE PTR [r9 + rcx]");
    emitter.instruction("mov BYTE PTR [rax + rcx], dil");
    emitter.instruction("inc rcx");
    emitter.instruction("jmp __rt_spa_kept_copy_x86");
    emitter.label("__rt_spa_kept_copied_x86");

    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");
    emitter.instruction("add rax, r10");                                        // where the new chunk lands
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");
    emitter.instruction("mov r8, QWORD PTR [rbp - 24]");
    emitter.instruction("xor rcx, rcx");
    emitter.label("__rt_spa_new_copy_x86");
    emitter.instruction("cmp rcx, r8");
    emitter.instruction("jge __rt_spa_new_copied_x86");
    emitter.instruction("movzx edi, BYTE PTR [r11 + rcx]");
    emitter.instruction("mov BYTE PTR [rax + rcx], dil");
    emitter.instruction("inc rcx");
    emitter.instruction("jmp __rt_spa_new_copy_x86");
    emitter.label("__rt_spa_new_copied_x86");

    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // the state
    emitter.instruction("mov rax, QWORD PTR [rbp - 48]");
    emitter.instruction(&format!("mov QWORD PTR [r9 + {STREAM_PENDING_PTR_OFFSET}], rax"));
    emitter.instruction("mov r10, QWORD PTR [rbp - 40]");
    emitter.instruction("add r10, QWORD PTR [rbp - 24]");
    emitter.instruction(&format!("mov QWORD PTR [r9 + {STREAM_PENDING_LEN_OFFSET}], r10"));
    emitter.instruction(&format!("mov QWORD PTR [r9 + {STREAM_PENDING_POS_OFFSET}], 0"));
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // the block it replaces
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_spa_done_x86");
    emitter.instruction("call __rt_heap_free");

    emitter.label("__rt_spa_done_x86");
    emitter.instruction("add rsp, 64");
    emitter.instruction("pop rbp");
    emitter.instruction("ret");
}

/// The AArch64 reader.
fn emit_take_aarch64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: drain retained stream bytes ---");
    emitter.label_global("__rt_stream_pending_take");
    // Frame: [0]=state [8]=dest [16]=max [24]=moved.
    emitter.instruction("sub sp, sp, #48");                                     // reserve the reader frame
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the helper frame pointer
    emitter.instruction("str x1, [sp, #8]");                                    // where the bytes go
    emitter.instruction("str x2, [sp, #16]");                                   // and how many at most
    emitter.instruction("str xzr, [sp, #24]");                                  // nothing moved yet
    emitter.instruction("cbz x2, __rt_spt_done");                               // room for nothing
    emitter.instruction("bl __rt_stream_state");                                // resolve the owning stream state
    emitter.instruction("cbz x0, __rt_spt_done");                               // a stale handle holds nothing
    emitter.instruction("str x0, [sp, #0]");
    emitter.instruction(&format!("ldr x9, [x0, #{STREAM_PENDING_PTR_OFFSET}]"));
    emitter.instruction("cbz x9, __rt_spt_done");                               // the common case: nothing held
    emitter.instruction(&format!("ldr x10, [x0, #{STREAM_PENDING_LEN_OFFSET}]"));
    emitter.instruction(&format!("ldr x11, [x0, #{STREAM_PENDING_POS_OFFSET}]"));
    emitter.instruction("sub x12, x10, x11");                                   // bytes still held
    emitter.instruction("cbz x12, __rt_spt_release");                           // drained: let the block go
    emitter.instruction("ldr x13, [sp, #16]");                                  // the caller's room
    emitter.instruction("cmp x12, x13");
    emitter.instruction("csel x12, x13, x12, gt");                              // move MIN(held, room)
    emitter.instruction("str x12, [sp, #24]");                                  // that is the answer
    emitter.instruction("ldr x14, [sp, #8]");                                   // the destination
    emitter.instruction("add x9, x9, x11");                                     // the first byte still held
    emitter.instruction("mov x15, #0");                                         // copy cursor
    emitter.label("__rt_spt_copy");
    emitter.instruction("cmp x15, x12");
    emitter.instruction("b.ge __rt_spt_copied");
    emitter.instruction("ldrb w16, [x9, x15]");
    emitter.instruction("strb w16, [x14, x15]");
    emitter.instruction("add x15, x15, #1");
    emitter.instruction("b __rt_spt_copy");
    emitter.label("__rt_spt_copied");
    emitter.instruction("ldr x0, [sp, #0]");                                    // the state
    emitter.instruction("add x11, x11, x12");                                   // advance the read cursor
    emitter.instruction(&format!("str x11, [x0, #{STREAM_PENDING_POS_OFFSET}]"));
    emitter.instruction(&format!("ldr x10, [x0, #{STREAM_PENDING_LEN_OFFSET}]"));
    emitter.instruction("cmp x11, x10");
    emitter.instruction("b.lt __rt_spt_done");                                  // more still held for the next read
    emitter.label("__rt_spt_release");
    emitter.instruction("ldr x0, [sp, #0]");                                    // the state
    emitter.instruction(&format!("ldr x9, [x0, #{STREAM_PENDING_PTR_OFFSET}]"));
    emitter.instruction(&format!("str xzr, [x0, #{STREAM_PENDING_PTR_OFFSET}]"));
    emitter.instruction(&format!("str xzr, [x0, #{STREAM_PENDING_LEN_OFFSET}]"));
    emitter.instruction(&format!("str xzr, [x0, #{STREAM_PENDING_POS_OFFSET}]"));
    emitter.instruction("cbz x9, __rt_spt_done");
    emitter.instruction("mov x0, x9");
    emitter.instruction("bl __rt_heap_free");                                   // the holding area is empty again
    emitter.label("__rt_spt_done");
    emitter.instruction("ldr x0, [sp, #24]");                                   // how many bytes moved
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the reader frame
    emitter.instruction("ret");
}

/// Emits `__rt_stream_pending_clear(handle)`, which drops everything the stream still holds.
///
/// A SEEK invalidates the buffer by definition: the bytes in it come from wherever the last read
/// stopped, and the caller has just asked to continue somewhere else. Without this a chunk read
/// ahead of a `fseek()` was served after it — the bytes were valid, for a position the program had
/// left.
pub fn emit_stream_pending_clear(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.blank();
            emitter.comment("--- runtime: drop retained stream bytes ---");
            emitter.label_global("__rt_stream_pending_clear");
            emitter.instruction("sub sp, sp, #32");
            emitter.instruction("stp x29, x30, [sp, #16]");
            emitter.instruction("add x29, sp, #16");
            emitter.instruction("bl __rt_stream_state");                        // resolve the owning stream state
            emitter.instruction("cbz x0, __rt_spc_done");                       // a stale handle holds nothing
            emitter.instruction(&format!("ldr x9, [x0, #{STREAM_PENDING_PTR_OFFSET}]"));
            emitter.instruction(&format!("str xzr, [x0, #{STREAM_PENDING_PTR_OFFSET}]"));
            emitter.instruction(&format!("str xzr, [x0, #{STREAM_PENDING_LEN_OFFSET}]"));
            emitter.instruction(&format!("str xzr, [x0, #{STREAM_PENDING_POS_OFFSET}]"));
            emitter.instruction("cbz x9, __rt_spc_done");                       // nothing was allocated
            emitter.instruction("mov x0, x9");
            emitter.instruction("bl __rt_heap_free");                           // the holding area is empty again
            emitter.label("__rt_spc_done");
            emitter.instruction("ldp x29, x30, [sp, #16]");
            emitter.instruction("add sp, sp, #32");
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.blank();
            emitter.comment("--- runtime: drop retained stream bytes ---");
            emitter.label_global("__rt_stream_pending_clear");
            emitter.instruction("push rbp");
            emitter.instruction("mov rbp, rsp");
            emitter.instruction("sub rsp, 16");
            emitter.instruction("call __rt_stream_state");                      // resolve the owning stream state
            emitter.instruction("test rax, rax");
            emitter.instruction("jz __rt_spc_done_x86");                        // a stale handle holds nothing
            emitter.instruction(&format!(
                "mov r9, QWORD PTR [rax + {STREAM_PENDING_PTR_OFFSET}]"
            ));
            emitter.instruction(&format!(
                "mov QWORD PTR [rax + {STREAM_PENDING_PTR_OFFSET}], 0"
            ));
            emitter.instruction(&format!(
                "mov QWORD PTR [rax + {STREAM_PENDING_LEN_OFFSET}], 0"
            ));
            emitter.instruction(&format!(
                "mov QWORD PTR [rax + {STREAM_PENDING_POS_OFFSET}], 0"
            ));
            emitter.instruction("test r9, r9");
            emitter.instruction("jz __rt_spc_done_x86");                        // nothing was allocated
            // RAX, not rdi: `__rt_heap_free` reads its pointer there on this target, and rax
            // still held the STREAM STATE that `__rt_stream_state` answered — so this freed the
            // state rather than the holding area.
            emitter.instruction("mov rax, r9");
            emitter.instruction("call __rt_heap_free");                         // the holding area is empty again
            emitter.label("__rt_spc_done_x86");
            emitter.instruction("mov rsp, rbp");
            emitter.instruction("pop rbp");
            emitter.instruction("ret");
        }
    }
}

/// Emits `__rt_stream_pending_consume(handle, n)`, which advances the read cursor by `n`.
///
/// The companion of reading the holding area IN PLACE. `__rt_stream_pending_take` copies bytes
/// out one call at a time, and a line reader that wants to find a newline before it commits had
/// to take ONE BYTE per call: `fgets()` over a 900 KB file spent 420 ms in this helper's frame,
/// where php takes 4 ms. A reader can now scan the held bytes where they lie and consume the
/// line in one call.
///
/// Releases the block once it is drained, exactly as `take` does — a stream that is not holding
/// anything must not keep an allocation alive.
pub fn emit_stream_pending_consume(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.blank();
            emitter.comment("--- runtime: consume retained stream bytes ---");
            emitter.label_global("__rt_stream_pending_consume");
            emitter.instruction("sub sp, sp, #32");
            emitter.instruction("stp x29, x30, [sp, #16]");
            emitter.instruction("add x29, sp, #16");
            emitter.instruction("str x1, [sp, #0]");                            // how many bytes the caller consumed
            emitter.instruction("bl __rt_stream_state");                        // resolve the owning stream state
            emitter.instruction("cbz x0, __rt_spc2_done");                      // a stale handle holds nothing
            emitter.instruction(&format!("ldr x9, [x0, #{STREAM_PENDING_PTR_OFFSET}]"));
            emitter.instruction("cbz x9, __rt_spc2_done");                      // nothing held: nothing to advance
            emitter.instruction("ldr x10, [sp, #0]");                           // the consumed count
            emitter.instruction(&format!("ldr x11, [x0, #{STREAM_PENDING_POS_OFFSET}]"));
            emitter.instruction("add x11, x11, x10");                           // advance the read cursor
            emitter.instruction(&format!("str x11, [x0, #{STREAM_PENDING_POS_OFFSET}]"));
            emitter.instruction(&format!("ldr x12, [x0, #{STREAM_PENDING_LEN_OFFSET}]"));
            emitter.instruction("cmp x11, x12");
            emitter.instruction("b.lt __rt_spc2_done");                         // more is still held
            emitter.instruction(&format!("str xzr, [x0, #{STREAM_PENDING_PTR_OFFSET}]"));
            emitter.instruction(&format!("str xzr, [x0, #{STREAM_PENDING_LEN_OFFSET}]"));
            emitter.instruction(&format!("str xzr, [x0, #{STREAM_PENDING_POS_OFFSET}]"));
            emitter.instruction("mov x0, x9");
            emitter.instruction("bl __rt_heap_free");                           // drained: the holding area is empty again
            emitter.label("__rt_spc2_done");
            emitter.instruction("ldp x29, x30, [sp, #16]");
            emitter.instruction("add sp, sp, #32");
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.blank();
            emitter.comment("--- runtime: consume retained stream bytes ---");
            emitter.label_global("__rt_stream_pending_consume");
            emitter.instruction("push rbp");
            emitter.instruction("mov rbp, rsp");
            emitter.instruction("sub rsp, 16");
            emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                // how many bytes the caller consumed
            emitter.instruction("call __rt_stream_state");                      // resolve the owning stream state
            emitter.instruction("test rax, rax");
            emitter.instruction("jz __rt_spc2_done_x86");                       // a stale handle holds nothing
            emitter.instruction(&format!(
                "mov r9, QWORD PTR [rax + {STREAM_PENDING_PTR_OFFSET}]"
            ));
            emitter.instruction("test r9, r9");
            emitter.instruction("jz __rt_spc2_done_x86");                       // nothing held: nothing to advance
            emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                // the consumed count
            emitter.instruction(&format!(
                "add QWORD PTR [rax + {STREAM_PENDING_POS_OFFSET}], r10"
            ));                                                                 // advance the read cursor
            emitter.instruction(&format!(
                "mov r10, QWORD PTR [rax + {STREAM_PENDING_POS_OFFSET}]"
            ));
            emitter.instruction(&format!(
                "cmp r10, QWORD PTR [rax + {STREAM_PENDING_LEN_OFFSET}]"
            ));
            emitter.instruction("jl __rt_spc2_done_x86");                       // more is still held
            emitter.instruction(&format!(
                "mov QWORD PTR [rax + {STREAM_PENDING_PTR_OFFSET}], 0"
            ));
            emitter.instruction(&format!(
                "mov QWORD PTR [rax + {STREAM_PENDING_LEN_OFFSET}], 0"
            ));
            emitter.instruction(&format!(
                "mov QWORD PTR [rax + {STREAM_PENDING_POS_OFFSET}], 0"
            ));
            // See `__rt_stream_pending_clear`: the free reads RAX, which still holds the state.
            emitter.instruction("mov rax, r9");
            emitter.instruction("call __rt_heap_free");                         // drained: the holding area is empty again
            emitter.label("__rt_spc2_done_x86");
            emitter.instruction("mov rsp, rbp");
            emitter.instruction("pop rbp");
            emitter.instruction("ret");
        }
    }
}

/// Emits `__rt_stream_temp_eof_probe(handle)`, php's over-read at the end of a LINE read.
///
/// `php://temp` reports EOF one read EARLIER than every other stream, and only for a line read:
/// MEASURED on `php -n` 8.5.6 with `"a\nbb\n"`, `fgets()` returning the final `"bb\n"` leaves
/// `feof()` TRUE on `php://temp` and FALSE on `php://memory`, on a plain file, and — on all
/// three — after the equivalent `fread()` or `fgetc()`. `stream_get_line()` behaves like
/// `fgets()`. It is not a size effect: a 9001-byte first line, well past the chunk, answers the
/// same way.
///
/// The reason is php-src's own plumbing rather than a rule about temporary files. `php://temp`
/// WRAPS an inner memory stream and copies that stream's `eof` after every read. A line read
/// asks for a whole chunk, so filling the buffer drives the inner stream one read past its last
/// byte — the read that returns nothing and raises its `eof`. A sized `fread()` stops as soon as
/// it has what the caller asked for, which is why it never sees that extra read.
///
/// So this reproduces the extra read rather than guessing at its consequence: when a line reader
/// has emptied the holding area on a `php://temp` stream, fill it once more. Bytes arriving keep
/// the stream open — that is the 9001-byte case — and nothing arriving is the end.
///
/// Input: x0/rdi = the opaque stream handle. Clobbers the argument registers, nothing else.
pub fn emit_stream_temp_eof_probe(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.blank();
            emitter.comment("--- runtime: php://temp reports EOF as soon as a line read drains it ---");
            emitter.label_global("__rt_stream_temp_eof_probe");
            emitter.instruction("sub sp, sp, #32");                             // frame for the handle and the saved linkage
            emitter.instruction("stp x29, x30, [sp, #16]");                     // save frame pointer and return address
            emitter.instruction("add x29, sp, #16");                            // establish the helper frame pointer
            emitter.instruction("str x0, [sp, #0]");                            // the opaque stream handle
            emitter.instruction("bl __rt_stream_state");                        // resolve the stable stream state
            emitter.instruction("cbz x0, __rt_step_done");                      // no state: nothing to judge
            emitter.instruction(&format!("ldr x9, [x0, #{STREAM_WRAPPER_ID_OFFSET}]")); // which wrapper opened it
            emitter.instruction(&format!("cmp x9, #{WRAPPER_ID_PHP}"));
            emitter.instruction("b.ne __rt_step_compress");                     // not php://: a compress wrapper reads the same way
            emitter.instruction(&format!("ldr x10, [x0, #{STREAM_URI_PTR_OFFSET}]")); // the recorded URI
            emitter.instruction(&format!("ldr x11, [x0, #{STREAM_URI_LEN_OFFSET}]")); // and its length
            emitter.instruction("cbz x10, __rt_step_done");                     // no URI: the sub-wrapper is unknown
            emitter.instruction("cmp x11, #7");                                 // "php://" plus the naming byte
            emitter.instruction("b.lt __rt_step_done");
            emitter.instruction("ldrb w12, [x10, #6]");                         // the php:// sub-wrapper's initial
            emitter.instruction("cmp w12, #0x74");                              // 't' as in temp, and nothing else
            emitter.instruction("b.ne __rt_step_done");
            emitter.instruction("b __rt_step_probe");
            // A `compress.zlib://` or `compress.bzip2://` stream answers the same way — MEASURED,
            // `fgets()` of the last of three lines leaves `feof()` TRUE through either, and false
            // through a plain file. See the doc above for why one rule covers both.
            emitter.label("__rt_step_compress");
            emitter.instruction(&format!("cmp x9, #{WRAPPER_ID_COMPRESS_ZLIB}"));
            emitter.instruction("b.eq __rt_step_probe");
            emitter.instruction(&format!("cmp x9, #{WRAPPER_ID_COMPRESS_BZIP2}"));
            emitter.instruction("b.ne __rt_step_done");
            emitter.label("__rt_step_probe");
            emitter.instruction("ldr x0, [sp, #0]");
            emitter.instruction("bl __rt_stream_pending_held");                 // are there bytes still in hand?
            emitter.instruction("cbnz x0, __rt_step_done");                     // then the stream is plainly not finished
            emitter.instruction("ldr x0, [sp, #0]");
            emitter.instruction("bl __rt_stream_pending_fill");                 // the extra read php's line path performs
            emitter.instruction("cbnz x0, __rt_step_done");                     // a chunk arrived: hold it and stay open
            emitter.instruction("ldr x0, [sp, #0]");
            emitter.instruction("mov x1, #1");                                  // nothing arrived: this is the end
            emitter.instruction("bl __rt_stream_eof_set");                      // publish it on this stream's own state
            emitter.label("__rt_step_done");
            emitter.instruction("ldp x29, x30, [sp, #16]");                     // restore frame pointer and return address
            emitter.instruction("add sp, sp, #32");                             // release the helper frame
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.blank();
            emitter.comment("--- runtime: php://temp reports EOF as soon as a line read drains it ---");
            emitter.label_global("__rt_stream_temp_eof_probe");
            emitter.instruction("push rbp");                                    // preserve the caller frame pointer
            emitter.instruction("mov rbp, rsp");                                // establish the helper frame
            emitter.instruction("sub rsp, 16");                                 // keep the nested calls 16-byte aligned
            emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                // the opaque stream handle
            emitter.instruction("call __rt_stream_state");                      // resolve the stable stream state
            emitter.instruction("test rax, rax");
            emitter.instruction("jz __rt_step_done_x");                         // no state: nothing to judge
            emitter.instruction(&format!(
                "mov r9, QWORD PTR [rax + {STREAM_WRAPPER_ID_OFFSET}]"
            ));                                                                 // which wrapper opened it
            emitter.instruction(&format!("cmp r9, {WRAPPER_ID_PHP}"));
            emitter.instruction("jne __rt_step_compress_x");                    // not php://: a compress wrapper reads the same way
            emitter.instruction(&format!(
                "mov r10, QWORD PTR [rax + {STREAM_URI_PTR_OFFSET}]"
            ));                                                                 // the recorded URI
            emitter.instruction(&format!(
                "mov r11, QWORD PTR [rax + {STREAM_URI_LEN_OFFSET}]"
            ));                                                                 // and its length
            emitter.instruction("test r10, r10");
            emitter.instruction("jz __rt_step_done_x");                         // no URI: the sub-wrapper is unknown
            emitter.instruction("cmp r11, 7");                                  // "php://" plus the naming byte
            emitter.instruction("jl __rt_step_done_x");
            // r8, not r9: the wrapper id is still live for the compress arm below.
            emitter.instruction("movzx r8d, BYTE PTR [r10 + 6]");               // the php:// sub-wrapper's initial
            emitter.instruction("cmp r8d, 0x74");                               // 't' as in temp, and nothing else
            emitter.instruction("jne __rt_step_done_x");
            emitter.instruction("jmp __rt_step_probe_x");
            // See the AArch64 arm: a `compress.zlib://` or `compress.bzip2://` stream reports the
            // end on the same rule, and MEASURED answers the same way through either.
            emitter.label("__rt_step_compress_x");
            emitter.instruction(&format!("cmp r9, {WRAPPER_ID_COMPRESS_ZLIB}"));
            emitter.instruction("je __rt_step_probe_x");
            emitter.instruction(&format!("cmp r9, {WRAPPER_ID_COMPRESS_BZIP2}"));
            emitter.instruction("jne __rt_step_done_x");
            emitter.label("__rt_step_probe_x");
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");
            emitter.instruction("call __rt_stream_pending_held");               // are there bytes still in hand?
            emitter.instruction("test rax, rax");
            emitter.instruction("jnz __rt_step_done_x");                        // then the stream is plainly not finished
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");
            emitter.instruction("call __rt_stream_pending_fill");               // the extra read php's line path performs
            emitter.instruction("test rax, rax");
            emitter.instruction("jnz __rt_step_done_x");                        // a chunk arrived: hold it and stay open
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");
            emitter.instruction("mov rsi, 1");                                  // nothing arrived: this is the end
            emitter.instruction("call __rt_stream_eof_set");                    // publish it on this stream's own state
            emitter.label("__rt_step_done_x");
            emitter.instruction("mov rsp, rbp");                                // release the helper frame
            emitter.instruction("pop rbp");                                     // restore the caller frame pointer
            emitter.instruction("ret");
        }
    }
}

/// Emits `__rt_stream_pending_fill(handle) -> n`, which reads ONE CHUNK onto the stream.
///
/// php never asks a file for the bytes a caller wanted; it asks for a whole chunk and keeps the
/// surplus in the stream's own read buffer. `__rt_fread` already does that for a read request.
/// A LINE reader cannot: it does not know how many bytes it wants until it finds the newline, so
/// `fgets()` fell back to reading ONE BYTE at a time — one `read(2)` per byte. MEASURED over a
/// 900 KB file of 100 000 lines: 538 ms, where php takes 7 ms.
///
/// This is the missing half — fill, with nothing taken back. The reader then finds the newline
/// in the holding area and takes the whole line out in one go.
///
/// Answers 0 whenever it did not buffer anything (a non-regular backend, a failed read, end of
/// file), so a caller can simply fall through to whatever it did before.
///
/// EOF is the CALLER's question, not the fill's: `__rt_fread` judges its short chunk against the
/// CHUNK it asked for and would leave `feof()` true with 8 000 unread bytes on the stream, so a
/// fill that produced bytes clears it again.
pub fn emit_stream_pending_fill(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.blank();
            emitter.comment("--- runtime: fill the stream holding area with one chunk ---");
            emitter.label_global("__rt_stream_pending_fill");
            // Frame: [0]=handle [8]=chunk ptr [16]=chunk len
            emitter.instruction("sub sp, sp, #48");
            emitter.instruction("stp x29, x30, [sp, #32]");
            emitter.instruction("add x29, sp, #32");
            emitter.instruction("str x0, [sp, #0]");                            // the opaque stream handle
            emitter.instruction("bl __rt_stream_pending_held");
            emitter.instruction("cbnz x0, __rt_spf_done");                      // already holding: that IS the fill
            emitter.instruction("ldr x0, [sp, #0]");
            emitter.instruction("bl __rt_stream_chunk_size");                   // x0 = what php would ask for
            emitter.instruction("mov x1, x0");
            emitter.instruction("ldr x0, [sp, #0]");
            emitter.instruction("bl __rt_fread");                               // x0 = flag, x1 = ptr, x2 = len
            emitter.instruction("cbz x2, __rt_spf_none");                       // the source had nothing to give
            emitter.instruction("stp x1, x2, [sp, #8]");                        // the chunk outlives the calls below
            emitter.instruction("ldr x0, [sp, #0]");
            emitter.instruction("ldr x1, [sp, #8]");
            emitter.instruction("ldr x2, [sp, #16]");
            emitter.instruction("bl __rt_stream_pending_put");                  // the WHOLE chunk belongs to the stream
            emitter.instruction("ldr x1, [sp, #8]");
            emitter.instruction("mov x2, #0");
            emitter.instruction("bl __rt_concat_publish");                      // hand the scratch window back
            emitter.instruction("ldr x0, [sp, #8]");
            emitter.instruction("bl __rt_decref_any");                          // the copy on the stream is the one that lives
            emitter.instruction("ldr x0, [sp, #0]");
            emitter.instruction("mov x1, #0");
            emitter.instruction("bl __rt_stream_eof_set");                      // bytes in hand are not end of file
            emitter.instruction("ldr x0, [sp, #16]");                           // answer what was buffered
            emitter.instruction("b __rt_spf_done");
            emitter.label("__rt_spf_none");
            emitter.instruction("mov x0, xzr");                                 // nothing buffered: the caller decides
            emitter.label("__rt_spf_done");
            emitter.instruction("ldp x29, x30, [sp, #32]");
            emitter.instruction("add sp, sp, #48");
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.blank();
            emitter.comment("--- runtime: fill the stream holding area with one chunk ---");
            emitter.label_global("__rt_stream_pending_fill");
            emitter.instruction("push rbp");
            emitter.instruction("mov rbp, rsp");
            emitter.instruction("sub rsp, 32");
            emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                // the opaque stream handle
            emitter.instruction("call __rt_stream_pending_held");
            emitter.instruction("test rax, rax");
            emitter.instruction("jnz __rt_spf_done_x86");                       // already holding: that IS the fill
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");
            emitter.instruction("call __rt_stream_chunk_size");                 // rax = what php would ask for
            emitter.instruction("mov rsi, rax");
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");
            emitter.instruction("call __rt_fread");                             // rax = ptr, rdx = len, rcx = flag
            emitter.instruction("test rdx, rdx");
            emitter.instruction("jz __rt_spf_none_x86");                        // the source had nothing to give
            emitter.instruction("mov QWORD PTR [rbp - 16], rax");               // the chunk outlives the calls below
            emitter.instruction("mov QWORD PTR [rbp - 24], rdx");
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");
            emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");
            emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");
            emitter.instruction("call __rt_stream_pending_put");                // the WHOLE chunk belongs to the stream
            emitter.instruction("mov rax, QWORD PTR [rbp - 16]");               // publish reads RAX/RDX
            emitter.instruction("xor edx, edx");
            emitter.instruction("call __rt_concat_publish");                    // hand the scratch window back
            emitter.instruction("mov rax, QWORD PTR [rbp - 16]");               // decref reads RAX, not rdi
            emitter.instruction("call __rt_decref_any");                        // the copy on the stream is the one that lives
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");
            emitter.instruction("xor esi, esi");
            emitter.instruction("call __rt_stream_eof_set");                    // bytes in hand are not end of file
            emitter.instruction("mov rax, QWORD PTR [rbp - 24]");               // answer what was buffered
            emitter.instruction("jmp __rt_spf_done_x86");
            emitter.label("__rt_spf_none_x86");
            emitter.instruction("xor eax, eax");                                // nothing buffered: the caller decides
            emitter.label("__rt_spf_done_x86");
            emitter.instruction("mov rsp, rbp");
            emitter.instruction("pop rbp");
            emitter.instruction("ret");
        }
    }
}

/// Emits `__rt_stream_pending_held(handle) -> n`, the count of bytes still in the holding area.
///
/// `ftell()` probes the DESCRIPTOR, which has already moved past everything a read pulled ahead.
/// php reports the position it has handed to the caller, so the held bytes are subtracted from
/// the descriptor's own offset. Answers 0 for a stale handle, a stream with no state, and the
/// overwhelmingly common case of a stream holding nothing — two loads and a subtract.
pub fn emit_stream_pending_held(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.blank();
            emitter.comment("--- runtime: how many stream bytes are held ---");
            emitter.label_global("__rt_stream_pending_held");
            emitter.instruction("sub sp, sp, #32");
            emitter.instruction("stp x29, x30, [sp, #16]");
            emitter.instruction("add x29, sp, #16");
            emitter.instruction("bl __rt_stream_state");                        // resolve the owning stream state
            emitter.instruction("cbz x0, __rt_sph_none");                       // a stale handle holds nothing
            emitter.instruction(&format!("ldr x9, [x0, #{STREAM_PENDING_LEN_OFFSET}]"));
            emitter.instruction(&format!("ldr x10, [x0, #{STREAM_PENDING_POS_OFFSET}]"));
            emitter.instruction("subs x0, x9, x10");                            // what remains unhanded
            emitter.instruction("b.gt __rt_sph_done");                          // a positive remainder is the answer
            emitter.label("__rt_sph_none");
            emitter.instruction("mov x0, xzr");                                 // never report a negative hold
            emitter.label("__rt_sph_done");
            emitter.instruction("ldp x29, x30, [sp, #16]");
            emitter.instruction("add sp, sp, #32");
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.blank();
            emitter.comment("--- runtime: how many stream bytes are held ---");
            emitter.label_global("__rt_stream_pending_held");
            emitter.instruction("push rbp");
            emitter.instruction("mov rbp, rsp");
            emitter.instruction("sub rsp, 16");
            emitter.instruction("call __rt_stream_state");                      // resolve the owning stream state
            emitter.instruction("test rax, rax");
            emitter.instruction("jz __rt_sph_none_x86");                        // a stale handle holds nothing
            emitter.instruction(&format!(
                "mov r9, QWORD PTR [rax + {STREAM_PENDING_LEN_OFFSET}]"
            ));
            emitter.instruction(&format!(
                "mov r10, QWORD PTR [rax + {STREAM_PENDING_POS_OFFSET}]"
            ));
            emitter.instruction("sub r9, r10");                                 // what remains unhanded
            emitter.instruction("mov rax, r9");
            emitter.instruction("cmp rax, 0");
            emitter.instruction("jg __rt_sph_done_x86");                        // a positive remainder is the answer
            emitter.label("__rt_sph_none_x86");
            emitter.instruction("xor eax, eax");                                // never report a negative hold
            emitter.label("__rt_sph_done_x86");
            emitter.instruction("mov rsp, rbp");
            emitter.instruction("pop rbp");
            emitter.instruction("ret");
        }
    }
}

/// The x86_64 writer.
fn emit_put_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: retain unconsumed stream bytes ---");
    emitter.label_global("__rt_stream_pending_put");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the writer frame
    emitter.instruction("sub rsp, 48");                                         // reserve the state and source slots
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // the bytes to retain
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // and how many
    emitter.instruction("test rdx, rdx");
    emitter.instruction("jz __rt_spp_done_x86");                                // nothing to retain
    emitter.instruction("call __rt_stream_state");                              // resolve the owning stream state
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_spp_done_x86");                                // a stale handle retains nothing
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // the state outlives the allocation

    emitter.instruction(&format!(
        "mov r9, QWORD PTR [rax + {STREAM_PENDING_PTR_OFFSET}]"
    ));
    emitter.instruction("test r9, r9");
    emitter.instruction("jz __rt_spp_alloc_x86");
    emitter.instruction("mov rax, r9");                                         // __rt_heap_free takes its pointer in rax
    emitter.instruction("call __rt_heap_free");                                 // the caller already drained it
    emitter.label("__rt_spp_alloc_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // the byte count, in the register it reads
    emitter.instruction("call __rt_heap_alloc");
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_spp_done_x86");                                // out of memory: retain nothing
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // the state again
    emitter.instruction(&format!(
        "mov QWORD PTR [r9 + {STREAM_PENDING_PTR_OFFSET}], rax"
    ));
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");
    emitter.instruction(&format!(
        "mov QWORD PTR [r9 + {STREAM_PENDING_LEN_OFFSET}], r10"
    ));
    emitter.instruction(&format!(
        "mov QWORD PTR [r9 + {STREAM_PENDING_POS_OFFSET}], 0"
    ));
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // the source bytes
    emitter.instruction("xor rcx, rcx");                                        // copy cursor
    emitter.label("__rt_spp_copy_x86");
    emitter.instruction("cmp rcx, r10");
    emitter.instruction("jge __rt_spp_done_x86");
    emitter.instruction("movzx edi, BYTE PTR [r11 + rcx]");
    emitter.instruction("mov BYTE PTR [rax + rcx], dil");
    emitter.instruction("inc rcx");
    emitter.instruction("jmp __rt_spp_copy_x86");

    emitter.label("__rt_spp_done_x86");
    emitter.instruction("add rsp, 48");                                         // release the writer frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");
}

/// The x86_64 reader.
fn emit_take_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: drain retained stream bytes ---");
    emitter.label_global("__rt_stream_pending_take");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the reader frame
    emitter.instruction("sub rsp, 48");                                         // reserve the state, destination and tally slots
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // where the bytes go
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // and how many at most
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // nothing moved yet
    emitter.instruction("test rdx, rdx");
    emitter.instruction("jz __rt_spt_done_x86");                                // room for nothing
    emitter.instruction("call __rt_stream_state");                              // resolve the owning stream state
    emitter.instruction("test rax, rax");
    emitter.instruction("jz __rt_spt_done_x86");                                // a stale handle holds nothing
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");
    emitter.instruction(&format!(
        "mov r9, QWORD PTR [rax + {STREAM_PENDING_PTR_OFFSET}]"
    ));
    emitter.instruction("test r9, r9");
    emitter.instruction("jz __rt_spt_done_x86");                                // the common case: nothing held
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rax + {STREAM_PENDING_LEN_OFFSET}]"
    ));
    emitter.instruction(&format!(
        "mov r11, QWORD PTR [rax + {STREAM_PENDING_POS_OFFSET}]"
    ));
    emitter.instruction("mov rcx, r10");
    emitter.instruction("sub rcx, r11");                                        // bytes still held
    emitter.instruction("test rcx, rcx");
    emitter.instruction("jz __rt_spt_release_x86");                             // drained: let the block go
    emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                       // the caller's room
    emitter.instruction("cmp rcx, rdx");
    emitter.instruction("cmovg rcx, rdx");                                      // move MIN(held, room)
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // that is the answer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // the destination
    emitter.instruction("add r9, r11");                                         // the first byte still held
    emitter.instruction("xor r8, r8");                                          // copy cursor
    emitter.label("__rt_spt_copy_x86");
    emitter.instruction("cmp r8, rcx");
    emitter.instruction("jge __rt_spt_copied_x86");
    emitter.instruction("movzx edi, BYTE PTR [r9 + r8]");
    emitter.instruction("mov BYTE PTR [rsi + r8], dil");
    emitter.instruction("inc r8");
    emitter.instruction("jmp __rt_spt_copy_x86");
    emitter.label("__rt_spt_copied_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // the state
    emitter.instruction("add r11, rcx");                                        // advance the read cursor
    emitter.instruction(&format!(
        "mov QWORD PTR [rax + {STREAM_PENDING_POS_OFFSET}], r11"
    ));
    emitter.instruction(&format!(
        "mov r10, QWORD PTR [rax + {STREAM_PENDING_LEN_OFFSET}]"
    ));
    emitter.instruction("cmp r11, r10");
    emitter.instruction("jl __rt_spt_done_x86");                                // more still held for the next read
    emitter.label("__rt_spt_release_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // the state
    emitter.instruction(&format!(
        "mov r9, QWORD PTR [rax + {STREAM_PENDING_PTR_OFFSET}]"
    ));
    emitter.instruction(&format!(
        "mov QWORD PTR [rax + {STREAM_PENDING_PTR_OFFSET}], 0"
    ));
    emitter.instruction(&format!(
        "mov QWORD PTR [rax + {STREAM_PENDING_LEN_OFFSET}], 0"
    ));
    emitter.instruction(&format!(
        "mov QWORD PTR [rax + {STREAM_PENDING_POS_OFFSET}], 0"
    ));
    emitter.instruction("test r9, r9");
    emitter.instruction("jz __rt_spt_done_x86");
    emitter.instruction("mov rax, r9");                                         // __rt_heap_free takes its pointer in rax
    emitter.instruction("call __rt_heap_free");                                 // the holding area is empty again
    emitter.label("__rt_spt_done_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 32]");                       // how many bytes moved
    emitter.instruction("add rsp, 48");                                         // release the reader frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");
}

/// Silences the unused-import warning when neither arm references the ABI helper.
#[allow(dead_code)]
fn _abi_used(emitter: &mut Emitter) {
    let _ = abi::int_result_reg(emitter);
}
