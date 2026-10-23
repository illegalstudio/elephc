//! Purpose:
//! Emits the `print_r` return-mode capture helpers: `__rt_pr_append`,
//! `__rt_pr_write`, and `__rt_pr_finish`. These let `print_r($value, true)`
//! render into an in-memory buffer instead of stdout, then return the captured
//! bytes as an owned heap string.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::io`.
//! - The `print_r` runtime walkers (`__rt_print_r_*`) call `__rt_pr_write`
//!   instead of a raw `write(1, …)` syscall so return-mode captures their bytes.
//! - `__rt_stdout_write` tail-calls `__rt_pr_append` when `_print_r_mode` is set
//!   so codegen-side scalar/literal writes are captured too.
//! - The `print_r` builtin codegen (`crate::codegen::lower_inst::builtins::debug`)
//!   calls `__rt_pr_finish` to materialize the returned string.
//!
//! Key details:
//! - `_print_r_mode` (0/1), `_print_r_off` (byte count), and `_print_r_buf`
//!   (64 KiB) live in the fixed runtime data section and default to zero.
//! - `__rt_pr_append` owns the byte-copy loop and clamps the copy length to the
//!   remaining buffer capacity (oversized captures truncate at 64 KiB instead of
//!   overflowing adjacent data); `__rt_pr_write` is the walker-ABI wrapper that
//!   branches on mode (syscall vs. append); `__rt_pr_finish` persists the buffer
//!   through `__rt_str_persist` and resets the state.
//! - Return ABI of `__rt_pr_finish` matches the platform string result registers
//!   (AArch64 x1=ptr, x2=len; x86_64 rax=ptr, rdx=len) so the caller stores it
//!   directly as a `Str` value.

use crate::codegen_support::abi;
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Emits `__rt_pr_append`: append `len` bytes from `buf` to `_print_r_buf` at
/// `_print_r_off` and advance the offset. No mode check. The length is clamped
/// to the remaining buffer capacity (64 KiB total), so oversized captures
/// truncate instead of overflowing into adjacent runtime data.
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

    // -- frame: [0]=buf, [8]=len, [16]=len copy, [24]=new cap, [32]=offset, [48]=fp/lr --
    emitter.instruction("sub sp, sp, #64");                                     // allocate the append-helper frame
    emitter.instruction("stp x29, x30, [sp, #48]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #48");                                    // establish the append-helper frame pointer
    emitter.instruction("stp x0, x1, [sp]");                                    // save the source byte pointer and length

    // -- load the current capture state --
    abi::emit_load_symbol_to_reg(emitter, "x9", "_print_r_base", 0);
    abi::emit_load_symbol_to_reg(emitter, "x10", "_print_r_off", 0);
    abi::emit_load_symbol_to_reg(emitter, "x11", "_print_r_cap", 0);
    abi::emit_load_symbol_to_reg(emitter, "x9", "_print_r_base", 0);            // reload the base after the symbol-load helper reused its scratch register
    emitter.instruction("ldr x12, [sp, #8]");                                   // reload the requested byte length
    emitter.instruction("adds x13, x10, x12");                                  // end offset after this append
    let len_ok = "__rt_pr_append_len_ok";
    emitter.instruction(&format!("b.cc {}", len_ok));                           // a representable append continues to the capacity check
    emitter.instruction("b __rt_alloc_overflow");                               // a wrapped append can never fit any buffer
    emitter.label(len_ok);
    emitter.instruction("cmp x13, x11");                                        // does the append still fit the current capture block?
    emitter.instruction("b.ls __rt_pr_append_copy");                            // copy directly while the block has room

    // -- grow the capture block and publish the new base/capacity --
    emitter.instruction("str x12, [sp, #16]");                                  // preserve the requested length across the grow call
    emitter.instruction("str x10, [sp, #32]");                                  // preserve the current offset across the grow call
    emitter.instruction("lsl x2, x11, #1");                                     // start with double the current capacity
    emitter.instruction("cmp x2, x13");                                         // is the doubled capacity enough for this append?
    emitter.instruction("csel x2, x13, x2, lo");                                // otherwise grow straight to the required total
    emitter.instruction("str x2, [sp, #24]");                                   // preserve the new capacity across the grow call
    emitter.instruction("mov x0, x9");                                          // pass the current base
    emitter.instruction("mov x1, x10");                                         // pass the bytes already captured
    emitter.instruction("bl __rt_concat_grow");                                 // allocate, copy, and release the old capture block
    abi::emit_store_reg_to_symbol(emitter, "x0", "_print_r_base", 0);
    emitter.instruction("ldr x2, [sp, #24]");                                   // reload the new capacity
    abi::emit_store_reg_to_symbol(emitter, "x2", "_print_r_cap", 0);
    emitter.instruction("mov x9, x0");                                          // new capture base
    emitter.instruction("ldr x10, [sp, #32]");                                  // reload the current offset
    emitter.instruction("ldr x12, [sp, #16]");                                  // reload the requested length

    // -- copy len bytes from the source into the capture block --
    emitter.label("__rt_pr_append_copy");
    emitter.instruction("ldr x11, [sp, #0]");                                   // reload the source byte pointer
    emitter.label("__rt_pr_append_loop");
    emitter.instruction("cbz x12, __rt_pr_append_done");                        // no more bytes → finish
    emitter.instruction("ldrb w13, [x11], #1");                                 // load the next source byte
    emitter.instruction("strb w13, [x9, x10]");                                 // store it at capture_base + offset
    emitter.instruction("add x10, x10, #1");                                    // advance the destination offset
    emitter.instruction("sub x12, x12, #1");                                    // decrement the remaining count
    emitter.instruction("b __rt_pr_append_loop");                               // continue copying

    emitter.label("__rt_pr_append_done");
    abi::emit_store_reg_to_symbol(emitter, "x10", "_print_r_off", 0);
    emitter.instruction("ldp x29, x30, [sp, #48]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #64");                                     // release the append-helper frame
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 variant of `__rt_pr_append`.
fn emit_pr_append_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: pr_append ---");
    emitter.label_global("__rt_pr_append");

    emitter.instruction("push rbp");                                            // save the caller frame pointer
    emitter.instruction("mov rbp, rsp");                                        // establish the append-helper frame pointer
    emitter.instruction("sub rsp, 64");                                         // allocate the append-helper frame
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the source byte pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the source byte length

    abi::emit_load_symbol_to_reg(emitter, "r9", "_print_r_base", 0);
    abi::emit_load_symbol_to_reg(emitter, "r10", "_print_r_off", 0);
    abi::emit_load_symbol_to_reg(emitter, "r11", "_print_r_cap", 0);
    abi::emit_load_symbol_to_reg(emitter, "r9", "_print_r_base", 0);            // reload the base after the symbol-load helper reused its scratch register
    emitter.instruction("mov rax, r10");                                        // copy the offset before computing the end
    emitter.instruction("add rax, rsi");                                        // end offset after this append
    let len_ok = "__rt_pr_append_len_ok_x86";
    emitter.instruction(&format!("jnc {}", len_ok));                            // a representable append continues to the capacity check
    emitter.instruction("jmp __rt_alloc_overflow");                             // a wrapped append can never fit any buffer
    emitter.label(len_ok);
    emitter.instruction("cmp rax, r11");                                        // does the append still fit the current capture block?
    emitter.instruction("jbe __rt_pr_append_copy_x86");                         // copy directly while the block has room

    emitter.instruction("mov QWORD PTR [rbp - 24], rsi");                       // preserve the requested length
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // preserve the current offset
    emitter.instruction("mov rsi, r11");                                        // start with double the current capacity
    emitter.instruction("shl rsi, 1");                                          // double it
    emitter.instruction("cmp rsi, rax");                                        // is the doubled capacity enough?
    emitter.instruction("jae __rt_pr_append_cap_ready_x86");                    // keep the doubled capacity when it is
    emitter.instruction("mov rsi, rax");                                        // otherwise grow straight to the required total
    emitter.label("__rt_pr_append_cap_ready_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], rsi");                       // preserve the new capacity
    emitter.instruction("mov rax, r9");                                         // pass the current base
    emitter.instruction("mov rdi, r10");                                        // pass the bytes already captured
    emitter.instruction("call __rt_concat_grow");                               // allocate, copy, and release the old capture block
    abi::emit_store_reg_to_symbol(emitter, "rax", "_print_r_base", 0);
    emitter.instruction("mov r9, rax");                                         // new capture base
    emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                       // reload the new capacity
    abi::emit_store_reg_to_symbol(emitter, "rax", "_print_r_cap", 0);
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current offset
    emitter.instruction("mov rsi, QWORD PTR [rbp - 24]");                       // reload the requested length

    emitter.label("__rt_pr_append_copy_x86");
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the source byte pointer
    emitter.label("__rt_pr_append_loop_x86");
    emitter.instruction("test rsi, rsi");                                       // any bytes left to copy?
    emitter.instruction("jz __rt_pr_append_done_x86");                          // no more bytes → finish
    emitter.instruction("mov al, BYTE PTR [rdi]");                              // load the next source byte
    emitter.instruction("mov BYTE PTR [r9 + r10], al");                         // store it at capture_base + offset
    emitter.instruction("add r10, 1");                                          // advance the destination offset
    emitter.instruction("add rdi, 1");                                          // advance the source cursor
    emitter.instruction("sub rsi, 1");                                          // decrement the remaining count
    emitter.instruction("jmp __rt_pr_append_loop_x86");                         // continue copying

    emitter.label("__rt_pr_append_done_x86");
    abi::emit_store_reg_to_symbol(emitter, "r10", "_print_r_off", 0);
    emitter.instruction("add rsp, 64");                                         // release the append-helper frame
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

/// Emits `__rt_pr_finish`: materialize the captured bytes as an owned heap
/// string, reset the capture state, and return the string in the platform string
/// result registers (AArch64 x1=ptr, x2=len / x86_64 rax=ptr, rdx=len).
///
/// No inputs. Calls `__rt_str_persist` with ptr=`_print_r_buf` and len=`_print_r_off`,
/// then stores 0 back to `_print_r_off` and `_print_r_mode`.
pub fn emit_pr_finish(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_pr_finish_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: pr_finish ---");
    emitter.label_global("__rt_pr_finish");

    // -- set up a frame across the str_persist call --
    emitter.instruction("stp x29, x30, [sp, #-16]!");                           // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish a frame pointer for the call

    // -- load the captured length and buffer pointer into the str_persist ABI --
    abi::emit_symbol_address(emitter, "x9", "_print_r_off");                    // materialize the address of the append-offset global
    emitter.instruction("ldr x2, [x9]");                                        // load the captured byte length into the string-length result reg
    abi::emit_load_symbol_to_reg(emitter, "x1", "_print_r_base", 0);            // load the current capture-buffer address into the string-pointer result reg
    emitter.instruction("bl __rt_str_persist");                                 // persist the buffer as an owned heap string → x1=ptr, x2=len

    // -- reset the capture state (x1/x2 result survives scratch stores) --
    abi::emit_symbol_address(emitter, "x9", "_print_r_off");                    // materialize the address of the append-offset global
    emitter.instruction("str xzr, [x9]");                                       // reset the append offset to 0
    abi::emit_symbol_address(emitter, "x9", "_print_r_mode");                   // materialize the address of the capture-mode flag
    emitter.instruction("str xzr, [x9]");                                       // reset the capture-mode flag to 0 (restore stdout mode)
    emitter.instruction("mov x10, #65536");                                     // restore the inline capture capacity
    abi::emit_store_reg_to_symbol(emitter, "x10", "_print_r_cap", 0);
    abi::emit_symbol_address(emitter, "x9", "_print_r_buf");                    // restore the inline capture buffer
    abi::emit_store_reg_to_symbol(emitter, "x9", "_print_r_base", 0);

    emitter.instruction("ldp x29, x30, [sp], #16");                             // restore frame pointer and return address
    emitter.instruction("ret");                                                 // return with x1=owned string ptr, x2=length
}

/// Emits the Linux x86_64 variant of `__rt_pr_finish`.
fn emit_pr_finish_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: pr_finish ---");
    emitter.label_global("__rt_pr_finish");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer across the str_persist call
    emitter.instruction("mov rbp, rsp");                                        // establish a frame base

    abi::emit_symbol_address(emitter, "r11", "_print_r_off");                   // materialize the address of the append-offset global
    emitter.instruction("mov rdx, QWORD PTR [r11]");                            // load the captured byte length into the string-length result reg
    abi::emit_load_symbol_to_reg(emitter, "rax", "_print_r_base", 0);           // load the current capture-buffer address into the string-pointer result reg
    emitter.instruction("call __rt_str_persist");                               // persist the buffer as an owned heap string → rax=ptr, rdx=len

    abi::emit_symbol_address(emitter, "r11", "_print_r_off");                   // materialize the address of the append-offset global
    emitter.instruction("mov QWORD PTR [r11], 0");                              // reset the append offset to 0
    abi::emit_symbol_address(emitter, "r11", "_print_r_mode");                  // materialize the address of the capture-mode flag
    emitter.instruction("mov QWORD PTR [r11], 0");                              // reset the capture-mode flag to 0 (restore stdout mode)
    abi::emit_symbol_address(emitter, "r10", "_print_r_buf");                   // restore the inline capture buffer
    abi::emit_store_reg_to_symbol(emitter, "r10", "_print_r_base", 0);
    abi::emit_store_imm_to_symbol(emitter, "_print_r_cap", 0, 65536);

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

        let linux_x86 = render(Platform::Linux, Arch::X86_64);
        assert!(linux_x86.contains("call __rt_str_persist"));
        assert!(linux_x86.contains("_print_r_off"));
        assert!(linux_x86.contains("_print_r_buf"));
    }
}
