//! Purpose:
//! Preserves compiler integration tests and compatibility imports for Memoir emitters.
//!
//! Called from:
//! - The elephc compiler memory subsystem.
//!
//! Key details:
//! - This code runs in the compiler and emits assembly; it is never a target-program library.

pub use elephc_memoir::heap_free::*;

#[cfg(test)]
use crate::codegen_support::{emit::Emitter, platform::Arch};

#[cfg(test)]
#[path = "heap_free/tail_scan_tests.rs"]
mod tail_scan_tests;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Non-object frees bypass the preserving handle helper through its authoritative side table.
    #[test]
    fn heap_free_probes_object_handles_before_calling_the_release_helper() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_heap_free(&mut emitter, false, false);
            let output = emitter.output();
            let probe = output.find("_obj_handle_index").unwrap();
            let skip = output.find("__rt_heap_free_object_handle_done").unwrap();
            let call = output.find("__rt_object_handle_release").unwrap();
            assert!(probe < skip && skip < call, "{name}");
        }
    }

    /// `__rt_heap_free_safe` leaves an already-freed block alone on every target: a second
    /// release of the same string reached `__rt_heap_free` on x86_64 and heap debug reported a
    /// double free that AArch64 skipped.
    #[test]
    fn heap_free_safe_skips_freed_blocks_on_every_target() {
        for name in ["macos-aarch64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_heap_free(&mut emitter, false, false);
            let output = emitter.output();
            let safe = &output[output.find("__rt_heap_free_safe:").unwrap()..];
            let safe = &safe[..safe.find("__rt_heap_free_safe_skip:").unwrap()];
            let load = if name == "linux-x86_64" { "DWORD PTR [rax - 12]" } else { "ldr w12, [x11, #4]" };
            let refcount = safe.find(load).unwrap_or_else(|| panic!("{name}: no refcount load in {safe}"));
            let delegate = safe.rfind("__rt_heap_free").unwrap();
            assert!(refcount < delegate, "{name}: {safe}");
        }
    }

    /// The optional metadata callback must not replace the frameless AArch64 return address.
    #[test]
    fn heap_free_preserves_aarch64_linkage_across_mbstring_retirement() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_heap_free(&mut emitter, false, true);
            let output = emitter.output();
            assert!(output.contains(
                "stp x0, x30, [sp, #-16]!\n    bl __rt_mbstring_ini_forget\n    ldp x0, x30, [sp], #16"
            ), "{name}");
        }
    }

    /// Mixed wrappers skip only the string registry callback, after mandatory eval retirement.
    #[test]
    fn heap_free_skips_mixed_string_identity_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            for pic in [false, true] {
                for eval_bridge in [false, true] {
                    let mut emitter = Emitter::new(target);
                    emitter.pic_data_refs = pic;
                    emit_heap_free(&mut emitter, eval_bridge, true);
                    let output = emitter.output();
                    let guard = output.find(if target.arch == Arch::AArch64 {
                        "b.eq __rt_heap_free_mbstring_done"
                    } else { "je __rt_heap_free_mbstring_done" }).unwrap();
                    let call = output.find("__rt_mbstring_ini_forget").unwrap();
                    let done = output.find("__rt_heap_free_mbstring_done:").unwrap();
                    let clear = output.find(if target.arch == Arch::AArch64 {
                        "str xzr, [x9, #8]"
                    } else { "mov QWORD PTR [r9 + 8], 0" }).unwrap();
                    assert!(guard < call && call < done && done < clear, "{name}, PIC {pic}");
                    if eval_bridge {
                        let retired = output.find("__rt_heap_free_eval_references_done:").unwrap();
                        assert!(retired < guard, "{name}, PIC {pic}");
                    }
                }
                let mut emitter = Emitter::new(target);
                emitter.pic_data_refs = pic;
                emit_heap_free(&mut emitter, true, false);
                let output = emitter.output();
                assert!(!output.contains("__rt_heap_free_mbstring_done"), "{name}, PIC {pic}");
                assert!(!output.contains("__rt_mbstring_ini_forget"), "{name}, PIC {pic}");
            }
        }
    }
}
