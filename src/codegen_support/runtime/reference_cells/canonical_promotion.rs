//! Purpose:
//! Gives managed property cells persistent boxed reference identities.
//!
//! Called from:
//! - Explicit native-reference validation runtime emission and property reference selection.
//!
//! Key details:
//! - Promotion preserves the cell address, transfers its payload owner and retains header flags.
//! - Borrowed constructor storage never enters these managed-cell helpers.

use super::*;
use crate::types::CanonicalReferencePayload;

/// Maps reviewed physical payload plans to their authoritative runtime promotion entry.
pub(crate) fn promotion_entry(payload: CanonicalReferencePayload) -> &'static str {
    match payload {
        CanonicalReferencePayload::Boxed => "__rt_reference_cell_promote_mixed",
        CanonicalReferencePayload::Int => "__rt_reference_cell_promote_int",
        CanonicalReferencePayload::Float => "__rt_reference_cell_promote_float",
        CanonicalReferencePayload::Bool => "__rt_reference_cell_promote_bool",
        CanonicalReferencePayload::String => "__rt_reference_cell_promote_string",
        CanonicalReferencePayload::NullableInt => "__rt_reference_cell_promote_nullable_int",
        CanonicalReferencePayload::Object => "__rt_reference_cell_promote_object",
    }
}

/// Emits one typed entry per raw value layout, sharing ordinary boxing and reference promotion.
pub(super) fn emit(emitter: &mut Emitter) {
    for (payload, ty) in [(CanonicalReferencePayload::Int, PhpType::Int),
        (CanonicalReferencePayload::Float, PhpType::Float),
        (CanonicalReferencePayload::Bool, PhpType::Bool),
        (CanonicalReferencePayload::String, PhpType::Str),
        (CanonicalReferencePayload::NullableInt, PhpType::TaggedScalar),
        (CanonicalReferencePayload::Object, PhpType::Object(String::new()))] {
        let entry = promotion_entry(payload);
        let done = format!("{entry}_done");
        let result = abi::int_result_reg(emitter);
        let pointer = abi::symbol_scratch_reg(emitter);
        emitter.blank();
        emitter.label_global(entry);
        abi::emit_frame_prologue(emitter, 32);
        abi::store_at_offset(emitter, result, 8);
        abi::emit_call_label(emitter, "__rt_reference_cell_canonical");
        abi::emit_branch_if_int_result_nonzero(emitter, &done);
        abi::load_at_offset(emitter, pointer, 8);
        match ty {
            PhpType::Str => {
                let (low, high) = abi::string_result_regs(emitter);
                abi::emit_load_from_address(emitter, high, pointer, 8);
                abi::emit_load_from_address(emitter, low, pointer, 0);
            }
            PhpType::Float => abi::emit_load_from_address(emitter, abi::float_result_reg(emitter), pointer, 0),
            PhpType::TaggedScalar => {
                let tag = crate::codegen_support::sentinels::tagged_scalar_tag_reg(emitter);
                abi::emit_load_from_address(emitter, tag, pointer, 8);
                abi::emit_load_from_address(emitter, result, pointer, 0);
            }
            _ => abi::emit_load_from_address(emitter, result, pointer, 0),
        }
        crate::codegen_support::value_boxing::emit_box_current_owned_value_as_mixed(emitter, &ty);
        abi::load_at_offset(emitter, pointer, 8);
        abi::emit_store_to_address(emitter, result, pointer, 0);
        abi::emit_store_zero_to_address(emitter, pointer, 8);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("ldr x10, [{pointer}, #-8]"));     // preserve the managed cell's collector and heap flags
                emitter.instruction("mov x11, #7");                             // the new physical payload owns one boxed Mixed value
                emitter.instruction("bfi x10, x11, #8, #7");                    // replace only the payload descriptor bits
                emitter.instruction(&format!("str x10, [{pointer}, #-8]"));     // publish the new physical layout before canonical wrapping
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("and QWORD PTR [{pointer} - 8], -32513")); // clear only the seven descriptor bits above the heap kind
                emitter.instruction(&format!("or QWORD PTR [{pointer} - 8], 1792")); // publish descriptor seven while preserving collector flags
            }
        }
        abi::emit_reg_move(emitter, result, pointer);
        abi::emit_call_label(emitter, "__rt_reference_cell_promote_mixed");
        emitter.label(&done);
        abi::load_at_offset(emitter, result, 8);
        abi::emit_frame_restore(emitter, 32);
        abi::emit_return(emitter);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, process::{Command, Stdio}};

    /// Real assemblers accept each typed promotion helper on all supported object formats.
    #[test]
    #[ignore = "requires clang with ELF and Apple AArch64 assembler support"]
    fn scalar_reference_promotion_assembles_on_all_supported_targets() {
        for (name, triple) in [("linux-x86_64", "x86_64-linux-gnu"),
            ("linux-aarch64", "aarch64-linux-gnu"), ("macos-aarch64", "arm64-apple-macos11"),
            ("ios-arm64", "arm64-apple-ios13"), ("ios-sim-arm64", "arm64-apple-ios13-simulator")] {
            let target = crate::codegen_support::platform::Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit(&mut emitter);
            let syntax = if target.arch == Arch::X86_64 { ".intel_syntax noprefix\n" } else { "" };
            let assembly = format!("{syntax}.text\n{}", emitter.output());
            let mut process = Command::new("clang").args(["-target", triple, "-x", "assembler",
                "-c", "-o", "/dev/null", "-"]).stdin(Stdio::piped()).stderr(Stdio::piped())
                .spawn().expect("clang is required for the explicit cross-target assembly test");
            process.stdin.take().unwrap().write_all(assembly.as_bytes()).unwrap();
            let output = process.wait_with_output().unwrap();
            assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
            assert_eq!(assembly.matches("__rt_reference_cell_canonical").count(), 6, "{name}");
            assert_eq!(assembly.matches("__rt_reference_cell_promote_mixed").count(), 6, "{name}");
        }
    }
}
