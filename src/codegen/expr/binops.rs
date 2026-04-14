use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use super::super::{abi, platform::Arch};
use super::{
    coerce_null_to_zero, coerce_to_string, coerce_to_truthiness, emit_expr, emit_null_coalesce,
    emit_strict_compare, expr_result_heap_ownership, BinOp, Expr, HeapOwnership, PhpType,
};

mod target;

use target::{
    emit_float_binop, emit_float_compare, emit_pop_left_float_for_comparison,
    emit_promote_int_to_float, emit_set_bool_from_flags, emit_set_float_bool_from_flags,
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
        PhpType::Mixed | PhpType::Union(_) => {
            // -- mixed/union values coerce via the boxed runtime tag --
            emitter.instruction("bl __rt_mixed_cast_int");                      // runtime: inspect the boxed payload and cast to int
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
            abi::emit_branch_if_int_result_zero(emitter, &end_label);           // short-circuit immediately when the left operand is falsy
            let rt = emit_expr(right, emitter, ctx, data);
            coerce_to_truthiness(emitter, ctx, &rt);
            // -- evaluate right operand truthiness --
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("cmp x0, #0");                          // test whether the right operand truthiness is zero
                }
                Arch::X86_64 => {
                    emitter.instruction("test rax, rax");                       // test whether the right operand truthiness is zero
                }
            }
            emit_set_bool_from_flags(emitter, "ne");                            // normalize the right operand truthiness to a boolean 0/1 result
            emitter.label(&end_label);
            PhpType::Bool
        }
        BinOp::Or => {
            let end_label = ctx.next_label("or_end");
            let lt = emit_expr(left, emitter, ctx, data);
            coerce_to_truthiness(emitter, ctx, &lt);
            // -- short-circuit OR: skip right side if left is truthy --
            abi::emit_branch_if_int_result_nonzero(emitter, &end_label);        // short-circuit immediately when the left operand is truthy
            let rt = emit_expr(right, emitter, ctx, data);
            coerce_to_truthiness(emitter, ctx, &rt);
            emitter.label(&end_label);
            // -- normalize final value to boolean 0 or 1 --
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("cmp x0, #0");                          // test whether the surviving operand truthiness is zero
                }
                Arch::X86_64 => {
                    emitter.instruction("test rax, rax");                       // test whether the surviving operand truthiness is zero
                }
            }
            emit_set_bool_from_flags(emitter, "ne");                            // normalize whichever operand survived to a boolean 0/1 result
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
            emitter.bl_c("pow");                                     // call C library pow(base, exponent)
            PhpType::Float
        }
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
            let lt = emit_expr(left, emitter, ctx, data);
            coerce_null_to_zero(emitter, &lt);
            let use_float = lt == PhpType::Float;
            // -- save left operand on stack while evaluating right --
            if use_float {
                abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter)); // push left float operand onto the target temporary stack
            } else {
                abi::emit_push_reg(emitter, abi::int_result_reg(emitter));       // push left integer operand onto the target temporary stack
            }
            let rt = emit_expr(right, emitter, ctx, data);
            coerce_null_to_zero(emitter, &rt);

            // Division always uses float path (PHP: 10/3 -> 3.333...)
            if lt == PhpType::Float || rt == PhpType::Float || *op == BinOp::Div {
                // -- float arithmetic path --
                if rt != PhpType::Float {
                    emit_promote_int_to_float(
                        emitter,
                        abi::float_result_reg(emitter),
                        abi::int_result_reg(emitter),
                    );
                }
                // d0 = right operand (as float)
                abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter)); // save right float operand on the target temporary stack
                if lt == PhpType::Float {
                    let left_float_reg = match emitter.target.arch {
                        Arch::AArch64 => "d1",
                        Arch::X86_64 => "xmm1",
                    };
                    abi::emit_load_temporary_stack_slot(emitter, left_float_reg, 16);
                } else {
                    let left_int_reg = abi::symbol_scratch_reg(emitter);
                    let left_float_reg = match emitter.target.arch {
                        Arch::AArch64 => "d1",
                        Arch::X86_64 => "xmm1",
                    };
                    abi::emit_load_temporary_stack_slot(emitter, left_int_reg, 16);
                    emit_promote_int_to_float(emitter, left_float_reg, left_int_reg);
                }
                abi::emit_pop_float_reg(emitter, abi::float_result_reg(emitter)); // pop right operand back into the floating-point result register
                                                                          // d1/xmm1 = left, d0/xmm0 = right
                emit_float_binop(emitter, op);
                abi::emit_release_temporary_stack(emitter, 16);                  // discard the left operand's temporary stack slot
                PhpType::Float
            } else {
                // -- integer arithmetic path --
                let left_reg = match emitter.target.arch {
                    Arch::AArch64 => "x1",
                    Arch::X86_64 => "r10",
                };
                let result_reg = abi::int_result_reg(emitter);
                abi::emit_pop_reg(emitter, left_reg);                            // pop left integer operand into a scratch register
                match op {
                    BinOp::Add => {
                        match emitter.target.arch {
                            Arch::AArch64 => emitter.instruction("add x0, x1, x0"), // integer addition: left + right
                            Arch::X86_64 => {
                                emitter.instruction(&format!("add {}, {}", left_reg, result_reg)); // integer addition: left + right
                                emitter.instruction(&format!("mov {}, {}", result_reg, left_reg)); // move sum back to the integer result register
                            }
                        }
                    }
                    BinOp::Sub => {
                        match emitter.target.arch {
                            Arch::AArch64 => emitter.instruction("sub x0, x1, x0"), // integer subtraction: left - right
                            Arch::X86_64 => {
                                emitter.instruction(&format!("sub {}, {}", left_reg, result_reg)); // integer subtraction: left - right
                                emitter.instruction(&format!("mov {}, {}", result_reg, left_reg)); // move difference back to the integer result register
                            }
                        }
                    }
                    BinOp::Mul => {
                        match emitter.target.arch {
                            Arch::AArch64 => emitter.instruction("mul x0, x1, x0"), // integer multiplication: left * right
                            Arch::X86_64 => {
                                emitter.instruction(&format!("imul {}, {}", left_reg, result_reg)); // integer multiplication: left * right
                                emitter.instruction(&format!("mov {}, {}", result_reg, left_reg)); // move product back to the integer result register
                            }
                        }
                    }
                    BinOp::Div => {
                        emitter.instruction("sdiv x0, x1, x0");                 // signed integer division: left / right
                    }
                    BinOp::Mod => {
                        // -- integer modulo: a - (a/b) * b, with zero-divisor guard --
                        let skip = ctx.next_label("mod_ok");
                        let zero = ctx.next_label("mod_zero");
                        match emitter.target.arch {
                            Arch::AArch64 => {
                                emitter.instruction(&format!("cbz x0, {zero}")); // if divisor is zero, skip to return 0
                                emitter.instruction("sdiv x2, x1, x0");         // x2 = left / right (integer division)
                                emitter.instruction("msub x0, x2, x0, x1");     // x0 = left - (left/right)*right
                                emitter.instruction(&format!("b {skip}"));      // jump past zero-divisor fallback
                                emitter.label(&zero);
                                emitter.instruction("mov x0, #0");              // divisor was zero, return 0
                                emitter.label(&skip);
                            }
                            Arch::X86_64 => {
                                emitter.instruction(&format!("test {}, {}", result_reg, result_reg)); // if divisor is zero, skip to return 0
                                emitter.instruction(&format!("je {}", zero));   // divisor was zero
                                emitter.instruction(&format!("mov r11, {}", result_reg)); // preserve divisor while idiv uses rax
                                emitter.instruction(&format!("mov {}, {}", result_reg, left_reg)); // move dividend into rax for signed division
                                emitter.instruction("cqo");                     // sign-extend rax into rdx for idiv
                                emitter.instruction("idiv r11");                // divide left by right, leaving remainder in rdx
                                emitter.instruction(&format!("mov {}, rdx", result_reg)); // move remainder into the integer result register
                                emitter.instruction(&format!("jmp {}", skip));  // jump past zero-divisor fallback
                                emitter.label(&zero);
                                emitter.instruction(&format!("mov {}, 0", result_reg)); // divisor was zero, return 0
                                emitter.label(&skip);
                            }
                        }
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
                abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter)); // push left float operand onto the target temporary stack
            } else {
                if !lt_numeric {
                    // -- coerce non-numeric left to int for loose comparison --
                    coerce_to_int_for_loose_cmp(emitter, &lt);
                }
                abi::emit_push_reg(emitter, abi::int_result_reg(emitter));       // push left integer operand onto the target temporary stack
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
                    emit_promote_int_to_float(
                        emitter,
                        abi::float_result_reg(emitter),
                        abi::int_result_reg(emitter),
                    );
                }
                emit_pop_left_float_for_comparison(emitter, &lt);
                emit_float_compare(emitter);
            } else {
                // -- integer comparison path (cross-type coerced to int) --
                if !rt_numeric {
                    coerce_to_int_for_loose_cmp(emitter, &rt);
                }
                let left_reg = match emitter.target.arch {
                    Arch::AArch64 => "x1",
                    Arch::X86_64 => "r10",
                };
                abi::emit_pop_reg(emitter, left_reg);                            // pop left integer operand from the target temporary stack
                emitter.instruction(&format!("cmp {}, {}", left_reg, abi::int_result_reg(emitter))); // compare left vs right, setting flags
            }
            let cond = match op {
                BinOp::Eq => "eq",
                BinOp::NotEq => "ne",
                _ => unreachable!(),
            };
            // -- set boolean result based on comparison flags --
            if lt_numeric && rt_numeric && (lt == PhpType::Float || rt == PhpType::Float) {
                emit_set_float_bool_from_flags(emitter, cond);                   // result=1 if the float comparison condition matched
            } else {
                emit_set_bool_from_flags(emitter, cond);                         // result=1 if the integer comparison condition matched
            }
            PhpType::Bool
        }
        BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq => {
            let lt = emit_expr(left, emitter, ctx, data);
            coerce_null_to_zero(emitter, &lt);
            let use_float = lt == PhpType::Float;
            // -- save left operand on stack while evaluating right --
            if use_float {
                abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter)); // push left float operand onto the target temporary stack
            } else {
                abi::emit_push_reg(emitter, abi::int_result_reg(emitter));       // push left integer operand onto the target temporary stack
            }
            let rt = emit_expr(right, emitter, ctx, data);
            coerce_null_to_zero(emitter, &rt);

            if lt == PhpType::Float || rt == PhpType::Float {
                // -- float comparison path --
                if rt != PhpType::Float {
                    emit_promote_int_to_float(
                        emitter,
                        abi::float_result_reg(emitter),
                        abi::int_result_reg(emitter),
                    );
                }
                emit_pop_left_float_for_comparison(emitter, &lt);
                emit_float_compare(emitter);
            } else {
                // -- integer comparison path --
                let left_reg = match emitter.target.arch {
                    Arch::AArch64 => "x1",
                    Arch::X86_64 => "r10",
                };
                abi::emit_pop_reg(emitter, left_reg);                            // pop left integer operand from the target temporary stack
                emitter.instruction(&format!("cmp {}, {}", left_reg, abi::int_result_reg(emitter))); // compare left vs right, setting flags
            }
            let cond = match op {
                BinOp::Lt => "lt",
                BinOp::Gt => "gt",
                BinOp::LtEq => "le",
                BinOp::GtEq => "ge",
                _ => unreachable!(),
            };
            // -- set boolean result based on comparison flags --
            if lt == PhpType::Float || rt == PhpType::Float {
                emit_set_float_bool_from_flags(emitter, cond);                   // result=1 if the float comparison condition matched
            } else {
                emit_set_bool_from_flags(emitter, cond);                         // result=1 if the integer comparison condition matched
            }
            PhpType::Bool
        }
        BinOp::StrictEq | BinOp::StrictNotEq => {
            emit_strict_compare(left, op, right, emitter, ctx, data)
        }
        BinOp::Concat => {
            let left_ty = emit_expr(left, emitter, ctx, data);
            coerce_to_string(emitter, ctx, data, &left_ty);
            if expr_result_heap_ownership(left) == HeapOwnership::NonHeap {
                abi::emit_call_label(emitter, "__rt_str_persist");              // persist transient left strings before the right operand can reuse concat buffers
            }
            // -- save left string while evaluating right operand --
            let (left_ptr_reg, left_len_reg) = abi::string_result_regs(emitter);
            abi::emit_push_reg_pair(emitter, left_ptr_reg, left_len_reg);        // push left string ptr+len onto stack
            let right_ty = emit_expr(right, emitter, ctx, data);
            coerce_to_string(emitter, ctx, data, &right_ty);
            // -- set up concat(left_ptr, left_len, right_ptr, right_len) --
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("mov x3, x1");                          // move right string pointer to 3rd arg
                    emitter.instruction("mov x4, x2");                          // move right string length to 4th arg
                    abi::emit_pop_reg_pair(emitter, "x1", "x2");                // pop left string ptr/len into 1st/2nd args
                }
                Arch::X86_64 => {
                    emitter.instruction("mov rdi, rax");                        // move right string pointer to the x86_64 concat argument register
                    emitter.instruction("mov rsi, rdx");                        // move right string length to the x86_64 concat argument register
                    abi::emit_pop_reg_pair(emitter, "rax", "rdx");              // pop left string ptr/len into the x86_64 concat argument registers
                }
            }
            abi::emit_call_label(emitter, "__rt_concat");                       // call runtime to concatenate two strings
            PhpType::Str
        }
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::ShiftLeft | BinOp::ShiftRight => {
            let lt = emit_expr(left, emitter, ctx, data);
            coerce_null_to_zero(emitter, &lt);
            // -- save left operand on stack while evaluating right --
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));          // push the left integer operand onto the target temporary stack while evaluating the right operand
            let rt = emit_expr(right, emitter, ctx, data);
            coerce_null_to_zero(emitter, &rt);
            // -- pop left operand and apply bitwise operation --
            let left_reg = match emitter.target.arch {
                Arch::AArch64 => "x1",
                Arch::X86_64 => "r10",
            };
            let result_reg = abi::int_result_reg(emitter);
            abi::emit_pop_reg(emitter, left_reg);                               // pop the left integer operand into a scratch register that matches the current target ABI
            match op {
                BinOp::BitAnd => {
                    match emitter.target.arch {
                        Arch::AArch64 => {
                            emitter.instruction("and x0, x1, x0");              // bitwise AND: left & right
                        }
                        Arch::X86_64 => {
                            emitter.instruction(&format!("and {}, {}", left_reg, result_reg)); // bitwise AND: left & right in the x86_64 scratch register
                            emitter.instruction(&format!("mov {}, {}", result_reg, left_reg)); // move the bitwise-AND result back into the standard integer result register
                        }
                    }
                }
                BinOp::BitOr => {
                    match emitter.target.arch {
                        Arch::AArch64 => {
                            emitter.instruction("orr x0, x1, x0");              // bitwise OR: left | right
                        }
                        Arch::X86_64 => {
                            emitter.instruction(&format!("or {}, {}", left_reg, result_reg)); // bitwise OR: left | right in the x86_64 scratch register
                            emitter.instruction(&format!("mov {}, {}", result_reg, left_reg)); // move the bitwise-OR result back into the standard integer result register
                        }
                    }
                }
                BinOp::BitXor => {
                    match emitter.target.arch {
                        Arch::AArch64 => {
                            emitter.instruction("eor x0, x1, x0");              // bitwise XOR: left ^ right
                        }
                        Arch::X86_64 => {
                            emitter.instruction(&format!("xor {}, {}", left_reg, result_reg)); // bitwise XOR: left ^ right in the x86_64 scratch register
                            emitter.instruction(&format!("mov {}, {}", result_reg, left_reg)); // move the bitwise-XOR result back into the standard integer result register
                        }
                    }
                }
                BinOp::ShiftLeft => {
                    match emitter.target.arch {
                        Arch::AArch64 => {
                            emitter.instruction("lsl x0, x1, x0");              // left shift: left << right
                        }
                        Arch::X86_64 => {
                            emitter.instruction("mov rcx, rax");                // move the right shift amount into rcx because x86_64 variable shifts read their count from cl
                            emitter.instruction(&format!("shl {}, cl", left_reg)); // arithmetic left shift: left << right using the x86_64 variable-count form
                            emitter.instruction(&format!("mov {}, {}", result_reg, left_reg)); // move the shifted result back into the standard integer result register
                        }
                    }
                }
                BinOp::ShiftRight => {
                    match emitter.target.arch {
                        Arch::AArch64 => {
                            emitter.instruction("asr x0, x1, x0");              // arithmetic right shift: left >> right
                        }
                        Arch::X86_64 => {
                            emitter.instruction("mov rcx, rax");                // move the right shift amount into rcx because x86_64 variable shifts read their count from cl
                            emitter.instruction(&format!("sar {}, cl", left_reg)); // arithmetic right shift: left >> right using the x86_64 variable-count form
                            emitter.instruction(&format!("mov {}, {}", result_reg, left_reg)); // move the shifted result back into the standard integer result register
                        }
                    }
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
                abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter)); // push the left float operand onto the target temporary stack
            } else {
                abi::emit_push_reg(emitter, abi::int_result_reg(emitter));      // push the left integer operand onto the target temporary stack
            }
            let rt = emit_expr(right, emitter, ctx, data);
            coerce_null_to_zero(emitter, &rt);

            if lt == PhpType::Float || rt == PhpType::Float {
                // -- float spaceship comparison --
                if rt != PhpType::Float {
                    emit_promote_int_to_float(
                        emitter,
                        abi::float_result_reg(emitter),
                        abi::int_result_reg(emitter),
                    );
                }
                if lt == PhpType::Float {
                    emit_pop_left_float_for_comparison(emitter, &lt);            // pop the left float operand into the comparison scratch register for the current target
                } else {
                    emit_pop_left_float_for_comparison(emitter, &lt);            // pop the left integer operand and promote it to the comparison float scratch register
                }
                emit_float_compare(emitter);                                     // compare the left and right float operands using the target-native floating-point compare instruction
            } else {
                // -- integer spaceship comparison --
                let left_reg = match emitter.target.arch {
                    Arch::AArch64 => "x1",
                    Arch::X86_64 => "r10",
                };
                abi::emit_pop_reg(emitter, left_reg);                           // pop the left integer operand into a target-appropriate scratch register
                match emitter.target.arch {
                    Arch::AArch64 => {
                        emitter.instruction("cmp x1, x0");                      // compare left vs right integers
                    }
                    Arch::X86_64 => {
                        emitter.instruction(&format!("cmp {}, {}", left_reg, abi::int_result_reg(emitter))); // compare left vs right integers in the x86_64 integer registers
                    }
                }
            }
            // -- produce -1, 0, or 1 based on comparison flags --
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("cset x0, gt");                         // x0=1 if left > right
                    emitter.instruction("csinv x0, x0, xzr, ge");               // x0=-1 if left < right (invert zero -> all-ones)
                }
                Arch::X86_64 => {
                    let greater_label = ctx.next_label("spaceship_gt");
                    let less_label = ctx.next_label("spaceship_lt");
                    let done_label = ctx.next_label("spaceship_done");
                    if lt == PhpType::Float || rt == PhpType::Float {
                        emitter.instruction(&format!("ja {}", greater_label));   // jump to the positive result when the left float operand is greater than the right operand
                        emitter.instruction(&format!("jb {}", less_label));      // jump to the negative result when the left float operand is less than the right operand
                    } else {
                        emitter.instruction(&format!("jg {}", greater_label));   // jump to the positive result when the left integer operand is greater than the right operand
                        emitter.instruction(&format!("jl {}", less_label));      // jump to the negative result when the left integer operand is less than the right operand
                    }
                    emitter.instruction("mov rax, 0");                           // equal operands yield the neutral spaceship result 0
                    emitter.instruction(&format!("jmp {}", done_label));         // skip the positive/negative result blocks once equality has been selected
                    emitter.label(&greater_label);
                    emitter.instruction("mov rax, 1");                           // left > right yields the positive spaceship result 1
                    emitter.instruction(&format!("jmp {}", done_label));         // skip the negative result block after choosing the positive spaceship result
                    emitter.label(&less_label);
                    emitter.instruction("mov rax, -1");                          // left < right yields the negative spaceship result -1
                    emitter.label(&done_label);
                }
            }
            PhpType::Int
        }
        BinOp::NullCoalesce => {
            // Should not reach here - handled by ExprKind::NullCoalesce
            // But handle gracefully via the same mechanism
            emit_null_coalesce(left, right, emitter, ctx, data)
        }
    }
}
