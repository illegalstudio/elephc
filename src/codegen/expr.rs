mod arrays;
mod calls;
mod compare;
mod objects;

use super::abi;
use super::context::{Context, HeapOwnership};
use super::data_section::DataSection;
use super::emit::Emitter;
use crate::parser::ast::{BinOp, Expr, ExprKind, Visibility};
use crate::types::FunctionSig;
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
        ExprKind::BoolLiteral(b) => {
            emitter.comment(&format!("bool {}", b));
            // -- load boolean as integer 0 or 1 --
            emitter.instruction(&format!(                                       // store boolean: true=1, false=0
                "mov x0, #{}",
                if *b { 1 } else { 0 }
            ));
            PhpType::Bool
        }
        ExprKind::Null => {
            emitter.comment("null");
            // -- load null sentinel value (0x7FFF_FFFF_FFFF_FFFE) into x0 --
            emitter.instruction("movz x0, #0xFFFE");                            // load lowest 16 bits of null sentinel
            emitter.instruction("movk x0, #0xFFFF, lsl #16");                   // insert bits 16-31 of null sentinel
            emitter.instruction("movk x0, #0xFFFF, lsl #32");                   // insert bits 32-47 of null sentinel
            emitter.instruction("movk x0, #0x7FFF, lsl #48");                   // insert bits 48-63, completing 0x7FFFFFFFFFFFFFFE
            PhpType::Void
        }
        ExprKind::StringLiteral(s) => {
            let bytes = s.as_bytes();
            let (label, len) = data.add_string(bytes);
            emitter.comment(&format!("load string \"{}\"", s.escape_default()));
            // -- load string pointer into x1 and length into x2 --
            emitter.instruction(&format!("adrp x1, {}@PAGE", label));           // load page base address of string data
            emitter.instruction(&format!("add x1, x1, {}@PAGEOFF", label));     // add page offset to get exact string pointer
            emitter.instruction(&format!("mov x2, #{}", len));                  // load string byte length into x2
            PhpType::Str
        }
        ExprKind::IntLiteral(n) => {
            emitter.comment(&format!("load int {}", n));
            load_immediate(emitter, "x0", *n);
            PhpType::Int
        }
        ExprKind::FloatLiteral(f) => {
            emitter.comment(&format!("load float {}", f));
            let label = data.add_float(*f);
            // -- load float constant from data section into d0 --
            emitter.instruction(&format!("adrp x9, {}@PAGE", label));           // load page base of float constant
            emitter.instruction(&format!("add x9, x9, {}@PAGEOFF", label));     // add offset to get exact float address
            emitter.instruction("ldr d0, [x9]");                                // load 64-bit double from memory into float reg
            PhpType::Float
        }
        ExprKind::Variable(name) => {
            if let Some(ty) = ctx.extern_globals.get(name).cloned() {
                emitter.comment(&format!("load extern global ${}", name));
                match &ty {
                    PhpType::Bool | PhpType::Int | PhpType::Pointer(_) | PhpType::Callable => {
                        emitter.instruction(&format!("adrp x9, _{}@GOTPAGE", name)); //load page of extern global GOT entry
                        emitter.instruction(&format!("ldr x9, [x9, _{}@GOTPAGEOFF]", name)); //resolve extern global address
                        emitter.instruction("ldr x0, [x9]");                    // load extern integer/pointer value
                    }
                    PhpType::Float => {
                        emitter.instruction(&format!("adrp x9, _{}@GOTPAGE", name)); //load page of extern global GOT entry
                        emitter.instruction(&format!("ldr x9, [x9, _{}@GOTPAGEOFF]", name)); //resolve extern global address
                        emitter.instruction("ldr d0, [x9]");                    // load extern float value
                    }
                    PhpType::Str => {
                        emitter.instruction(&format!("adrp x9, _{}@GOTPAGE", name)); //load page of extern global GOT entry
                        emitter.instruction(&format!("ldr x9, [x9, _{}@GOTPAGEOFF]", name)); //resolve extern global address
                        emitter.instruction("ldr x0, [x9]");                    // load char* from extern global
                        emitter.instruction("bl __rt_cstr_to_str");             // convert C string to elephc string
                    }
                    PhpType::Void
                    | PhpType::Array(_)
                    | PhpType::AssocArray { .. }
                    | PhpType::Object(_) => {
                        emitter.comment(&format!(
                            "WARNING: unsupported extern global type for ${}",
                            name
                        ));
                        return PhpType::Int;
                    }
                }
                ty
            } else if ctx.global_vars.contains(name)
                || (ctx.in_main && ctx.all_global_var_names.contains(name))
            {
                // Load from global storage
                let var = match ctx.variables.get(name) {
                    Some(v) => v,
                    None => {
                        emitter.comment(&format!("WARNING: undefined variable ${}", name));
                        return PhpType::Int;
                    }
                };
                let ty = var.ty.clone();
                super::stmt::emit_global_load(emitter, ctx, name, &ty);
                ty
            } else if ctx.ref_params.contains(name) {
                // Load through reference pointer
                let var = match ctx.variables.get(name) {
                    Some(v) => v,
                    None => {
                        emitter.comment(&format!("WARNING: undefined variable ${}", name));
                        return PhpType::Int;
                    }
                };
                let offset = var.stack_offset;
                let ty = var.ty.clone();
                emitter.comment(&format!("load ref ${}", name));
                abi::load_at_offset(emitter, "x9", offset); // load pointer to referenced variable
                match &ty {
                    PhpType::Bool | PhpType::Int => {
                        emitter.instruction("ldr x0, [x9]");                    // dereference to get int/bool value
                    }
                    PhpType::Float => {
                        emitter.instruction("ldr d0, [x9]");                    // dereference to get float value
                    }
                    PhpType::Str => {
                        emitter.instruction("ldr x1, [x9]");                    // dereference to get string pointer
                        emitter.instruction("ldr x2, [x9, #8]");                // dereference to get string length
                    }
                    _ => {
                        emitter.instruction("ldr x0, [x9]");                    // dereference to get value
                    }
                }
                ty
            } else {
                let var = match ctx.variables.get(name) {
                    Some(v) => v,
                    None => {
                        emitter.comment(&format!("WARNING: undefined variable ${}", name));
                        return PhpType::Int;
                    }
                };
                let offset = var.stack_offset;
                let ty = var.ty.clone();
                emitter.comment(&format!("load ${}", name));
                abi::emit_load(emitter, &ty, offset);
                ty
            }
        }
        ExprKind::Negate(inner) => {
            let ty = emit_expr(inner, emitter, ctx, data);
            emitter.comment("negate");
            if ty == PhpType::Float {
                // -- negate floating-point value --
                emitter.instruction("fneg d0, d0");                             // flip the sign bit of double-precision float
                PhpType::Float
            } else {
                // -- negate integer value --
                emitter.instruction("neg x0, x0");                              // two's complement negation of 64-bit integer
                PhpType::Int
            }
        }
        ExprKind::ArrayLiteral(elems) => emit_array_literal(elems, emitter, ctx, data),
        ExprKind::ArrayLiteralAssoc(pairs) => emit_assoc_array_literal(pairs, emitter, ctx, data),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => emit_match_expr(subject, arms, default, emitter, ctx, data),
        ExprKind::ArrayAccess { array, index } => {
            emit_array_access(array, index, emitter, ctx, data)
        }
        ExprKind::Not(inner) => {
            let ty = emit_expr(inner, emitter, ctx, data);
            emitter.comment("logical not");
            coerce_to_truthiness(emitter, ctx, &ty);
            // -- PHP !$x: invert truthiness --
            emitter.instruction("cmp x0, #0");                                  // test if value is falsy (zero)
            emitter.instruction("cset x0, eq");                                 // x0=1 if was falsy, x0=0 if was truthy
            PhpType::Bool
        }
        ExprKind::BitNot(inner) => {
            let ty = emit_expr(inner, emitter, ctx, data);
            coerce_null_to_zero(emitter, &ty);
            emitter.comment("bitwise not");
            // -- PHP ~$x: invert all bits --
            emitter.instruction("mvn x0, x0");                                  // bitwise complement of all 64 bits
            PhpType::Int
        }
        ExprKind::NullCoalesce { value, default } => {
            emit_null_coalesce(value, default, emitter, ctx, data)
        }
        ExprKind::PreIncrement(name) => {
            if ctx.global_vars.contains(name) {
                let var = match ctx.variables.get(name) {
                    Some(v) => v,
                    None => {
                        emitter.comment(&format!("WARNING: undefined variable ${}", name));
                        return PhpType::Int;
                    }
                };
                let ty = var.ty.clone();
                emitter.comment(&format!("++${} (global)", name));
                super::stmt::emit_global_load(emitter, ctx, name, &ty);
                emitter.instruction("add x0, x0, #1");                          // increment the value by 1
                emit_global_store_inline(emitter, name);
                PhpType::Int
            } else if ctx.ref_params.contains(name) {
                let var = match ctx.variables.get(name) {
                    Some(v) => v,
                    None => {
                        emitter.comment(&format!("WARNING: undefined variable ${}", name));
                        return PhpType::Int;
                    }
                };
                let offset = var.stack_offset;
                emitter.comment(&format!("++${} (ref)", name));
                abi::load_at_offset(emitter, "x9", offset); // load ref pointer
                emitter.instruction("ldr x0, [x9]");                            // dereference
                emitter.instruction("add x0, x0, #1");                          // increment
                emitter.instruction("str x0, [x9]");                            // store back through pointer
                PhpType::Int
            } else {
                let var = match ctx.variables.get(name) {
                    Some(v) => v,
                    None => {
                        emitter.comment(&format!("WARNING: undefined variable ${}", name));
                        return PhpType::Int;
                    }
                };
                let offset = var.stack_offset;
                emitter.comment(&format!("++${}", name));
                // -- pre-increment: add 1 then return new value --
                abi::load_at_offset(emitter, "x0", offset); // load current variable value from stack frame
                emitter.instruction("add x0, x0, #1");                          // increment the value by 1
                abi::store_at_offset(emitter, "x0", offset); // store incremented value back to stack frame
                if ctx.in_main && ctx.all_global_var_names.contains(name) {
                    emit_global_store_inline(emitter, name);
                }
                PhpType::Int
            }
        }
        ExprKind::PostIncrement(name) => {
            if ctx.global_vars.contains(name) {
                let var = match ctx.variables.get(name) {
                    Some(v) => v,
                    None => {
                        emitter.comment(&format!("WARNING: undefined variable ${}", name));
                        return PhpType::Int;
                    }
                };
                let ty = var.ty.clone();
                emitter.comment(&format!("${}++ (global)", name));
                super::stmt::emit_global_load(emitter, ctx, name, &ty);
                emitter.instruction("add x1, x0, #1");                          // compute incremented value
                                                       // Store new value to global
                let label = format!("_gvar_{}", name);
                emitter.instruction(&format!("adrp x9, {}@PAGE", label));       // load page of global var storage
                emitter.instruction(&format!("add x9, x9, {}@PAGEOFF", label)); // add page offset
                emitter.instruction("str x1, [x9]");                            // store incremented value to global
                PhpType::Int
            } else if ctx.ref_params.contains(name) {
                let var = match ctx.variables.get(name) {
                    Some(v) => v,
                    None => {
                        emitter.comment(&format!("WARNING: undefined variable ${}", name));
                        return PhpType::Int;
                    }
                };
                let offset = var.stack_offset;
                emitter.comment(&format!("${}++ (ref)", name));
                abi::load_at_offset(emitter, "x9", offset); // load ref pointer
                emitter.instruction("ldr x0, [x9]");                            // dereference
                emitter.instruction("add x1, x0, #1");                          // compute incremented value
                emitter.instruction("str x1, [x9]");                            // store back through pointer
                PhpType::Int
            } else {
                let var = match ctx.variables.get(name) {
                    Some(v) => v,
                    None => {
                        emitter.comment(&format!("WARNING: undefined variable ${}", name));
                        return PhpType::Int;
                    }
                };
                let offset = var.stack_offset;
                emitter.comment(&format!("${}++", name));
                // -- post-increment: return old value, then add 1 --
                abi::load_at_offset(emitter, "x0", offset); // load current value into x0 (returned to caller)
                emitter.instruction("add x1, x0, #1");                          // compute incremented value in scratch register
                abi::store_at_offset(emitter, "x1", offset); // write new value back; x0 still has old value
                if ctx.in_main && ctx.all_global_var_names.contains(name) {
                    // Save new value (in x1) to global
                    let label = format!("_gvar_{}", name);
                    emitter.instruction(&format!("adrp x9, {}@PAGE", label));   // load page of global var storage
                    emitter.instruction(&format!("add x9, x9, {}@PAGEOFF", label)); //add page offset
                    emitter.instruction("str x1, [x9]");                        // store incremented value to global
                }
                PhpType::Int
            }
        }
        ExprKind::PreDecrement(name) => {
            if ctx.global_vars.contains(name) {
                let var = match ctx.variables.get(name) {
                    Some(v) => v,
                    None => {
                        emitter.comment(&format!("WARNING: undefined variable ${}", name));
                        return PhpType::Int;
                    }
                };
                let ty = var.ty.clone();
                emitter.comment(&format!("--${} (global)", name));
                super::stmt::emit_global_load(emitter, ctx, name, &ty);
                emitter.instruction("sub x0, x0, #1");                          // decrement the value by 1
                emit_global_store_inline(emitter, name);
                PhpType::Int
            } else if ctx.ref_params.contains(name) {
                let var = match ctx.variables.get(name) {
                    Some(v) => v,
                    None => {
                        emitter.comment(&format!("WARNING: undefined variable ${}", name));
                        return PhpType::Int;
                    }
                };
                let offset = var.stack_offset;
                emitter.comment(&format!("--${} (ref)", name));
                abi::load_at_offset(emitter, "x9", offset); // load ref pointer
                emitter.instruction("ldr x0, [x9]");                            // dereference
                emitter.instruction("sub x0, x0, #1");                          // decrement
                emitter.instruction("str x0, [x9]");                            // store back through pointer
                PhpType::Int
            } else {
                let var = match ctx.variables.get(name) {
                    Some(v) => v,
                    None => {
                        emitter.comment(&format!("WARNING: undefined variable ${}", name));
                        return PhpType::Int;
                    }
                };
                let offset = var.stack_offset;
                emitter.comment(&format!("--${}", name));
                // -- pre-decrement: subtract 1 then return new value --
                abi::load_at_offset(emitter, "x0", offset); // load current variable value from stack frame
                emitter.instruction("sub x0, x0, #1");                          // decrement the value by 1
                abi::store_at_offset(emitter, "x0", offset); // store decremented value back to stack frame
                PhpType::Int
            }
        }
        ExprKind::PostDecrement(name) => {
            if ctx.ref_params.contains(name) {
                let var = match ctx.variables.get(name) {
                    Some(v) => v,
                    None => {
                        emitter.comment(&format!("WARNING: undefined variable ${}", name));
                        return PhpType::Int;
                    }
                };
                let offset = var.stack_offset;
                emitter.comment(&format!("${}-- (ref)", name));
                abi::load_at_offset(emitter, "x9", offset); // load ref pointer
                emitter.instruction("ldr x0, [x9]");                            // dereference
                emitter.instruction("sub x1, x0, #1");                          // compute decremented value
                emitter.instruction("str x1, [x9]");                            // store back through pointer
                PhpType::Int
            } else {
                let var = match ctx.variables.get(name) {
                    Some(v) => v,
                    None => {
                        emitter.comment(&format!("WARNING: undefined variable ${}", name));
                        return PhpType::Int;
                    }
                };
                let offset = var.stack_offset;
                emitter.comment(&format!("${}--", name));
                // -- post-decrement: return old value, then subtract 1 --
                abi::load_at_offset(emitter, "x0", offset); // load current value into x0 (returned to caller)
                emitter.instruction("sub x1, x0, #1");                          // compute decremented value in scratch register
                abi::store_at_offset(emitter, "x1", offset); // write new value back; x0 still has old value
                PhpType::Int
            }
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            let else_label = ctx.next_label("tern_else");
            let end_label = ctx.next_label("tern_end");
            emitter.comment("ternary");
            let cond_ty = emit_expr(condition, emitter, ctx, data);
            coerce_to_truthiness(emitter, ctx, &cond_ty);
            // -- branch based on ternary condition --
            emitter.instruction("cmp x0, #0");                                  // test if condition is falsy
            emitter.instruction(&format!("b.eq {}", else_label));               // jump to else branch if condition was false
                                                                  // -- determine result type: widen to the broader type --
            let dummy_sig = FunctionSig {
                params: vec![],
                defaults: vec![],
                return_type: PhpType::Int,
                ref_params: vec![],
                variadic: None,
            };
            let then_syn = super::functions::infer_local_type_with_ctx(then_expr, &dummy_sig, ctx);
            let else_syn = super::functions::infer_local_type_with_ctx(else_expr, &dummy_sig, ctx);
            let result_ty = if then_syn == else_syn {
                then_syn
            } else if then_syn == PhpType::Str || else_syn == PhpType::Str {
                PhpType::Str
            } else if then_syn == PhpType::Float || else_syn == PhpType::Float {
                PhpType::Float
            } else {
                then_syn
            };
            let then_ty = emit_expr(then_expr, emitter, ctx, data);
            // -- coerce then-branch to result type if needed --
            if result_ty != then_ty {
                if result_ty == PhpType::Str {
                    coerce_to_string(emitter, &then_ty);
                } else if result_ty == PhpType::Float && then_ty == PhpType::Int {
                    emitter.instruction("scvtf d0, x0");                        // convert int to float for unified result type
                }
            }
            emitter.instruction(&format!("b {}", end_label));                   // skip else branch after evaluating then-expr
            emitter.label(&else_label);
            let else_ty = emit_expr(else_expr, emitter, ctx, data);
            // -- coerce else-branch to result type if needed --
            if result_ty != else_ty {
                if result_ty == PhpType::Str {
                    coerce_to_string(emitter, &else_ty);
                } else if result_ty == PhpType::Float && else_ty == PhpType::Int {
                    emitter.instruction("scvtf d0, x0");                        // convert int to float for unified result type
                }
            }
            emitter.label(&end_label);
            result_ty
        }
        ExprKind::Cast { target, expr } => emit_cast(target, expr, emitter, ctx, data),
        ExprKind::FunctionCall { name, args } => {
            if ctx.extern_functions.contains_key(name) {
                return super::ffi::emit_extern_call(name, args, emitter, ctx, data);
            }
            if let Some(ty) = super::builtins::emit_builtin_call(name, args, emitter, ctx, data) {
                return ty;
            }
            emit_function_call(name, args, emitter, ctx, data)
        }
        ExprKind::Closure {
            params,
            body,
            is_arrow: _,
            variadic: _,
            captures,
        } => emit_closure(params, body, captures, emitter, ctx, data),
        ExprKind::ClosureCall { var, args } => emit_closure_call(var, args, emitter, ctx, data),
        ExprKind::ExprCall { callee, args } => emit_expr_call(callee, args, emitter, ctx, data),
        ExprKind::ConstRef(name) => {
            let (value, _ty) = match ctx.constants.get(name) {
                Some(c) => c.clone(),
                None => {
                    emitter.comment(&format!("WARNING: undefined constant {}", name));
                    return PhpType::Int;
                }
            };
            let synthetic_expr = Expr::new(value, expr.span);
            emit_expr(&synthetic_expr, emitter, ctx, data)
        }
        ExprKind::BinaryOp { left, op, right } => emit_binop(left, op, right, emitter, ctx, data),
        ExprKind::Spread(inner) => {
            // Spread is handled at call site / array literal level.
            // If we reach here, just evaluate the inner expression.
            emit_expr(inner, emitter, ctx, data)
        }
        ExprKind::NewObject { class_name, args } => {
            emit_new_object(class_name, args, emitter, ctx, data)
        }
        ExprKind::PropertyAccess { object, property } => {
            emit_property_access(object, property, emitter, ctx, data)
        }
        ExprKind::MethodCall {
            object,
            method,
            args,
        } => emit_method_call(object, method, args, emitter, ctx, data),
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => emit_static_method_call(receiver, method, args, emitter, ctx, data),
        ExprKind::This => {
            emitter.comment("$this");
            let var = match ctx.variables.get("this") {
                Some(v) => v,
                None => {
                    emitter.comment("WARNING: $this used outside class scope");
                    return PhpType::Int;
                }
            };
            let offset = var.stack_offset;
            super::abi::load_at_offset(emitter, "x0", offset); // load $this object pointer
            let class_name = ctx.current_class.clone().unwrap_or_default();
            PhpType::Object(class_name)
        }
        ExprKind::PtrCast { target_type, expr } => {
            emitter.comment(&format!("ptr_cast<{}>()", target_type));
            emit_expr(expr, emitter, ctx, data);
            // Value stays in x0 unchanged — only the type tag changes
            PhpType::Pointer(Some(target_type.clone()))
        }
    }
}

fn emit_new_object(
    class_name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    objects::emit_new_object(class_name, args, emitter, ctx, data)
}

fn emit_property_access(
    object: &Expr,
    property: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    objects::emit_property_access(object, property, emitter, ctx, data)
}

fn emit_method_call(
    object: &Expr,
    method: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    objects::emit_method_call(object, method, args, emitter, ctx, data)
}

fn emit_static_method_call(
    receiver: &crate::parser::ast::StaticReceiver,
    method: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    objects::emit_static_method_call(receiver, method, args, emitter, ctx, data)
}

fn emit_array_literal(
    elems: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    arrays::emit_array_literal(elems, emitter, ctx, data)
}

fn emit_assoc_array_literal(
    pairs: &[(Expr, Expr)],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    arrays::emit_assoc_array_literal(pairs, emitter, ctx, data)
}

fn emit_match_expr(
    subject: &Expr,
    arms: &[(Vec<Expr>, Expr)],
    default: &Option<Box<Expr>>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    arrays::emit_match_expr(subject, arms, default, emitter, ctx, data)
}

fn emit_array_access(
    array: &Expr,
    index: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    arrays::emit_array_access(array, index, emitter, ctx, data)
}

/// Coerce a value to string (x1=ptr, x2=len) for concatenation.
/// Coerce a value to string (x1=ptr, x2=len) for concatenation.
/// PHP behavior: false → "", true → "1", null → "", int → itoa
pub fn coerce_to_string(emitter: &mut Emitter, ty: &PhpType) {
    match ty {
        PhpType::Int => {
            // -- convert integer in x0 to string in x1/x2 --
            emitter.instruction("bl __rt_itoa");                                // runtime: integer-to-ASCII string conversion
        }
        PhpType::Float => {
            // -- convert float in d0 to string in x1/x2 --
            emitter.instruction("bl __rt_ftoa");                                // runtime: float-to-ASCII string conversion
        }
        PhpType::Bool => {
            // true → "1" (via itoa), false → "" (len=0)
            // If x0 == 0, just set len=0; otherwise call itoa
            // Use conditional: itoa always works but false should produce ""
            // Simplest: if 0, mov x2,#0; if 1, call itoa
            // -- convert bool to string: true="1", false="" --
            emitter.instruction("cbz x0, 1f");                                  // if false (zero), skip to empty string path
            emitter.instruction("bl __rt_itoa");                                // convert true (1) to string "1"
            emitter.instruction("b 2f");                                        // skip over the empty-string fallback
            emitter.raw("1:");
            emitter.instruction("mov x2, #0");                                  // false produces empty string (length = 0)
            emitter.raw("2:");
        }
        PhpType::Void => {
            // -- null coerces to empty string in PHP --
            emitter.instruction("mov x2, #0");                                  // null produces empty string (length = 0)
        }
        PhpType::Str
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Callable
        | PhpType::Object(_)
        | PhpType::Pointer(_) => {}
    }
}

/// Replace null sentinel with 0 in x0 (for arithmetic/comparison with null).
/// Handles both compile-time null (Void type) and runtime null (variable
/// that was assigned null — sentinel value in x0).
pub fn coerce_null_to_zero(emitter: &mut Emitter, ty: &PhpType) {
    if *ty == PhpType::Void {
        // -- compile-time null: just load zero --
        emitter.instruction("mov x0, #0");                                      // null is zero in arithmetic/comparison context
    } else if *ty == PhpType::Bool {
        // Bool is already 0/1 in x0, compatible with Int arithmetic
    } else if *ty == PhpType::Float {
        // Float is already in d0, no null sentinel to check
    } else if *ty == PhpType::Int {
        // -- runtime null check: compare x0 against sentinel value --
        emitter.instruction("movz x9, #0xFFFE");                                // build null sentinel in x9: bits 0-15
        emitter.instruction("movk x9, #0xFFFF, lsl #16");                       // null sentinel bits 16-31
        emitter.instruction("movk x9, #0xFFFF, lsl #32");                       // null sentinel bits 32-47
        emitter.instruction("movk x9, #0x7FFF, lsl #48");                       // null sentinel bits 48-63, completing value
        emitter.instruction("cmp x0, x9");                                      // compare value against null sentinel
        emitter.instruction("csel x0, xzr, x0, eq");                            // if x0 == sentinel, replace with zero
    }
}

/// Coerce any type to a truthiness value in x0 for use in conditions
/// (if, while, for, ternary, &&, ||). For strings, PHP treats both ""
/// and "0" as falsy. For other types, x0 already holds the truthiness.
pub fn coerce_to_truthiness(emitter: &mut Emitter, ctx: &mut Context, ty: &PhpType) {
    coerce_null_to_zero(emitter, ty);
    if *ty == PhpType::Str {
        // -- PHP string truthiness: "" and "0" are falsy, everything else truthy --
        let falsy_label = ctx.next_label("str_falsy");
        let truthy_label = ctx.next_label("str_truthy");
        let done_label = ctx.next_label("str_truth_done");
        emitter.instruction(&format!("cbz x2, {falsy_label}"));                 // empty string is falsy
        emitter.instruction("cmp x2, #1");                                      // check if length is 1
        emitter.instruction(&format!("b.ne {truthy_label}"));                   // length != 1 means truthy
        emitter.instruction("ldrb w9, [x1]");                                   // load first byte of string
        emitter.instruction("cmp w9, #48");                                     // compare with ASCII '0'
        emitter.instruction(&format!("b.eq {falsy_label}"));                    // string "0" is falsy
        emitter.label(&truthy_label);
        emitter.instruction("mov x0, #1");                                      // truthy: set x0 = 1
        emitter.instruction(&format!("b {done_label}"));                        // skip falsy path
        emitter.label(&falsy_label);
        emitter.instruction("mov x0, #0");                                      // falsy: set x0 = 0
        emitter.label(&done_label);
    } else if *ty == PhpType::Float {
        // -- float truthiness: 0.0 is falsy --
        emitter.instruction("fcmp d0, #0.0");                                   // compare float against zero
        emitter.instruction("cset x0, ne");                                     // x0=1 if nonzero (truthy), 0 if zero
    }
}

/// Coerce any type to integer in x0 for loose comparison (==, !=).
/// PHP loose comparison coerces both sides to a common type.
/// Simplified: coerce everything to int, then compare.
///   - Bool → already 0/1 in x0
///   - Null (Void) → 0
///   - Int → coerce null sentinel to 0 (via coerce_null_to_zero)
///   - Float → truncate to int via fcvtzs
///   - String → 0 (empty string) or parse via atoi
fn coerce_to_int_for_loose_cmp(emitter: &mut Emitter, ty: &PhpType) {
    match ty {
        PhpType::Void => {
            // -- null coerces to 0 --
            emitter.instruction("mov x0, #0");                                  // null is zero for loose comparison
        }
        PhpType::Bool => {
            // Bool is already 0 or 1 in x0 — nothing to do
        }
        PhpType::Int => {
            coerce_null_to_zero(emitter, ty);
        }
        PhpType::Float => {
            // -- truncate float to integer --
            emitter.instruction("fcvtzs x0, d0");                               // convert float to signed int (truncate)
        }
        PhpType::Str => {
            // -- coerce string to int: empty string → 0, otherwise parse --
            emitter.instruction("bl __rt_atoi");                                // runtime: parse string as integer → x0
        }
        _ => {
            // Arrays, callables — coerce to 0 as fallback
            emitter.instruction("mov x0, #0");                                  // unsupported type coerces to 0
        }
    }
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

            // Division always uses float path (PHP: 10/3 → 3.333...)
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
            coerce_to_string(emitter, &left_ty);
            // -- save left string while evaluating right operand --
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // push left string ptr+len onto stack
            let right_ty = emit_expr(right, emitter, ctx, data);
            coerce_to_string(emitter, &right_ty);
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
            emitter.instruction("csinv x0, x0, xzr, ge");                       // x0=-1 if left < right (invert zero → all-ones)
            PhpType::Int
        }
        BinOp::NullCoalesce => {
            // Should not reach here — handled by ExprKind::NullCoalesce
            // But handle gracefully via the same mechanism
            emit_null_coalesce(left, right, emitter, ctx, data)
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
    calls::emit_function_call(name, args, emitter, ctx, data)
}

pub(crate) fn save_concat_offset_before_nested_call(emitter: &mut Emitter) {
    emitter.instruction("adrp x9, _concat_off@PAGE");                           // load page of caller concat offset
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                     // resolve caller concat offset address
    emitter.instruction("ldr x10, [x9]");                                       // read caller concat offset before nested call
    emitter.instruction("str x10, [sp, #-16]!");                                // save caller concat offset across nested call
}

pub(crate) fn restore_concat_offset_after_nested_call(emitter: &mut Emitter, return_ty: &PhpType) {
    if *return_ty == PhpType::Str {
        emitter.instruction("bl __rt_str_persist");                             // persist returned string before restoring caller concat cursor
    }
    emitter.instruction("ldr x10, [sp], #16");                                  // pop saved caller concat offset from stack
    emitter.instruction("adrp x9, _concat_off@PAGE");                           // load page of caller concat offset
    emitter.instruction("add x9, x9, _concat_off@PAGEOFF");                     // resolve caller concat offset address
    emitter.instruction("str x10, [x9]");                                       // restore caller concat offset after nested call
}

pub(crate) fn expr_result_heap_ownership(expr: &Expr) -> HeapOwnership {
    match &expr.kind {
        ExprKind::Variable(_)
        | ExprKind::ArrayAccess { .. }
        | ExprKind::PropertyAccess { .. }
        | ExprKind::This => HeapOwnership::Borrowed,
        ExprKind::Spread(inner)
        | ExprKind::PtrCast { expr: inner, .. }
        | ExprKind::Cast { expr: inner, .. } => expr_result_heap_ownership(inner),
        ExprKind::NullCoalesce { value, default } => {
            expr_result_heap_ownership(value).merge(expr_result_heap_ownership(default))
        }
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => expr_result_heap_ownership(then_expr).merge(expr_result_heap_ownership(else_expr)),
        ExprKind::Match { arms, default, .. } => {
            let mut ownership = default
                .as_ref()
                .map(|expr| expr_result_heap_ownership(expr))
                .unwrap_or(HeapOwnership::NonHeap);
            for (_, expr) in arms {
                ownership = ownership.merge(expr_result_heap_ownership(expr));
            }
            ownership
        }
        ExprKind::StringLiteral(_)
        | ExprKind::ArrayLiteral(_)
        | ExprKind::ArrayLiteralAssoc(_)
        | ExprKind::FunctionCall { .. }
        | ExprKind::ClosureCall { .. }
        | ExprKind::ExprCall { .. }
        | ExprKind::MethodCall { .. }
        | ExprKind::StaticMethodCall { .. }
        | ExprKind::NewObject { .. } => HeapOwnership::Owned,
        _ => HeapOwnership::NonHeap,
    }
}

fn retain_borrowed_heap_arg(emitter: &mut Emitter, expr: &Expr, ty: &PhpType) {
    if ty.is_refcounted() && expr_result_heap_ownership(expr) != HeapOwnership::Owned {
        abi::emit_incref_if_refcounted(emitter, ty);
    }
}

fn widen_codegen_type(a: &PhpType, b: &PhpType) -> PhpType {
    if a == b {
        return a.clone();
    }
    if *a == PhpType::Str || *b == PhpType::Str {
        return PhpType::Str;
    }
    if *a == PhpType::Float || *b == PhpType::Float {
        return PhpType::Float;
    }
    if *a == PhpType::Void {
        return b.clone();
    }
    if *b == PhpType::Void {
        return a.clone();
    }
    a.clone()
}

fn coerce_result_to_type(emitter: &mut Emitter, source_ty: &PhpType, target_ty: &PhpType) {
    if source_ty == target_ty {
        return;
    }
    if *target_ty == PhpType::Str {
        coerce_to_string(emitter, source_ty);
    } else if *target_ty == PhpType::Float
        && matches!(source_ty, PhpType::Int | PhpType::Bool | PhpType::Void)
    {
        if *source_ty == PhpType::Void {
            emitter.instruction("mov x0, #0");                                  // null widens to numeric zero before float coercion
        }
        emitter.instruction("scvtf d0, x0");                                    // convert integer-like value to float for unified result type
    }
}

fn emit_closure(
    params: &[(String, Option<Expr>, bool)],
    body: &[crate::parser::ast::Stmt],
    captures: &[String],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    calls::emit_closure(params, body, captures, emitter, ctx, data)
}

fn emit_closure_call(
    var: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    calls::emit_closure_call(var, args, emitter, ctx, data)
}

fn emit_expr_call(
    callee: &Expr,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    calls::emit_expr_call(callee, args, emitter, ctx, data)
}

fn emit_cast(
    target: &crate::parser::ast::CastType,
    expr: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    compare::emit_cast(target, expr, emitter, ctx, data)
}

fn emit_strict_compare(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    compare::emit_strict_compare(left, op, right, emitter, ctx, data)
}

fn emit_null_coalesce(
    value: &Expr,
    default: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    compare::emit_null_coalesce(value, default, emitter, ctx, data)
}

/// Quick helper: store x0 to _gvar_NAME (for pre-increment etc. where value is in x0)
fn emit_global_store_inline(emitter: &mut Emitter, name: &str) {
    let label = format!("_gvar_{}", name);
    emitter.instruction(&format!("adrp x9, {}@PAGE", label));                   // load page of global var storage
    emitter.instruction(&format!("add x9, x9, {}@PAGEOFF", label));             // add page offset
    emitter.instruction("str x0, [x9]");                                        // store value to global storage
}

fn load_immediate(emitter: &mut Emitter, reg: &str, value: i64) {
    if (0..=65535).contains(&value) {
        // -- small non-negative immediate fits in single MOV --
        emitter.instruction(&format!("mov {}, #{}", reg, value));               // load small immediate value directly
    } else if (-65536..0).contains(&value) {
        // -- small negative immediate fits in single MOV --
        emitter.instruction(&format!("mov {}, #{}", reg, value));               // load small negative immediate directly
    } else {
        // -- large immediate: build 64-bit value in 16-bit chunks --
        let uval = value as u64;
        emitter.instruction(&format!(                                           // load bits 0-15 and zero upper bits
            "movz {}, #0x{:x}",
            reg,
            uval & 0xFFFF
        ));
        if (uval >> 16) & 0xFFFF != 0 {
            emitter.instruction(&format!(                                       // insert bits 16-31 keeping other bits
                "movk {}, #0x{:x}, lsl #16",
                reg,
                (uval >> 16) & 0xFFFF
            ));
        }
        if (uval >> 32) & 0xFFFF != 0 {
            emitter.instruction(&format!(                                       // insert bits 32-47 keeping other bits
                "movk {}, #0x{:x}, lsl #32",
                reg,
                (uval >> 32) & 0xFFFF
            ));
        }
        if (uval >> 48) & 0xFFFF != 0 {
            emitter.instruction(&format!(                                       // insert bits 48-63 keeping other bits
                "movk {}, #0x{:x}, lsl #48",
                reg,
                (uval >> 48) & 0xFFFF
            ));
        }
    }
}
