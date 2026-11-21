//! Purpose:
//! Emits the dormant-gated bridge from Async scheduler transitions to monitoring.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::platform::emit_platform_runtime()`.
//!
//! Key details:
//! - The optional profiler callback and active-window word are checked before the tail call.
//! - Normal binaries perform no allocation or clock read; the active runtime context is arg 5.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_async_monitor_event(domain, task, parent, group, transition)`.
pub fn emit_async_monitor_event(emitter: &mut Emitter) {
    let callback = emitter.target.extern_symbol("elephc_instr_scheduler_fn");
    let active = emitter.target.extern_symbol("elephc_monitor_active");
    emitter.blank();
    emitter.comment("--- runtime: async_monitor_event ---");
    emitter.label_global("__rt_async_monitor_event");
    if emitter.target.arch == Arch::X86_64 {
        abi::emit_load_symbol_to_reg(emitter, "rax", &callback, 0);
        emitter.instruction("test rax, rax");                                   // is scheduler instrumentation linked?
        emitter.instruction("jz __rt_async_monitor_event_return_x86");          // no callback: remain completely dormant
        abi::emit_load_symbol_to_reg(emitter, "r10", &active, 0);
        emitter.instruction("test r10, r10");                                   // is an exact capture window active now?
        emitter.instruction("jz __rt_async_monitor_event_return_x86");          // linked but dormant: avoid the callback and clock
        emitter.instruction("mov r9, r14");                                     // arg 5: active runtime-context identity
        emitter.instruction("jmp rax");                                         // tail-call the optional profiler callback
        emitter.label("__rt_async_monitor_event_return_x86");
        emitter.instruction("ret");                                             // no monitoring work for this transition
        return;
    }

    abi::emit_load_symbol_to_reg(emitter, "x9", &callback, 0);
    emitter.instruction("cbz x9, __rt_async_monitor_event_return");             // no callback: remain completely dormant
    emitter.instruction("mov x10, x9");                                         // preserve callback across the active-word load
    abi::emit_load_symbol_to_reg(emitter, "x9", &active, 0);
    emitter.instruction("cbz x9, __rt_async_monitor_event_return");             // linked but dormant: avoid the callback and clock
    emitter.instruction("mov x5, x28");                                         // arg 5: active runtime-context identity
    emitter.instruction("br x10");                                              // tail-call the optional profiler callback
    emitter.label("__rt_async_monitor_event_return");
    emitter.instruction("ret");                                                 // no monitoring work for this transition
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Pins nullable callback, capture-window and context-identity routing on executable targets.
    #[test]
    fn scheduler_monitoring_is_nullable_dormant_gated_and_context_aware() {
        for (platform, arch, context_move, indirect) in [
            (Platform::MacOS, Arch::AArch64, "mov x5, x28", "br x10"),
            (Platform::Linux, Arch::AArch64, "mov x5, x28", "br x10"),
            (Platform::Linux, Arch::X86_64, "mov r9, r14", "jmp rax"),
        ] {
            let mut emitter = Emitter::new(Target::new(platform, arch));
            emit_async_monitor_event(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("elephc_instr_scheduler_fn"), "{platform:?}/{arch:?}: {asm}");
            assert!(asm.contains("elephc_monitor_active"), "{platform:?}/{arch:?}: {asm}");
            assert!(asm.contains(context_move), "{platform:?}/{arch:?}: {asm}");
            assert!(asm.contains(indirect), "{platform:?}/{arch:?}: {asm}");
        }
    }
}
