//! Purpose:
//! Borrows normalized eval argument cells for native method and constructor bridges.
//!
//! Called from:
//! - `super::eval_method_helpers` and `super::eval_constructor_helpers`.
//!
//! Key details:
//! - Magician owns a dense indexed array of Mixed cells throughout each native invocation.
//! - Reads allocate nothing and retain nothing; the enclosing argument array roots every borrow.
//! - Shape and bounds guards reject malformed bridge inputs before dereferencing an element.
//! - Consuming intrinsic helpers acquire separate owners after argument staging succeeds.
//! - Callable arguments are the exception to "retain nothing": the cast builds an owned
//!   descriptor, which the bridge parks in a zeroed owner slot and releases after the call.

use crate::codegen::{abi, emit::Emitter, platform::Arch};
use crate::intrinsics::IntrinsicCall;
use crate::types::PhpType;

/// Preserves the PHP array constraint while lowering all other bridge metadata to its ABI type.
///
/// A `?ptr<T>` keeps its union too: its ABI form is the pointer word either way, and boxing it
/// needs the null member so address 0 reaches eval as PHP null.
pub(super) fn bridge_storage_type(ty: &PhpType) -> PhpType {
    if ty.is_php_array() || crate::types::is_nullable_pointer_type(ty) {
        ty.clone()
    } else {
        ty.codegen_repr()
    }
}

/// Rejects non-array boxed inputs without transferring ownership or converting their storage.
/// The borrowed cell enters in the integer result register; callers reload it after the tag check.
pub(super) fn emit_require_php_array(emitter: &mut Emitter, fail_label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("bl __rt_mixed_unbox");                         // inspect the boxed argument without acquiring or consuming owners
            emitter.instruction("sub x0, x0, #4");                              // map packed and hash tags to the contiguous range zero through one
            emitter.instruction("cmp x0, #1");                                  // only PHP array payloads satisfy an array declaration
            emitter.instruction(&format!("b.hi {fail_label}"));                 // reject scalar, null, callable, and object payloads
        }
        Arch::X86_64 => {
            emitter.instruction("call __rt_mixed_unbox");                       // inspect the boxed argument without changing its ownership
            emitter.instruction("sub rax, 4");                                  // map packed and hash tags to the contiguous range zero through one
            emitter.instruction("cmp rax, 1");                                  // only PHP array payloads satisfy an array declaration
            emitter.instruction(&format!("ja {fail_label}"));                   // reject scalar, null, callable, and object payloads
        }
    }
}

/// Borrows a normalized by-value string payload, returning false for non-string parameters.
/// Magician applies PHP coercion before staging; its argument array roots the string for the call.
/// Reference slots and object-field initialization must keep their separate owning conversion.
pub(super) fn emit_borrowed_string_arg(
    emitter: &mut Emitter,
    param_type: &PhpType,
    frame_offset: usize,
    fail_label: &str,
) -> bool {
    if param_type.codegen_repr() != PhpType::Str {
        return false;
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x0, [x29, #-{frame_offset}]"));   // reload the argument cell retained by the normalized argument array
            emitter.instruction(&format!("cbz x0, {fail_label}"));              // reject a missing normalized string cell
            emitter.instruction("ldr x9, [x0]");                                // inspect the normalized argument tag before borrowing its payload
            emitter.instruction("cmp x9, #1");                                  // PHP parameter coercion must already have produced a string
            emitter.instruction(&format!("b.ne {fail_label}"));                 // reject malformed bridge input without allocating a conversion
            emitter.instruction("ldr x1, [x0, #8]");                            // borrow the stable string bytes for the native activation
            emitter.instruction("ldr x2, [x0, #16]");                           // preserve the full string length including embedded NUL bytes
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {frame_offset}]")); // reload the argument cell retained by the normalized argument array
            emitter.instruction("test rax, rax");                               // check whether the normalized string cell exists
            emitter.instruction(&format!("jz {fail_label}"));                   // reject a missing normalized string cell
            emitter.instruction("cmp QWORD PTR [rax], 1");                      // PHP parameter coercion must already have produced a string
            emitter.instruction(&format!("jne {fail_label}"));                  // reject malformed bridge input without allocating a conversion
            emitter.instruction("mov rdx, QWORD PTR [rax + 16]");               // preserve the full string length including embedded NUL bytes
            emitter.instruction("mov rax, QWORD PTR [rax + 8]");                // borrow the stable string bytes for the native activation
        }
    }
    true
}

/// By-value `callable` parameters whose staged descriptor the bridge owns across the call.
///
/// The eval callable cast hands back an OWNED descriptor on every path: a runtime adapter
/// around the eval callback, a receiver-bound descriptor for `[$obj, 'm']` or an invokable
/// object, a retained AOT descriptor, or a static descriptor whose release is a no-op. A native
/// callee borrows its `callable` arguments (an AOT caller releases the descriptor after the
/// call), so the bridge must release each one once the call has returned or thrown. It did
/// not: every `new C(fn …)` / `$o->m(fn …)` from eval leaked the adapter, the retained eval
/// closure and its context (5 blocks; 3 for a string callable like `"is_int"`).
pub(super) fn callable_arg_owner_params(params: &[PhpType], ref_params: &[bool]) -> Vec<usize> {
    params
        .iter()
        .enumerate()
        .filter(|(index, ty)| {
            !ref_params.get(*index).copied().unwrap_or(false)
                && ty.codegen_repr() == PhpType::Callable
        })
        .map(|(index, _)| index)
        .collect()
}

/// Reserves one zeroed temporary-stack slot per owned callable argument descriptor.
///
/// Reserve BEFORE the by-reference cells and argument temporaries are pushed, so that once the
/// call has popped those, the owner slots are the top of the temporary stack again.
pub(super) fn emit_reserve_callable_arg_owners(emitter: &mut Emitter, count: usize) {
    if count == 0 {
        return;
    }
    abi::emit_reserve_temporary_stack(emitter, count * 16);
    for index in 0..count {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("str xzr, [sp, #{}]", index * 16)); // start with no owned callable descriptor in this slot
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov QWORD PTR [rsp + {}], 0", index * 16)); // start with no owned callable descriptor in this slot
            }
        }
    }
}

/// Records the cast callable descriptor (in the integer result register) in its owner slot.
///
/// `pushed_bytes` is everything pushed above the owner slots so far: the by-reference cells
/// and the argument temporaries staged before this one.
pub(super) fn emit_record_callable_arg_owner(
    emitter: &mut Emitter,
    owner_index: usize,
    pushed_bytes: usize,
) {
    let result = abi::int_result_reg(emitter);
    abi::emit_store_to_sp(emitter, result, pushed_bytes + owner_index * 16);
}

/// Records the descriptor for parameter `param_index` when the bridge owns it.
pub(super) fn emit_record_callable_arg_owner_for_param(
    emitter: &mut Emitter,
    owners: &[usize],
    param_index: usize,
    pushed_bytes: usize,
) {
    if let Some(owner_index) = owners.iter().position(|index| *index == param_index) {
        emit_record_callable_arg_owner(emitter, owner_index, pushed_bytes);
    }
}

/// Releases every owned callable argument descriptor and pops the owner slots.
///
/// Runs on the normal and the throwing exit alike, after the by-reference cells are gone, so
/// the owner slots are on top. `preserve_result` keeps a boxed method result across the calls.
pub(super) fn emit_release_callable_arg_owners(
    emitter: &mut Emitter,
    count: usize,
    preserve_result: bool,
) {
    if count == 0 {
        return;
    }
    let result = abi::int_result_reg(emitter);
    let base = if preserve_result {
        abi::emit_push_reg(emitter, result);
        16
    } else {
        0
    };
    for index in 0..count {
        abi::emit_load_temporary_stack_slot(emitter, result, base + index * 16);
        abi::emit_call_label(emitter, "__rt_callable_descriptor_release");
    }
    if preserve_result {
        abi::emit_pop_reg(emitter, result);
    }
    abi::emit_release_temporary_stack(emitter, count * 16);
}

/// Transfers independent owners for staged boxed arguments consumed by an intrinsic helper.
/// Call after fallible preparation and boundary installation, before materializing ABI registers.
pub(super) fn emit_consumed_intrinsic_arguments(
    emitter: &mut Emitter,
    intrinsic: IntrinsicCall,
    parameters: &[PhpType],
) {
    for &index in intrinsic.consumed_mixed_parameters() {
        assert_eq!(parameters[index].codegen_repr(), PhpType::Mixed);
        let offset: usize = parameters[index + 1..].iter()
            .map(super::eval_ref_arg_helpers::eval_arg_temp_slot_size).sum();
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("ldr x0, [sp, #{offset}]"));       // load the staged argument whose owner passes to the intrinsic
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {offset}]")); // load the staged argument whose owner passes to the intrinsic
            }
        }
        abi::emit_call_label(emitter, "__rt_incref");
    }
}

/// Loads a borrowed argument into the bridge's fixed frame spill without creating a key or owner.
pub(super) fn emit_borrowed_argument(
    emitter: &mut Emitter,
    index: usize,
    array_frame_offset: usize,
    result_frame_offset: usize,
    fail_label: &str,
) {
    // The validation itself is the shared `__rt_eval_borrow_arg`: inline, it was 22
    // instructions per parameter of every bridged method.
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x0, [x29, #-{array_frame_offset}]")); // load the caller-owned argument array box
            abi::emit_load_int_immediate(emitter, "x1", index as i64);
            abi::emit_call_label(emitter, "__rt_eval_borrow_arg");
            emitter.instruction(&format!("cbz x1, {fail_label}"));              // reject a missing or malformed argument
            emitter.instruction(&format!("str x0, [x29, #-{result_frame_offset}]")); // keep the borrow available for coercion and reference writeback
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {array_frame_offset}]")); // load the caller-owned argument array box
            abi::emit_load_int_immediate(emitter, "rdi", index as i64);
            abi::emit_call_label(emitter, "__rt_eval_borrow_arg");
            emitter.instruction("test rdi, rdi");                               // did the shared borrow succeed?
            emitter.instruction(&format!("jz {fail_label}"));                   // reject a missing or malformed argument
            emitter.instruction(&format!("mov QWORD PTR [rbp - {result_frame_offset}], rax")); // preserve the borrow for coercion and reference writeback
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Target;

    /// Array guards preserve boxed metadata and reject every tag outside the two array layouts.
    #[test]
    fn php_array_bridge_guards_cover_every_supported_target() {
        let array = PhpType::php_array();
        assert_eq!(bridge_storage_type(&array), array);
        assert_eq!(bridge_storage_type(&PhpType::Int), PhpType::Int);
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_require_php_array(&mut emitter, "invalid_array");
            let asm = emitter.output();
            assert_eq!(asm.matches("__rt_mixed_unbox").count(), 1, "{name}");
            assert!(!asm.contains("incref") && !asm.contains("decref"), "{name}");
            if target.arch == Arch::AArch64 {
                assert!(asm.contains("sub x0, x0, #4"), "{name}");
                assert!(asm.contains("cmp x0, #1"), "{name}");
                assert!(asm.contains("b.hi invalid_array"), "{name}");
            } else {
                assert!(asm.contains("sub rax, 4"), "{name}");
                assert!(asm.contains("cmp rax, 1"), "{name}");
                assert!(asm.contains("ja invalid_array"), "{name}");
            }
        }
    }

    /// All targets borrow normalized strings without allocation and leave other parameter types alone.
    #[test]
    fn native_string_argument_borrows_are_guarded_and_allocation_free() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            assert!(!emit_borrowed_string_arg(&mut emitter, &PhpType::Int, 40, "invalid"));
            assert!(emitter.output().is_empty(), "{name}");
            let mut emitter = Emitter::new(target);
            assert!(emit_borrowed_string_arg(&mut emitter, &PhpType::Str, 40, "invalid"));
            let asm = emitter.output();
            assert!(!asm.contains("__rt_"), "{name}: {asm}");
            if target.arch == Arch::AArch64 {
                assert!(asm.contains("cmp x9, #1"), "{name}");
                assert!(asm.contains("ldr x1, [x0, #8]"), "{name}");
                assert!(asm.contains("ldr x2, [x0, #16]"), "{name}");
            } else {
                assert!(asm.contains("cmp QWORD PTR [rax], 1"), "{name}");
                assert!(asm.contains("mov rdx, QWORD PTR [rax + 16]"), "{name}");
                assert!(asm.contains("mov rax, QWORD PTR [rax + 8]"), "{name}");
            }
        }
    }

    /// Every target retains consumed offsets and values without retaining borrowed array arguments.
    #[test]
    fn intrinsic_consumers_acquire_only_their_declared_boxed_arguments() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for class in ["SplDoublyLinkedList", "SplStack", "SplQueue", "SplFixedArray"] {
                let mut emitter = Emitter::new(Target::parse(name).unwrap());
                let intrinsic = IntrinsicCall::instance_method(class, "offsetSet").unwrap();
                emit_consumed_intrinsic_arguments(&mut emitter, intrinsic, &[PhpType::Mixed, PhpType::Mixed]);
                let offsets = if emitter.target.arch == Arch::AArch64 {
                    ["[sp, #16]", "[sp, #0]"]
                } else { ["[rsp + 16]", "[rsp + 0]"] };
                let asm = emitter.output();
                assert_eq!(asm.matches("__rt_incref").count(), 2, "{name}:{class}");
                assert!(offsets.iter().all(|offset| asm.contains(offset)), "{name}:{class}");
            }
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            let intrinsic = IntrinsicCall::instance_method("SplFixedArray", "__unserialize").unwrap();
            emit_consumed_intrinsic_arguments(&mut emitter, intrinsic, &[PhpType::Array(Box::new(PhpType::Mixed))]);
            assert!(!emitter.output().contains("__rt_incref"), "{name}");
        }
    }

    /// Every supported target checks argument storage and bounds without allocating or retaining.
    #[test]
    fn native_argument_borrows_are_guarded_and_allocation_free() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_borrowed_argument(&mut emitter, 3, 32, 40, "invalid_arguments");
            let asm = emitter.output();
            assert!(!asm.contains("__elephc_eval_value_int"), "{name}");
            assert!(!asm.contains("__elephc_eval_value_array_get"), "{name}");
            assert!(!asm.contains("incref"), "{name}");
            assert!(asm.contains("__rt_eval_borrow_arg"), "{name}");
            if target.arch == Arch::AArch64 {
                assert!(asm.contains("cbz x1, invalid_arguments"), "{name}");
                assert!(asm.contains("str x0, [x29, #-40]"), "{name}");
            } else {
                assert!(asm.contains("jz invalid_arguments"), "{name}");
                assert!(asm.contains("mov QWORD PTR [rbp - 40], rax"), "{name}");
            }
        }
    }
}
