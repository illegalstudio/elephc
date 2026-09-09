//! Purpose:
//! Converts descriptor-owned Mixed results into the caller's concrete scalar ABI.
//!
//! Called from:
//! - `super::store_descriptor_invoker_result()` after descriptor invocation.
//!
//! Key details:
//! - Mixed results transfer their box; concrete results consume it after conversion.
//! - Converted register pairs and floating-point values survive the box release.

use crate::codegen::{CodegenIrError, Result};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::types::PhpType;

/// Converts an owned result box and retires it unless the caller retains the Mixed representation.
pub(super) fn emit_unbox_owned_descriptor_result(emitter: &mut Emitter, ty: &PhpType) -> Result<()> {
    let cast = match ty {
        PhpType::Mixed | PhpType::Union(_) => return Ok(()),
        PhpType::Void | PhpType::Never => {
            abi::emit_call_label(emitter, "__rt_decref_mixed");
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0x7fff_ffff_ffff_fffe);
            return Ok(());
        }
        PhpType::Int => "__rt_mixed_cast_int",
        PhpType::Bool => "__rt_mixed_cast_bool",
        PhpType::Float => "__rt_mixed_cast_float",
        PhpType::Str => "__rt_mixed_cast_string",
        PhpType::TaggedScalar => "__rt_mixed_unbox",
        other => return Err(CodegenIrError::unsupported(format!(
            "descriptor invoker result for PHP type {:?}", other,
        ))),
    };
    let result = abi::int_result_reg(emitter);
    abi::emit_push_reg(emitter, result);
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 0), result);
    abi::emit_call_label(emitter, cast);
    if *ty == PhpType::TaggedScalar {
        emit_tagged_scalar_result(emitter);
    }
    // String conversion detaches string payloads; scalar conversions borrow no heap children.
    abi::emit_push_result_value(emitter, ty);
    abi::emit_load_temporary_stack_slot(emitter, result, 16);
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    match ty {
        PhpType::Str => {
            let (pointer, length) = abi::string_result_regs(emitter);
            abi::emit_pop_reg_pair(emitter, pointer, length);
        }
        PhpType::Float => abi::emit_pop_float_reg(emitter, abi::float_result_reg(emitter)),
        PhpType::TaggedScalar => abi::emit_pop_reg_pair(
            emitter, result, crate::codegen_support::sentinels::tagged_scalar_tag_reg(emitter),
        ),
        _ => abi::emit_pop_reg(emitter, result),
    }
    abi::emit_release_temporary_stack(emitter, 16);
    Ok(())
}

/// Moves the unboxed tag and payload into the inline nullable-scalar return convention.
fn emit_tagged_scalar_result(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x9, x0");                                  // preserve the Mixed tag before moving the scalar payload
            emitter.instruction("mov x0, x1");                                  // place the payload in the inline scalar result register
            emitter.instruction("mov x1, x9");                                  // place the tag beside the inline scalar payload
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10, rax");                                // preserve the Mixed tag before moving the scalar payload
            emitter.instruction("mov rax, rdi");                                // place the payload in the inline scalar result register
            emitter.instruction("mov rdx, r10");                                // place the tag beside the inline scalar payload
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// All concrete scalar shapes release one source box after casting on every supported target.
    #[test]
    fn concrete_descriptor_results_consume_one_box_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            for (ty, cast) in [
                (PhpType::Int, "__rt_mixed_cast_int"),
                (PhpType::Bool, "__rt_mixed_cast_bool"),
                (PhpType::Float, "__rt_mixed_cast_float"),
                (PhpType::Str, "__rt_mixed_cast_string"),
                (PhpType::TaggedScalar, "__rt_mixed_unbox"),
            ] {
                let mut emitter = Emitter::new(target);
                emit_unbox_owned_descriptor_result(&mut emitter, &ty).unwrap();
                let asm = emitter.output();
                assert_eq!(asm.matches("__rt_decref_mixed").count(), 1, "{name}: {ty:?}");
                assert!(asm.find(cast).unwrap() < asm.find("__rt_decref_mixed").unwrap(), "{name}: {ty:?}");
            }
            let mut emitter = Emitter::new(target);
            emit_unbox_owned_descriptor_result(&mut emitter, &PhpType::Mixed).unwrap();
            assert!(emitter.output().is_empty(), "{name}");
            let mut emitter = Emitter::new(target);
            emit_unbox_owned_descriptor_result(&mut emitter, &PhpType::Void).unwrap();
            assert_eq!(emitter.output().matches("__rt_decref_mixed").count(), 1, "{name}");
        }
    }
}
