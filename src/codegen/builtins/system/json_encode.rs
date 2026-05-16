//! Purpose:
//! Emits PHP `json_encode` JSON builtin calls.
//! Marshals PHP scalar, array, and Mixed values into runtime JSON helpers and error state.
//!
//! Called from:
//! - `crate::codegen::builtins::system::emit()`.
//!
//! Key details:
//! - JSON error state is runtime-global observable state and must stay coupled to json_last_error().

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
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
    persist_string_result_if_needed(&ty, emitter);
    abi::emit_push_result_value(emitter, &ty);

    if let Some(flag_expr) = args.get(1) {
        emit_expr(flag_expr, emitter, ctx, data);
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // keep json_encode flags stable while later arguments evaluate
    }
    if let Some(depth_expr) = args.get(2) {
        emit_expr(depth_expr, emitter, ctx, data);
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // keep json_encode depth stable until all argument side effects are done
    }

    // PHP evaluates every argument before the builtin mutates global JSON
    // error/configuration state.
    abi::emit_store_zero_to_symbol(emitter, "_json_last_error", 0);
    abi::emit_store_zero_to_symbol(emitter, "_json_active_depth", 0);
    abi::emit_store_zero_to_symbol(emitter, "_json_indent_depth", 0);

    if args.get(2).is_some() {
        abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
        abi::emit_store_reg_to_symbol(
            emitter,
            abi::int_result_reg(emitter),
            "_json_depth_limit",
            0,
        );
    } else {
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 512);
        abi::emit_store_reg_to_symbol(
            emitter,
            abi::int_result_reg(emitter),
            "_json_depth_limit",
            0,
        );
    }
    if args.get(1).is_some() {
        abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
        abi::emit_store_reg_to_symbol(
            emitter,
            abi::int_result_reg(emitter),
            "_json_active_flags",
            0,
        );
    } else {
        abi::emit_store_zero_to_symbol(emitter, "_json_active_flags", 0);
    }

    restore_result_value(emitter, &ty);

    match ty {
        PhpType::Int => {
            // -- convert integer to JSON (just itoa) --
            abi::emit_call_label(emitter, "__rt_itoa");                         // convert the integer payload into a JSON decimal string for the active target ABI
        }
        PhpType::Float => {
            // -- convert float to JSON, rejecting Inf/NaN --
            abi::emit_call_label(emitter, "__rt_json_encode_float");            // detect Inf/NaN, set JSON_ERROR_INF_OR_NAN, throw if requested, then encode
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
        PhpType::Object(class_name) => {
            if crate::types::checker::builtin_stdclass::is_stdclass(&class_name) {
                // stdClass has no static descriptor; encode the dynamic
                // property hash through the assoc-array encoder.
                match emitter.target.arch {
                    crate::codegen::platform::Arch::AArch64 => {
                        emitter.instruction("ldr x0, [x0, #8]");                // load the dynamic-property hash from obj+8
                    }
                    crate::codegen::platform::Arch::X86_64 => {
                        emitter.instruction("mov rax, QWORD PTR [rax + 8]");    // load the dynamic-property hash from obj+8
                    }
                }
                abi::emit_call_label(emitter, "__rt_json_encode_stdclass");     // encode the hash through the stdClass-aware encoder (empty hash → `{}`)
            } else {
                // x0 = object pointer; dispatches to JsonSerializable when present.
                abi::emit_call_label(emitter, "__rt_json_encode_object");       // encode the object via the per-class JSON descriptor walker
            }
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

    box_json_encode_result(emitter, ctx);

    Some(PhpType::Mixed)
}

fn persist_string_result_if_needed(ty: &PhpType, emitter: &mut Emitter) {
    if ty.codegen_repr() == PhpType::Str {
        abi::emit_call_label(emitter, "__rt_str_persist");                      // keep the string value stable while later json_encode arguments evaluate
    }
}

fn restore_result_value(emitter: &mut Emitter, ty: &PhpType) {
    match ty.codegen_repr() {
        PhpType::Float => {
            abi::emit_pop_float_reg(emitter, abi::float_result_reg(emitter));
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_pop_reg_pair(emitter, ptr_reg, len_reg);
        }
        PhpType::Void | PhpType::Never => {}
        _ => {
            abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
        }
    }
}

fn box_json_encode_result(emitter: &mut Emitter, ctx: &mut Context) {
    let string_label = ctx.next_label("json_encode_string_result");
    let done_label = ctx.next_label("json_encode_boxed_result");

    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg_pair(emitter, "x1", "x2");                      // preserve the encoded JSON string while checking failure state
            abi::emit_load_symbol_to_reg(emitter, "x9", "_json_last_error", 0);
            emitter.instruction(&format!("cbz x9, {}", string_label));          // no JSON error means the string result is valid
            abi::emit_load_symbol_to_reg(emitter, "x9", "_json_active_flags", 0);
            emitter.instruction("tst x9, #512");                                // JSON_PARTIAL_OUTPUT_ON_ERROR keeps the partial string result
            emitter.instruction(&format!("b.ne {}", string_label));             // partial-output flag means return the encoded string
            abi::emit_pop_reg_pair(emitter, "x10", "x11");                     // discard the partial string result before returning false
            emitter.instruction("mov x0, #0");                                  // false payload for json_encode failure
            crate::codegen::emit_box_current_value_as_mixed(emitter, &PhpType::Bool);
            emitter.instruction(&format!("b {}", done_label));                  // skip the string boxing path after returning false
            emitter.label(&string_label);
            abi::emit_pop_reg_pair(emitter, "x1", "x2");                       // restore the successful JSON string result
            crate::codegen::emit_box_current_value_as_mixed(emitter, &PhpType::Str);
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                    // preserve the encoded JSON string while checking failure state
            emitter.instruction("mov r10, QWORD PTR [rip + _json_last_error]"); // load the current JSON error code
            emitter.instruction("test r10, r10");                               // check whether the encoder reported an error
            emitter.instruction(&format!("jz {}", string_label));               // no JSON error means the string result is valid
            emitter.instruction("mov r10, QWORD PTR [rip + _json_active_flags]"); // load the active JSON flag bitmask
            emitter.instruction("test r10, 512");                               // JSON_PARTIAL_OUTPUT_ON_ERROR keeps the partial string result
            emitter.instruction(&format!("jnz {}", string_label));              // partial-output flag means return the encoded string
            abi::emit_pop_reg_pair(emitter, "r10", "r11");                     // discard the partial string result before returning false
            emitter.instruction("xor eax, eax");                                // false payload for json_encode failure
            crate::codegen::emit_box_current_value_as_mixed(emitter, &PhpType::Bool);
            emitter.instruction(&format!("jmp {}", done_label));                // skip the string boxing path after returning false
            emitter.label(&string_label);
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                     // restore the successful JSON string result
            crate::codegen::emit_box_current_value_as_mixed(emitter, &PhpType::Str);
            emitter.label(&done_label);
        }
    }
}
