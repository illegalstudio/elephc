use super::super::context::Context;
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use super::super::abi;
use super::{
    coerce_result_to_type, coerce_to_string, coerce_to_truthiness, emit_expr,
};
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
                    abi::emit_float_result_to_int_result(emitter);              // convert double to signed 64-bit int (toward zero)
                }
                PhpType::Bool => {}
                PhpType::Void => {
                    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
                }
                PhpType::Str => {
                    abi::emit_call_label(emitter, "__rt_atoi");                 // parse the current string result into the active integer result register
                }
                PhpType::Array(_) | PhpType::AssocArray { .. } => {
                    emitter.instruction("ldr x0, [x0]");                        // load array length from header (first field)
                }
                PhpType::Mixed | PhpType::Union(_) => {
                    abi::emit_call_label(emitter, "__rt_mixed_cast_int");       // cast the boxed mixed payload to int through the target-aware helper
                }
                PhpType::Callable
                | PhpType::Object(_)
                | PhpType::Buffer(_)
                | PhpType::Packed(_)
                | PhpType::Pointer(_) => {}
            }
            PhpType::Int
        }
        CastType::Float => {
            match &src_ty {
                PhpType::Float => {}
                PhpType::Int | PhpType::Bool => {
                    abi::emit_int_result_to_float_result(emitter);              // signed int to double conversion
                }
                PhpType::Void => {
                    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
                    abi::emit_int_result_to_float_result(emitter);              // convert to 0.0 double
                }
                PhpType::Str => {
                    abi::emit_call_label(emitter, "__rt_cstr");                 // null-terminate the current string result through the target-aware C-string helper
                    if emitter.target.arch == crate::codegen::platform::Arch::X86_64 {
                        emitter.instruction("mov rdi, rax");                    // pass the null-terminated C string in the SysV first-argument register before atof()
                    }
                    emitter.bl_c("atof");                            // parse C string as double → d0=result
                }
                PhpType::Mixed | PhpType::Union(_) => {
                    abi::emit_call_label(emitter, "__rt_mixed_cast_float");     // cast the boxed mixed payload to float through the target-aware helper
                }
                PhpType::Array(_)
                | PhpType::AssocArray { .. }
                | PhpType::Callable
                | PhpType::Object(_)
                | PhpType::Buffer(_)
                | PhpType::Packed(_)
                | PhpType::Pointer(_) => {
                    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
                    abi::emit_int_result_to_float_result(emitter);              // convert to 0.0 double
                }
            }
            PhpType::Float
        }
        CastType::String => {
            coerce_to_string(emitter, ctx, data, &src_ty);
            PhpType::Str
        }
        CastType::Bool => {
            coerce_to_truthiness(emitter, ctx, &src_ty);                        // normalize any source value to PHP truthiness using the shared target-aware helper path
            PhpType::Bool
        }
        CastType::Array => {
            match &src_ty {
                PhpType::Array(_) | PhpType::AssocArray { .. } => {
                    return src_ty;
                }
                PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Buffer(_) | PhpType::Packed(_) => {
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
                abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter)); // push the left float for later comparison through the target-aware helper
            }
            PhpType::Str => {
                let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
                abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);             // push the left string pointer/length pair for later comparison through the target-aware helper
            }
            PhpType::Mixed => {
                abi::emit_push_reg(emitter, abi::int_result_reg(emitter));      // push the left boxed mixed pointer for payload-aware strict comparison through the target-aware helper
            }
            _ => {
                abi::emit_push_reg(emitter, abi::int_result_reg(emitter));      // push the left scalar or pointer-like value for later comparison through the target-aware helper
            }
        }

        let rt = emit_expr(right, emitter, ctx, data);

        if matches!(lt, PhpType::Mixed | PhpType::Union(_))
            || matches!(rt, PhpType::Mixed | PhpType::Union(_))
        {
            let left_temp = !matches!(lt, PhpType::Mixed | PhpType::Union(_));
            let right_temp = !matches!(rt, PhpType::Mixed | PhpType::Union(_));

            match &rt {
                PhpType::Float => {
                    abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter)); // spill the right float before reloading the left operand into the same register
                }
                PhpType::Str => {
                    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
                    abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);         // spill the right string payload before reloading the left operand into the same registers
                }
                _ => {
                    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));  // spill the right scalar/pointer/mixed box before reloading the left operand into the same register
                }
            }

            match &lt {
                PhpType::Float => {
                    abi::emit_load_temporary_stack_slot(emitter, abi::float_result_reg(emitter), 16); // reload the saved left float operand from the lower comparison stack slot
                    crate::codegen::emit_box_current_value_as_mixed(emitter, &lt);  // box the left float operand so mixed comparison can inspect its runtime tag
                    match emitter.target.arch {
                        crate::codegen::platform::Arch::AArch64 => {
                            emitter.instruction("str x0, [sp, #16]");           // replace the old left comparison slot with the boxed left mixed pointer
                        }
                        crate::codegen::platform::Arch::X86_64 => {
                            emitter.instruction("mov QWORD PTR [rsp + 16], rax"); // replace the old left comparison slot with the boxed left mixed pointer
                        }
                    }
                }
                PhpType::Str => {
                    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
                    abi::emit_load_temporary_stack_slot(emitter, ptr_reg, 16);  // reload the saved left string pointer from the lower comparison stack slot
                    abi::emit_load_temporary_stack_slot(emitter, len_reg, 24);  // reload the saved left string length from the lower comparison stack slot
                    crate::codegen::emit_box_current_value_as_mixed(emitter, &lt);  // box the left string payload so mixed comparison can inspect its runtime tag
                    match emitter.target.arch {
                        crate::codegen::platform::Arch::AArch64 => {
                            emitter.instruction("str x0, [sp, #16]");           // replace the old left comparison slot with the boxed left mixed pointer
                        }
                        crate::codegen::platform::Arch::X86_64 => {
                            emitter.instruction("mov QWORD PTR [rsp + 16], rax"); // replace the old left comparison slot with the boxed left mixed pointer
                        }
                    }
                }
                _ => {
                    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 16); // reload the saved left scalar/pointer operand from the lower comparison stack slot
                    crate::codegen::emit_box_current_value_as_mixed(emitter, &lt);  // box the left operand when it is not already mixed
                    match emitter.target.arch {
                        crate::codegen::platform::Arch::AArch64 => {
                            emitter.instruction("str x0, [sp, #16]");           // replace the old left comparison slot with the boxed left mixed pointer
                        }
                        crate::codegen::platform::Arch::X86_64 => {
                            emitter.instruction("mov QWORD PTR [rsp + 16], rax"); // replace the old left comparison slot with the boxed left mixed pointer
                        }
                    }
                }
            }

            match &rt {
                PhpType::Float => {
                    abi::emit_load_temporary_stack_slot(emitter, abi::float_result_reg(emitter), 0); // restore the spilled right float operand after boxing the left operand
                }
                PhpType::Str => {
                    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
                    abi::emit_load_temporary_stack_slot(emitter, ptr_reg, 0);   // restore the spilled right string pointer after boxing the left operand
                    abi::emit_load_temporary_stack_slot(emitter, len_reg, 8);   // restore the spilled right string length after boxing the left operand
                }
                _ => {
                    abi::emit_load_temporary_stack_slot(emitter, abi::int_result_reg(emitter), 0); // restore the spilled right scalar/pointer/mixed box after boxing the left operand
                }
            }
            crate::codegen::emit_box_current_value_as_mixed(emitter, &rt);          // box the right operand when it is not already mixed
            abi::emit_reserve_temporary_stack(emitter, 32);                     // reserve scratch space for boxed operands and the boolean result
            match emitter.target.arch {
                crate::codegen::platform::Arch::AArch64 => {
                    emitter.instruction("ldr x10, [sp, #48]");                  // reload the boxed left mixed pointer from the lower saved-comparison slot
                    emitter.instruction("str x10, [sp, #0]");                   // save the left boxed mixed pointer for cleanup after the helper call
                    emitter.instruction("str x0, [sp, #8]");                    // save the right boxed mixed pointer for cleanup after the helper call
                    emitter.instruction("mov x1, x0");                          // move the right boxed mixed pointer into the second helper argument
                    emitter.instruction("mov x0, x10");                         // move the left boxed mixed pointer into the first helper argument
                    abi::emit_call_label(emitter, "__rt_mixed_strict_eq");      // compare mixed values by runtime tag and payload
                    if !is_eq {
                        emitter.instruction("eor x0, x0, #1");                  // invert the helper result for strict inequality
                    }
                    emitter.instruction("str x0, [sp, #16]");                   // preserve the boolean comparison result across decref cleanup
                }
                crate::codegen::platform::Arch::X86_64 => {
                    emitter.instruction("mov r10, QWORD PTR [rsp + 48]");       // reload the boxed left mixed pointer from the lower saved-comparison slot
                    emitter.instruction("mov QWORD PTR [rsp], r10");            // save the left boxed mixed pointer for cleanup after the helper call
                    emitter.instruction("mov QWORD PTR [rsp + 8], rax");        // save the right boxed mixed pointer for cleanup after the helper call
                    emitter.instruction("mov rsi, rax");                        // move the right boxed mixed pointer into the second helper argument register
                    emitter.instruction("mov rdi, r10");                        // move the left boxed mixed pointer into the first helper argument register
                    abi::emit_call_label(emitter, "__rt_mixed_strict_eq");      // compare mixed values by runtime tag and payload
                    if !is_eq {
                        emitter.instruction("xor rax, 1");                      // invert the helper result for strict inequality
                    }
                    emitter.instruction("mov QWORD PTR [rsp + 16], rax");       // preserve the boolean comparison result across decref cleanup
                }
            }
            if left_temp {
                match emitter.target.arch {
                    crate::codegen::platform::Arch::AArch64 => {
                        emitter.instruction("ldr x0, [sp, #0]");                // reload the temporary left mixed box for cleanup
                    }
                    crate::codegen::platform::Arch::X86_64 => {
                        emitter.instruction("mov rax, QWORD PTR [rsp]");        // reload the temporary left mixed box for cleanup
                    }
                }
                abi::emit_call_label(emitter, "__rt_decref_mixed");             // release the temporary left mixed box created for comparison
            }
            if right_temp {
                match emitter.target.arch {
                    crate::codegen::platform::Arch::AArch64 => {
                        emitter.instruction("ldr x0, [sp, #8]");                // reload the temporary right mixed box for cleanup
                    }
                    crate::codegen::platform::Arch::X86_64 => {
                        emitter.instruction("mov rax, QWORD PTR [rsp + 8]");    // reload the temporary right mixed box for cleanup
                    }
                }
                abi::emit_call_label(emitter, "__rt_decref_mixed");             // release the temporary right mixed box created for comparison
            }
            match emitter.target.arch {
                crate::codegen::platform::Arch::AArch64 => {
                    emitter.instruction("ldr x0, [sp, #16]");                   // restore the boolean comparison result after cleanup
                }
                crate::codegen::platform::Arch::X86_64 => {
                    emitter.instruction("mov rax, QWORD PTR [rsp + 16]");       // restore the boolean comparison result after cleanup
                }
            }
            abi::emit_release_temporary_stack(emitter, 64);                     // release the boxed-operand scratch space plus the two comparison spill slots
            return PhpType::Bool;
        }

        if lt != rt
            && !matches!(
                (&lt, &rt),
                (PhpType::Pointer(_), PhpType::Pointer(_))
                    | (PhpType::Buffer(_), PhpType::Buffer(_))
            )
        {
            abi::emit_release_temporary_stack(emitter, 16);                     // discard the saved left operand from the temporary comparison stack
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), if is_eq { 0 } else { 1 }); // === yields false and !== yields true when the codegen types cannot match
            return PhpType::Bool;
        }

        match &lt {
            PhpType::Int | PhpType::Bool | PhpType::Void => {
                let left_reg = abi::symbol_scratch_reg(emitter);
                abi::emit_pop_reg(emitter, left_reg);                           // pop the saved left scalar or pointer-like value from the temporary comparison stack
                emitter.instruction(&format!("cmp {}, {}", left_reg, abi::int_result_reg(emitter))); // compare the left and right scalar values
                match emitter.target.arch {
                    crate::codegen::platform::Arch::AArch64 => {
                        emitter.instruction(&format!("cset x0, {}", if is_eq { "eq" } else { "ne" })); // materialize the scalar strict-comparison result on AArch64
                    }
                    crate::codegen::platform::Arch::X86_64 => {
                        emitter.instruction(&format!("set{} al", if is_eq { "e" } else { "ne" })); // materialize the scalar strict-comparison result in the low result byte on x86_64
                        emitter.instruction("movzx rax, al");                   // widen the x86_64 comparison byte back into the full integer result register
                    }
                }
            }
            PhpType::Float => {
                match emitter.target.arch {
                    crate::codegen::platform::Arch::AArch64 => {
                        abi::emit_pop_float_reg(emitter, "d1");                 // pop the saved left float operand from the temporary comparison stack
                        emitter.instruction("fcmp d1, d0");                     // compare the two doubles, setting NZCV flags
                        emitter.instruction(&format!("cset x0, {}", if is_eq { "eq" } else { "ne" })); // materialize the floating-point strict-comparison result on AArch64
                    }
                    crate::codegen::platform::Arch::X86_64 => {
                        abi::emit_pop_float_reg(emitter, "xmm1");               // pop the saved left float operand from the temporary comparison stack
                        emitter.instruction("ucomisd xmm1, xmm0");              // compare the two doubles in the native x86_64 floating-point registers
                        emitter.instruction(&format!("set{} al", if is_eq { "e" } else { "ne" })); // materialize the floating-point strict-comparison result in the low result byte on x86_64
                        emitter.instruction("movzx rax, al");                   // widen the x86_64 comparison byte back into the full integer result register
                    }
                }
            }
            PhpType::Str => {
                match emitter.target.arch {
                    crate::codegen::platform::Arch::AArch64 => {
                        emitter.instruction("mov x3, x1");                      // move the right string pointer into the third runtime argument register
                        emitter.instruction("mov x4, x2");                      // move the right string length into the fourth runtime argument register
                        abi::emit_pop_reg_pair(emitter, "x1", "x2");            // pop the left string pointer/length pair into the first runtime argument registers
                        abi::emit_call_label(emitter, "__rt_str_eq");           // compare the two strings byte-by-byte through the shared runtime helper
                        if !is_eq {
                            emitter.instruction("eor x0, x0, #1");              // invert the string equality result for strict inequality on AArch64
                        }
                    }
                    crate::codegen::platform::Arch::X86_64 => {
                        emitter.instruction("mov rcx, rdx");                    // move the right string length into the fourth SysV integer argument register
                        emitter.instruction("mov rdx, rax");                    // move the right string pointer into the third SysV integer argument register
                        abi::emit_pop_reg_pair(emitter, "rdi", "rsi");          // pop the left string pointer/length pair into the first two SysV integer argument registers
                        abi::emit_call_label(emitter, "__rt_str_eq");           // compare the two strings byte-by-byte through the shared runtime helper
                        if !is_eq {
                            emitter.instruction("xor rax, 1");                  // invert the string equality result for strict inequality on x86_64
                        }
                    }
                }
            }
            PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Callable
            | PhpType::Object(_)
            | PhpType::Buffer(_)
            | PhpType::Packed(_)
            | PhpType::Pointer(_) => {
                let left_reg = abi::symbol_scratch_reg(emitter);
                abi::emit_pop_reg(emitter, left_reg);                           // pop the saved left array/callable/object pointer from the temporary comparison stack
                emitter.instruction(&format!("cmp {}, {}", left_reg, abi::int_result_reg(emitter))); // compare the two pointers for reference equality
                match emitter.target.arch {
                    crate::codegen::platform::Arch::AArch64 => {
                        emitter.instruction(&format!("cset x0, {}", if is_eq { "eq" } else { "ne" })); // materialize the pointer strict-comparison result on AArch64
                    }
                    crate::codegen::platform::Arch::X86_64 => {
                        emitter.instruction(&format!("set{} al", if is_eq { "e" } else { "ne" })); // materialize the pointer strict-comparison result in the low result byte on x86_64
                        emitter.instruction("movzx rax, al");                   // widen the x86_64 comparison byte back into the full integer result register
                    }
                }
            }
            PhpType::Mixed | PhpType::Union(_) => {
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
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), if is_eq { 0 } else { 1 }); // === always false and !== always true when the codegen types can never match
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
    if matches!(val_ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // save the boxed mixed/union value across the null check and fallback evaluation
        abi::emit_call_label(emitter, "__rt_mixed_unbox");                      // inspect the boxed payload tag before deciding whether ?? should fall back
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction("cmp x0, #8");                              // runtime tag 8 = null
                emitter.instruction(&format!("b.ne {}", use_value_label));      // non-null mixed payload keeps the original boxed value
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction("cmp rax, 8");                              // runtime tag 8 = null
                emitter.instruction(&format!("jne {}", use_value_label));       // non-null mixed payload keeps the original boxed value
            }
        }
    } else {
        let null_reg = abi::symbol_scratch_reg(emitter);
        abi::emit_load_int_immediate(emitter, null_reg, 0x7fff_ffff_ffff_fffe_u64 as i64); // materialize the shared null sentinel for the direct null test
        if val_ty == PhpType::Float {
            match emitter.target.arch {
                crate::codegen::platform::Arch::AArch64 => {
                    emitter.instruction("fmov x0, d0");                         // copy float bits into x0 for the null-sentinel check on AArch64
                }
                crate::codegen::platform::Arch::X86_64 => {
                    emitter.instruction("movq rax, xmm0");                      // copy float bits into rax for the null-sentinel check on x86_64
                }
            }
        }
        let cmp_reg = if val_ty == PhpType::Str { abi::string_result_regs(emitter).0 } else { abi::int_result_reg(emitter) };
        emitter.instruction(&format!("cmp {}, {}", cmp_reg, null_reg));         // compare value against the null sentinel
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction(&format!("b.ne {}", use_value_label));      // if not null, skip default branch and keep value
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction(&format!("jne {}", use_value_label));       // if not null, skip default branch and keep value
            }
        }
    }

    let default_runtime_ty = emit_expr(default, emitter, ctx, data);
    coerce_result_to_type(emitter, ctx, data, &default_runtime_ty, &result_ty);
    if matches!(val_ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_release_temporary_stack(emitter, 16);                         // discard the saved original boxed mixed/union value on the null fallback path
    }
    abi::emit_jump(emitter, &end_label);                                        // skip the non-null branch after evaluating the default expression
    emitter.label(&use_value_label);
    if matches!(val_ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));               // restore the original boxed mixed/union payload for the keep-left branch
    }
    coerce_result_to_type(emitter, ctx, data, &val_ty, &result_ty);
    emitter.label(&end_label);

    result_ty
}
