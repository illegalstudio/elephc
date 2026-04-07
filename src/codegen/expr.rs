mod arrays;
mod binops;
pub(crate) mod calls;
mod coerce;
mod compare;
mod helpers;
mod objects;
mod ownership;

use super::abi;
use super::context::{Context, HeapOwnership};
use super::data_section::DataSection;
use super::emit::Emitter;
use crate::parser::ast::{BinOp, CallableTarget, Expr, ExprKind, TypeExpr};
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
            emitter.adrp("x1", &format!("{}", label));           // load page base address of string data
            emitter.add_lo12("x1", "x1", &format!("{}", label));     // add page offset to get exact string pointer
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
            emitter.adrp("x9", &format!("{}", label));           // load page base of float constant
            emitter.add_lo12("x9", "x9", &format!("{}", label));     // add offset to get exact float address
            emitter.instruction("ldr d0, [x9]");                                // load 64-bit double from memory into float reg
            PhpType::Float
        }
        ExprKind::Variable(name) => {
            if let Some(ty) = ctx.extern_globals.get(name).cloned() {
                emitter.comment(&format!("load extern global ${}", name));
                match &ty {
                    PhpType::Bool
                    | PhpType::Int
                    | PhpType::Pointer(_)
                    | PhpType::Buffer(_)
                    | PhpType::Packed(_)
                    | PhpType::Callable => {
                        {
                            let sym = match emitter.platform {
                                crate::codegen::platform::Platform::MacOS => format!("_{}", name),
                                crate::codegen::platform::Platform::Linux => name.to_string(),
                            };
                            emitter.adrp_got("x9", &format!("{}", sym)); //load page of extern global GOT entry
                            emitter.ldr_got_lo12("x9", "x9", &format!("{}", sym)); //resolve extern global address
                        }
                        emitter.instruction("ldr x0, [x9]");                    // load extern integer/pointer value
                    }
                    PhpType::Float => {
                        {
                            let sym = match emitter.platform {
                                crate::codegen::platform::Platform::MacOS => format!("_{}", name),
                                crate::codegen::platform::Platform::Linux => name.to_string(),
                            };
                            emitter.adrp_got("x9", &format!("{}", sym)); //load page of extern global GOT entry
                            emitter.ldr_got_lo12("x9", "x9", &format!("{}", sym)); //resolve extern global address
                        }
                        emitter.instruction("ldr d0, [x9]");                    // load extern float value
                    }
                    PhpType::Str => {
                        {
                            let sym = match emitter.platform {
                                crate::codegen::platform::Platform::MacOS => format!("_{}", name),
                                crate::codegen::platform::Platform::Linux => name.to_string(),
                            };
                            emitter.adrp_got("x9", &format!("{}", sym)); //load page of extern global GOT entry
                            emitter.ldr_got_lo12("x9", "x9", &format!("{}", sym)); //resolve extern global address
                        }
                        emitter.instruction("ldr x0, [x9]");                    // load char* from extern global
                        emitter.instruction("bl __rt_cstr_to_str");             // convert C string to elephc string
                    }
                    PhpType::Void
                    | PhpType::Mixed
                    | PhpType::Union(_)
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
                let Some(var) = ctx.variables.get(name) else {
                    emitter.comment(&format!("WARNING: undefined variable ${}", name));
                    return PhpType::Int;
                };
                let ty = var.ty.clone();
                super::stmt::emit_global_load(emitter, ctx, name, &ty);
                ty
            } else if ctx.ref_params.contains(name) {
                // Load through reference pointer
                let Some(var) = ctx.variables.get(name) else {
                    emitter.comment(&format!("WARNING: undefined variable ${}", name));
                    return PhpType::Int;
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
                let Some(var) = ctx.variables.get(name) else {
                    emitter.comment(&format!("WARNING: undefined variable ${}", name));
                    return PhpType::Int;
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
        ExprKind::BufferNew { element_type, len } => {
            arrays::emit_buffer_new(element_type, len, emitter, ctx, data)
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
        ExprKind::Throw(inner) => {
            let thrown_ty = emit_expr(inner, emitter, ctx, data);
            if thrown_ty.is_refcounted() && expr_result_heap_ownership(inner) != HeapOwnership::Owned {
                abi::emit_incref_if_refcounted(emitter, &thrown_ty);            // retain borrowed heap values before publishing them as the active exception
            }
            emitter.adrp("x9", "_exc_value");                    // load page of the current exception slot
            emitter.add_lo12("x9", "x9", "_exc_value");              // resolve the current exception slot address
            emitter.instruction("str x0, [x9]");                                // publish the thrown object pointer as the active exception
            emitter.instruction("bl __rt_throw_current");                       // unwind to the nearest active exception handler
            PhpType::Void
        }
        ExprKind::NullCoalesce { value, default } => {
            emit_null_coalesce(value, default, emitter, ctx, data)
        }
        ExprKind::PreIncrement(name) => {
            if ctx.global_vars.contains(name) {
                let Some(var) = ctx.variables.get(name) else {
                    emitter.comment(&format!("WARNING: undefined variable ${}", name));
                    return PhpType::Int;
                };
                let ty = var.ty.clone();
                emitter.comment(&format!("++${} (global)", name));
                super::stmt::emit_global_load(emitter, ctx, name, &ty);
                emitter.instruction("add x0, x0, #1");                          // increment the value by 1
                emit_global_store_inline(emitter, name);
                PhpType::Int
            } else if ctx.ref_params.contains(name) {
                let Some(var) = ctx.variables.get(name) else {
                    emitter.comment(&format!("WARNING: undefined variable ${}", name));
                    return PhpType::Int;
                };
                let offset = var.stack_offset;
                emitter.comment(&format!("++${} (ref)", name));
                abi::load_at_offset(emitter, "x9", offset); // load ref pointer
                emitter.instruction("ldr x0, [x9]");                            // dereference
                emitter.instruction("add x0, x0, #1");                          // increment
                emitter.instruction("str x0, [x9]");                            // store back through pointer
                PhpType::Int
            } else {
                let Some(var) = ctx.variables.get(name) else {
                    emitter.comment(&format!("WARNING: undefined variable ${}", name));
                    return PhpType::Int;
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
                let Some(var) = ctx.variables.get(name) else {
                    emitter.comment(&format!("WARNING: undefined variable ${}", name));
                    return PhpType::Int;
                };
                let ty = var.ty.clone();
                emitter.comment(&format!("${}++ (global)", name));
                super::stmt::emit_global_load(emitter, ctx, name, &ty);
                emitter.instruction("add x1, x0, #1");                          // compute incremented value
                                                       // Store new value to global
                let label = format!("_gvar_{}", name);
                emitter.adrp("x9", &format!("{}", label));       // load page of global var storage
                emitter.add_lo12("x9", "x9", &format!("{}", label)); // add page offset
                emitter.instruction("str x1, [x9]");                            // store incremented value to global
                PhpType::Int
            } else if ctx.ref_params.contains(name) {
                let Some(var) = ctx.variables.get(name) else {
                    emitter.comment(&format!("WARNING: undefined variable ${}", name));
                    return PhpType::Int;
                };
                let offset = var.stack_offset;
                emitter.comment(&format!("${}++ (ref)", name));
                abi::load_at_offset(emitter, "x9", offset); // load ref pointer
                emitter.instruction("ldr x0, [x9]");                            // dereference
                emitter.instruction("add x1, x0, #1");                          // compute incremented value
                emitter.instruction("str x1, [x9]");                            // store back through pointer
                PhpType::Int
            } else {
                let Some(var) = ctx.variables.get(name) else {
                    emitter.comment(&format!("WARNING: undefined variable ${}", name));
                    return PhpType::Int;
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
                    emitter.adrp("x9", &format!("{}", label));   // load page of global var storage
                    emitter.add_lo12("x9", "x9", &format!("{}", label)); //add page offset
                    emitter.instruction("str x1, [x9]");                        // store incremented value to global
                }
                PhpType::Int
            }
        }
        ExprKind::PreDecrement(name) => {
            if ctx.global_vars.contains(name) {
                let Some(var) = ctx.variables.get(name) else {
                    emitter.comment(&format!("WARNING: undefined variable ${}", name));
                    return PhpType::Int;
                };
                let ty = var.ty.clone();
                emitter.comment(&format!("--${} (global)", name));
                super::stmt::emit_global_load(emitter, ctx, name, &ty);
                emitter.instruction("sub x0, x0, #1");                          // decrement the value by 1
                emit_global_store_inline(emitter, name);
                PhpType::Int
            } else if ctx.ref_params.contains(name) {
                let Some(var) = ctx.variables.get(name) else {
                    emitter.comment(&format!("WARNING: undefined variable ${}", name));
                    return PhpType::Int;
                };
                let offset = var.stack_offset;
                emitter.comment(&format!("--${} (ref)", name));
                abi::load_at_offset(emitter, "x9", offset); // load ref pointer
                emitter.instruction("ldr x0, [x9]");                            // dereference
                emitter.instruction("sub x0, x0, #1");                          // decrement
                emitter.instruction("str x0, [x9]");                            // store back through pointer
                PhpType::Int
            } else {
                let Some(var) = ctx.variables.get(name) else {
                    emitter.comment(&format!("WARNING: undefined variable ${}", name));
                    return PhpType::Int;
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
                let Some(var) = ctx.variables.get(name) else {
                    emitter.comment(&format!("WARNING: undefined variable ${}", name));
                    return PhpType::Int;
                };
                let offset = var.stack_offset;
                emitter.comment(&format!("${}-- (ref)", name));
                abi::load_at_offset(emitter, "x9", offset); // load ref pointer
                emitter.instruction("ldr x0, [x9]");                            // dereference
                emitter.instruction("sub x1, x0, #1");                          // compute decremented value
                emitter.instruction("str x1, [x9]");                            // store back through pointer
                PhpType::Int
            } else {
                let Some(var) = ctx.variables.get(name) else {
                    emitter.comment(&format!("WARNING: undefined variable ${}", name));
                    return PhpType::Int;
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
                declared_params: vec![],
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
                    coerce_to_string(emitter, ctx, data, &then_ty);
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
                    coerce_to_string(emitter, ctx, data, &else_ty);
                } else if result_ty == PhpType::Float && else_ty == PhpType::Int {
                    emitter.instruction("scvtf d0, x0");                        // convert int to float for unified result type
                }
            }
            emitter.label(&end_label);
            result_ty
        }
        ExprKind::Cast { target, expr } => emit_cast(target, expr, emitter, ctx, data),
        ExprKind::FunctionCall { name, args } => {
            if ctx.extern_functions.contains_key(name.as_str()) {
                return super::ffi::emit_extern_call(name.as_str(), args, emitter, ctx, data);
            }
            if let Some(ty) =
                super::builtins::emit_builtin_call(name.as_str(), args, emitter, ctx, data)
            {
                return ty;
            }
            emit_function_call(name.as_str(), args, emitter, ctx, data)
        }
        ExprKind::Closure {
            params,
            body,
            is_arrow: _,
            variadic,
            captures,
        } => emit_closure(params, variadic, body, captures, emitter, ctx, data),
        ExprKind::FirstClassCallable(target) => {
            emit_first_class_callable(target, emitter, ctx, data)
        }
        ExprKind::ClosureCall { var, args } => emit_closure_call(var, args, emitter, ctx, data),
        ExprKind::ExprCall { callee, args } => emit_expr_call(callee, args, emitter, ctx, data),
        ExprKind::ConstRef(name) => {
            let (value, _ty) = match ctx.constants.get(name.as_str()) {
                Some(c) => c.clone(),
                None => {
                    emitter.comment(&format!("WARNING: undefined constant {}", name));
                    return PhpType::Int;
                }
            };
            let synthetic_expr = Expr::new(value, expr.span);
            emit_expr(&synthetic_expr, emitter, ctx, data)
        }
        ExprKind::EnumCase { enum_name, case_name } => {
            objects::emit_enum_case(enum_name.as_str(), case_name, emitter, ctx)
        }
        ExprKind::BinaryOp { left, op, right } => emit_binop(left, op, right, emitter, ctx, data),
        ExprKind::Spread(inner) => {
            // Spread is handled at call site / array literal level.
            // If we reach here, just evaluate the inner expression.
            emit_expr(inner, emitter, ctx, data)
        }
        ExprKind::NamedArg { value, .. } => emit_expr(value, emitter, ctx, data),
        ExprKind::NewObject { class_name, args } => {
            emit_new_object(class_name.as_str(), args, emitter, ctx, data)
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
            abi::load_at_offset(emitter, "x0", offset);          // load $this object pointer
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

pub(crate) fn emit_method_call_with_pushed_args(
    class_name: &str,
    method: &str,
    arg_types: &[PhpType],
    emitter: &mut Emitter,
    ctx: &mut Context,
) -> PhpType {
    objects::emit_method_call_with_pushed_args(class_name, method, arg_types, emitter, ctx)
}

pub(crate) fn push_magic_property_name_arg(
    property: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
) {
    objects::push_magic_property_name_arg(property, emitter, data)
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
/// PHP behavior: false → "", true → "1", null → "", int → itoa
pub fn coerce_to_string(
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    ty: &PhpType,
) {
    coerce::coerce_to_string(emitter, ctx, data, ty)
}

/// Replace null sentinel with 0 in x0 (for arithmetic/comparison with null).
/// Handles both compile-time null (Void type) and runtime null (variable
/// that was assigned null — sentinel value in x0).
pub fn coerce_null_to_zero(emitter: &mut Emitter, ty: &PhpType) {
    coerce::coerce_null_to_zero(emitter, ty)
}

/// Coerce any type to a truthiness value in x0 for use in conditions
/// (if, while, for, ternary, &&, ||). For strings, PHP treats both ""
/// and "0" as falsy. For other types, x0 already holds the truthiness.
pub fn coerce_to_truthiness(emitter: &mut Emitter, ctx: &mut Context, ty: &PhpType) {
    coerce::coerce_to_truthiness(emitter, ctx, ty)
}

/// Coerce any type to integer in x0 for loose comparison (==, !=).
fn emit_binop(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    binops::emit_binop(left, op, right, emitter, ctx, data)
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
    emitter.adrp("x9", "_concat_off");                           // load page of caller concat offset
    emitter.add_lo12("x9", "x9", "_concat_off");                     // resolve caller concat offset address
    emitter.instruction("ldr x10, [x9]");                                       // read caller concat offset before nested call
    emitter.instruction("str x10, [sp, #-16]!");                                // save caller concat offset across nested call
}

pub(crate) fn restore_concat_offset_after_nested_call(emitter: &mut Emitter, return_ty: &PhpType) {
    if *return_ty == PhpType::Str {
        emitter.instruction("bl __rt_str_persist");                             // persist returned string before restoring caller concat cursor
    }
    emitter.instruction("ldr x10, [sp], #16");                                  // pop saved caller concat offset from stack
    emitter.adrp("x9", "_concat_off");                           // load page of caller concat offset
    emitter.add_lo12("x9", "x9", "_concat_off");                     // resolve caller concat offset address
    emitter.instruction("str x10, [x9]");                                       // restore caller concat offset after nested call
}

pub(crate) fn expr_result_heap_ownership(expr: &Expr) -> HeapOwnership {
    ownership::expr_result_heap_ownership(expr)
}

fn retain_borrowed_heap_arg(emitter: &mut Emitter, expr: &Expr, ty: &PhpType) {
    helpers::retain_borrowed_heap_arg(emitter, expr, ty)
}

fn widen_codegen_type(a: &PhpType, b: &PhpType) -> PhpType {
    helpers::widen_codegen_type(a, b)
}

pub(crate) fn coerce_result_to_type(
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    source_ty: &PhpType,
    target_ty: &PhpType,
) {
    helpers::coerce_result_to_type(emitter, ctx, data, source_ty, target_ty)
}

fn emit_closure(
    params: &[(String, Option<TypeExpr>, Option<Expr>, bool)],
    variadic: &Option<String>,
    body: &[crate::parser::ast::Stmt],
    captures: &[String],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    calls::emit_closure(params, variadic, body, captures, emitter, ctx, data)
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

fn emit_first_class_callable(
    target: &CallableTarget,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    calls::emit_first_class_callable(target, emitter, ctx, data)
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
    emitter.adrp("x9", &format!("{}", label));                   // load page of global var storage
    emitter.add_lo12("x9", "x9", &format!("{}", label));             // add page offset
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
