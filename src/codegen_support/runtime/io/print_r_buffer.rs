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
//! - Capture mode, byte count, raw pointer, and capacity default to zero.
//! - Appends grow raw heap storage geometrically; finish persists once through
//!   the standard string metadata hooks, frees the buffer, and clears state.
//! - The write indirection selects capture or the existing output-buffer/web path.
//! - Return ABI of `__rt_pr_finish` matches the platform string result registers
//!   (AArch64 x1=ptr, x2=len; x86_64 rax=ptr, rdx=len) so the caller stores it
//!   directly as a `Str` value.

use crate::codegen_support::abi;
use crate::codegen_support::{emit::Emitter, platform::Arch};

/// Appends bytes into growable raw capture storage without a fixed output-size clamp.
/// Inputs are a C-ABI source pointer and byte length; zero bytes require no allocation.
pub fn emit_pr_append(emitter: &mut Emitter) {
    emitter.blank();
    emitter.label_global("__rt_pr_append");
    abi::emit_frame_prologue(emitter, 80);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), 16);
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::load_at_offset(emitter, result, 16);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_pr_append_done");
    for (symbol, offset) in [("_print_r_ptr", 24), ("_print_r_off", 32), ("_print_r_cap", 40)] {
        abi::emit_load_symbol_to_reg(emitter, result, symbol, 0);
        abi::store_at_offset(emitter, result, offset);
    }
    abi::load_at_offset(emitter, result, 24);
    abi::store_at_offset(emitter, result, 56);
    abi::load_at_offset(emitter, result, 32);
    abi::load_at_offset(emitter, scratch, 16);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("adds x0, x0, x10");                            // check the required capture size before any narrowing allocation
            emitter.instruction("b.cs __rt_heap_exhausted");                    // reject a wrapped byte count through the shared fatal path
        }
        Arch::X86_64 => {
            emitter.instruction("add rax, r10");                                // check the required capture size before any narrowing allocation
            emitter.instruction("jc __rt_heap_exhausted");                      // reject a wrapped byte count through the shared fatal path
        }
    }
    abi::store_at_offset(emitter, result, 48);
    abi::load_at_offset(emitter, result, 40);
    abi::load_at_offset(emitter, scratch, 48);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, x10");                                 // reuse an allocation that already fits the complete append
            emitter.instruction("b.hs __rt_pr_append_ready");                   // existing capacity avoids another allocation
            emitter.instruction("cmp x0, #64");                                 // start small captures with a bounded minimum allocation
            emitter.instruction("b.hs __rt_pr_append_grow");                    // preserve an existing larger capacity
            emitter.instruction("mov x0, #64");                                 // seed geometric growth for an empty capture
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, r10");                                // reuse an allocation that already fits the complete append
            emitter.instruction("jae __rt_pr_append_ready");                    // existing capacity avoids another allocation
            emitter.instruction("cmp rax, 64");                                 // start small captures with a bounded minimum allocation
            emitter.instruction("jae __rt_pr_append_grow");                     // preserve an existing larger capacity
            emitter.instruction("mov rax, 64");                                 // seed geometric growth for an empty capture
        }
    }
    emitter.label("__rt_pr_append_grow");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, x10");                                 // stop doubling once the pending append fits
            emitter.instruction("b.hs __rt_pr_append_allocate");                // allocate the selected capacity
            emitter.instruction("adds x0, x0, x0");                             // double geometrically without wrapping
            emitter.instruction("b.cs __rt_heap_exhausted");                    // reject capacity overflow instead of looping
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, r10");                                // stop doubling once the pending append fits
            emitter.instruction("jae __rt_pr_append_allocate");                 // allocate the selected capacity
            emitter.instruction("add rax, rax");                                // double geometrically without wrapping
            emitter.instruction("jc __rt_heap_exhausted");                      // reject capacity overflow instead of looping
        }
    }
    abi::emit_jump(emitter, "__rt_pr_append_grow");
    emitter.label("__rt_pr_append_allocate");
    abi::store_at_offset(emitter, result, 40);
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    abi::store_at_offset(emitter, result, 56);
    abi::load_at_offset(emitter, result, 32);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_pr_append_ready");
    load_capture_argument(emitter, 0, 56);
    load_capture_argument(emitter, 1, 24);
    load_capture_argument(emitter, 2, 32);
    abi::emit_call_label(emitter, &emitter.target.extern_symbol("memcpy"));
    emitter.label("__rt_pr_append_ready");
    load_capture_argument(emitter, 0, 56);
    abi::load_at_offset(emitter, scratch, 32);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction("add x0, x0, x10"),                // append after the previously captured prefix
        Arch::X86_64 => emitter.instruction("add rdi, r10"),                    // append after the previously captured prefix
    }
    load_capture_argument(emitter, 1, 8);
    load_capture_argument(emitter, 2, 16);
    abi::emit_call_label(emitter, &emitter.target.extern_symbol("memmove"));
    for (offset, symbol) in [(56, "_print_r_ptr"), (40, "_print_r_cap"), (48, "_print_r_off")] {
        abi::load_at_offset(emitter, result, offset);
        abi::emit_store_reg_to_symbol(emitter, result, symbol, 0);
    }
    abi::load_at_offset(emitter, result, 24);
    abi::load_at_offset(emitter, scratch, 56);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, x10");                                 // only a growth allocation retires the old raw buffer
            emitter.instruction("b.eq __rt_pr_append_done");                    // an in-place append keeps its allocation
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, r10");                                // only a growth allocation retires the old raw buffer
            emitter.instruction("je __rt_pr_append_done");                      // an in-place append keeps its allocation
        }
    }
    abi::emit_call_label(emitter, "__rt_heap_free_safe");
    emitter.label("__rt_pr_append_done");
    abi::emit_frame_restore(emitter, 80);
    abi::emit_return(emitter);
}

/// Reloads one append spill into the selected target C argument register.
fn load_capture_argument(emitter: &mut Emitter, index: usize, offset: usize) {
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
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

/// Releases an abandoned raw capture and clears its process-global state.
fn emit_pr_abort(emitter: &mut Emitter) {
    emitter.blank();
    emitter.label_global("__rt_cleanup_print_r_capture");
    abi::emit_jump(emitter, "__rt_pr_abort");
    emitter.label_global("__rt_pr_abort");
    abi::emit_frame_prologue(emitter, 16);
    let result = abi::int_result_reg(emitter);
    abi::emit_load_symbol_to_reg(emitter, result, "_print_r_ptr", 0);
    for symbol in ["_print_r_ptr", "_print_r_cap", "_print_r_off", "_print_r_mode"] {
        abi::emit_store_zero_to_symbol(emitter, symbol, 0);
    }
    abi::emit_call_label(emitter, "__rt_heap_free_safe");
    abi::emit_frame_restore(emitter, 16);
    abi::emit_return(emitter);
}

/// Copies raw capture bytes into an owned string with the standard string metadata hooks.
/// The raw buffer is freed and every capture global is cleared before returning the string pair.
pub fn emit_pr_finish(emitter: &mut Emitter) {
    emit_pr_abort(emitter);
    emitter.blank();
    emitter.label_global("__rt_pr_finish");
    abi::emit_frame_prologue(emitter, 48);
    let result = abi::int_result_reg(emitter);
    let (pointer, length) = abi::string_result_regs(emitter);
    abi::emit_load_symbol_to_reg(emitter, pointer, "_print_r_ptr", 0);
    abi::store_at_offset(emitter, pointer, 8);
    abi::emit_load_symbol_to_reg(emitter, length, "_print_r_off", 0);
    abi::emit_call_label(emitter, "__rt_str_persist");
    abi::store_at_offset(emitter, pointer, 16);
    abi::store_at_offset(emitter, length, 24);
    abi::load_at_offset(emitter, result, 8);
    abi::emit_call_label(emitter, "__rt_heap_free_safe");
    for symbol in ["_print_r_ptr", "_print_r_cap", "_print_r_off", "_print_r_mode"] {
        abi::emit_store_zero_to_symbol(emitter, symbol, 0);
    }
    abi::load_at_offset(emitter, pointer, 16);
    abi::load_at_offset(emitter, length, 24);
    abi::emit_frame_restore(emitter, 48);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Arch, Platform, Target};

    /// Renders all print_r capture helpers for one supported target.
    fn render_target(target: Target) -> String {
        let mut emitter = Emitter::new(target);
        emit_pr_append(&mut emitter);
        emit_pr_write(&mut emitter);
        emit_pr_finish(&mut emitter);
        emitter.output()
    }

    /// Renders a platform/architecture fixture used by the existing branch tests.
    fn render(platform: Platform, arch: Arch) -> String {
        render_target(Target::new(platform, arch))
    }

    /// Verifies every supported target exports the capture and cleanup helpers.
    #[test]
    fn emits_global_labels_for_all_targets() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let asm = render_target(Target::parse(target).unwrap());
            for label in ["__rt_pr_append", "__rt_pr_write", "__rt_pr_finish", "__rt_pr_abort", "__rt_cleanup_print_r_capture"] {
                assert!(asm.contains(&format!(".globl {label}\n")), "{target}: missing {label}");
            }
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
        assert!(mac.contains("_print_r_ptr"));

        let linux_x86 = render(Platform::Linux, Arch::X86_64);
        assert!(linux_x86.contains("call __rt_str_persist"));
        assert!(linux_x86.contains("_print_r_off"));
        assert!(linux_x86.contains("_print_r_ptr"));
    }
}
