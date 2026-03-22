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
            // -- return true/false based on compile-time type --
            let val = if ty == PhpType::Bool { 1 } else { 0 };
            emitter.instruction(&format!("mov x0, #{}", val));                  // set result: 1 if bool, 0 otherwise
            Some(PhpType::Bool)
        }
        "boolval" => {
            emitter.comment("boolval()");
            // -- convert any value to boolean (truthy/falsy) --
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("cmp x0, #0");                                  // compare value against zero
            emitter.instruction("cset x0, ne");                                 // x0 = 1 if nonzero (truthy), 0 if zero
            Some(PhpType::Bool)
        }
        "is_null" => {
            emitter.comment("is_null()");
            // -- check if value equals the null sentinel (0x7FFFFFFFFFFFFFFFE) --
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("movz x9, #0xFFFE");                            // load null sentinel bits [15:0]
            emitter.instruction("movk x9, #0xFFFF, lsl #16");                   // load null sentinel bits [31:16]
            emitter.instruction("movk x9, #0xFFFF, lsl #32");                   // load null sentinel bits [47:32]
            emitter.instruction("movk x9, #0x7FFF, lsl #48");                   // load null sentinel bits [63:48]
            emitter.instruction("cmp x0, x9");                                  // compare value against null sentinel
            emitter.instruction("cset x0, eq");                                 // x0 = 1 if value is null, 0 otherwise
            Some(PhpType::Bool)
        }
        "floatval" => {
            emitter.comment("floatval()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            if ty != PhpType::Float {
                // -- convert integer to double-precision float --
                emitter.instruction("scvtf d0, x0");                            // convert signed int x0 to float d0
            }
            Some(PhpType::Float)
        }
        "is_float" => {
            emitter.comment("is_float()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            // -- return true/false based on compile-time type --
            let val = if ty == PhpType::Float { 1 } else { 0 };
            emitter.instruction(&format!("mov x0, #{}", val));                  // set result: 1 if float, 0 otherwise
            Some(PhpType::Bool)
        }
        "is_int" => {
            emitter.comment("is_int()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            // -- return true/false based on compile-time type --
            let val = if ty == PhpType::Int { 1 } else { 0 };
            emitter.instruction(&format!("mov x0, #{}", val));                  // set result: 1 if int, 0 otherwise
            Some(PhpType::Bool)
        }
        "is_string" => {
            emitter.comment("is_string()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            // -- return true/false based on compile-time type --
            let val = if ty == PhpType::Str { 1 } else { 0 };
            emitter.instruction(&format!("mov x0, #{}", val));                  // set result: 1 if string, 0 otherwise
            Some(PhpType::Bool)
        }
        "is_numeric" => {
            emitter.comment("is_numeric()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            // -- return true if type is int or float --
            let val = if matches!(ty, PhpType::Int | PhpType::Float) { 1 } else { 0 };
            emitter.instruction(&format!("mov x0, #{}", val));                  // set result: 1 if numeric, 0 otherwise
            Some(PhpType::Bool)
        }
        "is_nan" => {
            emitter.comment("is_nan()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            // -- NaN is the only value that does not equal itself --
            if ty != PhpType::Float { emitter.instruction("scvtf d0, x0"); }    // convert int to float if needed
            emitter.instruction("fcmp d0, d0");                                 // compare float with itself (NaN != NaN)
            emitter.instruction("cset x0, vs");                                 // x0 = 1 if unordered (NaN), 0 otherwise
            Some(PhpType::Bool)
        }
        "is_infinite" => {
            emitter.comment("is_infinite()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            // -- check if |value| equals infinity --
            if ty != PhpType::Float { emitter.instruction("scvtf d0, x0"); }    // convert int to float if needed
            emitter.instruction("fabs d0, d0");                                 // take absolute value (catches +/- inf)
            let inf_label = data.add_float(f64::INFINITY);
            emitter.instruction(&format!("adrp x9, {}@PAGE", inf_label));       // load page address of infinity constant
            emitter.instruction(&format!("add x9, x9, {}@PAGEOFF", inf_label)); // add page offset of infinity constant
            emitter.instruction("ldr d1, [x9]");                                // load infinity value into d1
            emitter.instruction("fcmp d0, d1");                                 // compare |value| against infinity
            emitter.instruction("cset x0, eq");                                 // x0 = 1 if value is infinite, 0 otherwise
            Some(PhpType::Bool)
        }
        "is_finite" => {
            emitter.comment("is_finite()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            // -- check if |value| is strictly less than infinity (not NaN, not Inf) --
            if ty != PhpType::Float { emitter.instruction("scvtf d0, x0"); }    // convert int to float if needed
            emitter.instruction("fabs d0, d0");                                 // take absolute value
            let inf_label = data.add_float(f64::INFINITY);
            emitter.instruction(&format!("adrp x9, {}@PAGE", inf_label));       // load page address of infinity constant
            emitter.instruction(&format!("add x9, x9, {}@PAGEOFF", inf_label)); // add page offset of infinity constant
            emitter.instruction("ldr d1, [x9]");                                // load infinity value into d1
            emitter.instruction("fcmp d0, d1");                                 // compare |value| against infinity
            emitter.instruction("cset x0, mi");                                 // x0 = 1 if less than inf (finite), 0 if inf/NaN
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
            // -- load pointer and length of type name string --
            let (label, len) = data.add_string(type_str.as_bytes());
            emitter.instruction(&format!("adrp x1, {}@PAGE", label));           // load page address of type name string
            emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", label));     // add page offset to get full address
            emitter.instruction(&format!("mov x2, #{}", len));                  // load string length into x2
            Some(PhpType::Str)
        }
        "empty" => {
            emitter.comment("empty()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            match &ty {
                PhpType::Int => {
                    // -- int is empty if it equals zero --
                    crate::codegen::expr::coerce_null_to_zero(emitter, &ty);
                    emitter.instruction("cmp x0, #0");                          // compare int value against zero
                    emitter.instruction("cset x0, eq");                         // x0 = 1 if zero (empty), 0 otherwise
                }
                PhpType::Float => {
                    // -- float is empty if it equals 0.0 --
                    emitter.instruction("fcmp d0, #0.0");                       // compare float value against 0.0
                    emitter.instruction("cset x0, eq");                         // x0 = 1 if zero (empty), 0 otherwise
                }
                PhpType::Bool => {
                    // -- bool is empty if false (0) --
                    emitter.instruction("cmp x0, #0");                          // compare bool value against zero
                    emitter.instruction("cset x0, eq");                         // x0 = 1 if false (empty), 0 otherwise
                }
                PhpType::Void => {
                    // -- null is always empty --
                    emitter.instruction("mov x0, #1");                          // null is always empty, return true
                }
                PhpType::Str => {
                    // -- string is empty if length is zero --
                    emitter.instruction("cmp x2, #0");                          // compare string length against zero
                    emitter.instruction("cset x0, eq");                         // x0 = 1 if empty string, 0 otherwise
                }
                PhpType::Array(_) => {
                    // -- array is empty if element count is zero --
                    emitter.instruction("ldr x0, [x0]");                        // load array element count from header
                    emitter.instruction("cmp x0, #0");                          // compare element count against zero
                    emitter.instruction("cset x0, eq");                         // x0 = 1 if empty array, 0 otherwise
                }
            }
            Some(PhpType::Bool)
        }
        "unset" => {
            emitter.comment("unset()");
            if let crate::parser::ast::ExprKind::Variable(name) = &args[0].kind {
                let var = ctx.variables.get(name).expect("undefined variable");
                let offset = var.stack_offset;
                // -- set variable to null sentinel value (0x7FFFFFFFFFFFFFFFE) --
                emitter.instruction("movz x0, #0xFFFE");                        // load null sentinel bits [15:0]
                emitter.instruction("movk x0, #0xFFFF, lsl #16");               // load null sentinel bits [31:16]
                emitter.instruction("movk x0, #0xFFFF, lsl #32");               // load null sentinel bits [47:32]
                emitter.instruction("movk x0, #0x7FFF, lsl #48");               // load null sentinel bits [63:48]
                emitter.instruction(&format!("stur x0, [x29, #-{}]", offset));  // store null sentinel to variable's stack slot
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
                            // -- convert value to integer --
                            match &old_ty {
                                PhpType::Float => { emitter.instruction("fcvtzs x0, d0"); } // convert float to signed int (truncate toward zero)
                                PhpType::Bool | PhpType::Int => {}
                                _ => { emitter.instruction("mov x0, #0"); }     // unsupported types become 0
                            }
                            PhpType::Int
                        }
                        "float" | "double" => {
                            // -- convert value to float --
                            match &old_ty {
                                PhpType::Float => {}
                                _ => { emitter.instruction("scvtf d0, x0"); }   // convert signed int/bool to float
                            }
                            PhpType::Float
                        }
                        "string" => {
                            crate::codegen::expr::coerce_to_string(emitter, &old_ty);
                            PhpType::Str
                        }
                        "bool" | "boolean" => {
                            // -- convert value to boolean --
                            crate::codegen::expr::coerce_null_to_zero(emitter, &old_ty);
                            emitter.instruction("cmp x0, #0");                  // compare value against zero
                            emitter.instruction("cset x0, ne");                 // x0 = 1 if truthy, 0 if falsy
                            PhpType::Bool
                        }
                        _ => old_ty.clone(),
                    };
                    crate::codegen::abi::emit_store(emitter, &new_ty, offset);
                    ctx.variables.get_mut(vname).unwrap().ty = new_ty;
                }
            }
            // -- settype() always returns true --
            emitter.instruction("mov x0, #1");                                  // return true (settype always succeeds)
            Some(PhpType::Bool)
        }
        _ => None,
    }
}
