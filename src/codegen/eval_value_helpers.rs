//! Purpose:
//! Shares value ownership decisions for native eval method and constructor bridges.
//!
//! Called from:
//! - Eval method and constructor argument staging and method result boxing.
//!
//! Key details:
//! - String arguments borrow their boxed owner for the duration of the native call.
//! - Return ownership is derived from emitted EIR, including by-reference exclusions.

use super::{abi, emit::Emitter, platform::Arch};
use crate::ir::{Module, Op, Ownership, Terminator, ValueDef};
use crate::types::PhpType;

/// Preserves the PHP array constraint while lowering other bridge metadata to its ABI type.
pub(super) fn bridge_storage_type(ty: &PhpType) -> PhpType {
    if ty.is_php_array() {
        ty.clone()
    } else {
        ty.codegen_repr()
    }
}

/// Rejects non-array boxed inputs without transferring ownership or changing their storage.
/// The borrowed cell enters in the integer result register; callers reload it after this check.
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

/// Borrows a normalized argument cell from the private indexed array retained by the Rust caller.
/// The caller must validate arity first and keep that array alive through native dispatch.
pub(super) fn emit_borrowed_eval_argument(
    emitter: &mut Emitter,
    index: usize,
    array_frame_offset: usize,
    value_frame_offset: usize,
) {
    let offset = 24 + index * 8;
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x0, [x29, #-{array_frame_offset}]")); // load the private boxed argument array retained by the caller
            emitter.instruction("ldr x0, [x0, #8]");                            // expose the normalized indexed Mixed array payload
            emitter.instruction(&format!("ldr x0, [x0, #{offset}]"));           // borrow the validated argument without allocating an index or value owner
            emitter.instruction(&format!("str x0, [x29, #-{value_frame_offset}]")); // retain the borrowed cell while materializing its parameter ABI
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {array_frame_offset}]")); // load the caller-owned boxed argument array
            emitter.instruction("mov rax, QWORD PTR [rax + 8]");                // expose the normalized indexed Mixed array payload
            emitter.instruction(&format!("mov rax, QWORD PTR [rax + {offset}]")); // borrow the validated argument while its containing array remains alive
            emitter.instruction(&format!("mov QWORD PTR [rbp - {value_frame_offset}], rax")); // retain the borrowed cell for parameter coercion and reference staging
        }
    }
}

/// Borrows boxed string bytes or formats a non-string scalar into the existing concat scratch.
pub(super) fn emit_borrowed_eval_string_argument(emitter: &mut Emitter, label_prefix: &str) {
    let borrowed = format!("{label_prefix}_borrowed_string_argument");
    let done = format!("{label_prefix}_string_argument_done");
    let result = abi::int_result_reg(emitter);
    abi::emit_push_reg(emitter, result);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #1");                                  // inspect the dereferenced Mixed tag
            emitter.instruction(&format!("b.eq {borrowed}"));                   // borrow bytes from the argument array's string owner
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 1");                                  // inspect the dereferenced Mixed tag
            emitter.instruction(&format!("je {borrowed}"));                     // borrow bytes from the argument array's string owner
        }
    }
    abi::emit_pop_reg(emitter, result);
    abi::emit_call_label(emitter, "__rt_mixed_cast_string");
    abi::emit_jump(emitter, &done);
    emitter.label(&borrowed);
    abi::emit_release_temporary_stack(emitter, 16);
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rax, rdi");                                    // return the borrowed pointer while rdx retains the byte length
    }
    emitter.label(&done);
}

/// Proves that every normal method return transfers an owner that boxing can consume.
pub(super) fn native_method_returns_owned_value(
    module: &Module,
    class: &str,
    method: &str,
    is_static: bool,
) -> bool {
    let Some(function) = module.class_methods.iter().find(|function| {
        function.flags.is_static == is_static
            && function.name.rsplit_once("::").is_some_and(|(owner, name)| {
                owner == class && name.eq_ignore_ascii_case(method)
            })
    }) else { return false; };
    if function.flags.by_ref_return { return false; }
    match function.return_php_type.codegen_repr() {
        PhpType::Object(_) => true,
        PhpType::Str => {
            let mut returned = false;
            for block in &function.blocks {
                if let Some(Terminator::Return { value: Some(value) }) = &block.terminator {
                    returned = true;
                    if !function.value(*value).is_some_and(|value| {
                        value.ownership == Ownership::Owned
                            || matches!(value.def, ValueDef::Instruction { inst, .. }
                                if function.instruction(inst).is_some_and(|inst| {
                                    matches!(inst.op, Op::Acquire | Op::StrPersist)
                                }))
                    }) {
                        return false;
                    }
                }
            }
            returned
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use crate::codegen::emit::Emitter;
    use crate::codegen::platform::{AppleVariant, Arch, Platform, Target};
    use crate::types::PhpType;

    use super::{bridge_storage_type, emit_require_php_array};

    /// Preserves only the exact PHP array property contract as boxed bridge storage.
    #[test]
    fn bridge_storage_preserves_only_php_array_contract() {
        let php_array = PhpType::php_array();
        assert_eq!(bridge_storage_type(&php_array), php_array);
        assert_eq!(
            bridge_storage_type(&PhpType::Union(vec![PhpType::Int, PhpType::Str])),
            PhpType::Mixed,
        );
    }

    /// Rejects every boxed tag outside the packed-or-hash array range on all targets.
    #[test]
    fn php_array_requirement_checks_both_array_tags_without_ownership_calls() {
        for target in supported_targets() {
            let mut emitter = Emitter::new(target);
            emit_require_php_array(&mut emitter, "php_array_type_error");
            let asm = emitter.output();
            assert!(asm.contains("__rt_mixed_unbox"), "{target:?}: {asm}");
            assert!(asm.contains("php_array_type_error"), "{target:?}: {asm}");
            assert!(!asm.contains("__rt_incref"), "{target:?}: {asm}");
            assert!(!asm.contains("__rt_decref"), "{target:?}: {asm}");
            match target.arch {
                Arch::AArch64 => {
                    assert!(asm.contains("sub x0, x0, #4"), "{target:?}: {asm}");
                    assert!(asm.contains("cmp x0, #1"), "{target:?}: {asm}");
                }
                Arch::X86_64 => {
                    assert!(asm.contains("sub rax, 4"), "{target:?}: {asm}");
                    assert!(asm.contains("cmp rax, 1"), "{target:?}: {asm}");
                }
            }
        }
    }

    /// Returns every supported target for bridge ABI assertions.
    fn supported_targets() -> [Target; 5] {
        [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new_apple(Arch::AArch64, AppleVariant::IOS),
            Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ]
    }
}
