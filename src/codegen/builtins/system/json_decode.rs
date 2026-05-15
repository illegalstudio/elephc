//! Purpose:
//! Emits PHP `json_decode` JSON builtin calls.
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
use crate::codegen::expr::{coerce_to_string, coerce_to_truthiness, emit_expr};
use crate::codegen::platform::Arch;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("json_decode()");

    let json_ty = emit_expr(&args[0], emitter, ctx, data);
    coerce_to_string(emitter, ctx, data, &json_ty);
    abi::emit_call_label(emitter, "__rt_str_persist");                          // keep the JSON source stable while optional arguments evaluate
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);                         // preserve the JSON source until validation and decoding

    let assoc_arg = evaluate_assoc_arg(args, emitter, ctx, data);
    if let Some(depth_expr) = args.get(2) {
        emit_expr(depth_expr, emitter, ctx, data);
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // preserve the depth argument until later arguments have evaluated
    }
    if let Some(flag_expr) = args.get(3) {
        emit_expr(flag_expr, emitter, ctx, data);
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // preserve the flags argument until runtime JSON state is updated
    }

    // PHP evaluates every argument before the builtin mutates JSON error
    // state or call configuration.
    abi::emit_store_zero_to_symbol(emitter, "_json_last_error", 0);
    abi::emit_store_zero_to_symbol(emitter, "_json_active_depth", 0);

    if args.get(3).is_some() {
        abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
        abi::emit_store_reg_to_symbol(
            emitter,
            abi::int_result_reg(emitter),
            "_json_active_flags",
            0,
        );
        if matches!(assoc_arg, AssocArg::FromFlags) {
            write_assoc_from_flags(emitter);
        }
    } else {
        abi::emit_store_zero_to_symbol(emitter, "_json_active_flags", 0);
        if matches!(assoc_arg, AssocArg::FromFlags) {
            abi::emit_store_zero_to_symbol(emitter, "_json_decode_assoc", 0);   // missing/null associative arg and no flag → stdClass
        }
    }
    // PHP json_decode rejects nesting when active_depth >= depth (strict).
    // The shared __rt_json_depth_enter compares `active <= limit` so we
    // subtract 1 from the user-supplied depth here to get the same
    // observable behavior (depth=1 → limit=0 → top-level container fails).
    if args.get(2).is_some() {
        abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
        let reg = abi::int_result_reg(emitter);
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction(&format!("sub {reg}, {reg}, #1")), // strict-semantic offset for json_decode
            Arch::X86_64 => emitter.instruction(&format!("sub {reg}, 1")),      // strict-semantic offset for json_decode
        }
        abi::emit_store_reg_to_symbol(emitter, reg, "_json_depth_limit", 0);
    } else {
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 511);
        abi::emit_store_reg_to_symbol(
            emitter,
            abi::int_result_reg(emitter),
            "_json_depth_limit",
            0,
        );
    }

    if matches!(assoc_arg, AssocArg::Explicit) {
        abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
        abi::emit_store_reg_to_symbol(
            emitter,
            abi::int_result_reg(emitter),
            "_json_decode_assoc",
            0,
        );
    }

    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_pop_reg_pair(emitter, ptr_reg, len_reg);
    let valid_label = ctx.next_label("json_decode_valid");
    let done_label = ctx.next_label("json_decode_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            // x1 = string ptr, x2 = string len after emit_expr.
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // park the json source slice across the validator call
            emitter.instruction("bl __rt_json_validate");                       // RFC 8259 validator; returns 1 on success, 0 on failure (and sets _json_last_error)
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the json source slice for the structural decoder
            emitter.instruction(&format!("cbnz x0, {}", valid_label));          // valid input → fall through to structural decode
            // Invalid: return Mixed(null) without invoking the decoder.
            emitter.instruction("mov x0, #8");                                  // tag = 8 (null)
            emitter.instruction("mov x1, #0");                                  // value_lo = 0
            emitter.instruction("mov x2, #0");                                  // value_hi = 0
            emitter.instruction("bl __rt_mixed_from_value");                    // box Mixed(null) so callers see a uniform result shape
            emitter.instruction(&format!("b {}", done_label));                  // skip the structural decoder when validation already rejected the input
            emitter.label(&valid_label);
            emitter.instruction("bl __rt_json_decode_mixed");                   // structural decoder: scalars box natively; arrays decode as Mixed(array); objects honor _json_decode_assoc
            emitter.label(&done_label);
        }
        Arch::X86_64 => {
            // rax = string ptr, rdx = string len after emit_expr.
            emitter.instruction("push rax");                                    // park the json string pointer across the validator call
            emitter.instruction("push rdx");                                    // park the json string length across the validator call (kept aligned to 16 bytes)
            emitter.instruction("call __rt_json_validate");                     // RFC 8259 validator; returns 1 on success, 0 on failure (and sets _json_last_error)
            emitter.instruction("pop rdx");                                     // restore the json string length for the structural decoder
            emitter.instruction("pop rsi");                                     // pop the saved pointer into a scratch register before swapping into rax
            emitter.instruction("test rax, rax");                               // valid → non-zero; invalid → zero
            emitter.instruction(&format!("jne {}", valid_label));               // valid → fall through to structural decode (rsi has the saved ptr)
            // Invalid: return Mixed(null) without invoking the decoder.
            emitter.instruction("mov rax, 8");                                  // tag = 8 (null)
            emitter.instruction("mov rdi, 0");                                  // value_lo = 0
            emitter.instruction("mov rsi, 0");                                  // value_hi = 0
            emitter.instruction("call __rt_mixed_from_value");                  // box Mixed(null) so callers see a uniform result shape
            emitter.instruction(&format!("jmp {}", done_label));                // skip the structural decoder when validation already rejected the input
            emitter.label(&valid_label);
            emitter.instruction("mov rax, rsi");                                // restore the json string pointer into the rax/rdx string-arg pair
            emitter.instruction("call __rt_json_decode_mixed");                 // structural decoder honoring _json_decode_assoc
            emitter.label(&done_label);
        }
    }

    Some(PhpType::Mixed)
}

/// Evaluate the `$associative` argument in PHP source order.
///
/// PHP semantics: missing or `null` → false (stdClass), `false` → false,
/// `true` → true. When `$associative` is missing/null, `JSON_OBJECT_AS_ARRAY`
/// in the flags argument chooses the shape. Dynamic non-null expressions use
/// normal PHP truthiness and are stored after all later arguments evaluate.
fn evaluate_assoc_arg(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> AssocArg {
    if args.len() < 2 || matches!(args[1].kind, ExprKind::Null) {
        return AssocArg::FromFlags;
    }
    if let ExprKind::BoolLiteral(value) = args[1].kind {
        let scratch = abi::int_result_reg(emitter);
        abi::emit_load_int_immediate(emitter, scratch, if value { 1 } else { 0 });
        abi::emit_push_reg(emitter, scratch);                                   // preserve the explicit associative literal until JSON state is updated
        return AssocArg::Explicit;
    }
    let ty = emit_expr(&args[1], emitter, ctx, data);
    if ty == PhpType::Void {
        return AssocArg::FromFlags;
    }
    coerce_to_truthiness(emitter, ctx, &ty);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                  // preserve the truthiness result until later arguments have evaluated
    AssocArg::Explicit
}

fn write_assoc_from_flags(emitter: &mut Emitter) {
    let reg = abi::int_result_reg(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("and {reg}, {reg}, #1"));              // keep JSON_OBJECT_AS_ARRAY when associative is null/missing
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("and {reg}, 1"));                      // keep JSON_OBJECT_AS_ARRAY when associative is null/missing
        }
    }
    abi::emit_store_reg_to_symbol(emitter, reg, "_json_decode_assoc", 0);
}

enum AssocArg {
    Explicit,
    FromFlags,
}
