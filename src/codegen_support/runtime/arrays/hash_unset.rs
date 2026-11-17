//! Purpose:
//! Exposes Memoir's container emitter through the compiler runtime.
//!
//! Called from:
//! - Runtime assembly generation.
//!
//! Key details:
//! - The compiler controls emitter selection and validates the generated code.

pub use elephc_memoir::values::hash_unset::emit_hash_unset;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::{emit::Emitter, platform::{AppleVariant, Arch, Platform, Target}};

    /// All targets detach the entry before callbacks and leave reentrant receiver updates intact.
    #[test]
    fn hash_unset_commits_removal_before_releasing_values_on_every_target() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new_apple(Arch::AArch64, AppleVariant::IOS),
            Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_hash_unset(&mut emitter);
            let asm = emitter.output();
            assert!(!asm.contains("__rt_hash_ensure_unique"), "{target:?}: {asm}");
            let unlink = asm.find("__rt_hash_unset_unlink:").unwrap();
            let tombstone = asm.find("__rt_hash_unset_tombstone:").unwrap();
            let release = asm.find("__rt_hash_unset_release_any:").unwrap();
            let (count_write, tag_clear, guard_skip, heap_call, descriptor_call) = match target.arch {
                Arch::AArch64 => (
                    "str x14, [x5, #0]", "str xzr, [x12, #40]",
                    "cbz x15, __rt_hash_unset_done",
                    "bl __rt_decref_any", "bl __rt_callable_descriptor_release",
                ),
                Arch::X86_64 => (
                    "mov QWORD PTR [r10], rax", "mov QWORD PTR [r8 + 40], 0",
                    "je __rt_hash_unset_done",
                    "call __rt_decref_any", "call __rt_callable_descriptor_release",
                ),
            };
            let count = asm.find(count_write).unwrap();
            let clear = asm.find(tag_clear).unwrap();
            let guard = asm[clear..].find(guard_skip).unwrap() + clear;
            assert!(unlink < tombstone && tombstone < count && count < clear && clear < guard && guard < release,
                "removal must be visible before callbacks on {target:?}: {asm}");
            assert!(asm.find(heap_call).unwrap() > release, "{target:?}: {asm}");
            assert!(asm.find(descriptor_call).unwrap() > release, "{target:?}: {asm}");
            let after_release = &asm[release..];
            for table_register in ["[x12", "[x5", "[r8", "[r10"] {
                assert!(!after_release.contains(table_register),
                    "retired table accessed after callback on {target:?}: {after_release}");
            }
        }
    }

    /// Per-invocation presence survives callback clobbers and is zero on every no-op path.
    #[test]
    fn hash_unset_reports_presence_on_every_target() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new_apple(Arch::AArch64, AppleVariant::IOS),
            Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_hash_unset(&mut emitter);
            let asm = emitter.output();
            let (initialize, matched, result, null_result) = match target.arch {
                Arch::AArch64 => ("str xzr, [sp, #48]", "str x9, [sp, #48]",
                    "ldr x1, [sp, #48]", "mov x1, #0"),
                Arch::X86_64 => ("mov QWORD PTR [rbp - 56], 0", "mov QWORD PTR [rbp - 56], 1",
                    "mov rdx, QWORD PTR [rbp - 56]", "xor edx, edx"),
            };
            let found = asm.find("__rt_hash_unset_found:").unwrap();
            let done = asm.find("__rt_hash_unset_done:").unwrap();
            let null = asm.find("__rt_hash_unset_null:").unwrap();
            assert!(asm.find(initialize).unwrap() < found, "{target:?}");
            assert!(asm[found..done].contains(matched), "{target:?}");
            assert!(asm[done..null].contains(result), "{target:?}");
            assert!(asm[null..].contains(null_result), "{target:?}");
            let leaf = asm.find("__rt_hash_unset_leaf_value:").unwrap();
            let owner = asm.find("__rt_hash_unset_owner_value:").unwrap();
            let clear = match target.arch {
                Arch::AArch64 => "str xzr, [sp, #48]",
                Arch::X86_64 => "mov QWORD PTR [rbp - 56], 0",
            };
            assert!(found < leaf && leaf < owner && owner < done, "{target:?}");
            assert!(asm[leaf..owner].contains(clear), "leaf values clear the flag on {target:?}");
        }
    }
}
