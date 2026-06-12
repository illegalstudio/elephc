//! Purpose:
//! Emits PHP `range` builtin calls that allocate or reshape array values.
//! Coordinates element type selection with runtime helpers that build indexed or associative arrays.
//!
//! Called from:
//! - `crate::codegen::builtins::arrays::emit()`.
//!
//! Key details:
//! - Returned arrays must use the payload layout expected by later codegen and GC/refcount paths.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{coerce_to_int, emit_expr};
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `range($start, $end)` builtin call.
///
/// Evaluates the `$start` and `$end` expressions, calls the `__rt_range` runtime helper,
/// and returns an `array` of integers from `$start` to `$end` (inclusive).
///
/// # Architecture-specific ABI
/// - **x86_64**: Evaluates `$start` into `rax`, pushes it onto the stack, evaluates
///   `$end` into `rax`, then arranges arguments into `rdi` (start) and `rsi` (end)
///   before calling `__rt_range`.
/// - **ARM64**: Pushes `$start` onto the stack, evaluates `$end` into `x0`, pops the
///   saved start into `x0`, and moves end to `x1` (AAPCS64 register ordering) before
///   calling `__rt_range`.
///
/// # Return type
/// Always returns `array` of `int` (`PhpType::Array(Box::new(PhpType::Int))`).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("range()");
    if emitter.target.arch == Arch::X86_64 {
        let start_ty = emit_expr(&args[0], emitter, ctx, data);
        coerce_to_int(emitter, &start_ty);                                      // unbox a Mixed/Union range start into a raw integer
        abi::emit_push_reg(emitter, "rax");                                     // preserve the range start value while evaluating the range end value expression
        let end_ty = emit_expr(&args[1], emitter, ctx, data);
        coerce_to_int(emitter, &end_ty);                                        // unbox a Mixed/Union range end into a raw integer
        emitter.instruction("mov rsi, rax");                                    // place the inclusive range end value in the second x86_64 runtime argument register
        abi::emit_pop_reg(emitter, "rdi");                                      // restore the inclusive range start value into the first x86_64 runtime argument register
        abi::emit_call_label(emitter, "__rt_range");                            // build the integer range array through the x86_64 runtime helper
        return Some(PhpType::Array(Box::new(PhpType::Int)));
    }

    let start_ty = emit_expr(&args[0], emitter, ctx, data);
    coerce_to_int(emitter, &start_ty);                                          // unbox a Mixed/Union range start into a raw integer
    // -- save start value, evaluate end value --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push start value onto stack
    let end_ty = emit_expr(&args[1], emitter, ctx, data);
    coerce_to_int(emitter, &end_ty);                                            // unbox a Mixed/Union range end into a raw integer
    // -- call runtime to create array from start to end --
    emitter.instruction("mov x1, x0");                                          // move end value to x1 (second arg)
    emitter.instruction("ldr x0, [sp], #16");                                   // pop start value into x0 (first arg)
    emitter.instruction("bl __rt_range");                                       // call runtime: create range → x0=new array

    Some(PhpType::Array(Box::new(PhpType::Int)))
}
