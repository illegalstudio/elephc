//! Purpose:
//! Emits PHP `array_fill_keys` builtin calls over associative or key-aware array data.
//! Owns key/value payload setup and runtime hash-helper invocation for array results or lookups.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Array key typing and Mixed payload tags must match the runtime hash-table representation.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::{array_key_type_from_value_type, PhpType};
use super::hash_value_type_tag::hash_value_type_tag;

/// Emits the `array_fill_keys($keys, $value)` builtin call.
///
/// Dispatches to the x86_64 Linux implementation or uses ARM64 conventions.
/// Pushes the keys array onto the stack, evaluates the fill value into `x0`,
/// calls `__rt_array_fill_keys` (or `_refcounted` variant), and returns an
/// `AssocArray` type with the inferred key element type and value type.
///
/// # Arguments
/// * `_name` - Unused; present for dispatcher uniformity.
/// * `args` - Two expressions: `$keys` (array of keys) and `$value` (fill value).
/// * `emitter` - Target-specific assembly emitter.
/// * `ctx` - Codegen context (variable layout, ownership state).
/// * `data` - Data section for literals and runtime metadata.
///
/// # Returns
/// `Some(PhpType::AssocArray { key, value })` describing the result array.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("array_fill_keys()");
    if emitter.target.arch == Arch::X86_64 {
        return emit_array_fill_keys_linux_x86_64(args, emitter, ctx, data);
    }

    let keys_ty = emit_expr(&args[0], emitter, ctx, data);
    // -- save keys array, evaluate fill value --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push keys array pointer onto stack
    let mut value_ty = emit_expr(&args[1], emitter, ctx, data);
    crate::codegen::emit_box_iterable_value_for_mixed_container(emitter, &mut value_ty);
    let key_elem_ty = match &keys_ty {
        PhpType::Array(key) => (**key).clone(),
        _ => PhpType::Str,
    };
    let uses_refcounted_runtime = value_ty.is_refcounted();
    let value_type_tag = hash_value_type_tag(&value_ty);
    // -- call runtime to create assoc array from keys with given value --
    emitter.instruction(&format!("mov x2, #{}", value_type_tag));               // x2 = result hash value_type tag
    emitter.instruction("mov x1, x0");                                          // move fill value to x1 (second arg)
    emitter.instruction("ldr x0, [sp], #16");                                   // pop keys array pointer into x0 (first arg)
    let runtime_call = if uses_refcounted_runtime {
        "bl __rt_array_fill_keys_refcounted"
    } else {
        "bl __rt_array_fill_keys"
    };
    emitter.instruction(runtime_call);                                          // call runtime: fill keys → x0=new assoc array

    Some(PhpType::AssocArray {
        key: Box::new(array_key_type_from_value_type(key_elem_ty)),
        value: Box::new(value_ty),
    })
}

/// x86_64 Linux implementation of `array_fill_keys` using System V AMD64 ABI.
///
/// Preserves the keys array in `rax` while evaluating the fill value expression,
/// then arranges arguments per AMD64 calling convention (rdi=keys, rsi=value, rdx=type_tag)
/// before calling the appropriate runtime helper.
///
/// # Arguments
/// * `args` - Two expressions: `$keys` (indexed array) and `$value` (fill scalar).
/// * `emitter` - x86_64 assembly emitter.
/// * `ctx` - Codegen context.
/// * `data` - Data section.
///
/// # Returns
/// `Some(PhpType::AssocArray { key, value })` describing the result array.
fn emit_array_fill_keys_linux_x86_64(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let keys_ty = emit_expr(&args[0], emitter, ctx, data);
    abi::emit_push_reg(emitter, "rax");                                         // preserve the indexed array of keys while evaluating the fill payload expression
    let mut value_ty = emit_expr(&args[1], emitter, ctx, data);
    crate::codegen::emit_box_iterable_value_for_mixed_container(emitter, &mut value_ty);
    let key_elem_ty = match &keys_ty {
        PhpType::Array(key) => (**key).clone(),
        _ => PhpType::Str,
    };
    let uses_refcounted_runtime = value_ty.is_refcounted();
    let value_type_tag = hash_value_type_tag(&value_ty);
    if matches!(value_ty, PhpType::Float) {
        emitter.instruction("movq rsi, xmm0");                                  // move the floating-point fill payload bits into the second x86_64 runtime argument register
    } else {
        emitter.instruction("mov rsi, rax");                                    // place the fill payload in the second x86_64 runtime argument register
    }
    abi::emit_pop_reg(emitter, "rdi");                                          // restore the indexed array of keys into the first x86_64 runtime argument register
    abi::emit_load_int_immediate(emitter, "rdx", value_type_tag.into());
    if uses_refcounted_runtime {
        abi::emit_call_label(emitter, "__rt_array_fill_keys_refcounted");       // build an associative array by retaining the shared heap payload for every requested key
    } else {
        abi::emit_call_label(emitter, "__rt_array_fill_keys");                  // build an associative array by reusing the scalar payload for every requested key
    }

    Some(PhpType::AssocArray {
        key: Box::new(array_key_type_from_value_type(key_elem_ty)),
        value: Box::new(value_ty),
    })
}
