//! Purpose:
//! Balances temporary boxed indices and fetched scalar arguments in native eval bridges.
//!
//! Called from:
//! - Eval method and constructor argument preparation on both architectures.
//!
//! Key details:
//! - Converted arguments must be staged before releasing fetched cells.
//! - Reference and potentially aliasing Mixed-return paths retain their existing policy.

use crate::codegen::{abi, emit::Emitter, platform::Arch};
use crate::types::PhpType;

/// Borrows by-value string argument bytes while the enclosing argument array keeps them alive.
pub(super) fn borrow_string_argument(emitter: &mut Emitter, parameter: &PhpType) -> bool {
    if parameter.codegen_repr() != PhpType::Str { return false; }
    abi::emit_reserve_temporary_stack(emitter, 16);
    load_cached_cell(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x1, sp");                                  // receive the borrowed argument byte pointer
            emitter.instruction("add x2, sp, #8");                              // receive the borrowed argument byte length
            emitter.bl_c("__elephc_eval_value_string_bytes");
            emitter.instruction("ldp x1, x2, [sp]");                            // restore the native string argument pair without allocating a copy
        }
        Arch::X86_64 => {
            emitter.instruction("mov rsi, rsp");                                // receive the borrowed argument byte pointer
            emitter.instruction("lea rdx, [rsp + 8]");                          // receive the borrowed argument byte length
            emitter.bl_c("__elephc_eval_value_string_bytes");
            emitter.instruction("mov rax, QWORD PTR [rsp]");                    // restore the borrowed native string pointer
            emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                // restore the native string byte length
        }
    }
    abi::emit_release_temporary_stack(emitter, 16);
    true
}

/// Releases the cached boxed index while preserving the fetched argument result.
pub(super) fn release_argument_index(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    abi::emit_push_reg(emitter, result);
    load_cached_cell(emitter);
    emitter.bl_c("__elephc_eval_value_release");
    abi::emit_pop_reg(emitter, result);
}

/// Releases a fetched by-value cell when the result cannot reuse its box.
/// The bridge argument array retains boxed inputs throughout the native invocation.
pub(super) fn release_staged_scalar_box(
    emitter: &mut Emitter,
    parameter: &PhpType,
    returned: &PhpType,
) {
    let scalar_parameter = matches!(parameter.codegen_repr(),
        PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::Str | PhpType::TaggedScalar
            | PhpType::Mixed | PhpType::Object(_));
    let fresh_result_box = matches!(returned.codegen_repr(),
        PhpType::Void | PhpType::Int | PhpType::Bool | PhpType::Float | PhpType::Str
            | PhpType::TaggedScalar | PhpType::Object(_) | PhpType::Callable);
    if scalar_parameter && fresh_result_box {
        load_cached_cell(emitter);
        emitter.bl_c("__elephc_eval_value_release");
    }
}

/// Loads the shared method/constructor cached-cell slot into the first C argument register.
fn load_cached_cell(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x0, [x29, #-16]");                         // release the cached index or fetched boxed argument
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");               // release the cached index or fetched boxed argument
        }
    }
}

#[cfg(test)]
mod tests {
    /// The internal pointer-read alias copies bytes with the public helper's ownership contract.
    #[test]
    fn internal_pointer_string_alias_preserves_owned_result_contract() {
        assert_eq!(
            crate::ir::RuntimeFnId::ElephcPtrReadString.result_ownership(),
            crate::ir::RuntimeFnId::PtrReadString.result_ownership(),
        );
    }

    use super::*;
    use crate::codegen::platform::{Platform, Target};

    /// By-value string staging uses the borrowed byte-view API without a detached cast allocation.
    #[test]
    fn string_argument_staging_uses_borrowed_bytes_on_both_targets() {
        for target in [Target::new(Platform::MacOS, Arch::AArch64), Target::new(Platform::Linux, Arch::X86_64)] {
            let mut emitter = Emitter::new(target);
            assert!(borrow_string_argument(&mut emitter, &PhpType::Str));
            let asm = emitter.output();
            assert!(asm.contains("__elephc_eval_value_string_bytes"), "{asm}");
            assert!(!asm.contains("__rt_mixed_cast_string"), "{asm}");
            let mut emitter = Emitter::new(target);
            assert!(!borrow_string_argument(&mut emitter, &PhpType::Int));
            assert!(emitter.output().is_empty());
        }
    }

    /// Scalar staging releases fetched boxes only when the native result has independent box storage.
    #[test]
    fn scalar_staging_preserves_potential_mixed_return_aliases() {
        for target in [Target::new(Platform::MacOS, Arch::AArch64), Target::new(Platform::Linux, Arch::X86_64)] {
            let mut emitter = Emitter::new(target);
            release_staged_scalar_box(&mut emitter, &PhpType::Str, &PhpType::Mixed);
            assert!(emitter.output().is_empty());
            let mut emitter = Emitter::new(target);
            release_staged_scalar_box(&mut emitter, &PhpType::Str, &PhpType::Str);
            assert!(emitter.output().contains("__elephc_eval_value_release"));
            let mut emitter = Emitter::new(target);
            release_staged_scalar_box(&mut emitter, &PhpType::Mixed, &PhpType::Mixed);
            assert!(emitter.output().is_empty());
            let mut emitter = Emitter::new(target);
            release_staged_scalar_box(&mut emitter, &PhpType::Mixed, &PhpType::Object("DateTime".into()));
            assert!(emitter.output().contains("__elephc_eval_value_release"));
        }
    }

    /// Index cleanup preserves the fetched boxed result across the C release call on both ABIs.
    #[test]
    fn index_cleanup_preserves_fetched_argument_on_both_targets() {
        for target in [Target::new(Platform::MacOS, Arch::AArch64), Target::new(Platform::Linux, Arch::X86_64)] {
            let mut emitter = Emitter::new(target);
            release_argument_index(&mut emitter);
            let asm = emitter.output();
            assert!(asm.contains("__elephc_eval_value_release"), "{asm}");
            if target.arch == Arch::AArch64 {
                assert!(asm.contains("[x29, #-16]"), "{asm}");
            } else {
                assert!(asm.contains("[rbp - 40]"), "{asm}");
            }
        }
    }
}
