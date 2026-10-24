//! Purpose:
//! Emits the `print_r` capture helpers: `__rt_pr_append`, `__rt_pr_write`, `__rt_pr_finish`
//! and the buffer plumbing behind them (`__rt_pr_base`, `__rt_pr_reserve`, `__rt_pr_release`,
//! `__rt_pr_flush_deferred`). These let `print_r($value, true)` render into an in-memory buffer
//! instead of stdout, then return the captured bytes as an owned heap string.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`.
//! - The `print_r` runtime walkers (`__rt_print_r_*`) call `__rt_pr_write`
//!   instead of a raw `write(1, …)` syscall so return-mode captures their bytes.
//! - `__rt_stdout_write` tail-calls `__rt_pr_append` when `_print_r_mode` is set
//!   so codegen-side scalar/literal writes are captured too.
//! - The `print_r` builtin codegen (`crate::codegen::lower_inst::builtins::debug`)
//!   calls `__rt_pr_finish` to materialize the returned string and
//!   `__rt_pr_flush_deferred` to write out a deferred echo-mode capture.
//!
//! Key details:
//! - `_print_r_mode` (0 = write through, 1 = return-mode capture, 2 = deferred echo-mode capture),
//!   `_print_r_off` (byte count), `_print_r_base` / `_print_r_cap` (the active buffer) live in the
//!   fixed runtime data section and default to zero.
//! - THE BUFFER GROWS. A zero `_print_r_base` means the static 64 KiB `_print_r_buf`, a zero
//!   `_print_r_cap` means 65536. `__rt_pr_append` calls `__rt_pr_reserve` when a write would pass
//!   the capacity; that moves the capture to a malloc'ed block (doubling, or the exact need when
//!   larger), so a `print_r` result of any size is returned whole. The common small case never
//!   leaves the static buffer. `__rt_pr_release` frees the block and returns to the static buffer
//!   once the capture is finished or flushed.
//! - `__rt_pr_write` is the walker-ABI wrapper that branches on mode (syscall vs. append);
//!   `__rt_pr_finish` persists the buffer through `__rt_str_persist`, releases it and resets the state.
//! - Return ABI of `__rt_pr_finish` matches the platform string result registers
//!   (AArch64 x1=ptr, x2=len; x86_64 rax=ptr, rdx=len) so the caller stores it
//!   directly as a `Str` value.

use crate::codegen_support::abi;
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Parks (`restore == false`) or reloads (`restore == true`) the registers a print_r walker may still
/// need after `__rt_pr_append` returns but that the deferred-mode flush path would clobber.
///
/// `__rt_pr_append` itself only touches scratch registers every walker already treats as lost, yet the
/// overflow path calls `__rt_pr_flush_deferred` / `__rt_pr_write`, which reach output buffers, the heap
/// and `write(2)`. The spill keeps that rare path as transparent as the common one.
fn spill_walker_registers(emitter: &mut Emitter, restore: bool) {
    match (emitter.target.arch, restore) {
        (Arch::AArch64, false) => {
            emitter.instruction("sub sp, sp, #80");                             // reserve the walker-register spill area
            emitter.instruction("stp x0, x1, [sp]");                            // park the argument registers
            emitter.instruction("stp x2, x3, [sp, #16]");                       // park x2/x3
            emitter.instruction("stp x4, x5, [sp, #32]");                       // park x4/x5
            emitter.instruction("stp x6, x7, [sp, #48]");                       // park x6/x7
            emitter.instruction("stp x8, x15, [sp, #64]");                      // park x8 and x15
        }
        (Arch::AArch64, true) => {
            emitter.instruction("ldp x0, x1, [sp]");                            // reload the argument registers
            emitter.instruction("ldp x2, x3, [sp, #16]");                       // reload x2/x3
            emitter.instruction("ldp x4, x5, [sp, #32]");                       // reload x4/x5
            emitter.instruction("ldp x6, x7, [sp, #48]");                       // reload x6/x7
            emitter.instruction("ldp x8, x15, [sp, #64]");                      // reload x8 and x15
            emitter.instruction("add sp, sp, #80");                             // release the spill area
        }
        (Arch::X86_64, false) => {
            emitter.instruction("push rdi");                                    // park rdi
            emitter.instruction("push rsi");                                    // park rsi
            emitter.instruction("push r8");                                     // park r8
            emitter.instruction("sub rsp, 8");                                  // keep the stack 16-byte aligned for the call
        }
        (Arch::X86_64, true) => {
            emitter.instruction("add rsp, 8");                                  // drop the alignment pad
            emitter.instruction("pop r8");                                      // reload r8
            emitter.instruction("pop rsi");                                     // reload rsi
            emitter.instruction("pop rdi");                                     // reload rdi
        }
    }
}

/// Emits `__rt_pr_base`: returns the address of the active capture buffer.
///
/// No inputs. Output: AArch64 x0 / x86_64 rax = `_print_r_base`, or the static `_print_r_buf`
/// while the capture has not outgrown it. Clobbers only the result register.
pub fn emit_pr_base(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: pr_base ---");
    emitter.label_global("__rt_pr_base");
    if emitter.target.arch == Arch::X86_64 {
        abi::emit_symbol_address(emitter, "rax", "_print_r_base");
        emitter.instruction("mov rax, QWORD PTR [rax]");                        // load the heap capture block, if any
        emitter.instruction("test rax, rax");                                   // has the capture outgrown the static buffer?
        emitter.instruction("jnz __rt_pr_base_ready");                          // yes: use the heap block
        abi::emit_symbol_address(emitter, "rax", "_print_r_buf");
        emitter.label("__rt_pr_base_ready");
        emitter.instruction("ret");                                             // return the buffer address in rax
        return;
    }
    abi::emit_symbol_address(emitter, "x0", "_print_r_base");
    emitter.instruction("ldr x0, [x0]");                                        // load the heap capture block, if any
    emitter.instruction("cbnz x0, __rt_pr_base_ready");                         // outgrown the static buffer: use the heap block
    abi::emit_symbol_address(emitter, "x0", "_print_r_buf");
    emitter.label("__rt_pr_base_ready");
    emitter.instruction("ret");                                                 // return the buffer address in x0
}

/// Emits `__rt_pr_reserve`: make the capture buffer able to hold `needed` bytes in total.
///
/// Input: AArch64 x0 / x86_64 rdi = required total capacity in bytes. No result. Moves the capture
/// from the static buffer to a malloc'ed block, or reallocs the block, to `max(2 * capacity,
/// needed)` bytes while preserving the first `_print_r_off` bytes. Clobbers caller-saved registers.
/// A failed allocation ends in the runtime's allocation-overflow fatal.
pub fn emit_pr_reserve(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: pr_reserve ---");
    emitter.label_global("__rt_pr_reserve");
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("push rbp");                                        // save the caller frame pointer
        emitter.instruction("mov rbp, rsp");                                    // establish the helper frame
        emitter.instruction("sub rsp, 16");                                     // room for the new capacity and the new block
        emitter.instruction("and rsp, -16");                                    // align the stack for the libc calls
        abi::emit_symbol_address(emitter, "r9", "_print_r_cap");
        emitter.instruction("mov rcx, QWORD PTR [r9]");                         // current capacity
        emitter.instruction("test rcx, rcx");                                   // zero means the static 64 KiB buffer
        emitter.instruction("jnz __rt_pr_reserve_cap_x86");                     // explicit capacity recorded
        emitter.instruction("mov ecx, 65536");                                  // static buffer capacity
        emitter.label("__rt_pr_reserve_cap_x86");
        emitter.instruction("cmp rdi, rcx");                                    // does the request fit?
        emitter.instruction("jbe __rt_pr_reserve_done_x86");                    // yes: nothing to do
        emitter.instruction("add rcx, rcx");                                    // doubled capacity
        emitter.instruction("cmp rdi, rcx");                                    // is the doubled capacity enough?
        emitter.instruction("cmova rcx, rdi");                                  // no: use the exact need
        emitter.instruction("mov QWORD PTR [rbp - 8], rcx");                    // remember the new capacity
        abi::emit_symbol_address(emitter, "r9", "_print_r_base");
        emitter.instruction("mov rdi, QWORD PTR [r9]");                         // current heap block, if any
        emitter.instruction("test rdi, rdi");                                   // still in the static buffer?
        emitter.instruction("jnz __rt_pr_reserve_realloc_x86");                 // grow the heap block in place
        emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                    // new capacity
        emitter.bl_c("malloc");                                                 // allocate the first heap block
        emitter.instruction("test rax, rax");                                   // allocation failed?
        emitter.instruction("jz __rt_pr_reserve_fail_x86");                     // report the allocation overflow
        emitter.instruction("mov QWORD PTR [rbp - 16], rax");                   // remember the new block
        abi::emit_symbol_address(emitter, "r9", "_print_r_off");
        emitter.instruction("mov rdx, QWORD PTR [r9]");                         // bytes captured so far
        abi::emit_symbol_address(emitter, "rsi", "_print_r_buf");
        emitter.instruction("mov rdi, rax");                                    // destination = new block
        emitter.bl_c("memcpy");                                                 // carry the captured prefix over
        emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                   // reload the new block
        emitter.instruction("jmp __rt_pr_reserve_publish_x86");                 // publish it
        emitter.label("__rt_pr_reserve_realloc_x86");
        emitter.instruction("mov rsi, QWORD PTR [rbp - 8]");                    // new capacity
        emitter.bl_c("realloc");                                                // grow the heap block
        emitter.instruction("test rax, rax");                                   // reallocation failed?
        emitter.instruction("jz __rt_pr_reserve_fail_x86");                     // report the allocation overflow
        emitter.label("__rt_pr_reserve_publish_x86");
        abi::emit_symbol_address(emitter, "r9", "_print_r_base");
        emitter.instruction("mov QWORD PTR [r9], rax");                         // publish the heap block
        abi::emit_symbol_address(emitter, "r9", "_print_r_cap");
        emitter.instruction("mov rcx, QWORD PTR [rbp - 8]");                    // reload the new capacity
        emitter.instruction("mov QWORD PTR [r9], rcx");                         // publish the capacity
        emitter.label("__rt_pr_reserve_done_x86");
        emitter.instruction("mov rsp, rbp");                                    // release the helper frame
        emitter.instruction("pop rbp");                                         // restore the caller frame pointer
        emitter.instruction("ret");                                             // return to the appender
        emitter.label("__rt_pr_reserve_fail_x86");
        emitter.instruction("jmp __rt_alloc_overflow");                         // fatal: the capture cannot grow
        return;
    }
    emitter.instruction("sub sp, sp, #32");                                     // allocate the helper frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the helper frame pointer
    abi::emit_symbol_address(emitter, "x9", "_print_r_cap");
    emitter.instruction("ldr x1, [x9]");                                        // current capacity
    emitter.instruction("cbnz x1, __rt_pr_reserve_cap");                        // explicit capacity recorded
    emitter.instruction("mov x1, #65536");                                      // zero means the static 64 KiB buffer
    emitter.label("__rt_pr_reserve_cap");
    emitter.instruction("cmp x0, x1");                                          // does the request fit?
    emitter.instruction("b.ls __rt_pr_reserve_done");                           // yes: nothing to do
    emitter.instruction("lsl x2, x1, #1");                                      // doubled capacity
    emitter.instruction("cmp x0, x2");                                          // is the doubled capacity enough?
    emitter.instruction("csel x2, x0, x2, hi");                                 // no: use the exact need
    emitter.instruction("str x2, [sp, #8]");                                    // remember the new capacity
    abi::emit_symbol_address(emitter, "x9", "_print_r_base");
    emitter.instruction("ldr x10, [x9]");                                       // current heap block, if any
    emitter.instruction("cbnz x10, __rt_pr_reserve_realloc");                   // grow the heap block in place
    emitter.instruction("mov x0, x2");                                          // new capacity
    emitter.bl_c("malloc");                                                     // allocate the first heap block
    emitter.instruction("cbz x0, __rt_pr_reserve_fail");                        // report the allocation overflow
    emitter.instruction("str x0, [sp, #0]");                                    // remember the new block
    abi::emit_symbol_address(emitter, "x9", "_print_r_off");
    emitter.instruction("ldr x2, [x9]");                                        // bytes captured so far
    abi::emit_symbol_address(emitter, "x1", "_print_r_buf");
    emitter.bl_c("memcpy");                                                     // carry the captured prefix over (x0 = new block)
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the new block
    emitter.instruction("b __rt_pr_reserve_publish");                           // publish it
    emitter.label("__rt_pr_reserve_realloc");
    emitter.instruction("mov x0, x10");                                         // current heap block
    emitter.instruction("ldr x1, [sp, #8]");                                    // new capacity
    emitter.bl_c("realloc");                                                    // grow the heap block
    emitter.instruction("cbz x0, __rt_pr_reserve_fail");                        // report the allocation overflow
    emitter.label("__rt_pr_reserve_publish");
    abi::emit_symbol_address(emitter, "x9", "_print_r_base");
    emitter.instruction("str x0, [x9]");                                        // publish the heap block
    emitter.instruction("ldr x1, [sp, #8]");                                    // reload the new capacity
    abi::emit_symbol_address(emitter, "x9", "_print_r_cap");
    emitter.instruction("str x1, [x9]");                                        // publish the capacity
    emitter.label("__rt_pr_reserve_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper frame
    emitter.instruction("ret");                                                 // return to the appender
    emitter.label("__rt_pr_reserve_fail");
    emitter.instruction("b __rt_alloc_overflow");                               // fatal: the capture cannot grow
}

/// Emits `__rt_pr_release`: free a grown capture block and fall back to the static buffer.
///
/// No inputs and no result. Clobbers caller-saved registers.
pub fn emit_pr_release(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: pr_release ---");
    emitter.label_global("__rt_pr_release");
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("push rbp");                                        // save the caller frame pointer
        emitter.instruction("mov rbp, rsp");                                    // establish the helper frame
        emitter.instruction("and rsp, -16");                                    // align the stack for the libc call
        abi::emit_symbol_address(emitter, "r9", "_print_r_base");
        emitter.instruction("mov rdi, QWORD PTR [r9]");                         // heap capture block, if any
        emitter.instruction("test rdi, rdi");                                   // still on the static buffer?
        emitter.instruction("jz __rt_pr_release_done_x86");                     // nothing to free
        emitter.instruction("mov QWORD PTR [r9], 0");                           // forget the block before freeing it
        abi::emit_symbol_address(emitter, "r9", "_print_r_cap");
        emitter.instruction("mov QWORD PTR [r9], 0");                           // back to the static capacity
        emitter.bl_c("free");                                                   // release the grown block
        emitter.label("__rt_pr_release_done_x86");
        emitter.instruction("mov rsp, rbp");                                    // release the helper frame
        emitter.instruction("pop rbp");                                         // restore the caller frame pointer
        emitter.instruction("ret");                                             // return to the caller
        return;
    }
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the helper frame pointer
    abi::emit_symbol_address(emitter, "x9", "_print_r_base");
    emitter.instruction("ldr x0, [x9]");                                        // heap capture block, if any
    emitter.instruction("cbz x0, __rt_pr_release_done");                        // still on the static buffer: nothing to free
    emitter.instruction("str xzr, [x9]");                                       // forget the block before freeing it
    abi::emit_symbol_address(emitter, "x9", "_print_r_cap");
    emitter.instruction("str xzr, [x9]");                                       // back to the static capacity
    emitter.bl_c("free");                                                       // release the grown block
    emitter.label("__rt_pr_release_done");
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits `__rt_pr_append`: append `len` bytes from `buf` to the capture buffer at
/// `_print_r_off` and advance the offset. No mode check. A write that would pass the buffer
/// capacity first grows it through `__rt_pr_reserve`, so no capture is ever truncated.
///
/// Inputs: AArch64 x0=buf, x1=len / x86_64 rdi=buf, rsi=len. No result.
pub fn emit_pr_append(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_pr_append_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: pr_append ---");
    emitter.label_global("__rt_pr_append");

    // -- set up a frame and spill the incoming buf/len across the helper calls --
    emitter.instruction("sub sp, sp, #32");                                     // allocate the append-helper frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the append-helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the source byte pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the source byte length

    // -- grow the buffer when the write would pass its capacity --
    abi::emit_symbol_address(emitter, "x9", "_print_r_off");                    // materialize the address of the append-offset global
    emitter.instruction("ldr x10, [x9]");                                       // load the current append offset
    emitter.instruction("add x0, x10, x1");                                     // total bytes needed after this write
    abi::emit_symbol_address(emitter, "x9", "_print_r_cap");                    // materialize the address of the capacity global
    emitter.instruction("ldr x11, [x9]");                                       // load the recorded capacity
    emitter.instruction("cbnz x11, __rt_pr_append_cap");                        // explicit capacity recorded
    emitter.instruction("mov x11, #65536");                                     // zero means the static 64 KiB buffer
    emitter.label("__rt_pr_append_cap");
    emitter.instruction("cmp x0, x11");                                         // does the write fit?
    emitter.instruction("b.ls __rt_pr_append_fits");                            // yes: copy straight in
    spill_walker_registers(emitter, false);
    emitter.instruction("bl __rt_pr_reserve");                                  // grow the capture buffer to the needed size
    spill_walker_registers(emitter, true);
    emitter.label("__rt_pr_append_fits");
    emitter.instruction("bl __rt_pr_base");                                     // x0 = active capture buffer
    emitter.instruction("mov x11, x0");                                         // keep the buffer base for the copy loop
    abi::emit_symbol_address(emitter, "x9", "_print_r_off");                    // materialize the address of the append-offset global
    emitter.instruction("ldr x10, [x9]");                                       // reload the append offset

    // -- copy len bytes from buf to buf_base + off (byte loop) --
    emitter.label("__rt_pr_append_loop");
    emitter.instruction("ldr x12, [sp, #8]");                                   // reload the remaining byte count
    emitter.instruction("cbz x12, __rt_pr_append_done");                        // no more bytes → finish
    emitter.instruction("ldr x13, [sp, #0]");                                   // reload the current source cursor
    emitter.instruction("ldrb w14, [x13]");                                     // load the next source byte
    emitter.instruction("strb w14, [x11, x10]");                                // store the byte at buf_base + off
    emitter.instruction("add x10, x10, #1");                                    // advance the destination offset
    emitter.instruction("add x13, x13, #1");                                    // advance the source cursor
    emitter.instruction("str x13, [sp, #0]");                                   // save the advanced source cursor
    emitter.instruction("sub x12, x12, #1");                                    // decrement the remaining count
    emitter.instruction("str x12, [sp, #8]");                                   // save the decremented remaining count
    emitter.instruction("b __rt_pr_append_loop");                               // continue copying

    emitter.label("__rt_pr_append_done");
    abi::emit_symbol_address(emitter, "x9", "_print_r_off");                    // materialize the address of the append-offset global
    emitter.instruction("str x10, [x9]");                                       // store the advanced append offset
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the append-helper frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 variant of `__rt_pr_append`.
fn emit_pr_append_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: pr_append ---");
    emitter.label_global("__rt_pr_append");

    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the append-helper frame pointer
    emitter.instruction("sub rsp, 32");                                         // allocate the append-helper frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the source byte pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the source byte length

    // -- grow the buffer when the write would pass its capacity --
    abi::emit_symbol_address(emitter, "r9", "_print_r_off");                    // materialize the address of the append-offset global
    emitter.instruction("mov rdi, QWORD PTR [r9]");                             // load the current append offset
    emitter.instruction("add rdi, rsi");                                        // total bytes needed after this write
    abi::emit_symbol_address(emitter, "r9", "_print_r_cap");                    // materialize the address of the capacity global
    emitter.instruction("mov rcx, QWORD PTR [r9]");                             // load the recorded capacity
    emitter.instruction("test rcx, rcx");                                       // zero means the static 64 KiB buffer
    emitter.instruction("jnz __rt_pr_append_cap_x86");                          // explicit capacity recorded
    emitter.instruction("mov ecx, 65536");                                      // static buffer capacity
    emitter.label("__rt_pr_append_cap_x86");
    emitter.instruction("cmp rdi, rcx");                                        // does the write fit?
    emitter.instruction("jbe __rt_pr_append_fits_x86");                         // yes: copy straight in
    spill_walker_registers(emitter, false);
    emitter.instruction("call __rt_pr_reserve");                                // grow the capture buffer to the needed size
    spill_walker_registers(emitter, true);
    emitter.label("__rt_pr_append_fits_x86");
    emitter.instruction("call __rt_pr_base");                                   // rax = active capture buffer
    emitter.instruction("mov r11, rax");                                        // keep the buffer base for the copy loop
    abi::emit_symbol_address(emitter, "r9", "_print_r_off");                    // materialize the address of the append-offset global
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // reload the append offset

    emitter.label("__rt_pr_append_loop_x86");
    emitter.instruction("mov rcx, QWORD PTR [rbp - 16]");                       // reload the remaining byte count
    emitter.instruction("test rcx, rcx");                                       // any bytes left to copy?
    emitter.instruction("jz __rt_pr_append_done_x86");                          // no more bytes → finish
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the current source cursor
    emitter.instruction("mov dl, BYTE PTR [rax]");                              // load the next source byte
    emitter.instruction("mov BYTE PTR [r11 + r10], dl");                        // store the byte at buf_base + off
    emitter.instruction("add r10, 1");                                          // advance the destination offset
    emitter.instruction("add rax, 1");                                          // advance the source cursor
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the advanced source cursor
    emitter.instruction("sub rcx, 1");                                          // decrement the remaining count
    emitter.instruction("mov QWORD PTR [rbp - 16], rcx");                       // save the decremented remaining count
    emitter.instruction("jmp __rt_pr_append_loop_x86");                         // continue copying

    emitter.label("__rt_pr_append_done_x86");
    abi::emit_symbol_address(emitter, "r9", "_print_r_off");                    // materialize the address of the append-offset global
    emitter.instruction("mov QWORD PTR [r9], r10");                             // store the advanced append offset
    emitter.instruction("add rsp, 32");                                         // release the append-helper frame
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits `__rt_pr_write`: the print_r walker terminal-write indirection.
/// Branches on `_print_r_mode`: 1 → append to the capture buffer via
/// `__rt_pr_append`; 0 → the plain write path, which itself appends to the top
/// output buffer via `__rt_ob_append` while `_ob_level` is non-zero and only
/// falls through to the raw `write(1, buf, len)` syscall otherwise.
///
/// Inputs match the walker's pre-syscall register layout:
/// AArch64 x1=buf, x2=len / x86_64 rsi=buf, rdx=len. No result.
pub fn emit_pr_write(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_pr_write_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: pr_write ---");
    emitter.label_global("__rt_pr_write");

    // -- set up a frame so the append branch can call __rt_pr_append --
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // save frame pointer and return address (the append branch clobbers x30)
    emitter.instruction("mov x29, sp");                                         // establish a frame pointer for the call

    // -- branch on the capture-mode flag --
    abi::emit_symbol_address(emitter, "x9", "_print_r_mode");                   // materialize the address of the capture-mode flag
    emitter.instruction("ldr x9, [x9]");                                        // load the capture-mode flag
    emitter.instruction("cbz x9, __rt_pr_write_syscall");                       // mode 0 → plain write syscall
    emitter.instruction("mov x0, x1");                                          // __rt_pr_append buf arg = incoming buf pointer
    emitter.instruction("mov x1, x2");                                          // __rt_pr_append len arg = incoming length
    emitter.instruction("bl __rt_pr_append");                                   // append the bytes to the capture buffer
    emitter.instruction("b __rt_pr_write_done");                                // skip the syscall path

    // -- plain write path: while the ob_* stack is non-empty, append to the top
    //    output buffer instead of issuing the write(1, …) syscall so print_r's
    //    structural output is captured like every other terminal write. --
    emitter.label("__rt_pr_write_syscall");
    abi::emit_symbol_address(emitter, "x9", "_ob_in_handler");                  // materialize the address of the in-handler flag
    emitter.instruction("ldr x9, [x9]");                                        // load the in-handler flag
    emitter.instruction("cbnz x9, __rt_pr_write_done");                         // inside a handler — discard the bytes entirely
    abi::emit_symbol_address(emitter, "x9", "_ob_level");                       // materialize the address of the output-buffer stack depth
    emitter.instruction("ldr x9, [x9]");                                        // load the output-buffer stack depth
    emitter.instruction("cbz x9, __rt_pr_write_raw");                           // no active output buffer — take the raw syscall path
    emitter.instruction("mov x0, x1");                                          // __rt_ob_append buf arg = incoming buf pointer
    emitter.instruction("mov x1, x2");                                          // __rt_ob_append len arg = incoming length
    emitter.instruction("bl __rt_ob_append");                                   // append the bytes to the top output buffer
    emitter.instruction("b __rt_pr_write_done");                                // capture handled the bytes — skip the syscall path
    emitter.label("__rt_pr_write_raw");
    emitter.instruction("mov x0, #1");                                          // syscall fd = stdout (buf already in x1, len already in x2)
    emitter.syscall(4);                                                         // write the bytes to stdout

    emitter.label("__rt_pr_write_done");
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 variant of `__rt_pr_write`.
fn emit_pr_write_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: pr_write ---");
    emitter.label_global("__rt_pr_write");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer and align rsp for the append-branch call
    emitter.instruction("mov rbp, rsp");                                        // establish a frame base

    abi::emit_symbol_address(emitter, "r11", "_print_r_mode");                  // materialize the address of the capture-mode flag
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // load the capture-mode flag
    emitter.instruction("test r11, r11");                                       // is return-mode capture enabled?
    emitter.instruction("jz __rt_pr_write_syscall");                            // mode 0 → plain write syscall
    emitter.instruction("mov rdi, rsi");                                        // __rt_pr_append buf arg = incoming buf pointer
    emitter.instruction("mov rsi, rdx");                                        // __rt_pr_append len arg = incoming length
    emitter.instruction("call __rt_pr_append");                                 // append the bytes to the capture buffer
    emitter.instruction("jmp __rt_pr_write_done");                              // skip the syscall path

    // -- plain write path: while the ob_* stack is non-empty, append to the top
    //    output buffer instead of issuing the write(1, …) syscall so print_r's
    //    structural output is captured like every other terminal write. --
    emitter.label("__rt_pr_write_syscall");
    abi::emit_symbol_address(emitter, "r11", "_ob_in_handler");                 // materialize the address of the in-handler flag
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // load the in-handler flag
    emitter.instruction("test r11, r11");                                       // is a user output handler running?
    emitter.instruction("jnz __rt_pr_write_done");                              // inside a handler — discard the bytes entirely
    abi::emit_symbol_address(emitter, "r11", "_ob_level");                      // materialize the address of the output-buffer stack depth
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // load the output-buffer stack depth
    emitter.instruction("test r11, r11");                                       // is any output buffer active?
    emitter.instruction("jz __rt_pr_write_raw_x86");                            // no active output buffer — take the raw syscall path
    emitter.instruction("mov rdi, rsi");                                        // __rt_ob_append buf arg = incoming buf pointer
    emitter.instruction("mov rsi, rdx");                                        // __rt_ob_append len arg = incoming length
    emitter.instruction("call __rt_ob_append");                                 // append the bytes to the top output buffer
    emitter.instruction("jmp __rt_pr_write_done");                              // capture handled the bytes — skip the syscall path
    emitter.label("__rt_pr_write_raw_x86");
    emitter.instruction("mov rdi, 1");                                          // syscall fd = stdout
    // buf is already in rsi; len is already in rdx
    emitter.instruction("mov eax, 1");                                          // Linux x86_64 syscall 1 = write
    emitter.instruction("syscall");                                             // write the bytes to stdout

    emitter.label("__rt_pr_write_done");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits `__rt_pr_flush_deferred`: write out the text of a deferred echo-mode `print_r`.
///
/// Does nothing unless `_print_r_mode` is 2. Otherwise it switches the mode back to 0, sends the
/// buffered bytes through `__rt_pr_write` (so output buffers and `ob_start` callbacks see them
/// exactly as a streaming `print_r` would have delivered them), resets the offset and releases a
/// grown buffer. No inputs and no result; clobbers caller-saved registers.
pub fn emit_pr_flush_deferred(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: pr_flush_deferred ---");
    emitter.label_global("__rt_pr_flush_deferred");
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("push rbp");                                        // preserve the caller frame pointer and align rsp for the write call
        emitter.instruction("mov rbp, rsp");                                    // establish a frame base
        abi::emit_symbol_address(emitter, "r11", "_print_r_mode");              // materialize the address of the capture-mode flag
        emitter.instruction("cmp QWORD PTR [r11], 2");                          // is a deferred echo-mode print_r active?
        emitter.instruction("jne __rt_pr_flush_deferred_done_x86");             // no: nothing to write
        emitter.instruction("mov QWORD PTR [r11], 0");                          // leave capture mode before writing
        abi::emit_symbol_address(emitter, "r11", "_print_r_off");               // materialize the address of the append-offset global
        emitter.instruction("mov rdx, QWORD PTR [r11]");                        // buffered byte count
        emitter.instruction("mov QWORD PTR [r11], 0");                          // the buffer is empty after this flush
        emitter.instruction("test rdx, rdx");                                   // anything buffered?
        emitter.instruction("jz __rt_pr_flush_deferred_release_x86");           // no: nothing to write
        emitter.instruction("push rdx");                                        // keep the byte count across the base lookup
        emitter.instruction("push rdx");                                        // keep the stack 16-byte aligned
        emitter.instruction("call __rt_pr_base");                               // rax = buffered bytes
        emitter.instruction("pop rdx");                                         // restore the alignment pad
        emitter.instruction("pop rdx");                                         // restore the byte count
        emitter.instruction("mov rsi, rax");                                    // buffered bytes
        emitter.instruction("call __rt_pr_write");                              // plain write path (output buffers or stdout)
        emitter.label("__rt_pr_flush_deferred_release_x86");
        emitter.instruction("call __rt_pr_release");                            // free a grown buffer
        emitter.label("__rt_pr_flush_deferred_done_x86");
        emitter.instruction("pop rbp");                                         // restore the caller frame pointer
        emitter.instruction("ret");                                             // return to the caller
        return;
    }
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish a frame pointer for the call
    abi::emit_symbol_address(emitter, "x9", "_print_r_mode");                   // materialize the address of the capture-mode flag
    emitter.instruction("ldr x10, [x9]");                                       // load the capture mode
    emitter.instruction("cmp x10, #2");                                         // is a deferred echo-mode print_r active?
    emitter.instruction("b.ne __rt_pr_flush_deferred_done");                    // no: nothing to write
    emitter.instruction("str xzr, [x9]");                                       // leave capture mode before writing
    abi::emit_symbol_address(emitter, "x9", "_print_r_off");                    // materialize the address of the append-offset global
    emitter.instruction("ldr x2, [x9]");                                        // buffered byte count
    emitter.instruction("str xzr, [x9]");                                       // the buffer is empty after this flush
    emitter.instruction("cbz x2, __rt_pr_flush_deferred_release");              // nothing buffered: nothing to write
    emitter.instruction("str x2, [sp, #-16]!");                                 // keep the byte count across the base lookup
    emitter.instruction("bl __rt_pr_base");                                     // x0 = buffered bytes
    emitter.instruction("ldr x2, [sp], #16");                                   // restore the byte count
    emitter.instruction("mov x1, x0");                                          // buffered bytes
    emitter.instruction("bl __rt_pr_write");                                    // plain write path (output buffers or stdout)
    emitter.label("__rt_pr_flush_deferred_release");
    emitter.instruction("bl __rt_pr_release");                                  // free a grown buffer
    emitter.label("__rt_pr_flush_deferred_done");
    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits `__rt_pr_finish`: materialize the captured bytes as an owned heap
/// string, release a grown buffer, reset the capture state, and return the string in the platform
/// string result registers (AArch64 x1=ptr, x2=len / x86_64 rax=ptr, rdx=len).
///
/// No inputs. Calls `__rt_str_persist` with the active buffer and `_print_r_off`,
/// then stores 0 back to `_print_r_off` and `_print_r_mode`.
pub fn emit_pr_finish(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_pr_finish_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: pr_finish ---");
    emitter.label_global("__rt_pr_finish");

    // -- set up a frame across the helper calls --
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish a frame pointer for the call

    // -- load the captured length and buffer pointer into the str_persist ABI --
    emitter.instruction("bl __rt_pr_base");                                     // x0 = active capture buffer
    emitter.instruction("mov x1, x0");                                          // buffer address into the string-pointer register
    abi::emit_symbol_address(emitter, "x9", "_print_r_off");                    // materialize the address of the append-offset global
    emitter.instruction("ldr x2, [x9]");                                        // load the captured byte length into the string-length result reg
    emitter.instruction("bl __rt_str_persist");                                 // persist the buffer as an owned heap string → x1=ptr, x2=len

    // -- free a grown buffer (the result string owns its own copy) --
    emitter.instruction("stp x1, x2, [sp, #-16]!");                             // keep the result across the release call
    emitter.instruction("bl __rt_pr_release");                                  // free a grown capture block
    emitter.instruction("ldp x1, x2, [sp], #16");                               // restore the result string

    // -- reset the capture state (x1/x2 result survives scratch stores) --
    abi::emit_symbol_address(emitter, "x9", "_print_r_off");                    // materialize the address of the append-offset global
    emitter.instruction("str xzr, [x9]");                                       // reset the append offset to 0
    abi::emit_symbol_address(emitter, "x9", "_print_r_mode");                   // materialize the address of the capture-mode flag
    emitter.instruction("str xzr, [x9]");                                       // reset the capture-mode flag to 0 (restore stdout mode)

    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return with x1=owned string ptr, x2=length
}

/// Emits the Linux x86_64 variant of `__rt_pr_finish`.
fn emit_pr_finish_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: pr_finish ---");
    emitter.label_global("__rt_pr_finish");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer across the helper calls
    emitter.instruction("mov rbp, rsp");                                        // establish a frame base

    emitter.instruction("call __rt_pr_base");                                   // rax = active capture buffer
    abi::emit_symbol_address(emitter, "r11", "_print_r_off");                   // materialize the address of the append-offset global
    emitter.instruction("mov rdx, QWORD PTR [r11]");                            // load the captured byte length into the string-length result reg
    emitter.instruction("call __rt_str_persist");                               // persist the buffer as an owned heap string → rax=ptr, rdx=len

    emitter.instruction("push rax");                                            // keep the result pointer across the release call
    emitter.instruction("push rdx");                                            // keep the result length across the release call
    emitter.instruction("call __rt_pr_release");                                // free a grown capture block
    emitter.instruction("pop rdx");                                             // restore the result length
    emitter.instruction("pop rax");                                             // restore the result pointer

    abi::emit_symbol_address(emitter, "r11", "_print_r_off");                   // materialize the address of the append-offset global
    emitter.instruction("mov QWORD PTR [r11], 0");                              // reset the append offset to 0
    abi::emit_symbol_address(emitter, "r11", "_print_r_mode");                  // materialize the address of the capture-mode flag
    emitter.instruction("mov QWORD PTR [r11], 0");                              // reset the capture-mode flag to 0 (restore stdout mode)

    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return with rax=owned string ptr, rdx=length
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Arch, Platform, Target};

    /// Renders all three print_r return-mode helpers for one target.
    fn render(platform: Platform, arch: Arch) -> String {
        let mut emitter = Emitter::new(Target::new(platform, arch));
        emit_pr_append(&mut emitter);
        emit_pr_write(&mut emitter);
        emit_pr_finish(&mut emitter);
        emit_pr_base(&mut emitter);
        emit_pr_reserve(&mut emitter);
        emit_pr_release(&mut emitter);
        emit_pr_flush_deferred(&mut emitter);
        emitter.output()
    }

    /// Verifies every target exports the three helper labels.
    #[test]
    fn emits_global_labels_for_all_targets() {
        for (platform, arch) in [
            (Platform::MacOS, Arch::AArch64),
            (Platform::Linux, Arch::AArch64),
            (Platform::Linux, Arch::X86_64),
        ] {
            let asm = render(platform, arch);
            assert!(
                asm.contains(".globl __rt_pr_append\n"),
                "missing __rt_pr_append label for {:?}/{:?}",
                platform,
                arch
            );
            assert!(
                asm.contains(".globl __rt_pr_write\n"),
                "missing __rt_pr_write label for {:?}/{:?}",
                platform,
                arch
            );
            assert!(
                asm.contains(".globl __rt_pr_finish\n"),
                "missing __rt_pr_finish label for {:?}/{:?}",
                platform,
                arch
            );
        }
    }

    /// Verifies the walker indirection branches on the capture-mode flag and
    /// references the append helper on every target.
    #[test]
    fn pr_write_branches_on_mode_and_calls_append() {
        let mac = render(Platform::MacOS, Arch::AArch64);
        assert!(mac.contains("_print_r_mode"));
        assert!(mac.contains("bl __rt_pr_append"));
        assert!(mac.contains("__rt_pr_write_syscall"));

        let linux_x86 = render(Platform::Linux, Arch::X86_64);
        assert!(linux_x86.contains("_print_r_mode"));
        assert!(linux_x86.contains("call __rt_pr_append"));
        assert!(linux_x86.contains("__rt_pr_write_syscall"));
    }

    /// Verifies the finish helper persists the buffer and resets the state.
    #[test]
    fn pr_finish_persists_and_resets_state() {
        let mac = render(Platform::MacOS, Arch::AArch64);
        assert!(mac.contains("bl __rt_str_persist"));
        assert!(mac.contains("_print_r_off"));
        assert!(mac.contains("_print_r_buf"));
        assert!(mac.contains("bl __rt_pr_release"));

        let linux_x86 = render(Platform::Linux, Arch::X86_64);
        assert!(linux_x86.contains("call __rt_str_persist"));
        assert!(linux_x86.contains("_print_r_off"));
        assert!(linux_x86.contains("_print_r_buf"));
        assert!(linux_x86.contains("call __rt_pr_release"));
    }

    /// Verifies the capture buffer grows instead of truncating: the appender calls the reserve helper, which
    /// mallocs/reallocs the block, and the release helper frees it.
    #[test]
    fn pr_append_grows_the_capture_through_the_heap() {
        for (platform, arch, call) in [
            (Platform::MacOS, Arch::AArch64, "bl __rt_pr_reserve"),
            (Platform::Linux, Arch::X86_64, "call __rt_pr_reserve"),
        ] {
            let asm = render(platform, arch);
            assert!(asm.contains(call), "{platform:?}/{arch:?}");
            assert!(asm.contains("_print_r_base") && asm.contains("_print_r_cap"), "{platform:?}/{arch:?}");
            assert!(asm.contains("malloc") && asm.contains("realloc") && asm.contains("free"), "{platform:?}/{arch:?}");
        }
    }
}
