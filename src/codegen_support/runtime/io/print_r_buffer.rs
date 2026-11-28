//! Purpose:
//! Combines capture memory helpers with print_r output routing.
//!
//! Called from:
//! - Compiler runtime orchestration and print_r rendering.
//!
//! Key details:
//! - Memoir owns byte storage; this adapter selects capture, output buffering or stdout.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use elephc_memoir::buffers::output::{self, ControlField};
pub use elephc_memoir::strings::capture_buffer::emit_pr_append;

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
    output::emit_control_address(emitter, "x9", ControlField::InHandler);
    emitter.instruction("ldr x9, [x9]");                                        // load the in-handler flag
    emitter.instruction("cbnz x9, __rt_pr_write_done");                         // inside a handler — discard the bytes entirely
    output::emit_depth_address(emitter, "x9");
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
    output::emit_control_address(emitter, "r11", ControlField::InHandler);
    emitter.instruction("mov r11, QWORD PTR [r11]");                            // load the in-handler flag
    emitter.instruction("test r11, r11");                                       // is a user output handler running?
    emitter.instruction("jnz __rt_pr_write_done");                              // inside a handler — discard the bytes entirely
    output::emit_depth_address(emitter, "r11");
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

/// Emits capture finalization with the compiler-owned routing reset.
pub fn emit_pr_finish(emitter: &mut Emitter) {
    elephc_memoir::strings::capture_buffer::emit_pr_finish(emitter, reset_capture_mode);
}

/// Deactivates return-mode capture only after the buffer storage has been retired.
fn reset_capture_mode(emitter: &mut Emitter) {
    abi::emit_store_zero_to_symbol(emitter, "_print_r_mode", 0);
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

    /// Overflow conditionals stay inside their atom and only a shared fatal entry escapes.
    #[test]
    fn pr_append_overflow_branches_stay_in_own_atom() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            emit_pr_append(&mut emitter);
            let asm = emitter.output();
            for line in asm.lines().map(str::trim) {
                if let Some(destination) = line.strip_prefix("b.cs ").or_else(|| line.strip_prefix("jc ")) {
                    let destination = destination.split_whitespace().next().unwrap();
                    assert!(asm.contains(&format!("\n{destination}:\n")), "{target}: overflow leaves the append atom: {line}");
                }
            }
            assert!(!asm.contains("__rt_heap_exhausted"), "{target}: private allocator label escaped");
            assert!(asm.contains("__rt_heap_allocation_failed"), "{target}: missing shared fatal entry");
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
