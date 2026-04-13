use crate::codegen::abi;
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
            abi::emit_call_label(emitter, "__rt_itoa");                         // convert the integer payload into a JSON decimal string for the active target ABI
        }
        PhpType::Float => {
            // -- convert float to JSON (ftoa) --
            abi::emit_call_label(emitter, "__rt_ftoa");                         // convert the float payload into a JSON decimal string for the active target ABI
        }
        PhpType::Bool => {
            // -- convert bool to JSON "true"/"false" --
            abi::emit_call_label(emitter, "__rt_json_encode_bool");             // convert the bool payload into the JSON literals true/false for the active target ABI
        }
        PhpType::Str => {
            // -- wrap string with JSON quotes and escape special chars --
            abi::emit_call_label(emitter, "__rt_json_encode_str");              // escape and quote the string payload into JSON using the active target ABI
        }
        PhpType::Void => {
            // -- null → "null" --
            abi::emit_call_label(emitter, "__rt_json_encode_null");             // produce the JSON null literal using the active target ABI
        }
        PhpType::Array(ref elem_ty) => {
            match elem_ty.as_ref() {
                PhpType::Int => {
                    // x0 = array pointer
                    abi::emit_call_label(emitter, "__rt_json_encode_array_int"); // encode an integer array to JSON using the active target ABI
                }
                PhpType::Str => {
                    // x0 = array pointer
                    abi::emit_call_label(emitter, "__rt_json_encode_array_str"); // encode a string array to JSON using the active target ABI
                }
                _ => {
                    // Fallback: inspect the packed runtime value_type tag per array
                    abi::emit_call_label(emitter, "__rt_json_encode_array_dynamic"); // encode the array to JSON by inspecting its runtime value_type tag
                }
            }
        }
        PhpType::AssocArray { .. } => {
            // x0 = hash table pointer
            abi::emit_call_label(emitter, "__rt_json_encode_assoc");            // encode the associative array to JSON using the active target ABI
        }
        PhpType::Mixed => {
            // x0 = boxed mixed pointer
            abi::emit_call_label(emitter, "__rt_json_encode_mixed");            // inspect the boxed payload and encode it as JSON for the active target ABI
        }
        _ => {
            // Fallback: encode as "null"
            abi::emit_call_label(emitter, "__rt_json_encode_null");             // produce the JSON null literal for unsupported payloads
        }
    }

    Some(PhpType::Str)
}
