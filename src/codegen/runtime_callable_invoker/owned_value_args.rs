//! Purpose:
//! Acquires stable string buffers for descriptor invoker by-value arguments.
//!
//! Called from:
//! - The indexed, associative, and default argument builders in the parent module.
//!
//! Key details:
//! - Every resulting string has one owner recorded by `InvokerArgumentOwners`.
//! - Boxed strings bypass the allocating cast so persistence happens exactly once.
//! - Other scalar casts are persisted before later arguments can overwrite shared scratch.

use super::{abi, Arch, DataSection, Emitter, InvokerEmitContext, PhpType};

/// Coerces a borrowed argument and detaches any resulting string for invocation cleanup.
pub(super) fn coerce(
    emitter: &mut Emitter,
    ctx: &mut InvokerEmitContext,
    data: &mut DataSection,
    source_ty: &PhpType,
    target_ty: Option<&PhpType>,
) -> (PhpType, bool) {
    if source_ty.codegen_repr() == PhpType::Mixed
        && target_ty.is_some_and(|ty| ty.codegen_repr() == PhpType::Str)
    {
        let string_label = ctx.next_label("owned_string_argument");
        let persist_label = ctx.next_label("persist_string_argument");
        emit_owned_mixed_string(emitter, &string_label, &persist_label);
        return (PhpType::Str, false);
    }
    let coerced = super::coerce_current_value_to_target(emitter, ctx, data, source_ty, target_ty);
    if coerced.0 == PhpType::Str {
        abi::emit_call_label(emitter, "__rt_str_persist");
    }
    coerced
}

/// Borrows boxed string bytes or casts another tag, then acquires one independent string owner.
fn emit_owned_mixed_string(emitter: &mut Emitter, string_label: &str, persist_label: &str) {
    let result = abi::int_result_reg(emitter);
    abi::emit_push_reg(emitter, result);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #1");                                  // a string payload can be borrowed without the allocating cast
            emitter.instruction(&format!("b.eq {string_label}"));               // keep the unboxed string pointer and length for one persistence call
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 1");                                  // a string payload can be borrowed without the allocating cast
            emitter.instruction(&format!("je {string_label}"));                 // skip the allocating string cast for an existing string
        }
    }
    abi::emit_load_temporary_stack_slot(emitter, result, 0);
    abi::emit_call_label(emitter, "__rt_mixed_cast_string");
    abi::emit_jump(emitter, persist_label);
    emitter.label(string_label);
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rax, rdi");                                    // expose the borrowed string payload in the native result pair
    }
    emitter.label(persist_label);
    abi::emit_call_label(emitter, "__rt_str_persist");
    abi::emit_pop_reg(emitter, abi::secondary_scratch_reg(emitter));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Target;

    /// Every supported target uses one persistence point for strings and converted scalar arguments.
    #[test]
    fn mixed_string_arguments_have_one_owned_persistence_point_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_owned_mixed_string(&mut emitter, "string_argument", "persist_argument");
            let asm = emitter.output();
            assert_eq!(asm.matches("__rt_str_persist").count(), 1, "{name}: {asm}");
            assert_eq!(asm.matches("__rt_mixed_cast_string").count(), 1, "{name}: {asm}");
            assert!(asm.find("__rt_mixed_cast_string").unwrap() < asm.find("string_argument:").unwrap(), "{name}: {asm}");
            assert!(asm.find("persist_argument:").unwrap() < asm.find("__rt_str_persist").unwrap(), "{name}: {asm}");
        }
    }
}
