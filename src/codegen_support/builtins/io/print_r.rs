//! Purpose:
//! Emits PHP `print_r` diagnostic output for scalar, array, and mixed values.
//! Owns recursive/runtime-aware formatting needed for PHP-visible stdout text.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - Output is a side effect, and refcounted values must be inspected without consuming ownership.

use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits PHP `print_r` diagnostic output to stdout for a single argument.
///
/// # Arguments
/// - `_name`: The builtin name (unused, always `print_r`).
/// - `args`: Single expression to print. Must not be empty.
/// - `emitter`: Target-aware assembly emitter.
/// - `ctx`: Codegen context carrying type information for the argument.
/// - `data`: Writable data section for string/symbol materialization.
///
/// # Returns
/// Always returns `Some(PhpType::Void)`.
///
/// # Behavior
/// - `bool`: Prints `"1"` for `true`, nothing for `false`.
/// - `void` (null): Prints nothing.
/// - `array`: Prints `"Array\n"` label only (recursion not supported).
/// - `int`, `float`, `string`: Same output as `echo` via `emit_write_stdout`.
///
/// # Side effects
/// Writes to stdout. Does not consume ownership of the argument value.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("print_r()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    match &ty {
        PhpType::Bool => {
            // print_r(true) prints "1", print_r(false) prints nothing
            let skip = ctx.next_label("pr_skip");
            match emitter.target.arch {
                Arch::X86_64 => {
                    emitter.instruction("cmp rax, 0");                          // test the boolean payload in the x86_64 integer result register before deciding whether print_r() should print anything
                    emitter.instruction(&format!("je {}", skip));               // skip the print_r() write path entirely when the boolean payload is false on x86_64
                }
                Arch::AArch64 => {
                    emitter.instruction("cmp x0, #0");                          // test the boolean payload in the AArch64 integer result register before deciding whether print_r() should print anything
                    emitter.instruction(&format!("cbz x0, {}", skip));          // skip the print_r() write path entirely when the boolean payload is false on AArch64
                }
            }
            abi::emit_write_stdout(emitter, &ty);
            emitter.label(&skip);
        }
        PhpType::Void => {
            // print_r(null) prints nothing
        }
        PhpType::Array(elem_ty) => {
            // -- print "Array\n" --
            let (lbl, len) = data.add_string(b"Array\n");
            abi::emit_symbol_address(emitter, abi::string_result_regs(emitter).0, &lbl); // materialize the borrowed \"Array\\n\" string pointer in the active target string-result pointer register
            abi::emit_load_int_immediate(emitter, abi::string_result_regs(emitter).1, len as i64); // materialize the borrowed \"Array\\n\" string length in the paired target string-result length register
            abi::emit_write_stdout(emitter, &PhpType::Str);                     // print the synthetic array label through the shared target-aware string stdout helper
            let _ = elem_ty;
        }
        _ => {
            // print_r for int, float, string — same as echo
            abi::emit_write_stdout(emitter, &ty);
        }
    }
    Some(PhpType::Void)
}
