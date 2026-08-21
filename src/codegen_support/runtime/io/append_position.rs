//! Purpose:
//! Emits the two helpers that let `ftell()` answer PHP's position for an append stream, and
//! `fseek()` put it back in agreement with the descriptor.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - The `ftell()`, `fseek()` and `rewind()` lowerings.
//!
//! Key details:
//! - PHP maintains an append stream's position itself: `fopen($f, 'a')` on a four-byte file
//!   answers `1` after one byte is written, not `5`. The descriptor really is at 5 — `O_APPEND`
//!   puts every write at the end — so the two disagree by however much was jumped over, which
//!   `__rt_fwrite` accumulates on the stream state.
//! - Only the reported number diverges. PHP's own reads use the descriptor's offset too, so a read
//!   after an append write hits EOF on both sides and the file contents are identical.
//! - A raw descriptor has no state and no total, which is what the zero answers cover.

use crate::codegen_support::runtime::resources::layout::{
    STREAM_APPEND_SKIP_OFFSET, STREAM_FILTERED_BUF_PTR_OFFSET, STREAM_FILTERED_POS_OFFSET,
    STREAM_READ_FILTER_HEAD_OFFSET, STREAM_WRAPPER_POS_OFFSET,
};
use crate::codegen_support::runtime::data::{
    DYNAMIC_PROP_DEPRECATED_HEAD, DYNAMIC_PROP_DEPRECATED_TAIL,
};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_stream_append_skip(handle)`, answering the bytes `O_APPEND` has jumped over.
///
/// AArch64 takes and answers `x0`; x86_64 takes `rdi` and answers `rax`. Zero for every stream
/// that is not an append one, which makes the subtraction at `ftell()` unconditional.
pub fn emit_stream_append_skip(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_append_skip ---");
    emitter.label_global("__rt_stream_append_skip");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #16");
            emitter.instruction("stp x29, x30, [sp, #0]");                      // save frame pointer and return address
            emitter.instruction("mov x29, sp");
            emitter.instruction("bl __rt_stream_state");                        // x0 = the owning state, zero for a raw descriptor
            emitter.instruction("cbz x0, __rt_sas_none");
            emitter.instruction(&format!("ldr x0, [x0, #{STREAM_APPEND_SKIP_OFFSET}]"));
            emitter.instruction("ldp x29, x30, [sp, #0]");
            emitter.instruction("add sp, sp, #16");
            emitter.instruction("ret");
            emitter.label("__rt_sas_none");
            emitter.instruction("mov x0, #0");                                  // nothing was jumped over
            emitter.instruction("ldp x29, x30, [sp, #0]");
            emitter.instruction("add sp, sp, #16");
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");
            emitter.instruction("mov rbp, rsp");
            emitter.instruction("call __rt_stream_state");                      // rax = the owning state, zero for a raw descriptor
            emitter.instruction("test rax, rax");
            emitter.instruction("jz __rt_sas_none_x86");
            emitter.instruction(&format!("mov rax, QWORD PTR [rax + {STREAM_APPEND_SKIP_OFFSET}]"));
            emitter.instruction("pop rbp");
            emitter.instruction("ret");
            emitter.label("__rt_sas_none_x86");
            emitter.instruction("xor eax, eax");                                // nothing was jumped over
            emitter.instruction("pop rbp");
            emitter.instruction("ret");
        }
    }
}

/// Emits `__rt_stream_clear_append_skip(handle)`, called after a successful seek.
///
/// A seek puts PHP's position and the descriptor back in agreement — PHP answers `0` right after
/// `fseek($h, 0)` on an append stream, and `1` after writing one more byte — so the running total
/// starts again from there. Without this the answer goes negative on the write after a seek.
pub fn emit_stream_clear_append_skip(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_clear_append_skip ---");
    emitter.label_global("__rt_stream_clear_append_skip");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #16");
            emitter.instruction("stp x29, x30, [sp, #0]");
            emitter.instruction("mov x29, sp");
            emitter.instruction("bl __rt_stream_state");                        // x0 = the owning state, zero for a raw descriptor
            emitter.instruction("cbz x0, __rt_scas_none");
            emitter.instruction(&format!("str xzr, [x0, #{STREAM_APPEND_SKIP_OFFSET}]"));
            emitter.label("__rt_scas_none");
            emitter.instruction("ldp x29, x30, [sp, #0]");
            emitter.instruction("add sp, sp, #16");
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");
            emitter.instruction("mov rbp, rsp");
            emitter.instruction("call __rt_stream_state");                      // rax = the owning state, zero for a raw descriptor
            emitter.instruction("test rax, rax");
            emitter.instruction("jz __rt_scas_none_x86");
            emitter.instruction(&format!("mov QWORD PTR [rax + {STREAM_APPEND_SKIP_OFFSET}], 0"));
            emitter.label("__rt_scas_none_x86");
            emitter.instruction("pop rbp");
            emitter.instruction("ret");
        }
    }
}

/// Emits `__rt_stream_wrapper_pos(handle)`, answering the position PHP keeps for a wrapper stream.
///
/// php-src has no tell op for userspace wrappers: `stream_tell` is called only from inside
/// `php_userstreamop_seek`, to reconcile after a seek. The position itself is PHP's own, advanced
/// by whatever each read and write moved — which is why `ftell()` answers `7` after seven bytes
/// even when the wrapper's `stream_tell()` is written to return something else.
pub fn emit_stream_wrapper_pos(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_wrapper_pos ---");
    emitter.label_global("__rt_stream_wrapper_pos");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #16");
            emitter.instruction("stp x29, x30, [sp, #0]");
            emitter.instruction("mov x29, sp");
            emitter.instruction("bl __rt_stream_state");                        // x0 = the owning state, zero for a raw descriptor
            emitter.instruction("cbz x0, __rt_swp_none");
            emitter.instruction(&format!("ldr x0, [x0, #{STREAM_WRAPPER_POS_OFFSET}]"));
            emitter.instruction("ldp x29, x30, [sp, #0]");
            emitter.instruction("add sp, sp, #16");
            emitter.instruction("ret");
            emitter.label("__rt_swp_none");
            emitter.instruction("mov x0, #0");                                  // no state: the stream is at its start
            emitter.instruction("ldp x29, x30, [sp, #0]");
            emitter.instruction("add sp, sp, #16");
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");
            emitter.instruction("mov rbp, rsp");
            emitter.instruction("call __rt_stream_state");                      // rax = the owning state, zero for a raw descriptor
            emitter.instruction("test rax, rax");
            emitter.instruction("jz __rt_swp_none_x86");
            emitter.instruction(&format!("mov rax, QWORD PTR [rax + {STREAM_WRAPPER_POS_OFFSET}]"));
            emitter.instruction("pop rbp");
            emitter.instruction("ret");
            emitter.label("__rt_swp_none_x86");
            emitter.instruction("xor eax, eax");                                // no state: the stream is at its start
            emitter.instruction("pop rbp");
            emitter.instruction("ret");
        }
    }
}

/// Emits `__rt_stream_wrapper_pos_advance(handle, delta)`, moving that position.
///
/// Called from the wrapper read and write paths with the bytes they actually moved, which is
/// exactly how php-src maintains `stream->position` for these streams.
pub fn emit_stream_wrapper_pos_advance(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_wrapper_pos_advance ---");
    emitter.label_global("__rt_stream_wrapper_pos_advance");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #32");
            emitter.instruction("stp x29, x30, [sp, #16]");
            emitter.instruction("add x29, sp, #16");
            emitter.instruction("str x1, [sp, #0]");                            // hold the delta across the lookup
            emitter.instruction("bl __rt_stream_state");                        // x0 = the owning state
            emitter.instruction("cbz x0, __rt_swpa_done");
            emitter.instruction(&format!("ldr x9, [x0, #{STREAM_WRAPPER_POS_OFFSET}]"));
            emitter.instruction("ldr x10, [sp, #0]");
            emitter.instruction("add x9, x9, x10");
            emitter.instruction(&format!("str x9, [x0, #{STREAM_WRAPPER_POS_OFFSET}]"));
            emitter.label("__rt_swpa_done");
            emitter.instruction("ldp x29, x30, [sp, #16]");
            emitter.instruction("add sp, sp, #32");
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");
            emitter.instruction("mov rbp, rsp");
            emitter.instruction("sub rsp, 16");
            emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                // hold the delta across the lookup
            emitter.instruction("call __rt_stream_state");                      // rax = the owning state
            emitter.instruction("test rax, rax");
            emitter.instruction("jz __rt_swpa_done_x86");
            emitter.instruction("mov r10, QWORD PTR [rbp - 8]");
            emitter.instruction(&format!("add QWORD PTR [rax + {STREAM_WRAPPER_POS_OFFSET}], r10"));
            emitter.label("__rt_swpa_done_x86");
            emitter.instruction("mov rsp, rbp");
            emitter.instruction("pop rbp");
            emitter.instruction("ret");
        }
    }
}

/// Emits `__rt_stream_wrapper_pos_set(handle, position)`, called after a wrapper seek.
///
/// php-src reconciles here by calling `stream_tell` and taking its answer when it is an integer.
/// elephc uses the offset the seek moved to, which is the same number for any wrapper whose
/// `stream_tell()` reports its real position; a wrapper that reports something else is the one
/// case still to cover, and needs the boxed-return mask that `a7c92d475` never landed here.
pub fn emit_stream_wrapper_pos_set(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_wrapper_pos_set ---");
    emitter.label_global("__rt_stream_wrapper_pos_set");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #32");
            emitter.instruction("stp x29, x30, [sp, #16]");
            emitter.instruction("add x29, sp, #16");
            emitter.instruction("str x1, [sp, #0]");                            // hold the position across the lookup
            emitter.instruction("bl __rt_stream_state");
            emitter.instruction("cbz x0, __rt_swps_done");
            emitter.instruction("ldr x9, [sp, #0]");
            emitter.instruction(&format!("str x9, [x0, #{STREAM_WRAPPER_POS_OFFSET}]"));
            emitter.label("__rt_swps_done");
            emitter.instruction("ldp x29, x30, [sp, #16]");
            emitter.instruction("add sp, sp, #32");
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");
            emitter.instruction("mov rbp, rsp");
            emitter.instruction("sub rsp, 16");
            emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                // hold the position across the lookup
            emitter.instruction("call __rt_stream_state");
            emitter.instruction("test rax, rax");
            emitter.instruction("jz __rt_swps_done_x86");
            emitter.instruction("mov r10, QWORD PTR [rbp - 8]");
            emitter.instruction(&format!("mov QWORD PTR [rax + {STREAM_WRAPPER_POS_OFFSET}], r10"));
            emitter.label("__rt_swps_done_x86");
            emitter.instruction("mov rsp, rbp");
            emitter.instruction("pop rbp");
            emitter.instruction("ret");
        }
    }
}

/// Emits `__rt_stream_filtered_pos(handle)`, the position PHP reports for a filtered read stream.
///
/// php advances `stream->position` by the bytes each read RETURNED TO THE CALLER, never by the
/// bytes it pulled from the descriptor. A filtered read pulls ahead — the buffered wrapper reads
/// whole chunks so the filter has something to work on and parks what the caller did not ask for —
/// so `lseek(SEEK_CUR)` reports where the READ-AHEAD stopped instead.
///
/// Answers `-1` when the buffered path has not run for this stream: no read chain at all, or a
/// chain whose buffer was never allocated because the reads went through `fgets()`, which does not
/// use it. `ftell()` reads that as "keep the descriptor probe", which is the right answer there.
///
/// AArch64 takes and answers `x0`; x86_64 takes `rdi` and answers `rax`.
pub fn emit_stream_filtered_pos(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_filtered_pos ---");
    emitter.label_global("__rt_stream_filtered_pos");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #16");
            emitter.instruction("stp x29, x30, [sp, #0]");                      // save frame pointer and return address
            emitter.instruction("mov x29, sp");
            emitter.instruction("bl __rt_stream_state");                        // x0 = the owning state, zero for a raw descriptor
            emitter.instruction("cbz x0, __rt_sfp_untracked");
            emitter.instruction(&format!("ldr x9, [x0, #{STREAM_READ_FILTER_HEAD_OFFSET}]"));
            emitter.instruction("cbz x9, __rt_sfp_untracked");                  // an unfiltered stream is at its descriptor offset
            emitter.instruction(&format!("ldr x9, [x0, #{STREAM_FILTERED_BUF_PTR_OFFSET}]"));
            emitter.instruction("cbz x9, __rt_sfp_untracked");                  // the buffered path never ran: fgets() reads do not use it
            emitter.instruction(&format!("ldr x0, [x0, #{STREAM_FILTERED_POS_OFFSET}]"));
            emitter.instruction("ldp x29, x30, [sp, #0]");
            emitter.instruction("add sp, sp, #16");
            emitter.instruction("ret");
            emitter.label("__rt_sfp_untracked");
            emitter.instruction("mov x0, #-1");                                 // the caller keeps its own descriptor probe
            emitter.instruction("ldp x29, x30, [sp, #0]");
            emitter.instruction("add sp, sp, #16");
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");
            emitter.instruction("mov rbp, rsp");
            emitter.instruction("call __rt_stream_state");                      // rax = the owning state, zero for a raw descriptor
            emitter.instruction("test rax, rax");
            emitter.instruction("jz __rt_sfp_untracked_x86");
            emitter.instruction(&format!(
                "mov r10, QWORD PTR [rax + {STREAM_READ_FILTER_HEAD_OFFSET}]"
            ));
            emitter.instruction("test r10, r10");
            emitter.instruction("jz __rt_sfp_untracked_x86");                   // an unfiltered stream is at its descriptor offset
            emitter.instruction(&format!(
                "mov r10, QWORD PTR [rax + {STREAM_FILTERED_BUF_PTR_OFFSET}]"
            ));
            emitter.instruction("test r10, r10");
            emitter.instruction("jz __rt_sfp_untracked_x86");                   // the buffered path never ran: fgets() reads do not use it
            emitter.instruction(&format!(
                "mov rax, QWORD PTR [rax + {STREAM_FILTERED_POS_OFFSET}]"
            ));
            emitter.instruction("pop rbp");
            emitter.instruction("ret");
            emitter.label("__rt_sfp_untracked_x86");
            emitter.instruction("mov rax, -1");                                 // the caller keeps its own descriptor probe
            emitter.instruction("pop rbp");
            emitter.instruction("ret");
        }
    }
}

/// Emits `__rt_stream_filtered_pos_set(handle, position)`, called after a successful seek.
///
/// php's position restarts from wherever the seek landed and advances from there: measured with
/// `php -n` (8.5.6), `fread($f, 3)` then `fseek($f, 10)` then `fread($f, 4)` through a read filter
/// answers `3`, `10`, `14`. The seek already discards the filtered buffer, so the bytes it had
/// read ahead are gone; without this the stale count from before the seek would be reported.
pub fn emit_stream_filtered_pos_set(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_filtered_pos_set ---");
    emitter.label_global("__rt_stream_filtered_pos_set");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #32");
            emitter.instruction("stp x29, x30, [sp, #16]");
            emitter.instruction("add x29, sp, #16");
            emitter.instruction("str x1, [sp, #0]");                            // hold the position across the lookup
            emitter.instruction("bl __rt_stream_state");
            emitter.instruction("cbz x0, __rt_sfps_done");
            emitter.instruction("ldr x9, [sp, #0]");
            emitter.instruction(&format!("str x9, [x0, #{STREAM_FILTERED_POS_OFFSET}]"));
            emitter.label("__rt_sfps_done");
            emitter.instruction("ldp x29, x30, [sp, #16]");
            emitter.instruction("add sp, sp, #32");
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");
            emitter.instruction("mov rbp, rsp");
            emitter.instruction("sub rsp, 16");
            emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                // hold the position across the lookup
            emitter.instruction("call __rt_stream_state");
            emitter.instruction("test rax, rax");
            emitter.instruction("jz __rt_sfps_done_x86");
            emitter.instruction("mov r10, QWORD PTR [rbp - 8]");
            emitter.instruction(&format!(
                "mov QWORD PTR [rax + {STREAM_FILTERED_POS_OFFSET}], r10"
            ));
            emitter.label("__rt_sfps_done_x86");
            emitter.instruction("mov rsp, rbp");
            emitter.instruction("pop rbp");
            emitter.instruction("ret");
        }
    }
}

/// Emits `__rt_dynamic_context_deprecation(class_id)`, PHP 8.2's notice for an invented property.
///
/// A stream wrapper that declares no `$context` still receives one, and since PHP 8.2 the
/// assignment is deprecated: `Deprecated: Creation of dynamic property Mem::$context is
/// deprecated`. The class name comes from the shared class-id table that `var_dump` reads, so an
/// id with no name writes nothing between the fragments rather than dangling.
pub fn emit_dynamic_context_deprecation(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: dynamic context property deprecation ---");
    emitter.label_global("__rt_dynamic_context_deprecation");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #32");
            emitter.instruction("stp x29, x30, [sp, #16]");
            emitter.instruction("add x29, sp, #16");
            emitter.instruction("str x0, [sp, #0]");                            // hold the class id across the writes
            abi::emit_symbol_address(emitter, "x1", "_dyn_prop_dep_head");
            emitter.instruction(&format!("mov x2, #{}", DYNAMIC_PROP_DEPRECATED_HEAD.len()));
            emitter.instruction("bl __rt_diag_warning");                        // honours the @ suppression depth
            emitter.instruction("ldr x9, [sp, #0]");
            abi::emit_symbol_address(emitter, "x10", "_class_name_count");
            emitter.instruction("ldr x10, [x10]");
            emitter.instruction("cmp x9, x10");
            emitter.instruction("b.hs __rt_dcd_tail");                          // an unknown id names nothing
            abi::emit_symbol_address(emitter, "x11", "_class_name_entries");
            emitter.instruction("add x11, x11, x9, lsl #4");                    // 16-byte (ptr, len) entries
            emitter.instruction("ldr x1, [x11]");
            emitter.instruction("ldr x2, [x11, #8]");
            emitter.instruction("bl __rt_diag_warning");
            emitter.label("__rt_dcd_tail");
            abi::emit_symbol_address(emitter, "x1", "_dyn_prop_dep_tail");
            emitter.instruction(&format!("mov x2, #{}", DYNAMIC_PROP_DEPRECATED_TAIL.len()));
            emitter.instruction("bl __rt_diag_warning");
            emitter.instruction("ldp x29, x30, [sp, #16]");
            emitter.instruction("add sp, sp, #32");
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");
            emitter.instruction("mov rbp, rsp");
            emitter.instruction("sub rsp, 16");
            emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                // hold the class id across the writes
            abi::emit_symbol_address(emitter, "rdi", "_dyn_prop_dep_head");
            emitter.instruction(&format!("mov rsi, {}", DYNAMIC_PROP_DEPRECATED_HEAD.len()));
            emitter.instruction("call __rt_diag_warning");
            emitter.instruction("mov r10, QWORD PTR [rbp - 8]");
            abi::emit_symbol_address(emitter, "r11", "_class_name_count");
            emitter.instruction("mov r11, QWORD PTR [r11]");
            emitter.instruction("cmp r10, r11");
            emitter.instruction("jae __rt_dcd_tail_x86");                       // an unknown id names nothing
            abi::emit_symbol_address(emitter, "r11", "_class_name_entries");
            emitter.instruction("shl r10, 4");                                  // 16-byte (ptr, len) entries
            emitter.instruction("add r11, r10");
            emitter.instruction("mov rdi, QWORD PTR [r11]");
            emitter.instruction("mov rsi, QWORD PTR [r11 + 8]");
            emitter.instruction("call __rt_diag_warning");
            emitter.label("__rt_dcd_tail_x86");
            abi::emit_symbol_address(emitter, "rdi", "_dyn_prop_dep_tail");
            emitter.instruction(&format!("mov rsi, {}", DYNAMIC_PROP_DEPRECATED_TAIL.len()));
            emitter.instruction("call __rt_diag_warning");
            emitter.instruction("mov rsp, rbp");
            emitter.instruction("pop rbp");
            emitter.instruction("ret");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Arch, Platform, Target};

    /// The filtered-position probe answers `-1` for every stream that never engaged the buffered
    /// filtered read, and `ftell()` reads that as "keep the descriptor probe".
    ///
    /// Both guards matter and both are cheap to lose: dropping the read-chain check would make an
    /// unfiltered stream report a field nothing ever writes, and dropping the buffer-pointer check
    /// would make a filtered stream read only through `fgets()` — which never touches that buffer —
    /// report `0` instead of where its reads left the descriptor.
    #[test]
    fn test_stream_filtered_pos_guards_both_conditions_on_both_arches() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_stream_filtered_pos(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_stream_filtered_pos:\n"));
            assert!(
                asm.contains(&format!("{STREAM_READ_FILTER_HEAD_OFFSET}]")),
                "the read-chain guard is missing for {:?}",
                target.arch
            );
            assert!(
                asm.contains(&format!("{STREAM_FILTERED_BUF_PTR_OFFSET}]")),
                "the buffered-path guard is missing for {:?}",
                target.arch
            );
            assert!(
                asm.contains(&format!("{STREAM_FILTERED_POS_OFFSET}]")),
                "the position load is missing for {:?}",
                target.arch
            );
            let sentinel = match target.arch {
                Arch::AArch64 => "mov x0, #-1",
                Arch::X86_64 => "mov rax, -1",
            };
            assert!(
                asm.contains(sentinel),
                "the untracked sentinel is missing for {:?}",
                target.arch
            );
        }
    }

    /// The seat helper writes the filtered position and nothing else, on both arches.
    #[test]
    fn test_stream_filtered_pos_set_writes_only_the_filtered_position() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_stream_filtered_pos_set(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__rt_stream_filtered_pos_set:\n"));
            let store = match target.arch {
                Arch::AArch64 => format!("str x9, [x0, #{STREAM_FILTERED_POS_OFFSET}]"),
                Arch::X86_64 => {
                    format!("mov QWORD PTR [rax + {STREAM_FILTERED_POS_OFFSET}], r10")
                }
            };
            assert!(
                asm.contains(&store),
                "the position store is missing for {:?}",
                target.arch
            );
        }
    }
}
