use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("empty()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    match &ty {
        PhpType::Int => {
            // -- int is empty if it equals zero --
            crate::codegen::expr::coerce_null_to_zero(emitter, &ty);
            emitter.instruction("cmp x0, #0");                                  // compare int value against zero
            emitter.instruction("cset x0, eq");                                 // x0 = 1 if zero (empty), 0 otherwise
        }
        PhpType::Float => {
            // -- float is empty if it equals 0.0 --
            emitter.instruction("fcmp d0, #0.0");                               // compare float value against 0.0
            emitter.instruction("cset x0, eq");                                 // x0 = 1 if zero (empty), 0 otherwise
        }
        PhpType::Bool => {
            // -- bool is empty if false (0) --
            emitter.instruction("cmp x0, #0");                                  // compare bool value against zero
            emitter.instruction("cset x0, eq");                                 // x0 = 1 if false (empty), 0 otherwise
        }
        PhpType::Void => {
            // -- null is always empty --
            emitter.instruction("mov x0, #1");                                  // null is always empty, return true
        }
        PhpType::Mixed => {
            // -- mixed values are non-empty when the boxed pointer is non-null --
            emitter.instruction("cmp x0, #0");                                  // compare boxed mixed pointer against null
            emitter.instruction("cset x0, eq");                                 // x0 = 1 if null mixed pointer, 0 otherwise
        }
        PhpType::Str => {
            // -- string is empty if length is zero --
            emitter.instruction("cmp x2, #0");                                  // compare string length against zero
            emitter.instruction("cset x0, eq");                                 // x0 = 1 if empty string, 0 otherwise
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            // -- array is empty if element count is zero --
            emitter.instruction("ldr x0, [x0]");                                // load array element count from header
            emitter.instruction("cmp x0, #0");                                  // compare element count against zero
            emitter.instruction("cset x0, eq");                                 // x0 = 1 if empty array, 0 otherwise
        }
        PhpType::Callable | PhpType::Object(_) => {
            // -- callable/object is never empty --
            emitter.instruction("mov x0, #0");                                  // callable/object is never empty, return false
        }
        PhpType::Pointer(_) => {
            // -- pointer is empty only when it is the null pointer --
            emitter.instruction("cmp x0, #0");                                  // compare pointer value against null
            emitter.instruction("cset x0, eq");                                 // x0 = 1 if null pointer, 0 otherwise
        }
    }
    Some(PhpType::Bool)
}
