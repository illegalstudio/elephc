use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use super::{
    coerce_null_to_zero, coerce_to_string, coerce_to_truthiness, emit_expr, emit_null_coalesce,
    emit_strict_compare, expr_result_heap_ownership, BinOp, Expr, HeapOwnership, PhpType,
};

/// PHP loose comparison coerces both sides to a common type.
/// Simplified: coerce everything to int, then compare.
///   - Bool -> already 0/1 in x0
///   - Null (Void) -> 0
///   - Int -> coerce null sentinel to 0 (via coerce_null_to_zero)
///   - Float -> truncate to int via fcvtzs
///   - String -> 0 (empty string) or parse via atoi
fn coerce_to_int_for_loose_cmp(emitter: &mut Emitter, ty: &PhpType) {
    match ty {
        PhpType::Void => {
            // -- null coerces to 0 --
            emitter.instruction("mov x0, #0");                                  // null is zero for loose comparison
        }
        PhpType::Bool => {
            // Bool is already 0 or 1 in x0 - nothing to do
        }
        PhpType::Int => {
            coerce_null_to_zero(emitter, ty);
        }
        PhpType::Float => {
            // -- truncate float to integer --
            emitter.instruction("fcvtzs x0, d0");                               // convert float to signed int (truncate)
        }
        PhpType::Str => {
            // -- coerce string to int: empty string -> 0, otherwise parse --
            emitter.instruction("bl __rt_atoi");                                // runtime: parse string as integer -> x0
        }
        _ => {
            // Arrays, callables - coerce to 0 as fallback
            emitter.instruction("mov x0, #0");                                  // unsupported type coerces to 0
        }
    }
}

pub(super) fn emit_binop(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    match op {
        BinOp::And => {
            let end_label = ctx.next_label("and_end");
            let lt = emit_expr(left, emitter, ctx, data);
            coerce_to_truthiness(emitter, ctx, &lt);
            // -- short-circuit AND: skip right side if left is falsy --
            emitter.instruction("cmp x0, #0");                                  // test if left operand is falsy
            emitter.instruction(&format!("b.eq {}", end_label));                // short-circuit: left is false so result is 0
            let rt = emit_expr(right, emitter, ctx, data);
            coerce_to_truthiness(emitter, ctx, &rt);
            // -- evaluate right operand truthiness --
            emitter.instruction("cmp x0, #0");                                  // test if right operand is falsy
            emitter.instruction("cset x0, ne");                                 // result=1 if right is truthy, 0 if falsy
            emitter.label(&end_label);
            PhpType::Bool
        }
        BinOp::Or => {
            let end_label = ctx.next_label("or_end");
            let lt = emit_expr(left, emitter, ctx, data);
            coerce_to_truthiness(emitter, ctx, &lt);
            // -- short-circuit OR: skip right side if left is truthy --
            emitter.instruction("cmp x0, #0");                                  // test if left operand is truthy
            emitter.instruction(&format!("b.ne {}", end_label));                // short-circuit: left is true, skip right
            let rt = emit_expr(right, emitter, ctx, data);
            coerce_to_truthiness(emitter, ctx, &rt);
            emitter.label(&end_label);
            // -- normalize final value to boolean 0 or 1 --
            emitter.instruction("cmp x0, #0");                                  // test whichever operand survived
            emitter.instruction("cset x0, ne");                                 // normalize to 1 if truthy, 0 if falsy
            PhpType::Bool
        }
        BinOp::Pow => {
            let lt = emit_expr(left, emitter, ctx, data);
            coerce_null_to_zero(emitter, &lt);
            // -- exponentiation: convert to floats and call libm pow() --
            if lt != PhpType::Float {
                emitter.instruction("scvtf d0, x0");                            // convert integer base to double-precision float
            }
            emitter.instruction("str d0, [sp, #-16]!");                         // save base on stack while evaluating exponent
            let rt = emit_expr(right, emitter, ctx, data);
            coerce_null_to_zero(emitter, &rt);
            if rt != PhpType::Float {
                emitter.instruction("scvtf d0, x0");                            // convert integer exponent to double float
            }
            // -- arrange arguments for pow(base, exp) --
            emitter.instruction("fmov d1, d0");                                 // move exponent to d1 (second argument)
            emitter.instruction("ldr d0, [sp], #16");                           // pop base from stack into d0 (first argument)
            emitter.instruction("bl _pow");                                     // call C library pow(base, exponent)
            PhpType::Float
        }
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
            let lt = emit_expr(left, emitter, ctx, data);
            coerce_null_to_zero(emitter, &lt);
            let use_float = lt == PhpType::Float;
            // -- save left operand on stack while evaluating right --
            if use_float {
                emitter.instruction("str d0, [sp, #-16]!");                     // push left float operand onto stack
            } else {
                emitter.instruction("str x0, [sp, #-16]!");                     // push left integer operand onto stack
            }
            let rt = emit_expr(right, emitter, ctx, data);
            coerce_null_to_zero(emitter, &rt);

            // Division always uses float path (PHP: 10/3 -> 3.333...)
            if lt == PhpType::Float || rt == PhpType::Float || *op == BinOp::Div {
                // -- float arithmetic path --
                if rt != PhpType::Float {
                    emitter.instruction("scvtf d0, x0");                        // promote right int operand to float
                }
                // d0 = right operand (as float)
                emitter.instruction("str d0, [sp, #-16]!");                     // save right float operand on stack
                if lt == PhpType::Float {
                    emitter.instruction("ldr d1, [sp, #16]");                   // load left operand (was already float)
                } else {
                    emitter.instruction("ldr x9, [sp, #16]");                   // load left operand (was integer)
                    emitter.instruction("scvtf d1, x9");                        // promote left integer operand to float
                }
                emitter.instruction("ldr d0, [sp], #16");                       // pop right operand back into d0
                                                                          // d1 = left, d0 = right
                match op {
                    BinOp::Add => {
                        emitter.instruction("fadd d0, d1, d0");                 // float addition: left + right
                    }
                    BinOp::Sub => {
                        emitter.instruction("fsub d0, d1, d0");                 // float subtraction: left - right
                    }
                    BinOp::Mul => {
                        emitter.instruction("fmul d0, d1, d0");                 // float multiplication: left * right
                    }
                    BinOp::Div => {
                        emitter.instruction("fdiv d0, d1, d0");                 // float division: left / right
                    }
                    BinOp::Mod => {
                        // -- float modulo: a - trunc(a/b) * b (C/PHP truncated mod) --
                        emitter.instruction("fdiv d2, d1, d0");                 // d2 = left / right
                        emitter.instruction("frintz d2, d2");                   // d2 = trunc(left / right) toward zero
                        emitter.instruction("fmsub d0, d2, d0, d1");            // d0 = left - trunc(l/r)*right
                    }
                    _ => unreachable!(),
                }
                emitter.instruction("add sp, sp, #16");                         // discard left operand's stack slot
                PhpType::Float
            } else {
                // -- integer arithmetic path --
                emitter.instruction("ldr x1, [sp], #16");                       // pop left integer operand into x1
                match op {
                    BinOp::Add => {
                        emitter.instruction("add x0, x1, x0");                  // integer addition: left + right
                    }
                    BinOp::Sub => {
                        emitter.instruction("sub x0, x1, x0");                  // integer subtraction: left - right
                    }
                    BinOp::Mul => {
                        emitter.instruction("mul x0, x1, x0");                  // integer multiplication: left * right
                    }
                    BinOp::Div => {
                        emitter.instruction("sdiv x0, x1, x0");                 // signed integer division: left / right
                    }
                    BinOp::Mod => {
                        // -- integer modulo: a - (a/b) * b, with zero-divisor guard --
                        let skip = ctx.next_label("mod_ok");
                        let zero = ctx.next_label("mod_zero");
                        emitter.instruction(&format!("cbz x0, {zero}"));        // if divisor is zero, skip to return 0
                        emitter.instruction("sdiv x2, x1, x0");                 // x2 = left / right (integer division)
                        emitter.instruction("msub x0, x2, x0, x1");             // x0 = left - (left/right)*right
                        emitter.instruction(&format!("b {skip}"));              // jump past zero-divisor fallback
                        emitter.label(&zero);
                        emitter.instruction("mov x0, #0");                      // divisor was zero, return 0
                        emitter.label(&skip);
                    }
                    _ => unreachable!(),
                }
                PhpType::Int
            }
        }
        BinOp::Eq | BinOp::NotEq => {
            let lt = emit_expr(left, emitter, ctx, data);
            let lt_numeric = matches!(
                lt,
                PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void
            );
            coerce_null_to_zero(emitter, &lt);
            let use_float = lt == PhpType::Float;
            // -- save left operand on stack while evaluating right --
            if use_float {
                emitter.instruction("str d0, [sp, #-16]!");                     // push left float operand onto stack
            } else {
                if !lt_numeric {
                    // -- coerce non-numeric left to int for loose comparison --
                    coerce_to_int_for_loose_cmp(emitter, &lt);
                }
                emitter.instruction("str x0, [sp, #-16]!");                     // push left integer operand onto stack
            }
            let rt = emit_expr(right, emitter, ctx, data);
            let rt_numeric = matches!(
                rt,
                PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void
            );
            coerce_null_to_zero(emitter, &rt);

            if lt_numeric && rt_numeric && (lt == PhpType::Float || rt == PhpType::Float) {
                // -- float comparison path (both sides numeric, at least one float) --
                if rt != PhpType::Float {
                    emitter.instruction("scvtf d0, x0");                        // promote right int to float for comparison
                }
                if lt == PhpType::Float {
                    emitter.instruction("ldr d1, [sp], #16");                   // pop left float operand from stack
                } else {
                    emitter.instruction("ldr x9, [sp], #16");                   // pop left integer operand from stack
                    emitter.instruction("scvtf d1, x9");                        // promote left int to float for comparison
                }
                emitter.instruction("fcmp d1, d0");                             // compare two doubles, setting NZCV flags
            } else {
                // -- integer comparison path (cross-type coerced to int) --
                if !rt_numeric {
                    coerce_to_int_for_loose_cmp(emitter, &rt);
                }
                emitter.instruction("ldr x1, [sp], #16");                       // pop left integer operand from stack
                emitter.instruction("cmp x1, x0");                              // compare left vs right, setting flags
            }
            let cond = match op {
                BinOp::Eq => "eq",
                BinOp::NotEq => "ne",
                _ => unreachable!(),
            };
            // -- set boolean result based on comparison flags --
            emitter.instruction(&format!("cset x0, {}", cond));                 // x0=1 if condition met, 0 otherwise
            PhpType::Bool
        }
        BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq => {
            let lt = emit_expr(left, emitter, ctx, data);
            coerce_null_to_zero(emitter, &lt);
            let use_float = lt == PhpType::Float;
            // -- save left operand on stack while evaluating right --
            if use_float {
                emitter.instruction("str d0, [sp, #-16]!");                     // push left float operand onto stack
            } else {
                emitter.instruction("str x0, [sp, #-16]!");                     // push left integer operand onto stack
            }
            let rt = emit_expr(right, emitter, ctx, data);
            coerce_null_to_zero(emitter, &rt);

            if lt == PhpType::Float || rt == PhpType::Float {
                // -- float comparison path --
                if rt != PhpType::Float {
                    emitter.instruction("scvtf d0, x0");                        // promote right int to float for comparison
                }
                if lt == PhpType::Float {
                    emitter.instruction("ldr d1, [sp], #16");                   // pop left float operand from stack
                } else {
                    emitter.instruction("ldr x9, [sp], #16");                   // pop left integer operand from stack
                    emitter.instruction("scvtf d1, x9");                        // promote left int to float for comparison
                }
                // d1 = left, d0 = right
                emitter.instruction("fcmp d1, d0");                             // compare two doubles, setting NZCV flags
            } else {
                // -- integer comparison path --
                emitter.instruction("ldr x1, [sp], #16");                       // pop left integer operand from stack
                emitter.instruction("cmp x1, x0");                              // compare left vs right, setting flags
            }
            let cond = match op {
                BinOp::Lt => "lt",
                BinOp::Gt => "gt",
                BinOp::LtEq => "le",
                BinOp::GtEq => "ge",
                _ => unreachable!(),
            };
            // -- set boolean result based on comparison flags --
            emitter.instruction(&format!("cset x0, {}", cond));                 // x0=1 if condition met, 0 otherwise
            PhpType::Bool
        }
        BinOp::StrictEq | BinOp::StrictNotEq => {
            emit_strict_compare(left, op, right, emitter, ctx, data)
        }
        BinOp::Concat => {
            let left_ty = emit_expr(left, emitter, ctx, data);
            coerce_to_string(emitter, ctx, data, &left_ty);
            if expr_result_heap_ownership(left) == HeapOwnership::NonHeap {
                emitter.instruction("bl __rt_str_persist");                     // persist transient left strings before the right operand can reuse concat buffers
            }
            // -- save left string while evaluating right operand --
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push left string ptr+len onto stack
            let right_ty = emit_expr(right, emitter, ctx, data);
            coerce_to_string(emitter, ctx, data, &right_ty);
            // -- set up concat(left_ptr, left_len, right_ptr, right_len) --
            emitter.instruction("mov x3, x1");                                  // move right string pointer to 3rd arg
            emitter.instruction("mov x4, x2");                                  // move right string length to 4th arg
            emitter.instruction("ldp x1, x2, [sp], #16");                       // pop left string ptr/len into 1st/2nd args
            emitter.instruction("bl __rt_concat");                              // call runtime to concatenate two strings
            PhpType::Str
        }
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::ShiftLeft | BinOp::ShiftRight => {
            let lt = emit_expr(left, emitter, ctx, data);
            coerce_null_to_zero(emitter, &lt);
            // -- save left operand on stack while evaluating right --
            emitter.instruction("str x0, [sp, #-16]!");                         // push left integer operand onto stack
            let rt = emit_expr(right, emitter, ctx, data);
            coerce_null_to_zero(emitter, &rt);
            // -- pop left operand and apply bitwise operation --
            emitter.instruction("ldr x1, [sp], #16");                           // pop left integer operand into x1
            match op {
                BinOp::BitAnd => {
                    emitter.instruction("and x0, x1, x0");                      // bitwise AND: left & right
                }
                BinOp::BitOr => {
                    emitter.instruction("orr x0, x1, x0");                      // bitwise OR: left | right
                }
                BinOp::BitXor => {
                    emitter.instruction("eor x0, x1, x0");                      // bitwise XOR: left ^ right
                }
                BinOp::ShiftLeft => {
                    emitter.instruction("lsl x0, x1, x0");                      // left shift: left << right
                }
                BinOp::ShiftRight => {
                    emitter.instruction("asr x0, x1, x0");                      // arithmetic right shift: left >> right
                }
                _ => unreachable!(),
            }
            PhpType::Int
        }
        BinOp::Spaceship => {
            let lt = emit_expr(left, emitter, ctx, data);
            coerce_null_to_zero(emitter, &lt);
            let use_float = lt == PhpType::Float;
            // -- save left operand on stack while evaluating right --
            if use_float {
                emitter.instruction("str d0, [sp, #-16]!");                     // push left float operand onto stack
            } else {
                emitter.instruction("str x0, [sp, #-16]!");                     // push left integer operand onto stack
            }
            let rt = emit_expr(right, emitter, ctx, data);
            coerce_null_to_zero(emitter, &rt);

            if lt == PhpType::Float || rt == PhpType::Float {
                // -- float spaceship comparison --
                if rt != PhpType::Float {
                    emitter.instruction("scvtf d0, x0");                        // promote right int to float
                }
                if lt == PhpType::Float {
                    emitter.instruction("ldr d1, [sp], #16");                   // pop left float
                } else {
                    emitter.instruction("ldr x9, [sp], #16");                   // pop left int
                    emitter.instruction("scvtf d1, x9");                        // promote left int to float
                }
                // d1 = left, d0 = right
                emitter.instruction("fcmp d1, d0");                             // compare left vs right floats
            } else {
                // -- integer spaceship comparison --
                emitter.instruction("ldr x1, [sp], #16");                       // pop left integer
                emitter.instruction("cmp x1, x0");                              // compare left vs right integers
            }
            // -- produce -1, 0, or 1 based on comparison flags --
            emitter.instruction("cset x0, gt");                                 // x0=1 if left > right
            emitter.instruction("csinv x0, x0, xzr, ge");                       // x0=-1 if left < right (invert zero -> all-ones)
            PhpType::Int
        }
        BinOp::NullCoalesce => {
            // Should not reach here - handled by ExprKind::NullCoalesce
            // But handle gracefully via the same mechanism
            emit_null_coalesce(left, right, emitter, ctx, data)
        }
    }
}
