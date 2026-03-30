use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("json_encode()");

    let ty = emit_expr(&args[0], emitter, ctx, data);

    match ty {
        PhpType::Int => {
            // -- convert integer to JSON (just itoa) --
            emitter.instruction("bl __rt_itoa");                                // convert int x0 → string x1/x2
        }
        PhpType::Float => {
            // -- convert float to JSON (ftoa) --
            emitter.instruction("bl __rt_ftoa");                                // convert float d0 → string x1/x2
        }
        PhpType::Bool => {
            // -- convert bool to JSON "true"/"false" --
            emitter.instruction("bl __rt_json_encode_bool");                    // convert bool x0 → string x1/x2
        }
        PhpType::Str => {
            // -- wrap string with JSON quotes and escape special chars --
            emitter.instruction("bl __rt_json_encode_str");                     // escape and quote string x1/x2 → x1/x2
        }
        PhpType::Void => {
            // -- null → "null" --
            emitter.instruction("bl __rt_json_encode_null");                    // produce "null" → x1/x2
        }
        PhpType::Array(ref elem_ty) => {
            match elem_ty.as_ref() {
                PhpType::Int => {
                    // x0 = array pointer
                    emitter.instruction("bl __rt_json_encode_array_int");       // encode int array → x1/x2
                }
                PhpType::Str => {
                    // x0 = array pointer
                    emitter.instruction("bl __rt_json_encode_array_str");       // encode string array → x1/x2
                }
                _ => {
                    // Fallback: inspect the packed runtime value_type tag per array
                    emitter.instruction("bl __rt_json_encode_array_dynamic");   // encode array → x1/x2
                }
            }
        }
        PhpType::AssocArray { .. } => {
            // x0 = hash table pointer
            emitter.instruction("bl __rt_json_encode_assoc");                   // encode assoc array → x1/x2
        }
        PhpType::Mixed => {
            // x0 = boxed mixed pointer
            emitter.instruction("bl __rt_json_encode_mixed");                   // inspect boxed payload and encode it as JSON
        }
        _ => {
            // Fallback: encode as "null"
            emitter.instruction("bl __rt_json_encode_null");                    // produce "null" → x1/x2
        }
    }

    Some(PhpType::Str)
}
