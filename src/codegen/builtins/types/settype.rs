//! Purpose:
//! Emits PHP `settype` type conversion or type-name builtin calls.
//! Applies PHP scalar conversion rules or materializes runtime type names for values.
//!
//! Called from:
//! - `crate::codegen::builtins::types::emit()`.
//!
//! Key details:
//! - Conversion results must stay aligned with type-checker signatures and boxed Mixed handling.

use crate::codegen::abi;
use crate::codegen::context::{Context, HeapOwnership};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for PHP's `settype($var, $type)` builtin.
///
/// Converts the variable named in `args[0]` to the type specified by the string literal in
/// `args[1]`. Supports `"int"`/`"integer"`, `"float"`/`"double"`, `"string"`, and `"bool"`/`"boolean"`,
/// coercing string and boxed `Mixed`/`Union` sources per PHP cast rules (e.g. `"3.14"` → 3.14,
/// `"0"` → false) rather than zeroing them. `"array"`/`"null"` and a non-literal type name are not
/// yet supported (the variable is left unchanged). Updates the variable's type and returns `true`.
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
            crate::codegen::abi::emit_load(emitter, &old_ty, offset);
            let new_ty = match type_name.as_str() {
                "int" | "integer" => {
                    // -- convert value to integer --
                    match &old_ty {
                        PhpType::Float => {
                            abi::emit_float_result_to_int_result(emitter);      // truncate the floating-point source value into the active integer result register for the current target ABI
                        }
                        PhpType::Bool | PhpType::Int => {}
                        PhpType::Str => {
                            abi::emit_call_label(emitter, "__rt_str_to_int");   // parse a string source with PHP string-to-int cast rules
                        }
                        PhpType::Mixed | PhpType::Union(_) => {
                            abi::emit_call_label(emitter, "__rt_mixed_cast_int"); // unbox a boxed source and coerce it to int per PHP casting
                        }
                        _ => {
                            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0); // coerce unsupported settype(..., \"integer\") sources to zero in the active integer result register
                        }
                    }
                    PhpType::Int
                }
                "float" | "double" => {
                    // -- convert value to float --
                    match &old_ty {
                        PhpType::Str => {
                            abi::emit_call_label(emitter, "__rt_str_to_number"); // parse a string source to a double via strtod (numeric flag ignored)
                        }
                        // Float is a no-op; Mixed/Union unbox via __rt_mixed_cast_float; int/bool convert.
                        _ => crate::codegen::expr::coerce_to_float(emitter, &old_ty),
                    }
                    PhpType::Float
                }
                "string" => {
                    crate::codegen::expr::coerce_to_string(emitter, ctx, _data, &old_ty);
                    PhpType::Str
                }
                "bool" | "boolean" => {
                    // -- convert value to boolean (PHP truthiness: handles strings "0"/"", floats, and boxed Mixed) --
                    crate::codegen::expr::coerce_to_truthiness(emitter, ctx, &old_ty);
                    PhpType::Bool
                }
                _ => old_ty.clone(),
            };
            crate::codegen::abi::emit_store(emitter, &new_ty, offset);
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
