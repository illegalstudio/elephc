//! Purpose:
//! Emits PHP `stat` filesystem metadata builtin calls.
//! Delegates platform stat work to runtime helpers and boxes PHP false-or-value results.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - Filesystem state is observable, so emitters must preserve call order and failure sentinels.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;
use super::stat_result::box_stat_array_or_false_result;

/// Emits the PHP `stat` builtin call.
///
/// Consumes `args[0]` as the filesystem path expression, evaluates it, calls the
/// target-aware runtime helper `__rt_stat_array` to build the PHP-compatible stat
/// array, boxes the result into `PhpType::Mixed`, and returns that type.
///
/// Arguments:
/// - `name`: unused ( builtin dispatch is by name in the catalog)
/// - `args`: must contain exactly one path argument; the first element is consumed
///
/// Outputs:
/// - Emits path evaluation code followed by a `bl __rt_stat_array` call
/// - Boxes the returned stat array (or false sentinel) into `PhpType::Mixed`
/// - Returns `Some(PhpType::Mixed)`
///
/// Side effects:
/// - Filesystem is accessed by `__rt_stat_array`; call order is observable
/// - `ctx` may be mutated by `emit_expr` and `box_stat_array_or_false_result`
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stat()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_stat_array");                           // call the target-aware runtime helper that builds the PHP-compatible stat array
    box_stat_array_or_false_result(emitter, ctx);
    Some(PhpType::Mixed)
}
