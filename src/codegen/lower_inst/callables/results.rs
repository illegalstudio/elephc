//! Purpose:
//! Converts descriptor-owned Mixed results into the caller's concrete ABI.
//!
//! Called from:
//! - `super::store_descriptor_invoker_result()` after descriptor invocation.
//!
//! Key details:
//! - Mixed results transfer their box; concrete results consume it after conversion.
//! - Converted scalars and detached Mixed-element arrays survive the box release.

use crate::codegen::{CodegenIrError, Result};
use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::ir::Ownership;
use crate::types::PhpType;

/// Converts an owned result box and retires it unless the caller retains the Mixed representation.
pub(super) fn emit_unbox_owned_descriptor_result(
    emitter: &mut Emitter,
    ty: &PhpType,
    ownership: Ownership,
) -> Result<()> {
    // A `MaybeOwned` string is released through the heap-range check, which frees the buffer
    // the cast detached; the EIR keeps call-result strings conservative by design, so an
    // interface-receiver first-class callable (`$iface->m(...)`) reached this as `MaybeOwned`.
    if *ty == PhpType::Str && !matches!(ownership, Ownership::Owned | Ownership::MaybeOwned) {
        return Err(CodegenIrError::invalid_module(format!(
            "concrete descriptor string result must own its detached buffer, got {ownership:?}",
        )));
    }
    if matches!(ty, PhpType::Object(_)) {
        emit_object_descriptor_result(emitter, ty);
        return Ok(());
    }
    let cast = match ty {
        PhpType::Mixed | PhpType::Union(_) => return Ok(()),
        PhpType::Void | PhpType::Never => {
            abi::emit_call_label(emitter, "__rt_decref_mixed");
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0x7fff_ffff_ffff_fffe);
            return Ok(());
        }
        // A pointer or buffer handle is one raw word boxed as an integer (a `?ptr<T>` null as
        // PHP null), so the integer cast recovers the word, 0 for null: `buffer_ptr(...)` or
        // `ptr_null(...)` forwarded through a `callable` parameter answers its address this way.
        PhpType::Int | PhpType::Pointer(_) | PhpType::Buffer(_) => "__rt_mixed_cast_int",
        PhpType::Bool => "__rt_mixed_cast_bool",
        PhpType::Float => "__rt_mixed_cast_float",
        PhpType::Str => "__rt_mixed_cast_string",
        PhpType::TaggedScalar => "__rt_mixed_unbox",
        // Any container or object payload sits in the box's low word: retain it, retire the
        // box. A narrower inferred return type -- `array<string, mixed>` from a callback that
        // returns `get_object_vars()` -- reached this as an unsupported result type.
        PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Object(_)
        | PhpType::Iterable => {
            emit_detach_owned_mixed_array_result(emitter, &PhpType::Array(Box::new(PhpType::Mixed)));
            return Ok(());
        }
        // A closure returned by a closure (`return function () use (&$s) { ... };`) travels as a
        // descriptor pointer in the box's payload word; it is retained like any callable value.
        PhpType::Callable => {
            emit_detach_owned_mixed_array_result(emitter, &PhpType::Callable);
            return Ok(());
        }
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
    // String conversion detaches an owned payload that the EIR result must retire exactly once;
    // scalar conversions borrow no heap children.
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

/// Converts an owned Mixed result box into a raw object pointer result.
///
/// The box owns one reference to the object payload, so the raw EIR result needs its own
/// reference before the box is retired. `__rt_incref` preserves the pointer register, which
/// keeps the object payload live across the box release.
fn emit_object_descriptor_result(emitter: &mut Emitter, ty: &PhpType) {
    let result = abi::int_result_reg(emitter);
    abi::emit_push_reg(emitter, result);
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 0), result);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, x1");                                  // move the boxed object pointer into the raw result register
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, rdi");                                // move the boxed object pointer into the raw result register
        }
    }
    abi::emit_incref_if_refcounted(emitter, ty);
    abi::emit_push_result_value(emitter, ty);
    abi::emit_load_temporary_stack_slot(emitter, result, 16);
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    abi::emit_pop_reg(emitter, result);
    abi::emit_release_temporary_stack(emitter, 16);
}

/// Transfers a container or object payload out of its owned Mixed result box.
///
/// An array may use indexed or promoted hash storage at runtime, and an object is a single
/// pointer too. Every such shape exposes its pointer in the low Mixed payload word, and the
/// generic retain preserves any heap kind.
fn emit_detach_owned_mixed_array_result(emitter: &mut Emitter, retained_ty: &PhpType) {
    let result = abi::int_result_reg(emitter);
    abi::emit_push_reg(emitter, result);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, x1");                                  // move the borrowed array payload into the raw array result register
        }
        Arch::X86_64 => {
            emitter.instruction("mov rax, rdi");                                // move the borrowed array payload into the raw array result register
        }
    }
    abi::emit_incref_if_refcounted(emitter, retained_ty);
    abi::emit_push_reg(emitter, result);
    abi::emit_load_temporary_stack_slot(emitter, result, 16);
    abi::emit_call_label(emitter, "__rt_decref_mixed");
    abi::emit_pop_reg(emitter, result);
    abi::emit_release_temporary_stack(emitter, 16);
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
                let ownership = if ty == PhpType::Str {
                    Ownership::Owned
                } else {
                    Ownership::for_php_type(&ty)
                };
                emit_unbox_owned_descriptor_result(&mut emitter, &ty, ownership).unwrap();
                let asm = emitter.output();
                assert_eq!(asm.matches("__rt_decref_mixed").count(), 1, "{name}: {ty:?}");
                assert!(asm.find(cast).unwrap() < asm.find("__rt_decref_mixed").unwrap(), "{name}: {ty:?}");
            }
            let mut emitter = Emitter::new(target);
            emit_unbox_owned_descriptor_result(
                &mut emitter,
                &PhpType::Mixed,
                Ownership::Owned,
            )
            .unwrap();
            assert!(emitter.output().is_empty(), "{name}");
            let mut emitter = Emitter::new(target);
            emit_unbox_owned_descriptor_result(
                &mut emitter,
                &PhpType::Void,
                Ownership::NonHeap,
            )
            .unwrap();
            assert_eq!(emitter.output().matches("__rt_decref_mixed").count(), 1, "{name}");

            let object_ty = PhpType::Object("GMP".to_string());
            let mut emitter = Emitter::new(target);
            emit_unbox_owned_descriptor_result(&mut emitter, &object_ty, Ownership::Owned).unwrap();
            let asm = emitter.output();
            assert_eq!(asm.matches("__rt_mixed_unbox").count(), 1, "{name}");
            assert_eq!(asm.matches("__rt_incref").count(), 1, "{name}");
            assert_eq!(asm.matches("__rt_decref_mixed").count(), 1, "{name}");
            assert!(
                asm.find("__rt_mixed_unbox").unwrap() < asm.find("__rt_incref").unwrap(),
                "{name}",
            );
            assert!(
                asm.find("__rt_incref").unwrap() < asm.find("__rt_decref_mixed").unwrap(),
                "{name}",
            );
        }
    }

    /// A concrete descriptor string must carry the owner produced by its Mixed cast.
    ///
    /// `MaybeOwned` is accepted on purpose: the EIR keeps call-result strings conservative, so an
    /// interface-receiver first-class callable (`$iface->m(...)`) reaches this with `MaybeOwned`,
    /// and the release goes through the heap-range check that frees the buffer the cast detached
    /// (`ic.php`-style programs print the same bytes as php with a clean heap). A value that is
    /// borrowed, persistent or not a heap string at all still fails closed, with nothing emitted.
    #[test]
    fn concrete_string_descriptor_results_fail_closed_without_owned_metadata() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for ownership in [Ownership::Borrowed, Ownership::Persistent, Ownership::NonHeap, Ownership::Moved] {
                let mut emitter = Emitter::new(Target::parse(name).unwrap());
                let error = emit_unbox_owned_descriptor_result(&mut emitter, &PhpType::Str, ownership)
                    .unwrap_err();
                assert!(error.to_string().contains("must own its detached buffer"), "{name}: {error}");
                assert!(emitter.output().is_empty(), "{name}");
            }
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_unbox_owned_descriptor_result(&mut emitter, &PhpType::Str, Ownership::MaybeOwned)
                .unwrap();
            let asm = emitter.output();
            assert!(asm.find("__rt_mixed_cast_string").unwrap() < asm.find("__rt_decref_mixed").unwrap(), "{name}");
        }
    }

    /// Mixed-element array results retain their payload before consuming the descriptor box.
    #[test]
    fn mixed_array_descriptor_results_transfer_container_ownership_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_unbox_owned_descriptor_result(
                &mut emitter,
                &PhpType::Array(Box::new(PhpType::Mixed)),
                Ownership::Owned,
            )
            .unwrap();
            let asm = emitter.output();
            assert_eq!(asm.matches("__rt_mixed_unbox").count(), 1, "{name}");
            assert_eq!(asm.matches("__rt_incref").count(), 1, "{name}");
            assert_eq!(asm.matches("__rt_decref_mixed").count(), 1, "{name}");
            assert!(
                asm.find("__rt_mixed_unbox").unwrap() < asm.find("__rt_incref").unwrap(),
                "{name}",
            );
            assert!(
                asm.find("__rt_incref").unwrap() < asm.find("__rt_decref_mixed").unwrap(),
                "{name}",
            );
        }
    }
}
