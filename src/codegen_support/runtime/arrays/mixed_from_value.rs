//! Purpose:
//! Preserves compiler imports and tests for Memoir boxed-value ownership.
//!
//! Called from:
//! - Compiler runtime emission and memory integration tests.
//!
//! Key details:
//! - Assembly implementation lives in the compiler-only Memoir crate.

pub use elephc_memoir::values::mixed_from_value::*;

#[cfg(test)]
use crate::codegen_support::{emit::Emitter, platform::Arch};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Emits `__rt_mixed_from_value` for one target and returns the assembly text.
    fn emit_for(target: Target) -> String {
        let mut emitter = Emitter::new(target);
        emit_mixed_from_value(&mut emitter);
        emitter.output()
    }

    /// Pins ALL FIVE object-backed exclusions in the tag-9 arm, as exact multi-line
    /// sequences: the two hash contexts and the three libcurl handles (easy, multi,
    /// share).
    ///
    /// WHY THE WHOLE SEQUENCE AND NOT A SUBSTRING. `contains("cmp x2, #5")` is satisfied
    /// by `cmp x2, #50`, and `contains("cmp x2, #2")` by the `#2` inside `#24` — a pin
    /// that loose passes while the arm it claims to protect is gone. Pinning the compare
    /// TOGETHER WITH the branch that must follow it is what makes the test fail when
    /// either half is deleted, reordered, or pointed at a different label.
    ///
    /// Nothing pinned kind 2 on either target before this module existed, which is how
    /// the exclusion could have been dropped unnoticed; kinds 5, 6, 7 and 8 were added
    /// beside it and all five are asserted here so none can regress alone.
    #[test]
    fn aarch64_excludes_object_backed_kinds_from_resource_id_binding() {
        let asm = emit_for(Target::new(Platform::MacOS, Arch::AArch64));
        assert!(asm.contains("__rt_mixed_from_value_resource:\n"), "{asm}");
        assert!(
            asm.contains(
                "    cmp x2, #2\n\
                 \x20   b.eq __rt_mixed_from_value_alloc\n\
                 \x20   cmp x2, #5\n\
                 \x20   b.eq __rt_mixed_from_value_alloc\n\
                 \x20   cmp x2, #6\n\
                 \x20   b.eq __rt_mixed_from_value_alloc\n\
                 \x20   cmp x2, #7\n\
                 \x20   b.eq __rt_mixed_from_value_alloc\n\
                 \x20   cmp x2, #8\n\
                 \x20   b.eq __rt_mixed_from_value_alloc\n\
                 \x20   mov x0, x1\n\
                 \x20   bl __rt_resource_id_of\n"
            ),
            "{asm}"
        );
    }

    /// Pins the same five exclusions on x86_64.
    ///
    /// Separate emitters, separate arms: forgetting one target is the default way this
    /// change goes wrong, and an aarch64-only pin would not notice.
    #[test]
    fn x86_64_excludes_object_backed_kinds_from_resource_id_binding() {
        let asm = emit_for(Target::new(Platform::Linux, Arch::X86_64));
        assert!(asm.contains("__rt_mixed_from_value_resource:\n"), "{asm}");
        assert!(
            asm.contains(
                "    cmp QWORD PTR [rbp - 24], 2\n\
                 \x20   je __rt_mixed_from_value_alloc\n\
                 \x20   cmp QWORD PTR [rbp - 24], 5\n\
                 \x20   je __rt_mixed_from_value_alloc\n\
                 \x20   cmp QWORD PTR [rbp - 24], 6\n\
                 \x20   je __rt_mixed_from_value_alloc\n\
                 \x20   cmp QWORD PTR [rbp - 24], 7\n\
                 \x20   je __rt_mixed_from_value_alloc\n\
                 \x20   cmp QWORD PTR [rbp - 24], 8\n\
                 \x20   je __rt_mixed_from_value_alloc\n\
                 \x20   mov rax, QWORD PTR [rbp - 16]\n\
                 \x20   call __rt_resource_id_of\n"
            ),
            "{asm}"
        );
    }

    /// Pins that the exclusions read the resource KIND word, not the payload word.
    ///
    /// The one substitution that would keep both compares present and still be wrong:
    /// testing the LOW word (`x1` / `[rbp - 16]`) would compare an eval table key —
    /// `EVAL_RESOURCE_PAYLOAD_BASE + n`, never 2 or 5 — and silently bind ids again,
    /// while every sequence pin above still matched on the other target.
    #[test]
    fn the_exclusions_test_the_kind_word_on_both_targets() {
        for (target, kind_operand, payload_operand) in [
            (Target::new(Platform::MacOS, Arch::AArch64), "x2", "x1"),
            (
                Target::new(Platform::Linux, Arch::X86_64),
                "QWORD PTR [rbp - 24]",
                "QWORD PTR [rbp - 16]",
            ),
        ] {
            let asm = emit_for(target);
            for kind in [2, 5, 6, 7, 8] {
                assert!(
                    asm.contains(&format!("    cmp {kind_operand}, {}{kind}\n", sigil(target))),
                    "kind {kind} exclusion must compare the resource kind word\n{asm}"
                );
                assert!(
                    !asm.contains(&format!("    cmp {payload_operand}, {}{kind}\n", sigil(target))),
                    "kind {kind} exclusion must not compare the native payload word\n{asm}"
                );
            }
        }
    }

    /// Returns the immediate-operand sigil the target's assembly syntax uses.
    fn sigil(target: Target) -> &'static str {
        if target.arch == Arch::X86_64 {
            ""
        } else {
            "#"
        }
    }
}
