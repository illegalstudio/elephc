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

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::sentinels::emit_branch_if_null_container;

/// Unwraps a boxed Mixed cell into a concrete runtime payload triple.
///
/// Nested Mixed wrappers are peeled until a concrete tag is reached, and any
/// legacy container-shaped zero/sentinel payload is returned as canonical null.
/// Input:  x0 = boxed mixed pointer (may be null)
/// Output: x0 = runtime value tag, x1 = value_lo, x2 = value_hi
pub fn emit_mixed_unbox(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_unbox_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed_unbox ---");
    emitter.label_global("__rt_mixed_unbox");

    // -- null mixed pointers behave like null payloads --
    emitter.instruction("cbz x0, __rt_mixed_unbox_null");                       // null boxed values unwrap to the null runtime tag

    // -- keep following nested mixed payloads until we reach a concrete tag --
    emitter.label("__rt_mixed_unbox_loop");
    emitter.instruction("mov x10, x0");                                         // preserve the current mixed cell pointer while inspecting its tag
    emitter.instruction("ldr x9, [x0]");                                        // x9 = boxed payload tag
    emitter.instruction("cmp x9, #7");                                          // does this mixed box wrap another mixed value?
    emitter.instruction("b.ne __rt_mixed_unbox_done");                          // stop once the payload tag is concrete
    emitter.instruction("ldr x0, [x0, #8]");                                    // follow the nested mixed pointer stored in value_lo
    emitter.instruction("cbz x0, __rt_mixed_unbox_null");                       // null nested boxes unwrap to the null runtime tag
    emitter.instruction("b __rt_mixed_unbox_loop");                             // continue peeling nested mixed wrappers

    emitter.label("__rt_mixed_unbox_done");
    emitter.instruction("mov x0, x9");                                          // return the concrete runtime tag in x0
    emitter.instruction("ldr x1, [x10, #8]");                                   // return the concrete payload low word in x1
    emitter.instruction("ldr x2, [x10, #16]");                                  // return the concrete payload high word in x2
    emitter.instruction("cmp x0, #4");                                          // only container-shaped tags can encode null in their payload pointer
    emitter.instruction("b.lt __rt_mixed_unbox_return");                        // scalar payloads preserve sentinel-colliding integer bit patterns
    emitter.instruction("cmp x0, #6");                                          // indexed arrays, hashes, and objects occupy tags 4 through 6
    emitter.instruction("b.gt __rt_mixed_unbox_return");                        // other heap tags are not null-container encodings
    emit_branch_if_null_container(emitter, "x1", "x9", "__rt_mixed_unbox_null");
    emitter.label("__rt_mixed_unbox_return");
    emitter.instruction("ret");                                                 // return the unboxed payload triple

    emitter.label("__rt_mixed_unbox_null");
    emitter.instruction("mov x0, #8");                                          // runtime tag 8 = null
    emitter.instruction("mov x1, #0");                                          // null has no low payload word
    emitter.instruction("mov x2, #0");                                          // null has no high payload word
    emitter.instruction("ret");                                                 // return the normalized null payload triple
}

/// x86_64 Linux variant of `emit_mixed_unbox` using System V ABI register conventions.
fn emit_mixed_unbox_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_unbox ---");
    emitter.label_global("__rt_mixed_unbox");

    emitter.instruction("test rax, rax");                                       // null mixed pointers behave like null payloads
    emitter.instruction("je __rt_mixed_unbox_null");                            // null boxed values unwrap to the null runtime tag

    emitter.label("__rt_mixed_unbox_loop");
    emitter.instruction("mov r10, rax");                                        // preserve the current mixed cell pointer while inspecting its tag
    emitter.instruction("mov r11, QWORD PTR [rax]");                            // r11 = boxed payload tag
    emitter.instruction("cmp r11, 7");                                          // does this mixed box wrap another mixed value?
    emitter.instruction("jne __rt_mixed_unbox_done");                           // stop once the payload tag is concrete
    emitter.instruction("mov rax, QWORD PTR [rax + 8]");                        // follow the nested mixed pointer stored in value_lo
    emitter.instruction("test rax, rax");                                       // null nested boxes unwrap to the null runtime tag
    emitter.instruction("je __rt_mixed_unbox_null");                            // normalize null nested boxes to the null runtime tag
    emitter.instruction("jmp __rt_mixed_unbox_loop");                           // continue peeling nested mixed wrappers

    emitter.label("__rt_mixed_unbox_done");
    emitter.instruction("mov rax, r11");                                        // return the concrete runtime tag in rax
    emitter.instruction("mov rdi, QWORD PTR [r10 + 8]");                        // return the concrete payload low word in rdi
    emitter.instruction("mov rdx, QWORD PTR [r10 + 16]");                       // return the concrete payload high word in rdx
    emitter.instruction("cmp rax, 4");                                          // only container-shaped tags can encode null in their payload pointer
    emitter.instruction("jl __rt_mixed_unbox_return");                          // scalar payloads preserve sentinel-colliding integer bit patterns
    emitter.instruction("cmp rax, 6");                                          // indexed arrays, hashes, and objects occupy tags 4 through 6
    emitter.instruction("jg __rt_mixed_unbox_return");                          // other heap tags are not null-container encodings
    emit_branch_if_null_container(emitter, "rdi", "r11", "__rt_mixed_unbox_null");
    emitter.label("__rt_mixed_unbox_return");
    emitter.instruction("ret");                                                 // return the unboxed payload triple

    emitter.label("__rt_mixed_unbox_null");
    emitter.instruction("mov rax, 8");                                          // runtime tag 8 = null
    emitter.instruction("xor rdi, rdi");                                        // null has no low payload word
    emitter.instruction("xor rdx, rdx");                                        // null has no high payload word
    emitter.instruction("ret");                                                 // return the normalized null payload triple
}

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
