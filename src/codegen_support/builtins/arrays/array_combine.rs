//! Purpose:
//! Emits PHP `array_combine` builtin calls over associative or key-aware array data.
//! Owns key/value payload setup and runtime hash-helper invocation for array results or lookups.
//!
//! Called from:
//! - `crate::codegen_support::builtins::arrays::emit()`.
//!
//! Key details:
//! - Array key typing and Mixed payload tags must match the runtime hash-table representation.

use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::{array_key_type_from_value_type, PhpType};
use super::hash_value_type_tag::hash_value_type_tag;

/// Emits the `array_combine(keys, values)` builtin call.
///
/// Keys are emitted first and preserved (x86_64: pushed to stack; ARM64: stored to `[sp]`).
/// Values are then emitted into `x0`/`rax`, and the appropriate runtime helper is called:
/// - `__rt_array_combine` for non-refcounted value types (int, float)
/// - `__rt_array_combine_refcounted` for refcounted value types (string, array, object)
/// On ARM64 keys are passed in `x0`, values in `x1`, and the type tag in `x2`.
/// On x86_64 keys are passed in `rdi`, values in `rsi`, and the type tag in `rdx`.
///
/// Returns `PhpType::AssocArray` with the combined key/value element types.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_combine()");
    let keys_ty = emit_expr(&args[0], emitter, ctx, data);
    if emitter.target.arch == Arch::X86_64 {
        abi::emit_push_reg(emitter, "rax");                                     // preserve the indexed array of keys while evaluating the indexed array of values expression
        let values_ty = emit_expr(&args[1], emitter, ctx, data);
        let (key_elem_ty, value_elem_ty) = match (&keys_ty, &values_ty) {
            (PhpType::Array(key), PhpType::Array(value)) => ((**key).clone(), (**value).clone()),
            _ => (PhpType::Str, PhpType::Int),
        };
        let uses_refcounted_runtime = value_elem_ty.is_refcounted();
        let value_type_tag = hash_value_type_tag(&value_elem_ty);
        if !uses_refcounted_runtime {
            abi::emit_load_int_immediate(emitter, "rdx", value_type_tag.into());
            emitter.instruction("mov rsi, rax");                                // place the indexed array of values in the second x86_64 runtime argument register
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the indexed array of keys into the first x86_64 runtime argument register
            abi::emit_call_label(emitter, "__rt_array_combine");                // build the scalar associative array through the x86_64 runtime helper
        } else {
            emitter.instruction("mov rcx, rax");                                // preserve the indexed array of values while materializing the result hash value_type tag for the refcounted helper path
            abi::emit_load_int_immediate(emitter, "rdx", value_type_tag.into());
            emitter.instruction("mov rsi, rcx");                                // place the indexed array of values in the second x86_64 runtime argument register for the refcounted helper path
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the indexed array of keys into the first x86_64 runtime argument register for the refcounted helper path
            abi::emit_call_label(emitter, "__rt_array_combine_refcounted");     // build the refcounted associative array through the dedicated x86_64 runtime helper
        }

        return Some(PhpType::AssocArray {
            key: Box::new(array_key_type_from_value_type(key_elem_ty)),
            value: Box::new(value_elem_ty),
        });
    }

    // -- save keys array, evaluate values array --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push keys array pointer onto stack
    let values_ty = emit_expr(&args[1], emitter, ctx, data);
    let (key_elem_ty, value_elem_ty) = match (&keys_ty, &values_ty) {
        (PhpType::Array(key), PhpType::Array(value)) => ((**key).clone(), (**value).clone()),
        _ => (PhpType::Str, PhpType::Int),
    };
    let uses_refcounted_runtime = value_elem_ty.is_refcounted();
    let value_type_tag = hash_value_type_tag(&value_elem_ty);
    // -- call runtime to combine keys and values into assoc array --
    emitter.instruction(&format!("mov x2, #{}", value_type_tag));               // x2 = result hash value_type tag
    emitter.instruction("mov x1, x0");                                          // move values array pointer to x1
    emitter.instruction("ldr x0, [sp], #16");                                   // pop keys array pointer into x0
    let runtime_call = if uses_refcounted_runtime {
        "bl __rt_array_combine_refcounted"
    } else {
        "bl __rt_array_combine"
    };
    emitter.instruction(runtime_call);                                          // call runtime: combine → x0=new assoc array

    Some(PhpType::AssocArray {
        key: Box::new(array_key_type_from_value_type(key_elem_ty)),
        value: Box::new(value_elem_ty),
    })
}
