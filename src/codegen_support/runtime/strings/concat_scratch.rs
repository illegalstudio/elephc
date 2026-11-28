//! Purpose:
//! Preserves compiler imports and tests for Memoir string storage.
//!
//! Called from:
//! - Compiler memory emission and its integration tests.
//!
//! Key details:
//! - Runs only in the compiler and preserves the generated target assembly.

pub use elephc_memoir::strings::concat_scratch::*;

#[cfg(test)]
use crate::codegen_support::{emit::Emitter, platform::Arch};

#[cfg(test)]
use crate::codegen_support::sentinels::CONCAT_TEMP_HEAP_KIND;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Float formatters get a bounded window: the scratch tail while 64 bytes remain, otherwise a
    /// slot of `_float_spill_ring`, and the spill never moves `_concat_off`.
    #[test]
    fn float_window_spills_to_a_ring_when_the_scratch_is_nearly_full() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_concat_scratch(&mut emitter);
            let asm = emitter.output();
            let window = asm.split("__rt_float_window:").nth(1).unwrap();
            let window = window.split("--- runtime:").next().unwrap();
            assert!(window.contains("_float_spill_ring"), "{:?}", target.arch);
            assert!(window.contains("_float_spill_index"), "{:?}", target.arch);
            assert!(window.contains("_concat_off"), "{:?}", target.arch);
            assert!(!window.contains("mov QWORD PTR [r8], "), "{:?}: the window only reads _concat_off", target.arch);
        }
    }

    /// The shared grow helper returns a transient string on both ABIs, while exact reservations
    /// remain ordinary owned strings. This lets `__rt_str_persist` adopt grown results in place.
    #[test]
    fn concat_grow_marks_results_transient_on_both_architectures() {
        let mut arm = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_concat_scratch(&mut arm);
        let arm_asm = arm.output();
        assert!(arm_asm.contains("mov x9, #1"));
        assert!(arm_asm.contains(&format!("mov x9, #{}", CONCAT_TEMP_HEAP_KIND)));

        let mut x86 = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_concat_scratch(&mut x86);
        let x86_asm = x86.output();
        assert!(x86_asm.contains(&format!(
            "mov r10, 0x{:x}",
            crate::codegen_support::sentinels::x86_64_heap_kind_word(1)
        )));
        assert!(x86_asm.contains(&format!(
            "mov r10, 0x{:x}",
            crate::codegen_support::sentinels::x86_64_heap_kind_word(CONCAT_TEMP_HEAP_KIND)
        )));
    }

    /// Growth must publish the new base into the caller's slot. The throw path frees
    /// that slot, so a stale pre-growth pointer would either leak the grown block or
    /// free a block `__rt_concat_grow` already released.
    #[test]
    fn preg_cb_ensure_publishes_the_grown_base_before_returning() {
        let mut arm = Emitter::new(Target::new(Platform::Linux, Arch::AArch64));
        emit_concat_scratch(&mut arm);
        let arm_asm = arm.output();
        let ensure = arm_asm
            .find("__rt_preg_cb_ensure:")
            .expect("ensure helper");
        let arm_ensure = &arm_asm[ensure..];
        let grow = arm_ensure.find("bl __rt_concat_grow").expect("grow call");
        let publish = arm_ensure.find("str x9, [x10]").expect("published base");
        let retag = arm_ensure.find("mov x9, #1").expect("owned-string kind");
        assert!(retag < publish && publish > grow, "{arm_ensure}");

        let mut x86 = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_concat_scratch(&mut x86);
        let x86_asm = x86.output();
        let ensure = x86_asm
            .find("__rt_preg_cb_ensure:")
            .expect("ensure helper");
        let x86_ensure = &x86_asm[ensure..];
        let grow = x86_ensure.find("call __rt_concat_grow").expect("grow call");
        let publish = x86_ensure
            .find("mov QWORD PTR [r9], r8")
            .expect("published base");
        assert!(publish > grow, "{x86_ensure}");
        assert!(x86_ensure.contains(&format!(
            "mov r10, 0x{:x}",
            crate::codegen_support::sentinels::x86_64_heap_kind_word(1)
        )));
    }
}
