//! Purpose:
//! Preserves compiler imports and tests for Memoir string storage.
//!
//! Called from:
//! - Compiler memory emission and its integration tests.
//!
//! Key details:
//! - Runs only in the compiler and preserves the generated target assembly.

pub use elephc_memoir::strings::str_persist::*;

#[cfg(test)]
use crate::codegen_support::{emit::Emitter};

#[cfg(test)]
use crate::codegen_support::sentinels::CONCAT_TEMP_HEAP_KIND;

#[cfg(test)]
mod tests {
    use crate::codegen_support::platform::{Arch, Platform, Target};

    use super::*;

    /// The quadratic concat-assignment copy loop keeps stable instruction-cache alignment.
    #[test]
    fn persist_copy_loop_is_cache_line_aligned_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_str_persist(&mut emitter, false);
            assert!(
                emitter.output().contains(".p2align 6\n__rt_str_persist_copy:"),
                "{name}"
            );
        }
    }

    /// Every target emits the non-copying claim helper next to persist: it requires an exact
    /// transient block base, retags that block as an owned string, and never allocates.
    #[test]
    fn str_claim_retags_only_transient_blocks_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_str_persist(&mut emitter, false);
            let asm = emitter.output();
            let start = asm.find("__rt_str_claim:\n").unwrap_or_else(|| panic!("{name}: no claim helper"));
            let end = asm[start..].find("__rt_str_persist:\n").map(|end| start + end).unwrap_or(asm.len());
            let claim = &asm[start..end];
            assert!(claim.contains("__rt_str_transient_base"), "{name}: {claim}");
            assert!(!claim.contains("__rt_heap_alloc"), "{name}: {claim}");
            if target.arch == Arch::X86_64 {
                assert!(claim.contains("mov QWORD PTR [rax - 8], r10"), "{name}: {claim}");
            } else {
                assert!(claim.contains("str x9, [x1, #-8]"), "{name}: {claim}");
            }
        }
    }

    /// Exact bases come from the bitmap without a heap walk, with live and complete headers.
    #[test]
    fn transient_base_probe_rejects_interior_pointers_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_str_persist(&mut emitter, false);
            let asm = emitter.output();
            let start = asm.find("__rt_str_transient_base:\n")
                .unwrap_or_else(|| panic!("{name}: no exact transient-base probe"));
            let end = asm[start..].find("__rt_str_claim:\n")
                .map(|end| start + end)
                .unwrap_or(asm.len());
            let probe = &asm[start..end];
            assert!(probe.contains("_heap_buf"), "{name}: {probe}");
            assert!(probe.contains("_heap_off"), "{name}: {probe}");
            assert!(probe.contains("_heap_live_bases"), "{name}: {probe}");
            assert!(!probe.contains("transient_base_scan"), "{name}: {probe}");
            if target.arch == Arch::X86_64 {
                assert!(probe.contains("test BYTE PTR [r10 + r9], r11b"), "{name}: {probe}");
                assert!(probe.contains("cmp DWORD PTR [rax - 12], 0"), "{name}: {probe}");
                assert!(probe.contains(&format!(
                    "mov r11, 0x{:x}",
                    crate::codegen_support::sentinels::x86_64_heap_kind_word(CONCAT_TEMP_HEAP_KIND)
                )), "{name}: {probe}");
            } else {
                assert!(probe.contains("tst w14, w13"), "{name}: {probe}");
                assert!(probe.contains("ldr w14, [x0, #-12]"), "{name}: {probe}");
                assert!(probe.contains(&format!("cmp x11, #{}", CONCAT_TEMP_HEAP_KIND)), "{name}: {probe}");
            }
        }
    }

    /// Verifies emit str persist linux x86_64 uses heap helper.
    #[test]
    fn test_emit_str_persist_linux_x86_64_uses_heap_helper() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_str_persist(&mut emitter, false);
        let asm = emitter.output();

        assert!(asm.contains("__rt_str_persist:\n"));
        assert!(asm.contains("call __rt_heap_alloc\n"));
        assert!(asm.contains("mov QWORD PTR [rax - 8], r10\n"));
    }
}
