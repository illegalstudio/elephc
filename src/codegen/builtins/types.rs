use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    match name {
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
            emitter.instruction("movz x9, #0xFFFE");
            emitter.instruction("movk x9, #0xFFFF, lsl #16");
            emitter.instruction("movk x9, #0xFFFF, lsl #32");
            emitter.instruction("movk x9, #0x7FFF, lsl #48");
            emitter.instruction("cmp x0, x9");
            emitter.instruction("cset x0, eq");
            Some(PhpType::Bool)
        }
        "floatval" => {
            emitter.comment("floatval()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            if ty != PhpType::Float {
                emitter.instruction("scvtf d0, x0");
            }
            Some(PhpType::Float)
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
        "is_nan" => {
            emitter.comment("is_nan()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            if ty != PhpType::Float { emitter.instruction("scvtf d0, x0"); }
            emitter.instruction("fcmp d0, d0");
            emitter.instruction("cset x0, vs");
            Some(PhpType::Bool)
        }
        "is_infinite" => {
            emitter.comment("is_infinite()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            if ty != PhpType::Float { emitter.instruction("scvtf d0, x0"); }
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
            if ty != PhpType::Float { emitter.instruction("scvtf d0, x0"); }
            emitter.instruction("fabs d0, d0");
            let inf_label = data.add_float(f64::INFINITY);
            emitter.instruction(&format!("adrp x9, {}@PAGE", inf_label));
            emitter.instruction(&format!("add x9, x9, {}@PAGEOFF", inf_label));
            emitter.instruction("ldr d1, [x9]");
            emitter.instruction("fcmp d0, d1");
            emitter.instruction("cset x0, mi");
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
                    crate::codegen::expr::coerce_null_to_zero(emitter, &ty);
                    emitter.instruction("cmp x0, #0");
                    emitter.instruction("cset x0, eq");
                }
                PhpType::Float => {
                    emitter.instruction("fcmp d0, #0.0");
                    emitter.instruction("cset x0, eq");
                }
                PhpType::Bool => {
                    emitter.instruction("cmp x0, #0");
                    emitter.instruction("cset x0, eq");
                }
                PhpType::Void => {
                    emitter.instruction("mov x0, #1");
                }
                PhpType::Str => {
                    emitter.instruction("cmp x2, #0");
                    emitter.instruction("cset x0, eq");
                }
                PhpType::Array(_) => {
                    emitter.instruction("ldr x0, [x0]");
                    emitter.instruction("cmp x0, #0");
                    emitter.instruction("cset x0, eq");
                }
            }
            Some(PhpType::Bool)
        }
        "unset" => {
            emitter.comment("unset()");
            if let crate::parser::ast::ExprKind::Variable(name) = &args[0].kind {
                let var = ctx.variables.get(name).expect("undefined variable");
                let offset = var.stack_offset;
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
            if let crate::parser::ast::ExprKind::Variable(vname) = &args[0].kind {
                if let crate::parser::ast::ExprKind::StringLiteral(type_name) = &args[1].kind {
                    let var = ctx.variables.get(vname).expect("undefined variable");
                    let offset = var.stack_offset;
                    let old_ty = var.ty.clone();
                    crate::codegen::abi::emit_load(emitter, &old_ty, offset);
                    let new_ty = match type_name.as_str() {
                        "int" | "integer" => {
                            match &old_ty {
                                PhpType::Float => { emitter.instruction("fcvtzs x0, d0"); }
                                PhpType::Bool | PhpType::Int => {}
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
                            crate::codegen::expr::coerce_to_string(emitter, &old_ty);
                            PhpType::Str
                        }
                        "bool" | "boolean" => {
                            crate::codegen::expr::coerce_null_to_zero(emitter, &old_ty);
                            emitter.instruction("cmp x0, #0");
                            emitter.instruction("cset x0, ne");
                            PhpType::Bool
                        }
                        _ => old_ty.clone(),
                    };
                    crate::codegen::abi::emit_store(emitter, &new_ty, offset);
                    ctx.variables.get_mut(vname).unwrap().ty = new_ty;
                }
            }
            emitter.instruction("mov x0, #1");
            Some(PhpType::Bool)
        }
        _ => None,
    }
}
