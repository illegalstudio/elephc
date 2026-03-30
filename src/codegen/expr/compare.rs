use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use super::{coerce_null_to_zero, coerce_result_to_type, coerce_to_string, emit_expr};
use super::{widen_codegen_type, BinOp, Expr, ExprKind, PhpType};

pub(super) fn emit_cast(
    target: &crate::parser::ast::CastType,
    expr: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    use crate::parser::ast::CastType;
    let src_ty = emit_expr(expr, emitter, ctx, data);
    emitter.comment(&format!("cast to {:?}", target));
    match target {
        CastType::Int => {
            match &src_ty {
                PhpType::Int => {}
                PhpType::Float => {
                    emitter.instruction("fcvtzs x0, d0");                       // convert double to signed 64-bit int (toward zero)
                }
                PhpType::Bool => {}
                PhpType::Void => {
                    emitter.instruction("mov x0, #0");                          // null casts to integer zero
                }
                PhpType::Str => {
                    emitter.instruction("bl __rt_atoi");                        // runtime: ASCII string to integer conversion
                }
                PhpType::Array(_) | PhpType::AssocArray { .. } => {
                    emitter.instruction("ldr x0, [x0]");                        // load array length from header (first field)
                }
                PhpType::Mixed | PhpType::Callable | PhpType::Object(_) | PhpType::Pointer(_) => {}
            }
            PhpType::Int
        }
        CastType::Float => {
            match &src_ty {
                PhpType::Float => {}
                PhpType::Int | PhpType::Bool => {
                    emitter.instruction("scvtf d0, x0");                        // signed int to double conversion
                }
                PhpType::Void => {
                    emitter.instruction("mov x0, #0");                          // load zero integer
                    emitter.instruction("scvtf d0, x0");                        // convert to 0.0 double
                }
                PhpType::Str => {
                    emitter.instruction("bl __rt_cstr");                        // null-terminate string (x1=ptr, x2=len → x0=cstr)
                    emitter.instruction("bl _atof");                            // parse C string as double → d0=result
                }
                PhpType::Array(_)
                | PhpType::AssocArray { .. }
                | PhpType::Mixed
                | PhpType::Callable
                | PhpType::Object(_)
                | PhpType::Pointer(_) => {
                    emitter.instruction("mov x0, #0");                          // load zero integer
                    emitter.instruction("scvtf d0, x0");                        // convert to 0.0 double
                }
            }
            PhpType::Float
        }
        CastType::String => {
            coerce_to_string(emitter, &src_ty);
            PhpType::Str
        }
        CastType::Bool => {
            match &src_ty {
                PhpType::Bool => {}
                PhpType::Int | PhpType::Void => {
                    coerce_null_to_zero(emitter, &src_ty);
                    emitter.instruction("cmp x0, #0");                          // test if value is zero
                    emitter.instruction("cset x0, ne");                         // x0=1 if nonzero (truthy), 0 if zero (falsy)
                }
                PhpType::Float => {
                    emitter.instruction("fcmp d0, #0.0");                       // compare float against zero
                    emitter.instruction("cset x0, ne");                         // x0=1 if nonzero, 0 if zero
                }
                PhpType::Str => {
                    emitter.instruction("cmp x2, #0");                          // check if string length is zero
                    emitter.instruction("cset x0, ne");                         // x0=1 if non-empty, 0 if empty
                }
                PhpType::Array(_) | PhpType::AssocArray { .. } => {
                    emitter.instruction("ldr x0, [x0]");                        // load array length from header
                    emitter.instruction("cmp x0, #0");                          // check if array is empty
                    emitter.instruction("cset x0, ne");                         // x0=1 if non-empty, 0 if empty
                }
                PhpType::Mixed | PhpType::Callable | PhpType::Object(_) | PhpType::Pointer(_) => {
                    emitter.instruction("cmp x0, #0");                          // test if callable/object address is zero
                    emitter.instruction("cset x0, ne");                         // x0=1 if nonzero (truthy)
                }
            }
            PhpType::Bool
        }
        CastType::Array => {
            match &src_ty {
                PhpType::Array(_) | PhpType::AssocArray { .. } => {
                    return src_ty;
                }
                PhpType::Int | PhpType::Bool | PhpType::Callable => {
                    emitter.instruction("str x0, [sp, #-16]!");                 // save scalar value during allocation
                    emitter.instruction("mov x0, #1");                          // capacity: 1 element (exact fit)
                    emitter.instruction("mov x1, #8");                          // element size: 8 bytes
                    emitter.instruction("bl __rt_array_new");                   // allocate new array struct
                    emitter.instruction("ldr x1, [sp], #16");                   // pop saved scalar value
                    emitter.instruction("bl __rt_array_push_int");              // push scalar as first element
                }
                _ => {
                    emitter.instruction("mov x0, #4");                          // capacity: 4 (grows dynamically)
                    emitter.instruction("mov x1, #8");                          // element size: 8 bytes
                    emitter.instruction("bl __rt_array_new");                   // allocate empty array struct
                }
            }
            PhpType::Array(Box::new(PhpType::Int))
        }
    }
}

pub(super) fn emit_strict_compare(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let is_eq = *op == BinOp::StrictEq;
    emitter.comment(if is_eq { "===" } else { "!==" });

    let lt_peek = peek_expr_type(left, ctx);
    let rt_peek = peek_expr_type(right, ctx);

    let types_match = match (&lt_peek, &rt_peek) {
        (Some(PhpType::Pointer(_)), Some(PhpType::Pointer(_))) => true,
        (Some(l), Some(r)) => l == r,
        _ => true,
    };

    let lt = emit_expr(left, emitter, ctx, data);

    if types_match {
        match &lt {
            PhpType::Float => {
                emitter.instruction("str d0, [sp, #-16]!");                     // push left float for later comparison
            }
            PhpType::Str => {
                emitter.instruction("stp x1, x2, [sp, #-16]!");                 // push left string ptr+len for comparison
            }
            PhpType::Mixed => {
                emitter.instruction("str x0, [sp, #-16]!");                     // push left boxed mixed pointer for payload-aware strict comparison
            }
            _ => {
                emitter.instruction("str x0, [sp, #-16]!");                     // push left int/bool/null for comparison
            }
        }

        let rt = emit_expr(right, emitter, ctx, data);

        if matches!(lt, PhpType::Mixed) || matches!(rt, PhpType::Mixed) {
            let left_temp = !matches!(lt, PhpType::Mixed);
            let right_temp = !matches!(rt, PhpType::Mixed);

            match &rt {
                PhpType::Float => {
                    emitter.instruction("str d0, [sp, #-16]!");                     // spill the right float before reloading the left operand into the same register
                }
                PhpType::Str => {
                    emitter.instruction("stp x1, x2, [sp, #-16]!");                 // spill the right string payload before reloading the left operand into x1/x2
                }
                _ => {
                    emitter.instruction("str x0, [sp, #-16]!");                     // spill the right scalar/pointer/mixed box before reloading the left operand into x0
                }
            }

            match &lt {
                PhpType::Float => {
                    emitter.instruction("ldr d0, [sp, #16]");                       // reload the saved left float operand from the lower comparison stack slot
                    crate::codegen::emit_box_current_value_as_mixed(emitter, &lt);  // box the left float operand so mixed comparison can inspect its runtime tag
                    emitter.instruction("str x0, [sp, #16]");                       // replace the old left comparison slot with the boxed left mixed pointer
                }
                PhpType::Str => {
                    emitter.instruction("ldp x1, x2, [sp, #16]");                   // reload the saved left string payload from the lower comparison stack slot
                    crate::codegen::emit_box_current_value_as_mixed(emitter, &lt);  // box the left string payload so mixed comparison can inspect its runtime tag
                    emitter.instruction("str x0, [sp, #16]");                       // replace the old left comparison slot with the boxed left mixed pointer
                }
                _ => {
                    emitter.instruction("ldr x0, [sp, #16]");                       // reload the saved left scalar/pointer operand from the lower comparison stack slot
                    crate::codegen::emit_box_current_value_as_mixed(emitter, &lt);  // box the left operand when it is not already mixed
                    emitter.instruction("str x0, [sp, #16]");                       // replace the old left comparison slot with the boxed left mixed pointer
                }
            }

            match &rt {
                PhpType::Float => {
                    emitter.instruction("ldr d0, [sp]");                            // restore the spilled right float operand after boxing the left operand
                }
                PhpType::Str => {
                    emitter.instruction("ldp x1, x2, [sp]");                        // restore the spilled right string payload after boxing the left operand
                }
                _ => {
                    emitter.instruction("ldr x0, [sp]");                            // restore the spilled right scalar/pointer/mixed box after boxing the left operand
                }
            }
            crate::codegen::emit_box_current_value_as_mixed(emitter, &rt);          // box the right operand when it is not already mixed
            emitter.instruction("sub sp, sp, #32");                                 // reserve scratch space for boxed operands and the boolean result
            emitter.instruction("ldr x10, [sp, #48]");                              // reload the boxed left mixed pointer from the lower saved-comparison slot
            emitter.instruction("str x10, [sp, #0]");                               // save the left boxed mixed pointer for cleanup after the helper call
            emitter.instruction("str x0, [sp, #8]");                                // save the right boxed mixed pointer for cleanup after the helper call
            emitter.instruction("mov x1, x0");                                      // move the right boxed mixed pointer into the second helper argument
            emitter.instruction("mov x0, x10");                                     // move the left boxed mixed pointer into the first helper argument
            emitter.instruction("bl __rt_mixed_strict_eq");                         // compare mixed values by runtime tag and payload
            if !is_eq {
                emitter.instruction("eor x0, x0, #1");                              // invert the helper result for strict inequality
            }
            emitter.instruction("str x0, [sp, #16]");                               // preserve the boolean comparison result across decref cleanup
            if left_temp {
                emitter.instruction("ldr x0, [sp, #0]");                            // reload the temporary left mixed box for cleanup
                emitter.instruction("bl __rt_decref_mixed");                        // release the temporary left mixed box created for comparison
            }
            if right_temp {
                emitter.instruction("ldr x0, [sp, #8]");                            // reload the temporary right mixed box for cleanup
                emitter.instruction("bl __rt_decref_mixed");                        // release the temporary right mixed box created for comparison
            }
            emitter.instruction("ldr x0, [sp, #16]");                               // restore the boolean comparison result after cleanup
            emitter.instruction("add sp, sp, #64");                                 // release the boxed-operand scratch space plus the two comparison spill slots
            return PhpType::Bool;
        }

        if lt != rt && !matches!((&lt, &rt), (PhpType::Pointer(_), PhpType::Pointer(_))) {
            emitter.instruction("add sp, sp, #16");                             // discard saved left operand from stack
            emitter.instruction(&format!("mov x0, #{}", if is_eq { 0 } else { 1 })); // === yields false, !== yields true
            return PhpType::Bool;
        }

        match &lt {
            PhpType::Int | PhpType::Bool | PhpType::Void => {
                emitter.instruction("ldr x1, [sp], #16");                       // pop saved left operand from stack
                emitter.instruction("cmp x1, x0");                              // compare left vs right values
                let cond = if is_eq { "eq" } else { "ne" };
                emitter.instruction(&format!("cset x0, {}", cond));             // set boolean result from comparison
            }
            PhpType::Float => {
                emitter.instruction("ldr d1, [sp], #16");                       // pop saved left float operand
                emitter.instruction("fcmp d1, d0");                             // compare two doubles, setting NZCV flags
                let cond = if is_eq { "eq" } else { "ne" };
                emitter.instruction(&format!("cset x0, {}", cond));             // set boolean result from float comparison
            }
            PhpType::Str => {
                emitter.instruction("mov x3, x1");                              // move right string pointer to 3rd arg
                emitter.instruction("mov x4, x2");                              // move right string length to 4th arg
                emitter.instruction("ldp x1, x2, [sp], #16");                   // pop left string ptr/len into 1st/2nd args
                emitter.instruction("bl __rt_str_eq");                          // runtime: byte-by-byte string comparison
                if !is_eq {
                    emitter.instruction("eor x0, x0, #1");                      // invert result for !== (XOR with 1)
                }
            }
            PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Callable
            | PhpType::Object(_)
            | PhpType::Pointer(_) => {
                emitter.instruction("ldr x1, [sp], #16");                       // pop saved left array/callable/object pointer
                emitter.instruction("cmp x1, x0");                              // compare pointers (reference equality)
                let cond = if is_eq { "eq" } else { "ne" };
                emitter.instruction(&format!("cset x0, {}", cond));             // set boolean result from pointer comparison
            }
            PhpType::Mixed => {
                emitter.instruction("ldr x1, [sp], #16");                       // pop the saved left boxed mixed pointer into the second helper argument
                emitter.instruction("mov x9, x0");                              // preserve the right boxed mixed pointer across the register shuffle
                emitter.instruction("mov x0, x1");                              // move the left boxed mixed pointer into the first helper argument
                emitter.instruction("mov x1, x9");                              // move the right boxed mixed pointer into the second helper argument
                emitter.instruction("bl __rt_mixed_strict_eq");                 // compare mixed values by runtime tag and payload instead of box identity
                if !is_eq {
                    emitter.instruction("eor x0, x0, #1");                      // invert the helper result for strict inequality
                }
            }
        }
    } else {
        emit_expr(right, emitter, ctx, data);
        emitter.instruction(&format!("mov x0, #{}", if is_eq { 0 } else { 1 })); // === always false, !== always true
    }

    PhpType::Bool
}

pub(super) fn peek_expr_type(expr: &Expr, ctx: &Context) -> Option<PhpType> {
    match &expr.kind {
        ExprKind::IntLiteral(_) => Some(PhpType::Int),
        ExprKind::FloatLiteral(_) => Some(PhpType::Float),
        ExprKind::StringLiteral(_) => Some(PhpType::Str),
        ExprKind::BoolLiteral(_) => Some(PhpType::Bool),
        ExprKind::Null => Some(PhpType::Void),
        ExprKind::Variable(name) => ctx.variables.get(name).map(|v| v.ty.clone()),
        _ => None,
    }
}

pub(super) fn emit_null_coalesce(
    value: &Expr,
    default: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("null coalesce ??");
    let val_ty = emit_expr(value, emitter, ctx, data);

    if val_ty == PhpType::Void {
        return emit_expr(default, emitter, ctx, data);
    }

    let default_ty = super::super::functions::infer_contextual_type(default, ctx);
    let result_ty = widen_codegen_type(&val_ty, &default_ty);

    let use_value_label = ctx.next_label("nc_keep");
    let end_label = ctx.next_label("nc_end");
    emitter.instruction("movz x9, #0xFFFE");                                    // load lowest 16 bits of null sentinel
    emitter.instruction("movk x9, #0xFFFF, lsl #16");                           // insert bits 16-31 of null sentinel
    emitter.instruction("movk x9, #0xFFFF, lsl #32");                           // insert bits 32-47 of null sentinel
    emitter.instruction("movk x9, #0x7FFF, lsl #48");                           // insert bits 48-63 of null sentinel
    if val_ty == PhpType::Float {
        emitter.instruction("fmov x0, d0");                                     // copy float bits to x0 for null sentinel check
    }
    let cmp_reg = if val_ty == PhpType::Str { "x1" } else { "x0" };
    emitter.instruction(&format!("cmp {}, x9", cmp_reg));                       // compare value against null sentinel
    emitter.instruction(&format!("b.ne {}", use_value_label));                  // if not null, skip default branch and keep value

    let default_runtime_ty = emit_expr(default, emitter, ctx, data);
    coerce_result_to_type(emitter, &default_runtime_ty, &result_ty);
    emitter.instruction(&format!("b {}", end_label));                           // skip non-null branch after evaluating default
    emitter.label(&use_value_label);
    coerce_result_to_type(emitter, &val_ty, &result_ty);
    emitter.label(&end_label);

    result_ty
}
