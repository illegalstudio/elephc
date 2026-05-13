use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{coerce_to_string, emit_expr};
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
    emitter.comment("json_validate()");

    let json_ty = emit_expr(&args[0], emitter, ctx, data);
    coerce_to_string(emitter, ctx, data, &json_ty);
    abi::emit_call_label(emitter, "__rt_str_persist");                          // keep the JSON source stable while optional arguments evaluate
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);                         // preserve the JSON source until the validator call

    if let Some(depth_expr) = args.get(1) {
        emit_expr(depth_expr, emitter, ctx, data);
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // preserve the depth argument until flags have evaluated
    }
    if let Some(flag_expr) = args.get(2) {
        emit_expr(flag_expr, emitter, ctx, data);
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // preserve the flag argument until JSON runtime state is updated
    }

    // PHP evaluates arguments before the builtin clears error state or writes
    // runtime JSON configuration.
    abi::emit_store_zero_to_symbol(emitter, "_json_last_error", 0);
    abi::emit_store_zero_to_symbol(emitter, "_json_active_depth", 0);

    if args.get(2).is_some() {
        abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
        mask_json_validate_flags(emitter);
        abi::emit_store_reg_to_symbol(
            emitter,
            abi::int_result_reg(emitter),
            "_json_active_flags",
            0,
        );
    } else {
        abi::emit_store_zero_to_symbol(emitter, "_json_active_flags", 0);
    }

    // PHP json_validate rejects nesting when active_depth >= depth (strict).
    // The shared __rt_json_depth_enter compares `active <= limit` so we
    // subtract 1 from the user-supplied depth to align (depth=1 → limit=0
    // → top-level container fails).
    if args.get(1).is_some() {
        abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
        let reg = abi::int_result_reg(emitter);
        match emitter.target.arch {
            Arch::AArch64 => emitter.instruction(&format!("sub {reg}, {reg}, #1")), // strict-semantic offset for json_validate
            Arch::X86_64 => emitter.instruction(&format!("sub {reg}, 1")),      // strict-semantic offset for json_validate
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

    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_pop_reg_pair(emitter, ptr_reg, len_reg);
    abi::emit_call_label(emitter, "__rt_json_validate");
    Some(PhpType::Bool)
}

fn mask_json_validate_flags(emitter: &mut Emitter) {
    let reg = abi::int_result_reg(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x9, #1048576");                            // mask = JSON_INVALID_UTF8_IGNORE, the only json_validate flag PHP allows
            emitter.instruction(&format!("and {reg}, {reg}, x9"));              // ignore dynamically supplied unsupported validate flags
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("and {reg}, 1048576"));                // keep only JSON_INVALID_UTF8_IGNORE for dynamic validate flags
        }
    }
}
