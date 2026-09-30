//! Purpose:
//! Emits `__rt_builtin_wrapper_index`, which maps a wrapper scheme name to its
//! index in the built-in wrapper list, plus the disabled-wrapper bitmask helpers
//! backing `stream_wrapper_unregister()` / `stream_wrapper_restore()`.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//! - `stream_wrapper_unregister` / `stream_wrapper_restore`, `stream_get_wrappers`,
//!   and the `fopen` built-in-scheme guard.
//!
//! Key details:
//! - PHP lets a built-in wrapper be unregistered and later restored. elephc kept
//!   built-ins outside the user table, so unregistering one reported false and
//!   restoring it was a no-op that always claimed success.
//! - Disabled built-ins live in a bitmask rather than a table so the state costs
//!   one word and a restore is a single bit clear.

use crate::codegen_support::data_section::comm_directive;
use crate::codegen_support::runtime::data::{
    SWR_NEVER_CHANGED, SWR_NEVER_EXISTED, SWR_NTC_PREFIX, SWR_WRN_PREFIX,
};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch, platform::Target};
use crate::types::stream_constants::STREAM_WRAPPERS;

/// Emits the built-in wrapper name table into the runtime data section.
pub(crate) fn emit_builtin_wrapper_table(out: &mut String, target: Target) {
    for (index, name) in STREAM_WRAPPERS.iter().enumerate() {
        out.push_str(&format!(
            ".globl _bw_name_{index}\n_bw_name_{index}:\n    .ascii \"{name}\"\n"
        ));
    }
    out.push_str(".p2align 3\n.globl _bw_table\n_bw_table:\n");
    for (index, name) in STREAM_WRAPPERS.iter().enumerate() {
        out.push_str(&format!(
            "    .quad _bw_name_{index}\n    .quad {}\n    .quad {index}\n",
            name.len()
        ));
    }
    out.push_str("    .quad 0\n    .quad 0\n    .quad 0\n");
    // One bit per built-in wrapper; a set bit means stream_wrapper_unregister()
    // removed it and fopen() must refuse the scheme until it is restored.
    out.push_str(&comm_directive("_disabled_builtin_wrappers", 8, target));
}

/// `__rt_builtin_wrapper_index(ptr, len) -> index`, or -1 when not built in.
pub fn emit_builtin_wrapper_index(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_builtin_wrapper_index_linux_x86_64(emitter);
        return;
    }
    emitter.blank();
    emitter.comment("--- runtime: resolve a built-in wrapper scheme name ---");
    emitter.label_global("__rt_builtin_wrapper_index");
    abi::emit_symbol_address(emitter, "x9", "_bw_table");
    emitter.label("__rt_bwi_entry");
    emitter.instruction("ldr x10, [x9]");                                       // candidate name pointer
    emitter.instruction("cbz x10, __rt_bwi_miss");                              // null terminates the table
    emitter.instruction("ldr x11, [x9, #8]");                                   // candidate name length
    emitter.instruction("cmp x11, x1");                                         // does the length match?
    emitter.instruction("b.ne __rt_bwi_next");
    emitter.instruction("mov x12, #0");                                         // byte compare cursor
    emitter.label("__rt_bwi_bytes");
    emitter.instruction("cmp x12, x1");                                         // compared every byte?
    emitter.instruction("b.ge __rt_bwi_hit");
    emitter.instruction("ldrb w13, [x10, x12]");
    emitter.instruction("ldrb w14, [x0, x12]");
    emitter.instruction("cmp w13, w14");
    emitter.instruction("b.ne __rt_bwi_next");
    emitter.instruction("add x12, x12, #1");
    emitter.instruction("b __rt_bwi_bytes");
    emitter.label("__rt_bwi_next");
    emitter.instruction("add x9, x9, #24");                                     // next 3-word entry
    emitter.instruction("b __rt_bwi_entry");
    emitter.label("__rt_bwi_hit");
    emitter.instruction("ldr x0, [x9, #16]");                                   // return the built-in index
    emitter.instruction("ret");
    emitter.label("__rt_bwi_miss");
    emitter.instruction("mov x0, #-1");                                         // not a built-in wrapper
    emitter.instruction("ret");
}

/// x86_64 variant of [`emit_builtin_wrapper_index`].
fn emit_builtin_wrapper_index_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resolve a built-in wrapper scheme name ---");
    emitter.label_global("__rt_builtin_wrapper_index");
    abi::emit_symbol_address(emitter, "r9", "_bw_table");
    emitter.label("__rt_bwi_entry_x");
    emitter.instruction("mov r10, QWORD PTR [r9]");
    emitter.instruction("test r10, r10");
    emitter.instruction("jz __rt_bwi_miss_x");
    emitter.instruction("mov r11, QWORD PTR [r9 + 8]");
    emitter.instruction("cmp r11, rsi");
    emitter.instruction("jne __rt_bwi_next_x");
    emitter.instruction("xor rcx, rcx");
    emitter.label("__rt_bwi_bytes_x");
    emitter.instruction("cmp rcx, rsi");
    emitter.instruction("jge __rt_bwi_hit_x");
    emitter.instruction("mov dl, BYTE PTR [r10 + rcx]");
    emitter.instruction("mov r8b, BYTE PTR [rdi + rcx]");
    emitter.instruction("cmp dl, r8b");
    emitter.instruction("jne __rt_bwi_next_x");
    emitter.instruction("add rcx, 1");
    emitter.instruction("jmp __rt_bwi_bytes_x");
    emitter.label("__rt_bwi_next_x");
    emitter.instruction("add r9, 24");
    emitter.instruction("jmp __rt_bwi_entry_x");
    emitter.label("__rt_bwi_hit_x");
    emitter.instruction("mov rax, QWORD PTR [r9 + 16]");
    emitter.instruction("ret");
    emitter.label("__rt_bwi_miss_x");
    emitter.instruction("mov rax, -1");
    emitter.instruction("ret");
}

/// Emits `__rt_stream_wrapper_restore_diag(kind, ptr, len)`.
///
/// `kind` 0 writes PHP's Notice for a built-in scheme that was never unregistered, and 1
/// writes PHP's Warning for a scheme that never existed. Each message is three fragments —
/// prefix, the caller's scheme bytes, suffix — because the scheme is only known at run time.
///
/// The two severities go to different places, following what elephc already does elsewhere:
/// Notices through `__rt_stdout_write`, so an active output buffer captures them exactly as
/// PHP does with display_errors on; Warnings through `__rt_diag_warning`, which honours `@`.
/// PHP CLI puts both on stdout; that divergence is repo-wide and not settled here.
///
/// Keeping this in a helper rather than inline in the lowering means the stack discipline
/// lives in one framed function instead of being repeated across the caller's branches.
pub fn emit_stream_wrapper_restore_diag(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_stream_wrapper_restore_diag_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: stream_wrapper_restore_diag ---");
    emitter.label_global("__rt_stream_wrapper_restore_diag");

    emitter.instruction("sub sp, sp, #48");                                     // scheme ptr/len plus saved frame linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the diagnostic helper frame
    emitter.instruction("str x1, [sp, #0]");                                    // save the scheme pointer across the fragment writes
    emitter.instruction("str x2, [sp, #8]");                                    // save the scheme length across the fragment writes
    emitter.instruction("cbnz x0, __rt_swr_diag_warning");                      // kind 1 = the unknown-scheme warning

    // -- Notice: "<prefix><scheme>:// was never changed, nothing to restore" --
    abi::emit_symbol_address(emitter, "x1", "_swr_ntc_prefix");
    emitter.instruction(&format!("mov x2, #{}", SWR_NTC_PREFIX.len()));         // notice prefix byte count
    emitter.instruction("bl __rt_diag_warning");                                // a notice is a diagnostic: blank line, location, and @ suppression
    emitter.instruction("ldr x1, [sp, #0]");                                    // the caller's scheme pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // the caller's scheme length
    emitter.instruction("bl __rt_diag_warning");                                // write the scheme name itself
    abi::emit_symbol_address(emitter, "x1", "_swr_never_changed");
    emitter.instruction(&format!("mov x2, #{}", SWR_NEVER_CHANGED.len()));      // notice suffix byte count
    emitter.instruction("bl __rt_diag_warning");                                // finish the notice line
    emitter.instruction("b __rt_swr_diag_done");                                // skip the warning path

    // -- Warning: "<prefix><scheme>:// never existed, nothing to restore" --
    emitter.label("__rt_swr_diag_warning");
    abi::emit_symbol_address(emitter, "x1", "_swr_wrn_prefix");
    emitter.instruction(&format!("mov x2, #{}", SWR_WRN_PREFIX.len()));         // warning prefix byte count
    emitter.instruction("bl __rt_diag_warning");                                // warnings honour the @ suppression depth
    emitter.instruction("ldr x1, [sp, #0]");                                    // the caller's scheme pointer
    emitter.instruction("ldr x2, [sp, #8]");                                    // the caller's scheme length
    emitter.instruction("bl __rt_diag_warning");                                // write the scheme name itself
    abi::emit_symbol_address(emitter, "x1", "_swr_never_existed");
    emitter.instruction(&format!("mov x2, #{}", SWR_NEVER_EXISTED.len()));      // warning suffix byte count
    emitter.instruction("bl __rt_diag_warning");                                // finish the warning line

    emitter.label("__rt_swr_diag_done");
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the diagnostic helper frame
    emitter.instruction("ret");                                                 // return to stream_wrapper_restore
}

/// x86_64 variant of [`emit_stream_wrapper_restore_diag`].
///
/// Both `__rt_stdout_write` and `__rt_diag_warning` take `rdi`/`rsi` here, so the two paths
/// differ only in which helper they call.
fn emit_stream_wrapper_restore_diag_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_wrapper_restore_diag ---");
    emitter.label_global("__rt_stream_wrapper_restore_diag");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the diagnostic helper frame
    emitter.instruction("sub rsp, 32");                                         // scheme ptr/len spill slots, 16-byte aligned
    emitter.instruction("mov QWORD PTR [rbp - 8], rsi");                        // save the scheme pointer across the fragment writes
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the scheme length across the fragment writes
    emitter.instruction("test rdi, rdi");                                       // kind 0 = notice, 1 = warning
    emitter.instruction("jnz __rt_swr_diag_warning_x");                         // kind 1 = the unknown-scheme warning

    // -- Notice: "<prefix><scheme>:// was never changed, nothing to restore" --
    abi::emit_symbol_address(emitter, "rdi", "_swr_ntc_prefix");
    emitter.instruction(&format!("mov esi, {}", SWR_NTC_PREFIX.len()));         // notice prefix byte count
    emitter.instruction("call __rt_diag_warning");                              // a notice is a diagnostic: blank line, location, and @ suppression
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the caller's scheme pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // the caller's scheme length
    emitter.instruction("call __rt_diag_warning");                              // write the scheme name itself
    abi::emit_symbol_address(emitter, "rdi", "_swr_never_changed");
    emitter.instruction(&format!("mov esi, {}", SWR_NEVER_CHANGED.len()));      // notice suffix byte count
    emitter.instruction("call __rt_diag_warning");                              // finish the notice line
    emitter.instruction("jmp __rt_swr_diag_done_x");                            // skip the warning path

    // -- Warning: "<prefix><scheme>:// never existed, nothing to restore" --
    emitter.label("__rt_swr_diag_warning_x");
    abi::emit_symbol_address(emitter, "rdi", "_swr_wrn_prefix");
    emitter.instruction(&format!("mov esi, {}", SWR_WRN_PREFIX.len()));         // warning prefix byte count
    emitter.instruction("call __rt_diag_warning");                              // warnings honour the @ suppression depth
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // the caller's scheme pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // the caller's scheme length
    emitter.instruction("call __rt_diag_warning");                              // write the scheme name itself
    abi::emit_symbol_address(emitter, "rdi", "_swr_never_existed");
    emitter.instruction(&format!("mov esi, {}", SWR_NEVER_EXISTED.len()));      // warning suffix byte count
    emitter.instruction("call __rt_diag_warning");                              // finish the warning line

    emitter.label("__rt_swr_diag_done_x");
    emitter.instruction("mov rsp, rbp");                                        // release the diagnostic helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to stream_wrapper_restore
}
