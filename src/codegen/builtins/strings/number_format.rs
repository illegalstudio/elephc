//! Purpose:
//! Emits PHP `number_format` string transformation or formatting calls.
//! Marshals string/scalar arguments into runtime helpers that allocate returned PHP strings.
//!
//! Called from:
//! - `crate::codegen::builtins::strings::emit()`.
//!
//! Key details:
//! - Returned string pointer/length pairs must be treated as owned runtime values when the helper allocates.
//! - Both separators are passed to the runtime as full `(ptr, len)` pairs so multi-byte
//!   separators (e.g. a non-breaking space) are preserved; an empty separator has length 0.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Pushes a `number_format` separator argument onto the argument stack as a
/// `(pointer, length)` pair (pointer first, then length).
///
/// When `arg` is present it is materialized as a PHP string; otherwise the
/// `default` bytes (`"."` or `","`) are interned and used. An empty separator
/// string is passed through unchanged with length 0, which the runtime treats
/// as "insert no separator". Passing the full pointer/length pair is what makes
/// multi-byte separators work, replacing the previous single-byte truncation.
fn push_separator(
    arg: Option<&Expr>,
    default: &[u8],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    if let Some(arg) = arg {
        super::args::emit_string_arg(arg, emitter, ctx, data);
    } else {
        let (label, _) = data.add_string(default);
        abi::emit_symbol_address(emitter, ptr_reg, &label);
        abi::emit_load_int_immediate(emitter, len_reg, default.len() as i64);
    }
    abi::emit_push_reg(emitter, ptr_reg);                                        // preserve the separator pointer across later argument evaluation
    abi::emit_push_reg(emitter, len_reg);                                        // preserve the separator length across later argument evaluation
}

/// Emits the `number_format` builtin call.
///
/// Evaluates each argument left to right, pushing results onto the argument
/// stack so they survive subsequent evaluations, then pops them into ABI
/// registers and calls `__rt_number_format`. Handles all four parameters:
///
/// - `_name`: Unused name for dispatcher compatibility.
/// - `args[0]`: Numeric value as float (passed via `push_float_arg`).
/// - `args[1]`: Decimal count (default 0 when absent).
/// - `args[2]`: Decimal separator string, passed as `(ptr, len)` (default `"."`).
/// - `args[3]`: Thousands separator string, passed as `(ptr, len)` (default `","`).
///
/// # ABI
/// - AArch64: `d0` = number, `x1` = decimals, `x2`/`x3` = dec separator ptr/len, `x4`/`x5` = thousands separator ptr/len.
/// - x86_64 SysV: `xmm0` = number, `rdi` = decimals, `rsi`/`rdx` = dec separator ptr/len, `rcx`/`r8` = thousands separator ptr/len.
///
/// Returns `Some(PhpType::Str)` as the runtime helper allocates a PHP string.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("number_format()");
    // -- prepare the numeric value as a float --
    super::args::push_float_arg(&args[0], emitter, ctx, data);

    // -- prepare decimals argument (default 0) --
    if args.len() >= 2 {
        super::args::push_int_arg(&args[1], emitter, ctx, data);
    } else {
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // preserve the default decimal count while the separators are evaluated
    }

    // -- prepare decimal point separator as a (ptr, len) pair (default ".") --
    push_separator(args.get(2), b".", emitter, ctx, data);

    // -- prepare thousands separator as a (ptr, len) pair (default ",") --
    push_separator(args.get(3), b",", emitter, ctx, data);

    // -- pop all args from the stack into ABI registers and call the runtime --
    match emitter.target.arch {
        Arch::X86_64 => {
            abi::emit_pop_reg(emitter, "r8");                                   // restore the thousands-separator length into the fifth SysV runtime argument register
            abi::emit_pop_reg(emitter, "rcx");                                  // restore the thousands-separator pointer into the fourth SysV runtime argument register
            abi::emit_pop_reg(emitter, "rdx");                                  // restore the decimal-separator length into the third SysV runtime argument register
            abi::emit_pop_reg(emitter, "rsi");                                  // restore the decimal-separator pointer into the second SysV runtime argument register
            abi::emit_pop_reg(emitter, "rdi");                                  // restore the decimal-count argument into the first SysV runtime argument register
            abi::emit_pop_float_reg(emitter, "xmm0");                           // restore the floating number_format() input into the first SysV floating-point runtime argument register
        }
        Arch::AArch64 => {
            abi::emit_pop_reg(emitter, "x5");                                   // restore the thousands-separator length into the sixth AArch64 runtime argument register
            abi::emit_pop_reg(emitter, "x4");                                   // restore the thousands-separator pointer into the fifth AArch64 runtime argument register
            abi::emit_pop_reg(emitter, "x3");                                   // restore the decimal-separator length into the fourth AArch64 runtime argument register
            abi::emit_pop_reg(emitter, "x2");                                   // restore the decimal-separator pointer into the third AArch64 runtime argument register
            abi::emit_pop_reg(emitter, "x1");                                   // restore the decimal-count argument into the second AArch64 runtime argument register
            abi::emit_pop_float_reg(emitter, "d0");                             // restore the floating number_format() input into the first AArch64 floating-point runtime argument register
        }
    }
    abi::emit_call_label(emitter, "__rt_number_format");                        // call the target-aware number_format() runtime helper to produce the formatted string

    Some(PhpType::Str)
}
