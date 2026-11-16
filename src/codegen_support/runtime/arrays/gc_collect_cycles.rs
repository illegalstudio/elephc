//! Purpose:
//! Preserves compiler integration tests and compatibility imports for Memoir emitters.
//!
//! Called from:
//! - The elephc compiler memory subsystem.
//!
//! Key details:
//! - This code runs in the compiler and emits assembly; it is never a target-program library.

pub use elephc_memoir::gc_collect_cycles::*;

#[cfg(test)]
use crate::codegen_support::{emit::Emitter};


#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Object traversal restores an actual header from candidate slots before reading class data.
    #[test]
    fn aarch64_candidate_object_restores_resolve_vector_slots() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64"] {
            for pic in [false, true] {
                let mut emitter = Emitter::new(Target::parse(name).unwrap());
                emitter.pic_data_refs = pic;
                emit_gc_collect_cycles(&mut emitter);
                let output = emitter.output();
                for phase in ["count_object_restore", "count_dynamic_restore"] {
                    let end = output.find(&format!("__rt_gc_{phase}_header_ready:")).unwrap();
                    let begin = output[..end].rfind("ldr x12, [sp").unwrap();
                    assert!(output[begin..end].contains("ldr x12, [x12]"), "{name}, PIC={pic}, {phase}");
                    assert!(output[end..].find("add x12, x12, #16").unwrap()
                        < output[end..].find("ldr x14, [x12]").unwrap(), "{name}, {phase}");
                }
            }
        }
    }

    #[test]
    /// Verifies suppressed collection leaves the active flag unset on every AArch64 target.
    fn suppressed_aarch64_collection_does_not_set_active_flag() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_gc_collect_cycles(&mut emitter);
            let output = emitter.output();
            let suppression = output.find("_gc_release_suppressed").unwrap();
            let skip = output[suppression..].find("cbnz x10, __rt_gc_collect_cycles_done").unwrap() + suppression;
            let active = output[skip..].find("str x10, [x9]").unwrap() + skip;
            assert!(skip < active, "{name}");
            assert!(!output[..skip].contains("str x10, [x9]"), "{name}");
        }
    }

    #[test]
    /// Verifies AArch64 cycle-collector counters and cleanup state use distinct frame slots.
    fn aarch64_collector_keeps_count_rescan_and_cleanup_in_separate_frame_slots() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_gc_collect_cycles(&mut emitter);
            let output = emitter.output();
            assert!(output.contains("sub sp, sp, #128"), "{name}");
            assert!(output.contains("str xzr, [sp, #96]"), "{name}: rescan state");
            assert!(output.contains("str xzr, [sp, #104]"), "{name}: pending exception");
            assert!(output.contains("str x10, [sp, #112]"), "{name}: suppression state");
            let restore = output.rfind("ldr x0, [sp, #80]").unwrap();
            let rethrow = output.rfind("b __rt_gc_rethrow_pending").unwrap();
            assert!(restore < rethrow, "{name}: collected count survives cleanup");
            assert!(output.contains("add sp, sp, #128"), "{name}");
        }
    }
}
