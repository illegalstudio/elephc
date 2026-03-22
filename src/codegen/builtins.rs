use super::context::Context;
use super::data_section::DataSection;
use super::emit::Emitter;
use super::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emit code for a built-in function call.
/// Returns Some(return_type) if the function is a known built-in, None otherwise.
pub fn emit_builtin_call(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    match name {
        "exit" | "die" => {
            emitter.comment("exit()");
            if let Some(arg) = args.first() {
                emit_expr(arg, emitter, ctx, data);
            } else {
                emitter.instruction("mov x0, #0");
            }
            emitter.instruction("mov x16, #1");
            emitter.instruction("svc #0x80");
            Some(PhpType::Void)
        }
        "strlen" => {
            emitter.comment("strlen()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("mov x0, x2");
            Some(PhpType::Int)
        }
        "intval" => {
            emitter.comment("intval()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            if ty == PhpType::Str {
                emitter.instruction("bl __rt_atoi");
            }
            Some(PhpType::Int)
        }
        "is_bool" => {
            emitter.comment("is_bool()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            let val = if ty == PhpType::Bool { 1 } else { 0 };
            emitter.instruction(&format!("mov x0, #{}", val));
            Some(PhpType::Bool)
        }
        "boolval" => {
            emitter.comment("boolval()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("cmp x0, #0");
            emitter.instruction("cset x0, ne");
            Some(PhpType::Bool)
        }
        "is_null" => {
            emitter.comment("is_null()");
            emit_expr(&args[0], emitter, ctx, data);
            // Compare against null sentinel
            emitter.instruction("movz x9, #0xFFFE");
            emitter.instruction("movk x9, #0xFFFF, lsl #16");
            emitter.instruction("movk x9, #0xFFFF, lsl #32");
            emitter.instruction("movk x9, #0x7FFF, lsl #48");
            emitter.instruction("cmp x0, x9");
            emitter.instruction("cset x0, eq");
            Some(PhpType::Bool)
        }
        "array_pop" => {
            emitter.comment("array_pop()");
            let arr_ty = emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("ldr x9, [x0]");
            emitter.instruction("sub x9, x9, #1");
            emitter.instruction("str x9, [x0]");
            let elem_ty = match &arr_ty {
                PhpType::Array(t) => *t.clone(),
                _ => PhpType::Int,
            };
            match &elem_ty {
                PhpType::Int => {
                    emitter.instruction("add x0, x0, #24");
                    emitter.instruction("ldr x0, [x0, x9, lsl #3]");
                }
                PhpType::Str => {
                    emitter.instruction("lsl x10, x9, #4");
                    emitter.instruction("add x0, x0, x10");
                    emitter.instruction("add x0, x0, #24");
                    emitter.instruction("ldr x1, [x0]");
                    emitter.instruction("ldr x2, [x0, #8]");
                }
                _ => {}
            }
            Some(elem_ty)
        }
        "in_array" => {
            emitter.comment("in_array()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("str x0, [sp, #-16]!");
            emit_expr(&args[1], emitter, ctx, data);
            let found_label = ctx.next_label("in_array_found");
            let end_label = ctx.next_label("in_array_end");
            let done_label = ctx.next_label("in_array_done");
            emitter.instruction("ldr x9, [x0]");
            emitter.instruction("add x10, x0, #24");
            emitter.instruction("ldr x11, [sp], #16");
            emitter.instruction("mov x12, #0");
            let loop_label = ctx.next_label("in_array_loop");
            emitter.label(&loop_label);
            emitter.instruction("cmp x12, x9");
            emitter.instruction(&format!("b.ge {}", end_label));
            emitter.instruction("ldr x13, [x10, x12, lsl #3]");
            emitter.instruction("cmp x13, x11");
            emitter.instruction(&format!("b.eq {}", found_label));
            emitter.instruction("add x12, x12, #1");
            emitter.instruction(&format!("b {}", loop_label));
            emitter.label(&found_label);
            emitter.instruction("mov x0, #1");
            emitter.instruction(&format!("b {}", done_label));
            emitter.label(&end_label);
            emitter.instruction("mov x0, #0");
            emitter.label(&done_label);
            Some(PhpType::Int)
        }
        "array_keys" => {
            emitter.comment("array_keys()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("ldr x9, [x0]");
            emitter.instruction("str x9, [sp, #-16]!");
            emitter.instruction("mov x0, x9");
            emitter.instruction("mov x1, #8");
            emitter.instruction("bl __rt_array_new");
            emitter.instruction("str x0, [sp, #-16]!");
            emitter.instruction("str xzr, [sp, #-16]!");
            let loop_label = ctx.next_label("akeys_loop");
            let end_label = ctx.next_label("akeys_end");
            emitter.label(&loop_label);
            emitter.instruction("ldr x12, [sp]");
            emitter.instruction("ldr x9, [sp, #32]");
            emitter.instruction("cmp x12, x9");
            emitter.instruction(&format!("b.ge {}", end_label));
            emitter.instruction("ldr x0, [sp, #16]");
            emitter.instruction("mov x1, x12");
            emitter.instruction("bl __rt_array_push_int");
            emitter.instruction("ldr x12, [sp]");
            emitter.instruction("add x12, x12, #1");
            emitter.instruction("str x12, [sp]");
            emitter.instruction(&format!("b {}", loop_label));
            emitter.label(&end_label);
            emitter.instruction("add sp, sp, #16");
            emitter.instruction("ldr x0, [sp], #16");
            emitter.instruction("add sp, sp, #16");
            Some(PhpType::Array(Box::new(PhpType::Int)))
        }
        "array_values" => {
            emitter.comment("array_values()");
            emit_expr(&args[0], emitter, ctx, data);
            Some(PhpType::Array(Box::new(PhpType::Int)))
        }
        "sort" => {
            emitter.comment("sort()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("bl __rt_sort_int");
            Some(PhpType::Void)
        }
        "rsort" => {
            emitter.comment("rsort()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("bl __rt_rsort_int");
            Some(PhpType::Void)
        }
        "isset" => {
            emitter.comment("isset()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("mov x0, #1");
            Some(PhpType::Int)
        }
        "count" => {
            emitter.comment("count()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("ldr x0, [x0]");
            Some(PhpType::Int)
        }
        "array_push" => {
            emitter.comment("array_push()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("str x0, [sp, #-16]!");
            let val_ty = emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("ldr x9, [sp], #16");
            match &val_ty {
                PhpType::Int => {
                    emitter.instruction("mov x1, x0");
                    emitter.instruction("mov x0, x9");
                    emitter.instruction("bl __rt_array_push_int");
                }
                PhpType::Str => {
                    emitter.instruction("mov x0, x9");
                    emitter.instruction("bl __rt_array_push_str");
                }
                _ => {}
            }
            Some(PhpType::Void)
        }
        "floatval" => {
            emitter.comment("floatval()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            match ty {
                PhpType::Float => {}
                _ => {
                    emitter.instruction("scvtf d0, x0");
                }
            }
            Some(PhpType::Float)
        }
        "abs" => {
            emitter.comment("abs()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            if ty == PhpType::Float {
                emitter.instruction("fabs d0, d0");
                Some(PhpType::Float)
            } else {
                emitter.instruction("cmp x0, #0");
                emitter.instruction("cneg x0, x0, lt");
                Some(PhpType::Int)
            }
        }
        "floor" => {
            emitter.comment("floor()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            if ty != PhpType::Float {
                emitter.instruction("scvtf d0, x0");
            }
            emitter.instruction("frintm d0, d0");
            Some(PhpType::Float)
        }
        "ceil" => {
            emitter.comment("ceil()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            if ty != PhpType::Float {
                emitter.instruction("scvtf d0, x0");
            }
            emitter.instruction("frintp d0, d0");
            Some(PhpType::Float)
        }
        "round" => {
            emitter.comment("round()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            if ty != PhpType::Float {
                emitter.instruction("scvtf d0, x0");
            }
            emitter.instruction("frinta d0, d0");
            Some(PhpType::Float)
        }
        "sqrt" => {
            emitter.comment("sqrt()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            if ty != PhpType::Float {
                emitter.instruction("scvtf d0, x0");
            }
            emitter.instruction("fsqrt d0, d0");
            Some(PhpType::Float)
        }
        "pow" => {
            emitter.comment("pow()");
            let t0 = emit_expr(&args[0], emitter, ctx, data);
            if t0 != PhpType::Float {
                emitter.instruction("scvtf d0, x0");
            }
            emitter.instruction("str d0, [sp, #-16]!");
            let t1 = emit_expr(&args[1], emitter, ctx, data);
            if t1 != PhpType::Float {
                emitter.instruction("scvtf d0, x0");
            }
            // d0 = exponent, need d1 = exponent, d0 = base
            emitter.instruction("fmov d1, d0");
            emitter.instruction("ldr d0, [sp], #16");
            emitter.instruction("bl _pow");
            Some(PhpType::Float)
        }
        "min" => {
            emitter.comment("min()");
            let t0 = emit_expr(&args[0], emitter, ctx, data);
            if t0 == PhpType::Float {
                emitter.instruction("str d0, [sp, #-16]!");
            } else {
                emitter.instruction("str x0, [sp, #-16]!");
            }
            let t1 = emit_expr(&args[1], emitter, ctx, data);
            if t0 == PhpType::Float || t1 == PhpType::Float {
                if t1 != PhpType::Float {
                    emitter.instruction("scvtf d0, x0");
                }
                if t0 == PhpType::Float {
                    emitter.instruction("ldr d1, [sp], #16");
                } else {
                    emitter.instruction("ldr x9, [sp], #16");
                    emitter.instruction("scvtf d1, x9");
                }
                emitter.instruction("fmin d0, d1, d0");
                Some(PhpType::Float)
            } else {
                emitter.instruction("ldr x1, [sp], #16");
                emitter.instruction("cmp x1, x0");
                emitter.instruction("csel x0, x1, x0, lt");
                Some(PhpType::Int)
            }
        }
        "max" => {
            emitter.comment("max()");
            let t0 = emit_expr(&args[0], emitter, ctx, data);
            if t0 == PhpType::Float {
                emitter.instruction("str d0, [sp, #-16]!");
            } else {
                emitter.instruction("str x0, [sp, #-16]!");
            }
            let t1 = emit_expr(&args[1], emitter, ctx, data);
            if t0 == PhpType::Float || t1 == PhpType::Float {
                if t1 != PhpType::Float {
                    emitter.instruction("scvtf d0, x0");
                }
                if t0 == PhpType::Float {
                    emitter.instruction("ldr d1, [sp], #16");
                } else {
                    emitter.instruction("ldr x9, [sp], #16");
                    emitter.instruction("scvtf d1, x9");
                }
                emitter.instruction("fmax d0, d1, d0");
                Some(PhpType::Float)
            } else {
                emitter.instruction("ldr x1, [sp], #16");
                emitter.instruction("cmp x1, x0");
                emitter.instruction("csel x0, x1, x0, gt");
                Some(PhpType::Int)
            }
        }
        "intdiv" => {
            emitter.comment("intdiv()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("str x0, [sp, #-16]!");
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("ldr x1, [sp], #16");
            emitter.instruction("sdiv x0, x1, x0");
            Some(PhpType::Int)
        }
        "is_float" => {
            emitter.comment("is_float()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            let val = if ty == PhpType::Float { 1 } else { 0 };
            emitter.instruction(&format!("mov x0, #{}", val));
            Some(PhpType::Bool)
        }
        "is_int" => {
            emitter.comment("is_int()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            let val = if ty == PhpType::Int { 1 } else { 0 };
            emitter.instruction(&format!("mov x0, #{}", val));
            Some(PhpType::Bool)
        }
        "is_string" => {
            emitter.comment("is_string()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            let val = if ty == PhpType::Str { 1 } else { 0 };
            emitter.instruction(&format!("mov x0, #{}", val));
            Some(PhpType::Bool)
        }
        "is_numeric" => {
            emitter.comment("is_numeric()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            let val = if matches!(ty, PhpType::Int | PhpType::Float) { 1 } else { 0 };
            emitter.instruction(&format!("mov x0, #{}", val));
            Some(PhpType::Bool)
        }
        "gettype" => {
            emitter.comment("gettype()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            let type_str = match &ty {
                PhpType::Int => "integer",
                PhpType::Float => "double",
                PhpType::Str => "string",
                PhpType::Bool => "boolean",
                PhpType::Void => "NULL",
                PhpType::Array(_) => "array",
            };
            let (label, len) = data.add_string(type_str.as_bytes());
            emitter.instruction(&format!("adrp x1, {}@PAGE", label));
            emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", label));
            emitter.instruction(&format!("mov x2, #{}", len));
            Some(PhpType::Str)
        }
        "empty" => {
            emitter.comment("empty()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            match &ty {
                PhpType::Int => {
                    super::expr::coerce_null_to_zero(emitter, &ty);
                    emitter.instruction("cmp x0, #0");
                    emitter.instruction("cset x0, eq"); // empty if 0
                }
                PhpType::Float => {
                    emitter.instruction("fcmp d0, #0.0");
                    emitter.instruction("cset x0, eq");
                }
                PhpType::Bool => {
                    // empty if false (x0 == 0)
                    emitter.instruction("cmp x0, #0");
                    emitter.instruction("cset x0, eq");
                }
                PhpType::Void => {
                    emitter.instruction("mov x0, #1"); // null is always empty
                }
                PhpType::Str => {
                    // empty if length == 0
                    emitter.instruction("cmp x2, #0");
                    emitter.instruction("cset x0, eq");
                }
                PhpType::Array(_) => {
                    // empty if count == 0
                    emitter.instruction("ldr x0, [x0]");
                    emitter.instruction("cmp x0, #0");
                    emitter.instruction("cset x0, eq");
                }
            }
            Some(PhpType::Bool)
        }
        "unset" => {
            emitter.comment("unset()");
            // Get the variable name from the expression
            if let crate::parser::ast::ExprKind::Variable(name) = &args[0].kind {
                let var = ctx.variables.get(name).expect("undefined variable");
                let offset = var.stack_offset;
                // Store null sentinel
                emitter.instruction("movz x0, #0xFFFE");
                emitter.instruction("movk x0, #0xFFFF, lsl #16");
                emitter.instruction("movk x0, #0xFFFF, lsl #32");
                emitter.instruction("movk x0, #0x7FFF, lsl #48");
                emitter.instruction(&format!("stur x0, [x29, #-{}]", offset));
                ctx.variables.get_mut(name).unwrap().ty = PhpType::Void;
            }
            Some(PhpType::Void)
        }
        "settype" => {
            emitter.comment("settype()");
            // settype($var, "type") — resolve at compile time
            if let crate::parser::ast::ExprKind::Variable(name) = &args[0].kind {
                if let crate::parser::ast::ExprKind::StringLiteral(type_name) = &args[1].kind {
                    let var = ctx.variables.get(name).expect("undefined variable");
                    let offset = var.stack_offset;
                    let old_ty = var.ty.clone();
                    // Load current value
                    super::abi::emit_load(emitter, &old_ty, offset);
                    // Convert to target type
                    let new_ty = match type_name.as_str() {
                        "int" | "integer" => {
                            match &old_ty {
                                PhpType::Float => { emitter.instruction("fcvtzs x0, d0"); }
                                PhpType::Bool | PhpType::Int => {}
                                PhpType::Void => { emitter.instruction("mov x0, #0"); }
                                _ => { emitter.instruction("mov x0, #0"); }
                            }
                            PhpType::Int
                        }
                        "float" | "double" => {
                            match &old_ty {
                                PhpType::Float => {}
                                _ => { emitter.instruction("scvtf d0, x0"); }
                            }
                            PhpType::Float
                        }
                        "string" => {
                            super::expr::coerce_to_string(emitter, &old_ty);
                            PhpType::Str
                        }
                        "bool" | "boolean" => {
                            super::expr::coerce_null_to_zero(emitter, &old_ty);
                            emitter.instruction("cmp x0, #0");
                            emitter.instruction("cset x0, ne");
                            PhpType::Bool
                        }
                        _ => old_ty.clone(),
                    };
                    super::abi::emit_store(emitter, &new_ty, offset);
                    ctx.variables.get_mut(name).unwrap().ty = new_ty;
                }
            }
            emitter.instruction("mov x0, #1"); // settype returns true
            Some(PhpType::Bool)
        }
        "is_nan" => {
            emitter.comment("is_nan()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            if ty != PhpType::Float {
                emitter.instruction("scvtf d0, x0");
            }
            // NAN is the only value where d0 != d0
            emitter.instruction("fcmp d0, d0");
            emitter.instruction("cset x0, vs"); // VS = unordered (NAN)
            Some(PhpType::Bool)
        }
        "is_infinite" => {
            emitter.comment("is_infinite()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            if ty != PhpType::Float {
                emitter.instruction("scvtf d0, x0");
            }
            // Check if abs(d0) == INF
            emitter.instruction("fabs d0, d0");
            let inf_label = data.add_float(f64::INFINITY);
            emitter.instruction(&format!("adrp x9, {}@PAGE", inf_label));
            emitter.instruction(&format!("add x9, x9, {}@PAGEOFF", inf_label));
            emitter.instruction("ldr d1, [x9]");
            emitter.instruction("fcmp d0, d1");
            emitter.instruction("cset x0, eq");
            Some(PhpType::Bool)
        }
        "is_finite" => {
            emitter.comment("is_finite()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            if ty != PhpType::Float {
                emitter.instruction("scvtf d0, x0");
            }
            // is_finite = not NAN and not INF
            // fabs, compare with INF: if ordered-less-than INF → finite
            // ARM64: for unordered (NAN), NZCV=0011, so use 'mi' (N==1) which is false for unordered
            emitter.instruction("fabs d0, d0");
            let inf_label = data.add_float(f64::INFINITY);
            emitter.instruction(&format!("adrp x9, {}@PAGE", inf_label));
            emitter.instruction(&format!("add x9, x9, {}@PAGEOFF", inf_label));
            emitter.instruction("ldr d1, [x9]");
            emitter.instruction("fcmp d0, d1");
            emitter.instruction("cset x0, mi"); // MI = N flag set = ordered less-than
            Some(PhpType::Bool)
        }
        _ => None,
    }
}
