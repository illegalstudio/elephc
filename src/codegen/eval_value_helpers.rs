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
