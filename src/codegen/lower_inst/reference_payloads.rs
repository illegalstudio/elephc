//! Purpose:
//! Loads borrowed scalar reference values from raw or canonical managed cells.
//!
//! Called from:
//! - Native local and object-property reference reads.
//!
//! Key details:
//! - Physical shape is checked on each read because PHP callbacks may promote a cell.
//! - Canonical children are already type-validated; loading never coerces or acquires them.
//! - Headerless borrowed references retain their existing raw ABI.

use super::*;

/// Handles scalar reference layouts, returning false for the existing container-specific paths.
pub(super) fn load_scalar(
    ctx: &mut FunctionContext<'_>, pointer: &str, ty: &PhpType,
) -> bool {
    let ty = ty.codegen_repr();
    if !matches!(ty, PhpType::Int | PhpType::Bool | PhpType::False
        | PhpType::Float | PhpType::Str | PhpType::TaggedScalar)
    {
        return false;
    }
    let raw = ctx.next_label("scalar_reference_raw_payload");
    let done = ctx.next_label("scalar_reference_payload_done");
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_push_reg(ctx.emitter, pointer);
    abi::emit_reg_move(ctx.emitter, result, pointer);
    abi::emit_call_label(ctx.emitter, "__rt_reference_cell_canonical");
    abi::emit_branch_if_int_result_zero(ctx.emitter, &raw);
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match (&ty, ctx.emitter.target.arch) {
        (PhpType::Str, Arch::AArch64) => {}
        (PhpType::Str, Arch::X86_64) => {
            ctx.emitter.instruction("mov rax, rdi");                            // expose the borrowed string pointer beside its returned length
        }
        (PhpType::Float, Arch::AArch64) => {
            ctx.emitter.instruction("fmov d0, x1");                             // reinterpret the validated float payload without numeric coercion
        }
        (PhpType::Float, Arch::X86_64) => {
            ctx.emitter.instruction("movq xmm0, rdi");                          // reinterpret the validated float payload without numeric coercion
        }
        (PhpType::TaggedScalar, Arch::AArch64) => {
            ctx.emitter.instruction("mov x9, x0");                              // retain the nullable integer tag before exposing its payload
            ctx.emitter.instruction("mov x0, x1");                              // expose the validated integer or null low word
            ctx.emitter.instruction("mov x1, x9");                              // expose the existing tagged-scalar ABI tag
        }
        (PhpType::TaggedScalar, Arch::X86_64) => {
            ctx.emitter.instruction("mov rdx, rax");                            // retain the nullable integer tag before exposing its payload
            ctx.emitter.instruction("mov rax, rdi");                            // expose the validated integer or null low word
        }
        (_, Arch::AArch64) => {
            ctx.emitter.instruction("mov x0, x1");                              // expose the validated integer or boolean payload
        }
        (_, Arch::X86_64) => {
            ctx.emitter.instruction("mov rax, rdi");                            // expose the validated integer or boolean payload
        }
    }
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&raw);
    abi::emit_load_temporary_stack_slot(ctx.emitter, pointer, 0);
    match ty {
        PhpType::Str => {
            let (low, high) = abi::string_result_regs(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, high, pointer, 8);
            abi::emit_load_from_address(ctx.emitter, low, pointer, 0);
        }
        PhpType::Float => {
            abi::emit_load_from_address(ctx.emitter, abi::float_result_reg(ctx.emitter), pointer, 0);
        }
        PhpType::TaggedScalar => {
            let tag = crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, tag, pointer, 8);
            abi::emit_load_from_address(ctx.emitter, result, pointer, 0);
        }
        _ => abi::emit_load_from_address(ctx.emitter, result, pointer, 0),
    }
    ctx.emitter.label(&done);
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    true
}

#[cfg(test)]
mod tests {
    use crate::codegen_support::platform::Target;

    /// Scalar readers preserve the raw ABI while probing current canonical shape on every target.
    #[test]
    fn scalar_reference_readers_probe_current_physical_layout_on_every_target() {
        let source = "<?php \
            function readNativeIntReference(int &$value): int { return $value; } \
            function readNativeBoolReference(bool &$value): bool { return $value; } \
            function readNativeFloatReference(float &$value): float { return $value; } \
            function readNativeStringReference(string &$value): string { return $value; } \
            function readNativeNullableReference(?int &$value): ?int { return $value; }";
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let module = crate::ir_lower::tests::lower_source_at_for_target(source,
                std::path::Path::new("main.php"), std::path::Path::new("."), Target::parse(target).unwrap());
            let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
            for name in ["readNativeIntReference", "readNativeBoolReference", "readNativeFloatReference",
                "readNativeStringReference", "readNativeNullableReference"]
            {
                let body = assembly.split_once(&format!("@fn name={name} symbol=")).unwrap().1
                    .split_once(&format!("@endfn name={name}")).unwrap().0;
                let probe = body.find("__rt_reference_cell_canonical").unwrap();
                assert!(body[probe..].contains("__rt_mixed_unbox"),
                    "{target} {name}: canonical payloads must expose the actual child");
                assert!(body[probe..].contains("scalar_reference_raw_payload"),
                    "{target} {name}: headerless arguments must retain their raw layout");
                if target == "linux-x86_64" && name == "readNativeStringReference" {
                    let unbox = body[probe..].find("__rt_mixed_unbox").unwrap() + probe;
                    let canonical = body[unbox..].lines().take_while(|line|
                        !(line.trim_end().ends_with(':') && line.contains("scalar_reference_raw_payload")))
                        .collect::<Vec<_>>().join("\n");
                    assert!(canonical.contains("mov rax, rdi"),
                        "{target}: unbox returns the canonical string pointer in rdi");
                    assert!(!canonical.contains("mov rdx, rsi"),
                        "{target}: unbox already returns the string length in rdx");
                }
            }
        }
    }
}
