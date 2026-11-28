//! Purpose:
//! Preserves compiler integration tests and compatibility imports for Memoir emitters.
//!
//! Called from:
//! - The elephc compiler memory subsystem.
//!
//! Key details:
//! - This code runs in the compiler and emits assembly; it is never a target-program library.

pub use elephc_memoir::heap_alloc::*;

#[cfg(test)]
use crate::codegen_support::{emit::Emitter, platform::Arch};

#[cfg(test)]
mod split_remainder_tests;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{AppleVariant, Platform, Target};

    /// Cross-helper allocation failure has an unconditional shared entry on every target.
    #[test]
    fn heap_allocation_failure_shared_entry_keeps_local_branches() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new_apple(Arch::AArch64, AppleVariant::IOS),
            Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new_cdylib(target);
            emitter.dead_strip = true;
            emit_heap_alloc(&mut emitter);
            let internal = emitter.take_internal_labels();
            let asm = emitter.output();
            assert!(asm.contains("__rt_heap_allocation_failed:\n__rt_heap_exhausted:\n"));
            assert!(!internal.contains("__rt_heap_allocation_failed"));
            if target.platform == Platform::MacOS {
                assert!(internal.contains("__rt_heap_exhausted"));
                assert!(asm.contains(".alt_entry __rt_heap_allocation_failed\n"));
            }
            assert!(asm.contains(crate::codegen_support::cdylib::BOUNDARY_STATUS));
            assert!(asm.contains("__rt_throw_current"));
        }
    }

    /// Verifies the AArch64 allocator rejects payload sizes that cannot be
    /// represented by its 32-bit block-size header before any metadata write.
    #[test]
    fn aarch64_heap_allocator_guards_32_bit_header_size() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::AArch64));
        emit_heap_alloc(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("lsr x9, x0, #32\n"));
        assert!(asm.contains("cbnz x9, __rt_heap_alloc_size_overflow\n"));
        assert!(asm.contains("__rt_heap_alloc_size_overflow:\n"));
    }

    /// Verifies the bump path checks arena position against `_heap_max` and the live budget
    /// against `_heap_limit`, so a lowered limit does not reject allocations while free arena
    /// space remains.
    #[test]
    fn bump_path_checks_arena_position_and_live_budget_separately() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_heap_alloc(&mut emitter);
            let asm = emitter.output();
            let bump = asm.find("__rt_heap_alloc_bump").unwrap();
            let section = &asm[bump..];
            assert!(section.contains("_heap_max"), "{name}: position check");
            assert!(section.contains("_gc_live"), "{name}: live-byte check");
            assert!(section.contains("_heap_limit"), "{name}: runtime ceiling check");
        }
    }

    /// Verifies every free-block reuse path checks the live budget before publishing a block.
    #[test]
    fn reused_heap_blocks_respect_live_memory_limit() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_heap_alloc(&mut emitter);
            let asm = emitter.output();
            let section = |start: &str, end: &str| {
                let start = asm.find(start).unwrap_or_else(|| panic!("{name}: missing {start}"));
                let end = asm[start..]
                    .find(end)
                    .map(|offset| start + offset)
                    .unwrap_or_else(|| panic!("{name}: missing {end}"));
                &asm[start..end]
            };

            for (path, instructions) in [
                (
                    "small-bin reuse",
                    section("__rt_heap_alloc_small_bin_found:\n", "__rt_heap_alloc_small_bin_unmarked:\n"),
                ),
                (
                    "split free-block reuse",
                    section("__rt_heap_alloc_fl_found:\n", "__rt_heap_alloc_fl_take_whole:\n"),
                ),
                (
                    "whole free-block reuse",
                    section("__rt_heap_alloc_fl_take_whole:\n", "__rt_heap_alloc_count:\n"),
                ),
            ] {
                assert!(instructions.contains("_gc_live"), "{name}: {path} live-byte check");
                assert!(instructions.contains("_heap_limit"), "{name}: {path} runtime ceiling check");
                assert!(instructions.contains("__rt_heap_exhausted"), "{name}: {path} exhaustion branch");
            }
        }
    }

    /// Verifies PIC allocators recover through the cdylib boundary instead of exiting.
    #[test]
    fn pic_heap_allocator_routes_exhaustion_to_cdylib_boundary() {
        let mut arm = Emitter::new_cdylib(Target::new(Platform::MacOS, Arch::AArch64));
        emit_heap_alloc(&mut arm);
        let arm_asm = arm.output();
        assert!(arm_asm.contains(crate::codegen_support::cdylib::BOUNDARY_ACTIVE));
        assert!(arm_asm.contains("b __rt_throw_current"));

        let mut x86 = Emitter::new_cdylib(Target::new(Platform::Linux, Arch::X86_64));
        emit_heap_alloc(&mut x86);
        let x86_asm = x86.output();
        assert!(x86_asm.contains(crate::codegen_support::cdylib::BOUNDARY_STATUS));
        assert!(x86_asm.contains("jmp __rt_throw_current"));
    }

    /// Verifies the x86_64 allocator rejects payload sizes that cannot be
    /// represented by its 32-bit block-size header before truncating `rax`.
    #[test]
    fn x86_64_heap_allocator_guards_32_bit_header_size() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_heap_alloc(&mut emitter);
        let asm = emitter.output();

        assert!(asm.contains("mov r10d, 0xffffffff\n"));
        assert!(asm.contains("cmp rax, r10\n"));
        assert!(asm.contains("ja __rt_heap_alloc_size_overflow\n"));
        assert!(asm.contains("__rt_heap_alloc_size_overflow:\n"));
    }
}
