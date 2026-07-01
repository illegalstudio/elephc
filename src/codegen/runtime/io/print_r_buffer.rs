//! Purpose:
//! Emits the `print_r` return-mode capture helpers: `__rt_pr_append`,
//! `__rt_pr_write`, and `__rt_pr_finish`. These let `print_r($value, true)`
//! render into an in-memory buffer instead of stdout, then return the captured
//! bytes as an owned heap string.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::io`.
//! - The `print_r` runtime walkers (`__rt_print_r_*`) call `__rt_pr_write`
//!   instead of a raw `write(1, …)` syscall so return-mode captures their bytes.
//! - `__rt_stdout_write` tail-calls `__rt_pr_append` when `_print_r_mode` is set
//!   so codegen-side scalar/literal writes are captured too.
//! - The `print_r` builtin codegen (`codegen_ir::lower_inst::builtins::debug`)
//!   calls `__rt_pr_finish` to materialize the returned string.
//!
//! Key details:
//! - `_print_r_mode` (0/1), `_print_r_off` (byte count), and `_print_r_buf`
//!   (64 KiB) live in the fixed runtime data section and default to zero.
//! - `__rt_pr_append` owns the byte-copy loop; `__rt_pr_write` is the walker-ABI
//!   wrapper that branches on mode (syscall vs. append); `__rt_pr_finish`
//!   persists the buffer through `__rt_str_persist` and resets the state.
//! - Return ABI of `__rt_pr_finish` matches the platform string result registers
//!   (AArch64 x1=ptr, x2=len; x86_64 rax=ptr, rdx=len) so the caller stores it
//!   directly as a `Str` value.

use crate::codegen::abi;
use crate::codegen::{emit::Emitter, platform::Arch};

/// Emits `__rt_pr_append`: append `len` bytes from `buf` to `_print_r_buf` at
/// `_print_r_off` and advance the offset. No mode check.
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

    // -- set up a frame and spill the incoming buf/len across the offset loads --
    emitter.instruction("sub sp, sp, #32");                                     // allocate the append-helper frame
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("mov x29, sp");                                         // establish the append-helper frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the source byte pointer
    emitter.instruction("str x1, [sp, #8]");                                    // save the source byte length

    // -- load the current append offset --
    abi::emit_symbol_address(emitter, "x9", "_print_r_off");                    // materialize the address of the append-offset global
    emitter.instruction("ldr x10, [x9]");                                       // load the current append offset
    abi::emit_symbol_address(emitter, "x11", "_print_r_buf");                   // materialize the base address of the capture buffer

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

    abi::emit_symbol_address(emitter, "r9", "_print_r_off");                    // materialize the address of the append-offset global
    emitter.instruction("mov r10, QWORD PTR [r9]");                             // load the current append offset
    abi::emit_symbol_address(emitter, "r11", "_print_r_buf");                   // materialize the base address of the capture buffer

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
/// Branches on `_print_r_mode`: 0 → `write(1, buf, len)` syscall; 1 → append to
/// the capture buffer via `__rt_pr_append`.
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

    // -- plain write(1, buf, len) syscall path --
    emitter.label("__rt_pr_write_syscall");
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

    // -- plain write(1, buf, len) syscall path --
    emitter.label("__rt_pr_write_syscall");
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
    abi::emit_symbol_address(emitter, "x1", "_print_r_buf");                    // load the capture-buffer address into the string-pointer result reg
    emitter.instruction("bl __rt_str_persist");                                 // persist the buffer as an owned heap string → x1=ptr, x2=len

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

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer across the str_persist call
    emitter.instruction("mov rbp, rsp");                                        // establish a frame base

    abi::emit_symbol_address(emitter, "r11", "_print_r_off");                   // materialize the address of the append-offset global
    emitter.instruction("mov rdx, QWORD PTR [r11]");                            // load the captured byte length into the string-length result reg
    abi::emit_symbol_address(emitter, "rax", "_print_r_buf");                   // load the capture-buffer address into the string-pointer result reg
    emitter.instruction("call __rt_str_persist");                               // persist the buffer as an owned heap string → rax=ptr, rdx=len

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
    use crate::codegen::platform::{Arch, Platform, Target};

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