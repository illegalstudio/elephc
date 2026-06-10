//! Purpose:
//! Emits PHP `define` calls for compile-time and runtime constant registration.
//! Tracks generated symbols that guard repeated defines and constant lookups.
//!
//! Called from:
//! - `crate::codegen::builtins::system::emit()`.
//!
//! Key details:
//! - Constant visibility must stay consistent with resolver/type-checker handling of PHP global constants.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

const DEFINE_ALREADY_DEFINED_WARNING: &str =
    "Warning: define(): Constant already defined\n";

/// Emits code for the PHP `define(name, value)` builtin.
///
/// Stores the constant value in the context for compile-time resolution and
/// emits a runtime guard that checks whether the constant was already defined.
/// On repeated defines, emits a duplicate warning and returns `false`;
/// on first define, marks the constant as seen and returns `true`.
///
/// # Arguments
/// * `name` - The builtin name (unused, dispatch is by arity/signature)
/// * `args` - `[name_expr, value_expr]` where `name_expr` must be a string literal
///
/// # Returns
/// `Some(PhpType::Bool)` — `define()` always returns a boolean in PHP
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

/// Emits the runtime portion of `define()` that guards against duplicate definitions.
///
/// Reads the `flag_symbol` sentinel to determine if this is the first or a repeated
/// `define()` call at runtime. On first execution, stores `1` to the sentinel and
/// returns `true`. On repeated execution, emits a duplicate warning and returns `false`.
///
/// # Arguments
/// * `flag_symbol` - BSS symbol that tracks whether this constant has been defined
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

/// Constructs a unique BSS symbol name for tracking whether a constant has been defined.
///
/// Mangled name encodes alphanumeric characters verbatim, underscores as `_u`,
/// backslashes as `_ns`, and all other bytes as `_xHH` hex escape sequences.
///
/// # Arguments
/// * `name` - The PHP constant name to mangle into a valid assembly symbol
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

/// Emits a runtime warning for duplicate `define()` calls.
///
/// Loads the `_diag_define_already_defined_msg` string pointer and length
/// into ABI argument registers and calls `__rt_diag_warning`. Target-specific:
/// - ARM64: loads into `x1` (pointer) and `x2` (length)
/// - x86_64: loads into `rdi` (pointer) and `esi` (length)
fn emit_duplicate_warning(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x1", "_diag_define_already_defined_msg");
            emitter.instruction(&format!("mov x2, #{}", DEFINE_ALREADY_DEFINED_WARNING.len())); // pass the warning byte length to the diagnostic helper
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "rdi", "_diag_define_already_defined_msg"); // pass the define() duplicate warning pointer to the diagnostic helper
            emitter.instruction(&format!("mov esi, {}", DEFINE_ALREADY_DEFINED_WARNING.len())); // pass the warning byte length to the diagnostic helper
        }
    }
    abi::emit_call_label(emitter, "__rt_diag_warning");                         // emit or suppress the duplicate define() runtime warning
}
