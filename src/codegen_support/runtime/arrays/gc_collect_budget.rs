//! Purpose:
//! Preserves compiler integration tests and compatibility imports for Memoir emitters.
//!
//! Called from:
//! - The elephc compiler memory subsystem.
//!
//! Key details:
//! - This code runs in the compiler and emits assembly; it is never a target-program library.

pub use elephc_memoir::gc_collect_budget::*;

#[cfg(test)]
use crate::codegen_support::{emit::Emitter, platform::Arch};


#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{AppleVariant, Platform, Target};

    /// Every target checks candidate mode, the grown credit against `_heap_off`, then live bytes
    /// against their limit, and refreshes that limit after each collection.
    #[test]
    fn budgeted_safe_point_defers_until_credit_or_live_growth_requires_collection() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new_apple(Arch::AArch64, AppleVariant::IOS),
            Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_gc_collect_budget(&mut emitter);
            let asm = emitter.output();
            let candidates = asm.find("_gc_candidates_enabled").expect("candidate mode check");
            let credit = asm.find("_gc_safe_point_credit").expect("saved scan credit");
            let heap = asm.find("_heap_off").expect("heap extent");
            assert!(candidates < credit && credit < heap, "{target:?}\n{asm}");
            assert!(asm.contains("__rt_gc_collect_cycles_budget_collect:"), "{target:?}\n{asm}");
            assert!(asm.contains(&GC_SAFE_POINT_SCAN_CREDIT.to_string()), "{target:?}\n{asm}");
            let call = match target.arch {
                Arch::AArch64 => "bl __rt_gc_collect_cycles\n",
                Arch::X86_64 => "call __rt_gc_collect_cycles\n",
            };
            let collect = asm.find(call).expect("nested automatic collector call");
            let live = asm.find("_gc_live").expect("live-byte check");
            let limit = asm.rfind("_gc_safe_point_live_limit").expect("limit update");
            assert!(heap < live && live < collect && collect < limit, "{target:?}\n{asm}");
            assert!(asm.contains("_heap_max"), "{target:?}\n{asm}");
            for flag in ["_gc_enabled", "_gc_collecting", "_gc_release_suppressed"] {
                let check = asm.find(flag).unwrap_or_else(|| panic!("{flag} check"));
                assert!(check < collect, "{flag} must be checked before collecting on {target:?}");
            }
            // Apple's assembler rejects conditional branches to external symbols.
            for line in asm.lines().map(str::trim_start) {
                let conditional = line.starts_with("cb") || line.starts_with("b.")
                    || (line.starts_with('j') && !line.starts_with("jmp"));
                assert!(
                    !(conditional && line.contains("__rt_gc_collect_cycles ")
                        || conditional && line.ends_with("__rt_gc_collect_cycles")),
                    "{target:?}: {line}"
                );
            }
        }
    }
}
