//! Purpose:
//! Emits the `__rt_throw_current`, `__rt_throw_current_uncaught` runtime helper assembly for throw current.
//! Keeps exception object matching, unwinding state, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::exceptions`.
//!
//! Key details:
//! - Exception matching and unwinding must keep handler-stack, call-frame cleanup, and class metadata invariants aligned.

use crate::codegen_support::platform::Arch;
use crate::codegen_support::try_handlers::TRY_HANDLER_JMP_BUF_OFFSET;
use crate::codegen_support::{abi, emit::Emitter};

/// Emits `__rt_throw_current`, the runtime helper that propagates an exception upward through
/// the handler stack. Saves callee-saved registers, retrieves the top handler record from
/// `_exc_handler_top`, runs `__rt_exception_cleanup_frames` to unwind all activation frames,
/// then calls `longjmp` with return value 1 to resume at the nearest catch block. If no handler
/// is registered, falls through to `__rt_throw_current_uncaught`, which writes a 32-byte fatal
/// message to stderr and terminates the process with exit code 1.
/// Tells the exact profiler that an exception has begun unwinding, if it is
/// linked and this program carries the capability.
///
/// Emitted inside the throw helper rather than at each `throw` lowering, because
/// the runtime raises exceptions of its own — bcmath, array value errors, enum
/// cases, static property access, the eval callable helpers — and every one of
/// them jumps to this helper. A hook at the lowering sites would have covered
/// what PHP code throws and silently missed the rest, which is the worse kind of
/// partial: the profile would be right for some exceptions and wrong for others
/// with nothing to say which.
///
/// The slot is null unless `--with-monitoring` filled it, so a binary without
/// the capability pays one load and a not-taken branch, on a path only a throw
/// reaches. The counters are read after the branch, so a dormant binary does not
/// read them at all.
///
/// It passes `_gc_allocs` and `_gc_frees` for the same reason the enter and exit
/// hooks do: the frames an exception destroys are closed at the instant of the
/// throw, and closing them at the throw's clock while using the CATCHER's
/// allocation counters charged the function that threw for every object the
/// handler allocated.
fn emit_instr_throw_hook(emitter: &mut Emitter) {
    let slot = emitter.target.extern_symbol("elephc_instr_throw_fn");
    let skip = "__rt_throw_current_no_instr";
    if emitter.target.arch == Arch::X86_64 {
        abi::emit_load_symbol_to_reg(emitter, "rax", &slot, 0);
        emitter.instruction("test rax, rax");                                   // is the exact profiler linked into this binary?
        emitter.instruction(&format!("jz {skip}"));                             // no capability: skip the hook entirely
        abi::emit_load_symbol_to_reg(emitter, "rdi", "_gc_allocs", 0);          // arg 0: allocations so far, sampled at the throw
        abi::emit_load_symbol_to_reg(emitter, "rsi", "_gc_frees", 0);           // arg 1: frees so far, sampled at the throw
        emitter.instruction("call rax");                                        // record that an unwind started, and where the counters stood
    } else {
        abi::emit_load_symbol_to_reg(emitter, "x9", &slot, 0);
        emitter.instruction(&format!("cbz x9, {skip}"));                        // no capability: skip the hook entirely
        // Both AArch64 symbol loads borrow x9, which is holding the slot.
        emitter.instruction("mov x10, x9");                                     // keep the hook address clear of the loads below
        abi::emit_load_symbol_to_reg(emitter, "x0", "_gc_allocs", 0);           // arg 0: allocations so far, sampled at the throw
        abi::emit_load_symbol_to_reg(emitter, "x1", "_gc_frees", 0);            // arg 1: frees so far, sampled at the throw
        emitter.instruction("blr x10");                                         // record that an unwind started, and where the counters stood
    }
    emitter.label(skip);
}

/// Emits the runtime helper that throws the exception in the current slot.
///
/// x86_64 takes its own path: the two architectures differ in which register
/// carries the helper's single argument, which is not a detail the shared
/// body can paper over.
pub fn emit_throw_current(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_throw_current_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: throw_current ---");
    emitter.label_global("__rt_throw_current");

    // -- save callee-saved state while the throw helper inspects handler stacks --
    emitter.instruction("sub sp, sp, #48");                                     // reserve stack space for handler state and frame linkage
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address for the throw helper
    emitter.instruction("stp x19, x20, [sp, #16]");                             // preserve callee-saved registers that hold handler metadata
    emitter.instruction("add x29, sp, #32");                                    // install the throw helper's frame pointer
    emit_instr_throw_hook(emitter);
    abi::emit_load_symbol_to_reg(emitter, "x19", "_exc_handler_top", 0);
    emitter.instruction("cbz x19, __rt_throw_current_uncaught");                // fall back to a fatal uncaught-exception path when no handler exists
    emitter.instruction("bl __rt_pcntl_abort_dispatch");                        // restore PCNTL mask/guard state before a handler exception longjmps
    emitter.instruction("ldr x0, [x19, #8]");                                   // x0 = activation record that should survive this catch
    emitter.instruction("bl __rt_exception_cleanup_frames");                    // run cleanup callbacks for every unwound activation frame
    abi::emit_store_reg_to_symbol(emitter, "xzr", "_concat_off", 0);
    emitter.instruction(&format!("add x0, x19, #{}", TRY_HANDLER_JMP_BUF_OFFSET)); // x0 = jmp_buf base stored inside the active handler record
    emitter.instruction("mov x1, #1");                                          // longjmp return value = 1 to indicate exceptional control flow
    emitter.bl_c("longjmp"); // transfer control directly back to the saved catch resume point

    // -- uncaught exceptions terminate the process with a fatal message --
    emitter.label("__rt_throw_current_uncaught");
    emitter.instruction("b __rt_report_uncaught_exception");                    // report the Throwable's class and message, then exit 255 like PHP
}

/// Emits `__rt_throw_current` for Linux x86_64. Uses the System V AMD64 ABI: preserves rbp as
/// frame pointer, saves r12/r13 callee-saved registers, loads `_exc_handler_top` into r12,
/// checks for null handler to branch to the uncaught path, calls
/// `__rt_exception_cleanup_frames` for frame unwinding, then invokes `longjmp` to transfer
/// control to the saved catch resume point. The uncaught path writes 32 bytes to stderr via
/// syscall 1 (write) and terminates via syscall 231 (`exit_group`).
fn emit_throw_current_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: throw_current ---");
    emitter.label_global("__rt_throw_current");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while the throw helper inspects handler stacks
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the x86_64 throw helper
    emitter.instruction("push r12");                                            // preserve the active handler record pointer across helper calls
    emitter.instruction("push r13");                                            // preserve the scratch callee-saved register used for the fatal path
    emit_instr_throw_hook(emitter);
    abi::emit_load_symbol_to_reg(emitter, "r12", "_exc_handler_top", 0);
    emitter.instruction("test r12, r12");                                       // is there an active exception handler to receive this throw?
    emitter.instruction("jz __rt_throw_current_uncaught");                      // fall back to a fatal uncaught-exception path when no handler exists
    emitter.instruction("call __rt_pcntl_abort_dispatch");                      // restore PCNTL mask/guard state before a handler exception longjmps
    emitter.instruction("mov rdi, QWORD PTR [r12 + 8]");                        // rdi = activation record that should survive this catch
    emitter.instruction("call __rt_exception_cleanup_frames");                  // run cleanup callbacks for every unwound activation frame
    abi::emit_store_zero_to_symbol(emitter, "_concat_off", 0);
    emitter.instruction(&format!("lea rdi, [r12 + {}]", TRY_HANDLER_JMP_BUF_OFFSET)); // rdi = jmp_buf base stored inside the active handler record
    emitter.instruction("mov esi, 1");                                          // longjmp return value = 1 to indicate exceptional control flow
    emitter.bl_c("longjmp"); // transfer control directly back to the saved catch resume point

    emitter.label("__rt_throw_current_uncaught");
    emitter.instruction("jmp __rt_report_uncaught_exception");                  // report the Throwable's class and message, then exit 255 like PHP
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// The helper's assembly for one platform/arch pair, so a test can assert
    /// on what each target actually emits.
    fn emitted(platform: Platform, arch: Arch) -> String {
        let mut emitter = Emitter::new(Target::new(platform, arch));
        emit_throw_current(&mut emitter);
        emitter.output()
    }

    /// Every throw tells the profiler, and pays nothing when there is none.
    ///
    /// The call is INDIRECT through a runtime slot rather than a direct call to
    /// `elephc_instr_throw`, so a binary without the capability links and runs
    /// with a null slot instead of an undefined symbol.
    #[test]
    fn the_unwinder_notifies_the_profiler_through_a_nullable_slot() {
        for (platform, arch) in [
            (Platform::MacOS, Arch::AArch64),
            (Platform::Linux, Arch::AArch64),
            (Platform::Linux, Arch::X86_64),
        ] {
            let asm = emitted(platform, arch);
            let target = Target::new(platform, arch);
            let slot = target.extern_symbol("elephc_instr_throw_fn");
            assert!(
                asm.contains(&slot),
                "{platform:?}/{arch:?} never reads the throw slot:\n{asm}"
            );
            let guarded = asm.contains("cbz x9,") || asm.contains("jz __rt_throw_current_no_instr");
            assert!(guarded, "{platform:?}/{arch:?} calls the hook unguarded:\n{asm}");
            let indirect = asm.contains("blr x10") || asm.contains("call rax");
            assert!(indirect, "{platform:?}/{arch:?} does not call through the slot:\n{asm}");
        }
    }

    /// The hook hands over the heap counters, and does not lose the slot doing it.
    ///
    /// Both AArch64 symbol loads use x9 as scratch, so reading the counters into
    /// the argument registers overwrites a slot pointer left in x9. The call has
    /// to go through a register the loads do not touch — and the loads have to
    /// sit after the guard, so a binary without the capability still pays only
    /// the load and the branch.
    #[test]
    fn the_hook_passes_the_heap_counters_without_clobbering_itself() {
        for (platform, arch) in [
            (Platform::MacOS, Arch::AArch64),
            (Platform::Linux, Arch::AArch64),
            (Platform::Linux, Arch::X86_64),
        ] {
            let asm = emitted(platform, arch);
            for counter in ["_gc_allocs", "_gc_frees"] {
                assert!(
                    asm.contains(counter),
                    "{platform:?}/{arch:?} never reads {counter}:\n{asm}"
                );
            }
            let (guard, call) = match arch {
                Arch::X86_64 => ("jz __rt_throw_current_no_instr", "call rax"),
                Arch::AArch64 => ("cbz x9, __rt_throw_current_no_instr", "blr x10"),
            };
            let guard_at = asm.find(guard).expect("the hook is guarded");
            let allocs_at = asm.find("_gc_allocs").expect("the hook reads _gc_allocs");
            let call_at = asm.find(call).expect("the hook calls through the slot");
            assert!(
                guard_at < allocs_at,
                "{platform:?}/{arch:?} reads the counters before the guard, so a \
                 dormant binary pays for them:\n{asm}"
            );
            assert!(allocs_at < call_at, "{platform:?}/{arch:?} reads them too late:\n{asm}");
            if arch == Arch::AArch64 {
                assert!(
                    asm.contains("mov x10, x9"),
                    "{platform:?}/{arch:?} calls a slot the counter loads clobbered:\n{asm}"
                );
            }
        }
    }

    /// The hook must come after the callee-saved registers are pushed.
    ///
    /// That is what makes the call safe — the return address is already stored —
    /// and on x86_64 it is also what leaves `rsp` 16-byte aligned at the call
    /// site, which the ABI requires and which this branch has already had to fix
    /// once in `main`'s epilogue.
    #[test]
    fn the_hook_sits_after_the_registers_are_saved() {
        let asm = emitted(Platform::Linux, Arch::X86_64);
        let saves = asm.find("push r13").expect("x86_64 saves r13");
        let hook = asm.find("call rax").expect("x86_64 calls the hook");
        assert!(hook > saves, "the hook runs before the registers are saved:\n{asm}");

        let asm = emitted(Platform::MacOS, Arch::AArch64);
        let saves = asm.find("stp x19, x20").expect("aarch64 saves x19/x20");
        let hook = asm.find("blr x10").expect("aarch64 calls the hook");
        assert!(hook > saves, "the hook runs before the registers are saved:\n{asm}");
    }
}
