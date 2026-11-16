//! Purpose:
//! Emits the `__rt_mixed_unbox`, `__rt_mixed_unbox_null` runtime helper assembly for mixed unbox.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Mixed helpers use boxed tag/payload cells; tag constants and ownership rules are shared with type checking and codegen.
//! - Legacy container-shaped boxes with a null/sentinel payload unbox as canonical
//!   PHP null, so every tag-dispatch consumer receives a safe structural shape.
//! - The return triple is `(tag, payload_lo, payload_hi)` in `x0`/`x1`/`x2` on AArch64 and
//!   `rax`/`rdi`/`rdx` on x86_64. Callers read the payload register from
//!   `crate::codegen_support::mixed_unbox_payload_reg()`; the test below holds that helper
//!   and this emitter together on every supported target.

pub use elephc_memoir::values::mixed_unbox::emit_mixed_unbox;

#[cfg(test)]
use crate::codegen_support::{emit::Emitter, platform::Arch};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// The payload register the shared unbox contract names is the one the emitted helper
    /// actually writes its low word into, on every supported target.
    ///
    /// This is the regression for a real slip: reading the payload out of the first ARGUMENT
    /// register is right on x86_64 and wrong on all four AArch64 targets, where that register
    /// carries the TAG and an object pointer read from it is the constant `6`. Reading the
    /// answer back off this emitter, instead of restating it at the call sites, is what keeps
    /// the two sides unable to drift apart.
    ///
    /// The assertion walks the `__rt_mixed_unbox_done` block rather than matching comment text,
    /// because the emitter's plain output carries instructions only.
    #[test]
    fn the_unbox_payload_register_matches_the_emitted_runtime_helper_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_mixed_unbox(&mut emitter);
            let result_reg = crate::codegen_support::abi::int_result_reg(&emitter);
            let asm = emitter.output();
            let payload_reg = crate::codegen_support::mixed_unbox_payload_reg(target);

            let done = asm
                .split_once("__rt_mixed_unbox_done:\n")
                .unwrap_or_else(|| panic!("{name}: the unbox helper must reach a done block"))
                .1;
            let mut instructions = done.lines().map(str::trim).filter(|line| !line.is_empty());
            let tag_write = instructions.next().unwrap_or_default();
            let payload_write = instructions.next().unwrap_or_default();

            assert_eq!(
                destination_register(tag_write),
                result_reg,
                "{name}: the runtime tag must come back in the result register"
            );
            assert_eq!(
                destination_register(payload_write),
                payload_reg,
                "{name}: expected the payload low word in {payload_reg}, got `{payload_write}`"
            );
            assert!(
                payload_write.contains('8'),
                "{name}: the payload low word lives at cell offset 8, got `{payload_write}`"
            );
            if target.arch == Arch::AArch64 {
                assert_ne!(
                    payload_reg,
                    crate::codegen_support::abi::int_arg_reg_name(target, 0),
                    "{name}: the payload register must not be read off the argument-register helper"
                );
            }
        }
    }

    /// Names the register an emitted instruction writes into.
    ///
    /// Both architectures spell their destination as the first operand, so one splitter serves
    /// `ldr x1, [x10, #8]` and `mov rdi, QWORD PTR [r10 + 8]` alike.
    fn destination_register(instruction: &str) -> &str {
        instruction
            .split_once(' ')
            .map(|(_, operands)| operands.split(',').next().unwrap_or_default().trim())
            .unwrap_or_default()
    }
}
