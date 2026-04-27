use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

const DEFINE_ALREADY_DEFINED_WARNING: &str =
    "Warning: define(): Constant already defined\n";

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    // define("NAME", value) — store constant for compile-time resolution
    let const_name = match &args[0].kind {
        ExprKind::StringLiteral(s) => s.clone(),
        _ => panic!("define() first argument must be a string literal"),
    };

    let ty = match &args[1].kind {
        ExprKind::IntLiteral(_) => PhpType::Int,
        ExprKind::FloatLiteral(_) => PhpType::Float,
        ExprKind::StringLiteral(_) => PhpType::Str,
        ExprKind::BoolLiteral(_) => PhpType::Bool,
        ExprKind::Null => PhpType::Void,
        _ => PhpType::Int,
    };

    ctx.constants
        .entry(const_name.clone())
        .or_insert((args[1].kind.clone(), ty));

    let flag_symbol = data.add_comm(define_seen_symbol(&const_name), 8);
    emit_runtime_define_result(&flag_symbol, emitter, ctx);

    Some(PhpType::Bool)
}

fn emit_runtime_define_result(flag_symbol: &str, emitter: &mut Emitter, ctx: &mut Context) {
    let first_label = ctx.next_label("define_first");
    let done_label = ctx.next_label("define_done");
    let result_reg = abi::int_result_reg(emitter);

    abi::emit_load_symbol_to_reg(emitter, result_reg, flag_symbol, 0);
    abi::emit_branch_if_int_result_zero(emitter, &first_label);                 // first runtime execution defines the constant successfully
    emit_duplicate_warning(emitter);
    abi::emit_load_int_immediate(emitter, result_reg, 0);
    abi::emit_jump(emitter, &done_label);                                       // skip the first-define path after reporting the duplicate

    emitter.label(&first_label);
    abi::emit_load_int_immediate(emitter, result_reg, 1);
    abi::emit_store_reg_to_symbol(emitter, result_reg, flag_symbol, 0);

    emitter.label(&done_label);
}

fn define_seen_symbol(name: &str) -> String {
    let mut symbol = String::from("_define_seen");
    for byte in name.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' => symbol.push(byte as char),
            b'_' => symbol.push_str("_u"),
            b'\\' => symbol.push_str("_ns"),
            _ => symbol.push_str(&format!("_x{:02x}", byte)),
        }
    }
    symbol
}

fn emit_duplicate_warning(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.adrp("x1", "_diag_define_already_defined_msg");
            emitter.add_lo12("x1", "x1", "_diag_define_already_defined_msg");
            emitter.instruction(&format!("mov x2, #{}", DEFINE_ALREADY_DEFINED_WARNING.len())); // pass the warning byte length to the diagnostic helper
        }
        Arch::X86_64 => {
            emitter.instruction("lea rdi, [rip + _diag_define_already_defined_msg]"); // pass the define() duplicate warning pointer to the diagnostic helper
            emitter.instruction(&format!("mov esi, {}", DEFINE_ALREADY_DEFINED_WARNING.len())); // pass the warning byte length to the diagnostic helper
        }
    }
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the duplicate define() runtime warning
}
