use super::abi;
use super::context::Context;
use super::data_section::DataSection;
use super::emit::Emitter;
use crate::parser::ast::{BinOp, Expr, ExprKind};
use crate::types::PhpType;

/// Emits code to evaluate an expression.
/// Returns the type of the result.
/// - Strings: x1 = pointer, x2 = length
/// - Integers: x0 = value
pub fn emit_expr(
    expr: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    match &expr.kind {
        ExprKind::StringLiteral(s) => {
            let bytes = s.as_bytes();
            let (label, len) = data.add_string(bytes);
            emitter.comment(&format!("load string \"{}\"", s.escape_default()));
            emitter.instruction(&format!("adrp x1, {}@PAGE", label));
            emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", label));
            emitter.instruction(&format!("mov x2, #{}", len));
            PhpType::Str
        }
        ExprKind::IntLiteral(n) => {
            emitter.comment(&format!("load int {}", n));
            load_immediate(emitter, "x0", *n);
            PhpType::Int
        }
        ExprKind::Variable(name) => {
            let var = ctx.variables.get(name).expect("undefined variable");
            let offset = var.stack_offset;
            let ty = var.ty.clone();
            emitter.comment(&format!("load ${}", name));
            abi::emit_load(emitter, &ty, offset);
            ty
        }
        ExprKind::Negate(inner) => {
            emit_expr(inner, emitter, ctx, data);
            emitter.comment("negate");
            emitter.instruction("neg x0, x0");
            PhpType::Int
        }
        ExprKind::ArrayLiteral(elems) => emit_array_literal(elems, emitter, ctx, data),
        ExprKind::ArrayAccess { array, index } => {
            emit_array_access(array, index, emitter, ctx, data)
        }
        ExprKind::Not(inner) => {
            emit_expr(inner, emitter, ctx, data);
            emitter.comment("logical not");
            emitter.instruction("cmp x0, #0");
            emitter.instruction("cset x0, eq");
            PhpType::Int
        }
        ExprKind::PreIncrement(name) => {
            let var = ctx.variables.get(name).expect("undefined variable");
            let offset = var.stack_offset;
            emitter.comment(&format!("++${}", name));
            emitter.instruction(&format!("ldur x0, [x29, #-{}]", offset));
            emitter.instruction("add x0, x0, #1");
            emitter.instruction(&format!("stur x0, [x29, #-{}]", offset));
            PhpType::Int
        }
        ExprKind::PostIncrement(name) => {
            let var = ctx.variables.get(name).expect("undefined variable");
            let offset = var.stack_offset;
            emitter.comment(&format!("${}++", name));
            emitter.instruction(&format!("ldur x0, [x29, #-{}]", offset));
            emitter.instruction("add x1, x0, #1");
            emitter.instruction(&format!("stur x1, [x29, #-{}]", offset));
            PhpType::Int
        }
        ExprKind::PreDecrement(name) => {
            let var = ctx.variables.get(name).expect("undefined variable");
            let offset = var.stack_offset;
            emitter.comment(&format!("--${}", name));
            emitter.instruction(&format!("ldur x0, [x29, #-{}]", offset));
            emitter.instruction("sub x0, x0, #1");
            emitter.instruction(&format!("stur x0, [x29, #-{}]", offset));
            PhpType::Int
        }
        ExprKind::PostDecrement(name) => {
            let var = ctx.variables.get(name).expect("undefined variable");
            let offset = var.stack_offset;
            emitter.comment(&format!("${}--", name));
            emitter.instruction(&format!("ldur x0, [x29, #-{}]", offset));
            emitter.instruction("sub x1, x0, #1");
            emitter.instruction(&format!("stur x1, [x29, #-{}]", offset));
            PhpType::Int
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            let else_label = ctx.next_label("tern_else");
            let end_label = ctx.next_label("tern_end");
            emitter.comment("ternary");
            emit_expr(condition, emitter, ctx, data);
            emitter.instruction("cmp x0, #0");
            emitter.instruction(&format!("b.eq {}", else_label));
            let ty = emit_expr(then_expr, emitter, ctx, data);
            emitter.instruction(&format!("b {}", end_label));
            emitter.label(&else_label);
            emit_expr(else_expr, emitter, ctx, data);
            emitter.label(&end_label);
            ty
        }
        ExprKind::FunctionCall { name, args } => {
            if let Some(ty) = super::builtins::emit_builtin_call(name, args, emitter, ctx, data) {
                return ty;
            }
            emit_function_call(name, args, emitter, ctx, data)
        }
        ExprKind::BinaryOp { left, op, right } => emit_binop(left, op, right, emitter, ctx, data),
    }
}

fn emit_array_literal(
    elems: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    if elems.is_empty() {
        emitter.instruction("mov x0, #8");
        emitter.instruction("mov x1, #8");
        emitter.instruction("bl __rt_array_new");
        return PhpType::Array(Box::new(PhpType::Int));
    }

    let es = match &elems[0].kind {
        ExprKind::StringLiteral(_) => 16,
        _ => 8,
    };
    let elem_ty = if es == 16 { PhpType::Str } else { PhpType::Int };

    emitter.comment("array literal");
    emitter.instruction(&format!("mov x0, #{}", std::cmp::max(elems.len(), 8)));
    emitter.instruction(&format!("mov x1, #{}", es));
    emitter.instruction("bl __rt_array_new");
    emitter.instruction("str x0, [sp, #-16]!");

    for (i, elem) in elems.iter().enumerate() {
        let ty = emit_expr(elem, emitter, ctx, data);
        emitter.instruction("ldr x9, [sp]");
        match &ty {
            PhpType::Int => {
                emitter.instruction(&format!("str x0, [x9, #{}]", 24 + i * 8));
            }
            PhpType::Str => {
                emitter.instruction(&format!("str x1, [x9, #{}]", 24 + i * 16));
                emitter.instruction(&format!("str x2, [x9, #{}]", 24 + i * 16 + 8));
            }
            _ => {}
        }
        emitter.instruction(&format!("mov x10, #{}", i + 1));
        emitter.instruction("str x10, [x9]");
    }

    emitter.instruction("ldr x0, [sp], #16");
    PhpType::Array(Box::new(elem_ty))
}

fn emit_array_access(
    array: &Expr,
    index: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let arr_ty = emit_expr(array, emitter, ctx, data);
    emitter.instruction("str x0, [sp, #-16]!");
    emit_expr(index, emitter, ctx, data);
    emitter.instruction("ldr x9, [sp], #16");
    emitter.comment("array access");
    let elem_ty = match &arr_ty {
        PhpType::Array(t) => *t.clone(),
        _ => PhpType::Int,
    };
    match &elem_ty {
        PhpType::Int => {
            emitter.instruction("add x9, x9, #24");
            emitter.instruction("ldr x0, [x9, x0, lsl #3]");
        }
        PhpType::Str => {
            emitter.instruction("lsl x0, x0, #4");
            emitter.instruction("add x9, x9, x0");
            emitter.instruction("add x9, x9, #24");
            emitter.instruction("ldr x1, [x9]");
            emitter.instruction("ldr x2, [x9, #8]");
        }
        _ => {}
    }
    elem_ty
}

fn emit_binop(
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
            emit_expr(left, emitter, ctx, data);
            emitter.instruction("cmp x0, #0");
            emitter.instruction(&format!("b.eq {}", end_label));
            emit_expr(right, emitter, ctx, data);
            emitter.instruction("cmp x0, #0");
            emitter.instruction("cset x0, ne");
            emitter.label(&end_label);
            return PhpType::Int;
        }
        BinOp::Or => {
            let end_label = ctx.next_label("or_end");
            emit_expr(left, emitter, ctx, data);
            emitter.instruction("cmp x0, #0");
            emitter.instruction(&format!("b.ne {}", end_label));
            emit_expr(right, emitter, ctx, data);
            emitter.label(&end_label);
            emitter.instruction("cmp x0, #0");
            emitter.instruction("cset x0, ne");
            return PhpType::Int;
        }
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
            emit_expr(left, emitter, ctx, data);
            emitter.instruction("str x0, [sp, #-16]!");
            emit_expr(right, emitter, ctx, data);
            emitter.instruction("ldr x1, [sp], #16");
            match op {
                BinOp::Add => emitter.instruction("add x0, x1, x0"),
                BinOp::Sub => emitter.instruction("sub x0, x1, x0"),
                BinOp::Mul => emitter.instruction("mul x0, x1, x0"),
                BinOp::Div => emitter.instruction("sdiv x0, x1, x0"),
                BinOp::Mod => {
                    emitter.instruction("sdiv x2, x1, x0");
                    emitter.instruction("msub x0, x2, x0, x1");
                }
                _ => unreachable!(),
            }
            PhpType::Int
        }
        BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq => {
            emit_expr(left, emitter, ctx, data);
            emitter.instruction("str x0, [sp, #-16]!");
            emit_expr(right, emitter, ctx, data);
            emitter.instruction("ldr x1, [sp], #16");
            emitter.instruction("cmp x1, x0");
            let cond = match op {
                BinOp::Eq => "eq",
                BinOp::NotEq => "ne",
                BinOp::Lt => "lt",
                BinOp::Gt => "gt",
                BinOp::LtEq => "le",
                BinOp::GtEq => "ge",
                _ => unreachable!(),
            };
            emitter.instruction(&format!("cset x0, {}", cond));
            PhpType::Int
        }
        BinOp::Concat => {
            let left_ty = emit_expr(left, emitter, ctx, data);
            if left_ty == PhpType::Int {
                emitter.instruction("bl __rt_itoa");
            }
            emitter.instruction("stp x1, x2, [sp, #-16]!");
            let right_ty = emit_expr(right, emitter, ctx, data);
            if right_ty == PhpType::Int {
                emitter.instruction("bl __rt_itoa");
            }
            emitter.instruction("mov x3, x1");
            emitter.instruction("mov x4, x2");
            emitter.instruction("ldp x1, x2, [sp], #16");
            emitter.instruction("bl __rt_concat");
            PhpType::Str
        }
    }
}

fn emit_function_call(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment(&format!("call {}()", name));

    let mut arg_types = Vec::new();
    for arg in args {
        let ty = emit_expr(arg, emitter, ctx, data);
        match &ty {
            PhpType::Int | PhpType::Array(_) => {
                emitter.instruction("str x0, [sp, #-16]!");
            }
            PhpType::Str => {
                emitter.instruction("stp x1, x2, [sp, #-16]!");
            }
            PhpType::Void => {}
        }
        arg_types.push(ty);
    }

    let mut assignments: Vec<(PhpType, usize)> = Vec::new();
    let mut reg_idx = 0;
    for ty in &arg_types {
        assignments.push((ty.clone(), reg_idx));
        reg_idx += ty.register_count();
    }

    for i in (0..args.len()).rev() {
        let (ty, start_reg) = &assignments[i];
        match ty {
            PhpType::Int | PhpType::Array(_) => {
                emitter.instruction(&format!("ldr x{}, [sp], #16", start_reg));
            }
            PhpType::Str => {
                emitter.instruction(&format!(
                    "ldp x{}, x{}, [sp], #16",
                    start_reg,
                    start_reg + 1
                ));
            }
            PhpType::Void => {}
        }
    }

    emitter.instruction(&format!("bl _fn_{}", name));

    ctx.functions
        .get(name)
        .map(|sig| sig.return_type.clone())
        .unwrap_or(PhpType::Void)
}

fn load_immediate(emitter: &mut Emitter, reg: &str, value: i64) {
    if value >= 0 && value <= 65535 {
        emitter.instruction(&format!("mov {}, #{}", reg, value));
    } else if value < 0 && value >= -65536 {
        emitter.instruction(&format!("mov {}, #{}", reg, value));
    } else {
        let uval = value as u64;
        emitter.instruction(&format!("movz {}, #0x{:x}", reg, uval & 0xFFFF));
        if (uval >> 16) & 0xFFFF != 0 {
            emitter.instruction(&format!(
                "movk {}, #0x{:x}, lsl #16",
                reg,
                (uval >> 16) & 0xFFFF
            ));
        }
        if (uval >> 32) & 0xFFFF != 0 {
            emitter.instruction(&format!(
                "movk {}, #0x{:x}, lsl #32",
                reg,
                (uval >> 32) & 0xFFFF
            ));
        }
        if (uval >> 48) & 0xFFFF != 0 {
            emitter.instruction(&format!(
                "movk {}, #0x{:x}, lsl #48",
                reg,
                (uval >> 48) & 0xFFFF
            ));
        }
    }
}
