use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;
use super::stat_result::box_stat_array_or_false_result;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("fstat()");
    // The fd is an integer; emit_expr leaves the result in the integer return
    // register (x0 / rax), which is exactly what `__rt_fstat_array` reads.
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_fstat_array");                          // call the target-aware runtime helper that builds the PHP-compatible fstat array
    box_stat_array_or_false_result(emitter, ctx);
    Some(PhpType::Mixed)
}
