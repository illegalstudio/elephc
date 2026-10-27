//! Purpose:
//! Emits the `__rt_cstr`, `__rt_cstr2` runtime helpers and their growable buffers.
//! Keeps PHP filesystem/resource behavior, libc calls, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::io`.
//!
//! Key details:
//! - I/O helpers bridge PHP strings, resources, descriptors, and libc calls while returning runtime arrays or pointer/length strings.
//! - The first 4 KiB of each C-string destination stays in `_cstr_buf` / `_cstr_buf2`; a longer
//!   path, pattern, or command grows into an owned block tracked by `_cstr_base`/`_cstr_cap` (and
//!   the `2` pair), so a long PHP string can never overwrite adjacent runtime globals.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_cstr` and `__rt_cstr2` for the active target.
///
/// `__rt_cstr` copies the PHP string pair (`x1`/`x2` on AArch64, `rax`/`rdx` on x86_64)
/// into a NUL-terminated buffer and returns its address in `x0`/`rax`. `__rt_cstr2` is the
/// independent second buffer used by callers that must keep two C strings alive at once.
/// Both helpers grow into owned storage when the source does not fit the inline 4 KiB buffer.
pub fn emit_cstr(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_cstr_linux_x86_64(emitter);
        return;
    }

    emit_cstr_aarch64(emitter, CstrSlot { base: "_cstr_base", cap: "_cstr_cap", grow: "__rt_cstr_grow", entry: "__rt_cstr", loop_label: "__rt_cstr_loop", null_label: "__rt_cstr_null", have_label: "__rt_cstr_have", got_label: "__rt_cstr_got" });
    emit_cstr_aarch64(emitter, CstrSlot { base: "_cstr2_base", cap: "_cstr2_cap", grow: "__rt_cstr2_grow", entry: "__rt_cstr2", loop_label: "__rt_cstr2_loop", null_label: "__rt_cstr2_null", have_label: "__rt_cstr2_have", got_label: "__rt_cstr2_got" });
    emit_grow_aarch64(emitter, "_cstr_base", "_cstr_cap", "__rt_cstr_grow");
    emit_grow_aarch64(emitter, "_cstr2_base", "_cstr2_cap", "__rt_cstr2_grow");
}

/// Names one independent C-string destination and its grow entry point.
struct CstrSlot {
    /// Global pointer holding the current destination base.
    base: &'static str,
    /// Global capacity of that destination in bytes.
    cap: &'static str,
    /// Grow helper that replaces the base when the request does not fit.
    grow: &'static str,
    /// Public helper label.
    entry: &'static str,
    /// Copy-loop label.
    loop_label: &'static str,
    /// NUL-termination label.
    null_label: &'static str,
    /// Label used when the inline buffer already has room.
    have_label: &'static str,
    /// Label reached after a grow returns the new base.
    got_label: &'static str,
}

/// Emits one AArch64 C-string copy helper.
///
/// The source pair is spilled before the grow helper runs, because `__rt_heap_alloc`
/// clobbers every caller-saved register. The inline buffer is only used when `len + 1`
/// still fits its capacity.
fn emit_cstr_aarch64(emitter: &mut Emitter, slot: CstrSlot) {
    emitter.blank();
    emitter.comment(&format!("--- runtime: {} ---", slot.entry.trim_start_matches("__rt_")));
    emitter.label_global(slot.entry);

    // -- reserve a frame: [0]=source ptr, [8]=source len, [16] spare, [32]=fp/lr --
    emitter.instruction("sub sp, sp, #48");                                     // reserve spill space for the source pair and linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // preserve the frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // establish the C-string copy frame pointer
    emitter.instruction("stp x1, x2, [sp]");                                    // save the PHP string pointer and length

    // -- grow the destination when `len + 1` no longer fits --
    emitter.instruction("adds x0, x2, #1");                                     // request room for the payload plus the NUL terminator
    let len_ok = format!("{}_len_ok", slot.entry);
    emitter.instruction(&format!("b.cc {}", len_ok));                           // a representable length continues to the capacity check
    emitter.instruction("b __rt_alloc_overflow");                               // a wrapped length can never be terminated
    emitter.label(&len_ok);
    abi::emit_load_symbol_to_reg(emitter, "x9", slot.cap, 0);
    emitter.instruction("cmp x0, x9");                                          // does the request still fit the current destination?
    emitter.instruction(&format!("b.ls {}", slot.have_label));                  // keep the inline or already-grown buffer
    emitter.instruction(&format!("bl {}", slot.grow));                          // allocate a larger owned destination and publish it
    emitter.instruction(&format!("b {}", slot.got_label));                      // continue with the new base

    emitter.label(slot.have_label);
    abi::emit_load_symbol_to_reg(emitter, "x0", slot.base, 0);
    emitter.label(slot.got_label);
    emitter.instruction("mov x10, x0");                                         // preserve the destination start for the return value
    emitter.instruction("mov x9, x0");                                          // destination cursor

    // -- copy the payload and terminate it --
    emitter.instruction("ldp x1, x2, [sp]");                                    // reload the source pointer and length
    emitter.instruction("mov x11, x2");                                         // copy length as loop counter
    emitter.label(slot.loop_label);
    emitter.instruction(&format!("cbz x11, {}", slot.null_label));              // if no bytes remain, append the NUL terminator
    emitter.instruction("ldrb w12, [x1], #1");                                  // load one source byte and advance the source cursor
    emitter.instruction("strb w12, [x9], #1");                                  // store it into the destination and advance the cursor
    emitter.instruction("sub x11, x11, #1");                                    // decrement the remaining byte count
    emitter.instruction(&format!("b {}", slot.loop_label));                     // continue copying the payload

    emitter.label(slot.null_label);
    emitter.instruction("strb wzr, [x9]");                                      // terminate the C string after the last payload byte
    emitter.instruction("mov x0, x10");                                         // return the destination start
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore the frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // release the copy frame
    emitter.instruction("ret");                                                 // return the NUL-terminated destination
}

/// Emits one AArch64 grow helper for a C-string destination.
///
/// Input `x0` is the requested byte count including the terminator. The helper allocates an
/// owned block, stamps it as a string so `__rt_heap_free_safe` can release the superseded
/// block, and updates the base/capacity globals. The inline BSS buffer is never freed.
fn emit_grow_aarch64(emitter: &mut Emitter, base: &str, cap: &str, entry: &str) {
    emitter.blank();
    emitter.comment(&format!("--- runtime: {} ---", entry.trim_start_matches("__rt_")));
    emitter.label_global(entry);
    emitter.instruction("sub sp, sp, #32");                                     // reserve spill space for the request and new base
    emitter.instruction("stp x29, x30, [sp, #16]");                             // preserve the frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the grow frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the requested capacity
    emitter.instruction("bl __rt_heap_alloc");                                  // allocate the larger owned block
    emitter.instruction("mov x9, #1");                                          // heap kind 1 = owned string payload
    emitter.instruction("str x9, [x0, #-8]");                                   // stamp the block so later releases recognize it
    emitter.instruction("str x0, [sp, #8]");                                    // save the new base
    abi::emit_load_symbol_to_reg(emitter, "x0", base, 0);
    emitter.instruction("bl __rt_heap_free_safe");                              // release the superseded block; the inline buffer is a no-op
    emitter.instruction("ldr x0, [sp, #8]");                                    // reload the new base
    abi::emit_store_reg_to_symbol(emitter, "x0", base, 0);
    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the requested capacity
    abi::emit_store_reg_to_symbol(emitter, "x9", cap, 0);
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore the frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the grow frame
    emitter.instruction("ret");                                                 // return the new base in x0
}

/// Emits `__rt_cstr` and `__rt_cstr2` runtime helpers for the x86_64 Linux target.
///
/// The contract mirrors the AArch64 path with System V registers: the source pointer is in
/// `rax`, the length in `rdx`, and the NUL-terminated destination is returned in `rax`.
fn emit_cstr_linux_x86_64(emitter: &mut Emitter) {
    emit_cstr_x86_64(emitter, X86CstrSlot { base: "_cstr_base", cap: "_cstr_cap", grow: "__rt_cstr_grow", entry: "__rt_cstr", loop_label: "__rt_cstr_loop", null_label: "__rt_cstr_null", have_label: "__rt_cstr_have", got_label: "__rt_cstr_got" });
    emit_cstr_x86_64(emitter, X86CstrSlot { base: "_cstr2_base", cap: "_cstr2_cap", grow: "__rt_cstr2_grow", entry: "__rt_cstr2", loop_label: "__rt_cstr2_loop", null_label: "__rt_cstr2_null", have_label: "__rt_cstr2_have", got_label: "__rt_cstr2_got" });
    emit_grow_x86_64(emitter, "_cstr_base", "_cstr_cap", "__rt_cstr_grow");
    emit_grow_x86_64(emitter, "_cstr2_base", "_cstr2_cap", "__rt_cstr2_grow");
}

/// Names one independent x86_64 C-string destination and its grow entry point.
struct X86CstrSlot {
    /// Global pointer holding the current destination base.
    base: &'static str,
    /// Global capacity of that destination in bytes.
    cap: &'static str,
    /// Grow helper that replaces the base when the request does not fit.
    grow: &'static str,
    /// Public helper label.
    entry: &'static str,
    /// Copy-loop label.
    loop_label: &'static str,
    /// NUL-termination label.
    null_label: &'static str,
    /// Label used when the current buffer already has room.
    have_label: &'static str,
    /// Label reached after a grow returns the new base.
    got_label: &'static str,
}

/// Emits one x86_64 C-string copy helper.
fn emit_cstr_x86_64(emitter: &mut Emitter, slot: X86CstrSlot) {
    emitter.blank();
    emitter.comment(&format!("--- runtime: {} ---", slot.entry.trim_start_matches("__rt_")));
    emitter.label_global(slot.entry);
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving the copy spills
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 32");                                         // reserve aligned slots for the source pointer and length
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the PHP string pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rdx");                       // save the PHP string length

    emitter.instruction("lea rax, [rdx + 1]");                                  // request room for the payload plus the NUL terminator
    let len_ok = format!("{}_len_ok", slot.entry);
    emitter.instruction(&format!("jnc {}", len_ok));                            // a representable length continues to the capacity check
    emitter.instruction("jmp __rt_alloc_overflow");                             // a wrapped length can never be terminated
    emitter.label(&len_ok);
    abi::emit_load_symbol_to_reg(emitter, "r8", slot.cap, 0);
    emitter.instruction("cmp rax, r8");                                         // does the request still fit the current destination?
    emitter.instruction(&format!("jbe {}", slot.have_label));                   // keep the inline or already-grown buffer
    emitter.instruction(&format!("call {}", slot.grow));                        // allocate a larger owned destination and publish it
    emitter.instruction(&format!("jmp {}", slot.got_label));                    // continue with the new base

    emitter.label(slot.have_label);
    abi::emit_load_symbol_to_reg(emitter, "rax", slot.base, 0);
    emitter.label(slot.got_label);
    emitter.instruction("mov r9, rax");                                         // preserve the destination start for the return value
    emitter.instruction("mov r8, rax");                                         // destination cursor
    emitter.instruction("mov r10, QWORD PTR [rbp - 8]");                        // reload the source pointer
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the source length

    emitter.label(slot.loop_label);
    emitter.instruction("test rcx, rcx");                                       // stop copying once every payload byte has been consumed
    emitter.instruction(&format!("je {}", slot.null_label));                    // append the NUL terminator once no bytes remain
    emitter.instruction("mov r11b, BYTE PTR [r10]");                            // load one source byte
    emitter.instruction("mov BYTE PTR [r8], r11b");                             // store it into the destination
    emitter.instruction("add r10, 1");                                          // advance the source cursor
    emitter.instruction("add r8, 1");                                           // advance the destination cursor
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining byte count
    emitter.instruction(&format!("jmp {}", slot.loop_label));                   // continue copying the payload

    emitter.label(slot.null_label);
    emitter.instruction("mov BYTE PTR [r8], 0");                                // terminate the C string after the last payload byte
    emitter.instruction("mov rax, r9");                                         // return the destination start
    emitter.instruction("add rsp, 32");                                         // release the copy frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the NUL-terminated destination
}

/// Emits one x86_64 grow helper for a C-string destination.
fn emit_grow_x86_64(emitter: &mut Emitter, base: &str, cap: &str, entry: &str) {
    emitter.blank();
    emitter.comment(&format!("--- runtime: {} ---", entry.trim_start_matches("__rt_")));
    emitter.label_global(entry);
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving the grow spills
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base
    emitter.instruction("sub rsp, 16");                                         // reserve slots for the request and the new base
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the requested capacity
    emitter.instruction("call __rt_heap_alloc");                                // allocate the larger owned block
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(1))); // materialize the owned-string kind word
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // stamp the block so later releases recognize it
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // save the new base
    abi::emit_load_symbol_to_reg(emitter, "rax", base, 0);
    emitter.instruction("call __rt_heap_free_safe");                            // release the superseded block; the inline buffer is a no-op
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the new base
    abi::emit_store_reg_to_symbol(emitter, "rax", base, 0);
    emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                         // reload the requested capacity
    abi::emit_store_reg_to_symbol(emitter, "r9", cap, 0);
    emitter.instruction("add rsp, 16");                                         // release the grow frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the new base in rax
}
