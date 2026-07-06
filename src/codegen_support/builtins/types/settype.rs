//! Purpose:
//! Emits PHP `settype` type conversion or type-name builtin calls.
//! Applies PHP scalar conversion rules or materializes runtime type names for values.
//!
//! Called from:
//! - `crate::codegen_support::builtins::types::emit()`.
//!
//! Key details:
//! - Conversion results must stay aligned with type-checker signatures and boxed Mixed handling.

use crate::codegen_support::abi;
use crate::codegen_support::context::{Context, HeapOwnership};
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for PHP's `settype($var, $type)` builtin.
///
/// Converts the variable named in `args[0]` to the type specified by the string literal in
/// `args[1]`. Supports `"int"`/`"integer"`, `"float"`/`"double"`, `"string"`, and `"bool"`/`"boolean"`.
/// Updates the variable's type in the context and always returns `true` (bool).
///
/// # Arguments
/// - `args[0]` must be a `Variable` expression naming the target variable.
/// - `args[1]` must be a `StringLiteral` giving the target type name.
/// - `emitter` drives assembly emission with target-aware ABI helpers.
/// - `ctx` provides variable layout (stack offset) and is updated with the new type.
/// - `_data` is used for string coercion runtime calls.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    _data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("settype()");
    if let crate::parser::ast::ExprKind::Variable(vname) = &args[0].kind {
        if let crate::parser::ast::ExprKind::StringLiteral(type_name) = &args[1].kind {
            let var = ctx.variables.get(vname).expect("undefined variable");
            let offset = var.stack_offset;
            let old_ty = var.ty.clone();
            crate::codegen_support::abi::emit_load(emitter, &old_ty, offset);
            let new_ty = match type_name.as_str() {
                "int" | "integer" => {
                    // -- convert value to integer --
                    match &old_ty {
                        PhpType::Float => {
                            abi::emit_float_result_to_int_result(emitter);      // truncate the floating-point source value into the active integer result register for the current target ABI
                        }
                        PhpType::Bool | PhpType::Int => {}
                        _ => {
                            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0); // coerce unsupported settype(..., \"integer\") sources to zero in the active integer result register
                        }
                    }
                    PhpType::Int
                }
                "float" | "double" => {
                    // -- convert value to float --
                    match &old_ty {
                        PhpType::Float => {}
                        _ => {
                            abi::emit_int_result_to_float_result(emitter);      // convert the scalar settype(..., \"float\") source into the active floating-point result register
                        }
                    }
                    PhpType::Float
                }
                "string" => {
                    crate::codegen_support::expr::coerce_to_string(emitter, ctx, _data, &old_ty);
                    PhpType::Str
                }
                "bool" | "boolean" => {
                    // -- convert value to boolean --
                    crate::codegen_support::expr::coerce_null_to_zero(emitter, &old_ty);
                    match emitter.target.arch {
                        Arch::X86_64 => {
                            emitter.instruction("cmp rax, 0");                  // compare the coerced scalar source against zero before normalizing it into a boolean on x86_64
                            emitter.instruction("setne al");                    // set the low byte when the coerced scalar source is truthy on x86_64
                            emitter.instruction("movzx eax, al");               // widen the normalized boolean result back into the full x86_64 integer result register
                        }
                        Arch::AArch64 => {
                            emitter.instruction("cmp x0, #0");                  // compare the coerced scalar source against zero before normalizing it into a boolean on AArch64
                            emitter.instruction("cset x0, ne");                 // set the integer result register to 1 when the coerced scalar source is truthy on AArch64
                        }
                    }
                    PhpType::Bool
                }
                _ => old_ty.clone(),
            };
            crate::codegen_support::abi::emit_store(emitter, &new_ty, offset);
            ctx.update_var_type_and_ownership(
                vname,
                new_ty.clone(),
                HeapOwnership::local_owner_for_type(&new_ty),
            );
        }
    }
    // -- settype() always returns true --
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 1);    // return true in the active target integer result register because settype() reports success
    Some(PhpType::Bool)
}
