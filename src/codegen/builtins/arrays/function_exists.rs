//! Purpose:
//! Emits PHP `function_exists` checks for builtins, user functions, and include variants.
//! Connects codegen-visible declarations to PHP runtime boolean results.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Builtin checks must reflect the canonical catalog so case-insensitive and namespace fallback behavior stays coherent.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::names::function_variant_active_symbol;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

use super::super::callable_lookup::{lookup_function, FunctionLookup};

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("function_exists()");

    // -- resolve function name at compile time --
    let func_name = match &args[0].kind {
        ExprKind::StringLiteral(name) => name.clone(),
        _ => panic!("function_exists() argument must be a string literal"),
    };

    // -- emit constant true/false based on whether function is known --
    match lookup_function(ctx, &func_name) {
        Some(FunctionLookup::IncludeVariant(variant_name)) => {
            emit_variant_function_exists(&variant_name, emitter, data);
            return Some(PhpType::Bool);
        }
        Some(
            FunctionLookup::Builtin(_)
            | FunctionLookup::Extern(_)
            | FunctionLookup::UserFunction(_),
        ) => {
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 1);
        }
        None => {
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
        }
    }

    Some(PhpType::Bool)
}

fn emit_variant_function_exists(
    func_name: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    let active_symbol = function_variant_active_symbol(func_name);
    data.add_comm(active_symbol.clone(), 8);
    let result_reg = abi::int_result_reg(emitter);
    abi::emit_load_symbol_to_reg(emitter, result_reg, &active_symbol, 0);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, #0", result_reg));            // test whether an include has activated this function variant
            emitter.instruction(&format!("cset {}, ne", result_reg));           // return true only when a function variant is active
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {}, {}", result_reg, result_reg)); // test whether an include has activated this function variant
            emitter.instruction("setne al");                                    // return true only when a function variant is active
            emitter.instruction("movzx rax, al");                               // widen the boolean byte into the integer result register
        }
    }
}
