//! Purpose:
//! Preserves compiler integration tests and compatibility imports for Memoir emitters.
//!
//! Called from:
//! - The elephc compiler memory subsystem.
//!
//! Key details:
//! - This code runs in the compiler and emits assembly; it is never a target-program library.

pub use elephc_memoir::gc_control::*;

#[cfg(test)]
use crate::codegen_support::{emit::Emitter, platform::Arch};


#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{AppleVariant, Platform, Target};

    /// Verifies every target emits live GC paths with status branches before helper atoms.
    #[test]
    fn gc_control_emits_live_metrics_for_all_supported_targets() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new_apple(Arch::AArch64, AppleVariant::IOS),
            Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_gc_control(&mut emitter);
            let asm = emitter.output();
            for symbol in [
                "__rt_gc_mem_caches:",
                "__rt_gc_status_roots:",
                "__rt_gc_request_start:",
                "__rt_gc_collector_begin:",
                "__rt_gc_free_begin:",
                "__rt_gc_collector_end:",
                "__rt_gc_destructor_begin:",
                "__rt_gc_destructor_end:",
                "__rt_gc_status_application_time:",
            ] {
                assert!(asm.contains(symbol), "missing {symbol} on {target:?}\n{asm}");
            }
            let status_time = asm
                .find("__rt_gc_status_application_time:")
                .expect("application-time status label");
            let request_start = asm
                .find("__rt_gc_request_start:")
                .expect("request timing helper");
            assert!(
                status_time < request_start,
                "status branches must stay in the dispatcher atom on {target:?}\n{asm}"
            );
            match target.arch {
                Arch::AArch64 => {
                    assert!(asm.contains("bl __rt_heap_consolidate"));
                    assert!(asm.contains("fmov x0, d0"));
                }
                Arch::X86_64 => {
                    assert!(asm.contains("call __rt_heap_consolidate"));
                    assert!(asm.contains("movq rax, xmm0"));
                }
            }
        }
    }
}
