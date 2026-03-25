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
            if ctx.global_vars.contains(name) || (ctx.in_main && ctx.all_global_var_names.contains(name)) {
                // Load from global storage
                let var = ctx.variables.get(name).expect("undefined variable");
                let ty = var.ty.clone();
                super::stmt::emit_global_load(emitter, ctx, name, &ty);
                ty
            } else if ctx.ref_params.contains(name) {
                // Load through reference pointer
                let var = ctx.variables.get(name).expect("undefined variable");
                let offset = var.stack_offset;
                let ty = var.ty.clone();
                emitter.comment(&format!("load ref ${}", name));
                emitter.instruction(&format!("ldur x9, [x29, #-{}]", offset));  // load pointer to referenced variable
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
                let var = ctx.variables.get(name).expect("undefined variable");
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
        ExprKind::Match { subject, arms, default } => {
            emit_match_expr(subject, arms, default, emitter, ctx, data)
        }
        ExprKind::ArrayAccess { array, index } => {
            emit_array_access(array, index, emitter, ctx, data)
        }
        ExprKind::Not(inner) => {
            let ty = emit_expr(inner, emitter, ctx, data);
            coerce_null_to_zero(emitter, &ty);
            emitter.comment("logical not");
            if ty == PhpType::Str {
                // -- PHP !$str: string is falsy if length is zero --
                emitter.instruction("cmp x2, #0");                              // test if string length is zero (falsy)
                emitter.instruction("cset x0, eq");                             // x0=1 if empty string (falsy), x0=0 if non-empty
            } else {
                // -- PHP !$x: compare to zero and invert truthiness --
                emitter.instruction("cmp x0, #0");                              // test if value is falsy (zero)
                emitter.instruction("cset x0, eq");                             // x0=1 if was zero (falsy), x0=0 if truthy
            }
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
                let var = ctx.variables.get(name).expect("undefined variable");
                let ty = var.ty.clone();
                emitter.comment(&format!("++${} (global)", name));
                super::stmt::emit_global_load(emitter, ctx, name, &ty);
                emitter.instruction("add x0, x0, #1");                          // increment the value by 1
                emit_global_store_inline(emitter, name);
                PhpType::Int
            } else if ctx.ref_params.contains(name) {
                let var = ctx.variables.get(name).expect("undefined variable");
                let offset = var.stack_offset;
                emitter.comment(&format!("++${} (ref)", name));
                emitter.instruction(&format!("ldur x9, [x29, #-{}]", offset));  // load ref pointer
                emitter.instruction("ldr x0, [x9]");                            // dereference
                emitter.instruction("add x0, x0, #1");                          // increment
                emitter.instruction("str x0, [x9]");                            // store back through pointer
                PhpType::Int
            } else {
                let var = ctx.variables.get(name).expect("undefined variable");
                let offset = var.stack_offset;
                emitter.comment(&format!("++${}", name));
                // -- pre-increment: add 1 then return new value --
                emitter.instruction(&format!("ldur x0, [x29, #-{}]", offset));  // load current variable value from stack frame
                emitter.instruction("add x0, x0, #1");                          // increment the value by 1
                emitter.instruction(&format!("stur x0, [x29, #-{}]", offset));  // store incremented value back to stack frame
                if ctx.in_main && ctx.all_global_var_names.contains(name) {
                    emit_global_store_inline(emitter, name);
                }
                PhpType::Int
            }
        }
        ExprKind::PostIncrement(name) => {
            if ctx.global_vars.contains(name) {
                let var = ctx.variables.get(name).expect("undefined variable");
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
                let var = ctx.variables.get(name).expect("undefined variable");
                let offset = var.stack_offset;
                emitter.comment(&format!("${}++ (ref)", name));
                emitter.instruction(&format!("ldur x9, [x29, #-{}]", offset));  // load ref pointer
                emitter.instruction("ldr x0, [x9]");                            // dereference
                emitter.instruction("add x1, x0, #1");                          // compute incremented value
                emitter.instruction("str x1, [x9]");                            // store back through pointer
                PhpType::Int
            } else {
                let var = ctx.variables.get(name).expect("undefined variable");
                let offset = var.stack_offset;
                emitter.comment(&format!("${}++", name));
                // -- post-increment: return old value, then add 1 --
                emitter.instruction(&format!("ldur x0, [x29, #-{}]", offset));  // load current value into x0 (returned to caller)
                emitter.instruction("add x1, x0, #1");                          // compute incremented value in scratch register
                emitter.instruction(&format!("stur x1, [x29, #-{}]", offset));  // write new value back; x0 still has old value
                if ctx.in_main && ctx.all_global_var_names.contains(name) {
                    // Save new value (in x1) to global
                    let label = format!("_gvar_{}", name);
                    emitter.instruction(&format!("adrp x9, {}@PAGE", label));   // load page of global var storage
                    emitter.instruction(&format!("add x9, x9, {}@PAGEOFF", label)); // add page offset
                    emitter.instruction("str x1, [x9]");                        // store incremented value to global
                }
                PhpType::Int
            }
        }
        ExprKind::PreDecrement(name) => {
            if ctx.global_vars.contains(name) {
                let var = ctx.variables.get(name).expect("undefined variable");
                let ty = var.ty.clone();
                emitter.comment(&format!("--${} (global)", name));
                super::stmt::emit_global_load(emitter, ctx, name, &ty);
                emitter.instruction("sub x0, x0, #1");                          // decrement the value by 1
                emit_global_store_inline(emitter, name);
                PhpType::Int
            } else if ctx.ref_params.contains(name) {
                let var = ctx.variables.get(name).expect("undefined variable");
                let offset = var.stack_offset;
                emitter.comment(&format!("--${} (ref)", name));
                emitter.instruction(&format!("ldur x9, [x29, #-{}]", offset));  // load ref pointer
                emitter.instruction("ldr x0, [x9]");                            // dereference
                emitter.instruction("sub x0, x0, #1");                          // decrement
                emitter.instruction("str x0, [x9]");                            // store back through pointer
                PhpType::Int
            } else {
                let var = ctx.variables.get(name).expect("undefined variable");
                let offset = var.stack_offset;
                emitter.comment(&format!("--${}", name));
                // -- pre-decrement: subtract 1 then return new value --
                emitter.instruction(&format!("ldur x0, [x29, #-{}]", offset));  // load current variable value from stack frame
                emitter.instruction("sub x0, x0, #1");                          // decrement the value by 1
                emitter.instruction(&format!("stur x0, [x29, #-{}]", offset));  // store decremented value back to stack frame
                PhpType::Int
            }
        }
        ExprKind::PostDecrement(name) => {
            if ctx.ref_params.contains(name) {
                let var = ctx.variables.get(name).expect("undefined variable");
                let offset = var.stack_offset;
                emitter.comment(&format!("${}-- (ref)", name));
                emitter.instruction(&format!("ldur x9, [x29, #-{}]", offset));  // load ref pointer
                emitter.instruction("ldr x0, [x9]");                            // dereference
                emitter.instruction("sub x1, x0, #1");                          // compute decremented value
                emitter.instruction("str x1, [x9]");                            // store back through pointer
                PhpType::Int
            } else {
                let var = ctx.variables.get(name).expect("undefined variable");
                let offset = var.stack_offset;
                emitter.comment(&format!("${}--", name));
                // -- post-decrement: return old value, then subtract 1 --
                emitter.instruction(&format!("ldur x0, [x29, #-{}]", offset));  // load current value into x0 (returned to caller)
                emitter.instruction("sub x1, x0, #1");                          // compute decremented value in scratch register
                emitter.instruction(&format!("stur x1, [x29, #-{}]", offset));  // write new value back; x0 still has old value
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
            coerce_null_to_zero(emitter, &cond_ty);
            // -- branch based on ternary condition --
            emitter.instruction("cmp x0, #0");                                  // test if condition is falsy
            emitter.instruction(&format!("b.eq {}", else_label));               // jump to else branch if condition was false
            let ty = emit_expr(then_expr, emitter, ctx, data);
            emitter.instruction(&format!("b {}", end_label));                   // skip else branch after evaluating then-expr
            emitter.label(&else_label);
            emit_expr(else_expr, emitter, ctx, data);
            emitter.label(&end_label);
            ty
        }
        ExprKind::Cast { target, expr } => emit_cast(target, expr, emitter, ctx, data),
        ExprKind::FunctionCall { name, args } => {
            if let Some(ty) = super::builtins::emit_builtin_call(name, args, emitter, ctx, data) {
                return ty;
            }
            emit_function_call(name, args, emitter, ctx, data)
        }
        ExprKind::Closure { params, body, is_arrow: _, variadic: _ } => {
            let param_names: Vec<String> = params.iter().map(|(n, _, _)| n.clone()).collect();
            emit_closure(&param_names, body, emitter, ctx, data)
        }
        ExprKind::ClosureCall { var, args } => {
            emit_closure_call(var, args, emitter, ctx, data)
        }
        ExprKind::ConstRef(name) => {
            let (value, _ty) = ctx.constants.get(name).expect("undefined constant").clone();
            let synthetic_expr = Expr::new(value, expr.span);
            emit_expr(&synthetic_expr, emitter, ctx, data)
        }
        ExprKind::BinaryOp { left, op, right } => emit_binop(left, op, right, emitter, ctx, data),
        ExprKind::Spread(inner) => {
            // Spread is handled at call site / array literal level.
            // If we reach here, just evaluate the inner expression.
            emit_expr(inner, emitter, ctx, data)
        }
    }
}

fn emit_array_literal(
    elems: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    if elems.is_empty() {
        // -- allocate empty array with default capacity --
        emitter.instruction("mov x0, #8");                                      // initial capacity: 8 elements
        emitter.instruction("mov x1, #8");                                      // element size: 8 bytes (int-sized)
        emitter.instruction("bl __rt_array_new");                               // call runtime to heap-allocate array struct
        return PhpType::Array(Box::new(PhpType::Int));
    }

    // Check if any element is a spread — requires runtime-index code path
    let has_spread = elems.iter().any(|e| matches!(e.kind, ExprKind::Spread(_)));

    if has_spread {
        return emit_array_literal_with_spread(elems, emitter, ctx, data);
    }

    // -- determine element type and size from first element --
    // Evaluate first element to determine actual type
    let first_ty = {
        // Peek at the expression kind to infer element type without emitting code
        match &elems[0].kind {
            ExprKind::StringLiteral(_) => PhpType::Str,
            ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) => {
                // Nested array — element is a pointer (8 bytes)
                PhpType::Array(Box::new(PhpType::Int)) // placeholder, refined below
            }
            _ => PhpType::Int,
        }
    };
    let es: usize = match &first_ty {
        PhpType::Str => 16,
        _ => 8, // int, bool, float, array pointers are all 8 bytes
    };

    emitter.comment("array literal");
    // -- allocate array and populate elements --
    emitter.instruction(&format!(                                               // capacity: max of element count or 8
        "mov x0, #{}",
        std::cmp::max(elems.len(), 8)
    ));
    emitter.instruction(&format!("mov x1, #{}", es));                           // element size in bytes (8=int/ptr, 16=string)
    emitter.instruction("bl __rt_array_new");                                   // call runtime to heap-allocate array struct
    emitter.instruction("str x0, [sp, #-16]!");                                 // save array pointer on stack while filling

    let mut actual_elem_ty = PhpType::Int;
    for (i, elem) in elems.iter().enumerate() {
        let ty = emit_expr(elem, emitter, ctx, data);
        if i == 0 {
            actual_elem_ty = ty.clone();
        }
        // -- store element value into array at index i --
        emitter.instruction("ldr x9, [sp]");                                    // peek array pointer from stack (no pop)
        match &ty {
            PhpType::Int | PhpType::Bool => {
                emitter.instruction(&format!(                                   // store int/bool element at data offset
                    "str x0, [x9, #{}]",
                    24 + i * 8
                ));
            }
            PhpType::Float => {
                emitter.instruction(&format!(                                   // store float element at data offset
                    "str d0, [x9, #{}]",
                    24 + i * 8
                ));
            }
            PhpType::Str => {
                emitter.instruction(&format!(                                   // store string pointer at data offset
                    "str x1, [x9, #{}]",
                    24 + i * 16
                ));
                emitter.instruction(&format!(                                   // store string length right after pointer
                    "str x2, [x9, #{}]",
                    24 + i * 16 + 8
                ));
            }
            PhpType::Array(_) | PhpType::AssocArray { .. } => {
                emitter.instruction(&format!(                                   // store nested array pointer at data offset
                    "str x0, [x9, #{}]",
                    24 + i * 8
                ));
            }
            _ => {}
        }
        // -- update array length after adding element --
        emitter.instruction(&format!("mov x10, #{}", i + 1));                   // new length after adding this element
        emitter.instruction("str x10, [x9]");                                   // write updated length to array header
    }

    // -- return array pointer --
    emitter.instruction("ldr x0, [sp], #16");                                   // pop array pointer from stack into x0
    PhpType::Array(Box::new(actual_elem_ty))
}

/// Emit an array literal that contains spread elements like `[...$a, 1, ...$b]`.
/// Uses array_push at runtime since spread array lengths are not known at compile time.
fn emit_array_literal_with_spread(
    elems: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("array literal with spread");

    // -- allocate a new array with generous capacity --
    emitter.instruction("mov x0, #16");                                         // initial capacity: 16 elements
    emitter.instruction("mov x1, #8");                                          // element size: 8 bytes (int-sized)
    emitter.instruction("bl __rt_array_new");                                   // allocate destination array
    emitter.instruction("str x0, [sp, #-16]!");                                 // save dest array pointer on stack

    let mut actual_elem_ty = PhpType::Int;

    for (i, elem) in elems.iter().enumerate() {
        if let ExprKind::Spread(inner) = &elem.kind {
            // -- spread: copy all elements from source array into dest array --
            emitter.comment("spread array into dest");
            let _src_ty = emit_expr(inner, emitter, ctx, data);
            // x0 = source array pointer
            emitter.instruction("mov x1, x0");                                  // x1 = source array pointer
            emitter.instruction("ldr x0, [sp]");                                // x0 = dest array pointer (peek)
            emitter.instruction("bl __rt_array_merge_into");                    // append all src elements to dest array
        } else {
            // -- regular element: push single value --
            let ty = emit_expr(elem, emitter, ctx, data);
            if i == 0 || actual_elem_ty == PhpType::Int {
                actual_elem_ty = ty.clone();
            }
            emitter.instruction("ldr x9, [sp]");                                // peek dest array pointer from stack
            match &ty {
                PhpType::Int | PhpType::Bool => {
                    // -- push int element using runtime push --
                    emitter.instruction("mov x1, x0");                          // x1 = value to push
                    emitter.instruction("mov x0, x9");                          // x0 = array pointer
                    emitter.instruction("bl __rt_array_push_int");              // push value onto array
                }
                PhpType::Float => {
                    // -- push float element --
                    emitter.instruction("fmov x1, d0");                         // move float bits to int register
                    emitter.instruction("mov x0, x9");                          // x0 = array pointer
                    emitter.instruction("bl __rt_array_push_int");              // push value onto array
                }
                _ => {
                    emitter.instruction("mov x1, x0");                          // x1 = value to push
                    emitter.instruction("mov x0, x9");                          // x0 = array pointer
                    emitter.instruction("bl __rt_array_push_int");              // push value onto array
                }
            }
        }
    }

    // -- return array pointer --
    emitter.instruction("ldr x0, [sp], #16");                                   // pop dest array pointer from stack into x0
    PhpType::Array(Box::new(actual_elem_ty))
}

fn emit_assoc_array_literal(
    pairs: &[(Expr, Expr)],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("assoc array literal");

    // -- determine value type from first pair --
    let value_type_tag = match &pairs[0].1.kind {
        ExprKind::StringLiteral(_) => 1,
        ExprKind::FloatLiteral(_) => 2,
        ExprKind::BoolLiteral(_) => 3,
        _ => 0, // int
    };

    // -- create hash table --
    emitter.instruction(&format!(                                               // initial capacity: max of pair count*2 or 16
        "mov x0, #{}",
        std::cmp::max(pairs.len() * 2, 16)
    ));
    emitter.instruction(&format!("mov x1, #{}", value_type_tag));               // value type tag
    emitter.instruction("bl __rt_hash_new");                                    // create hash table → x0=ptr
    emitter.instruction("str x0, [sp, #-16]!");                                 // save hash table pointer

    let mut val_ty = PhpType::Int;
    for pair in pairs {
        // -- evaluate key --
        emit_expr(&pair.0, emitter, ctx, data);
        // key is a string → x1/x2
        emitter.instruction("stp x1, x2, [sp, #-16]!");                         // save key ptr/len
        // -- evaluate value --
        let ty = emit_expr(&pair.1, emitter, ctx, data);
        val_ty = ty.clone();
        // -- prepare args for hash_set --
        let (val_lo, val_hi) = match &ty {
            PhpType::Int | PhpType::Bool => ("x0", "xzr"),
            PhpType::Str => ("x1", "x2"),
            PhpType::Float => {
                emitter.instruction("fmov x9, d0");                             // move float bits to integer register
                ("x9", "xzr")
            }
            _ => ("x0", "xzr"),
        };
        emitter.instruction(&format!("mov x3, {}", val_lo));                    // value_lo
        emitter.instruction(&format!("mov x4, {}", val_hi));                    // value_hi
        emitter.instruction("ldp x1, x2, [sp], #16");                           // pop key ptr/len
        emitter.instruction("ldr x0, [sp]");                                    // peek hash table pointer
        emitter.instruction("bl __rt_hash_set");                                // insert key-value pair
    }

    // -- return hash table pointer --
    emitter.instruction("ldr x0, [sp], #16");                                   // pop hash table pointer into x0

    let key_ty = match &pairs[0].0.kind {
        ExprKind::IntLiteral(_) => PhpType::Int,
        _ => PhpType::Str,
    };

    PhpType::AssocArray {
        key: Box::new(key_ty),
        value: Box::new(val_ty),
    }
}

fn emit_match_expr(
    subject: &Expr,
    arms: &[(Vec<Expr>, Expr)],
    default: &Option<Box<Expr>>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("match expression");
    let subj_ty = emit_expr(subject, emitter, ctx, data);
    // -- save subject value --
    match &subj_ty {
        PhpType::Str => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // save string subject
        }
        PhpType::Float => {
            emitter.instruction("str d0, [sp, #-16]!");                         // save float subject
        }
        _ => {
            emitter.instruction("str x0, [sp, #-16]!");                         // save int/bool subject
        }
    }

    let end_label = ctx.next_label("match_end");
    let mut result_ty = PhpType::Void;

    // -- generate comparison for each arm --
    for (patterns, result) in arms {
        let arm_label = ctx.next_label("match_arm");
        let next_arm = ctx.next_label("match_next");

        for (i, pattern) in patterns.iter().enumerate() {
            let pat_ty = emit_expr(pattern, emitter, ctx, data);
            // -- reload subject for comparison --
            match &subj_ty {
                PhpType::Str => {
                    // pattern result in x1/x2, subject on stack
                    emitter.instruction("mov x3, x1");                          // move pattern ptr to x3
                    emitter.instruction("mov x4, x2");                          // move pattern len to x4
                    emitter.instruction("ldp x1, x2, [sp]");                    // peek subject string
                    emitter.instruction("bl __rt_str_eq");                      // compare strings → x0=1 if equal
                }
                PhpType::Float => {
                    emitter.instruction("ldr d1, [sp]");                        // peek subject float
                    emitter.instruction("fcmp d1, d0");                         // compare floats
                    emitter.instruction("cset x0, eq");                         // x0=1 if equal
                }
                _ => {
                    emitter.instruction("ldr x9, [sp]");                        // peek subject int/bool
                    emitter.instruction("cmp x9, x0");                          // compare integers
                    emitter.instruction("cset x0, eq");                         // x0=1 if equal
                }
            }
            emitter.instruction(&format!("cbnz x0, {}", arm_label));            // if matched, jump to arm body
            if i == patterns.len() - 1 {
                emitter.instruction(&format!("b {}", next_arm));                // no pattern matched → try next arm
            }
            let _ = pat_ty;
        }

        emitter.label(&arm_label);
        result_ty = emit_expr(result, emitter, ctx, data);
        emitter.instruction(&format!("b {}", end_label));                       // jump to end after evaluating arm

        emitter.label(&next_arm);
    }

    // -- default arm --
    if let Some(def) = default {
        result_ty = emit_expr(def, emitter, ctx, data);
    }

    emitter.label(&end_label);
    // -- pop saved subject --
    emitter.instruction("add sp, sp, #16");                                     // deallocate subject save slot

    result_ty
}

fn emit_array_access(
    array: &Expr,
    index: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let arr_ty = emit_expr(array, emitter, ctx, data);

    // -- check if this is an associative array access --
    if let PhpType::AssocArray { value, .. } = &arr_ty {
        let val_ty = *value.clone();
        // -- save hash table pointer, evaluate key expression --
        emitter.instruction("str x0, [sp, #-16]!");                             // push hash table pointer
        let _key_ty = emit_expr(index, emitter, ctx, data);
        // key is in x1/x2 (string)
        emitter.instruction("mov x3, x1");                                      // move key ptr to x3 (temp)
        emitter.instruction("mov x4, x2");                                      // move key len to x4 (temp)
        emitter.instruction("ldr x0, [sp], #16");                               // pop hash table pointer
        emitter.instruction("mov x1, x3");                                      // key ptr
        emitter.instruction("mov x2, x4");                                      // key len
        emitter.comment("assoc array access");
        emitter.instruction("bl __rt_hash_get");                                // lookup key → x0=found, x1=val_lo, x2=val_hi
        // Result is in x1/x2 (for strings) or x1 (for ints)
        match &val_ty {
            PhpType::Int | PhpType::Bool => {
                emitter.instruction("mov x0, x1");                              // move value to x0
            }
            PhpType::Str => {
                // x1=ptr, x2=len already set by hash_get
            }
            PhpType::Float => {
                emitter.instruction("fmov d0, x1");                             // move bits to float register
            }
            _ => {
                emitter.instruction("mov x0, x1");                              // move value to x0
            }
        }
        return val_ty;
    }

    // -- indexed array access (existing code) --
    // -- save array pointer, evaluate index expression --
    emitter.instruction("str x0, [sp, #-16]!");                                 // push array pointer while evaluating index
    emit_expr(index, emitter, ctx, data);
    emitter.instruction("ldr x9, [sp], #16");                                   // pop array pointer into scratch register x9
    emitter.comment("array access");
    let elem_ty = match &arr_ty {
        PhpType::Array(t) => *t.clone(),
        _ => PhpType::Int,
    };

    // -- bounds check: negative index or >= array length → null sentinel --
    let null_label = ctx.next_label("arr_null");
    let ok_label = ctx.next_label("arr_ok");
    emitter.instruction(&format!("cmp x0, #0"));                                // check if index is negative
    emitter.instruction(&format!("b.lt {null_label}"));                         // negative index → null sentinel
    emitter.instruction("ldr x10, [x9]");                                       // load array length from header (offset 0)
    emitter.instruction(&format!("cmp x0, x10"));                               // compare index against array length
    emitter.instruction(&format!("b.ge {null_label}"));                         // index >= length → null sentinel

    match &elem_ty {
        PhpType::Int => {
            // -- load integer element: base + 24-byte header + index*8 --
            emitter.instruction("add x9, x9, #24");                             // skip 24-byte array header to reach data
            emitter.instruction("ldr x0, [x9, x0, lsl #3]");                    // load element at x9 + index*8
        }
        PhpType::Str => {
            // -- load string element: base + 24-byte header + index*16 --
            emitter.instruction("lsl x0, x0, #4");                              // multiply index by 16 (string = ptr+len pair)
            emitter.instruction("add x9, x9, x0");                              // add scaled index offset to array base
            emitter.instruction("add x9, x9, #24");                             // skip 24-byte array header to reach data
            emitter.instruction("ldr x1, [x9]");                                // load string pointer from element slot
            emitter.instruction("ldr x2, [x9, #8]");                            // load string length from element slot
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            // -- load nested array/hash pointer: base + 24-byte header + index*8 --
            emitter.instruction("add x9, x9, #24");                             // skip 24-byte array header to reach data
            emitter.instruction("ldr x0, [x9, x0, lsl #3]");                    // load nested array pointer at index
        }
        _ => {}
    }
    emitter.instruction(&format!("b {ok_label}"));                              // skip null sentinel fallback

    // -- null sentinel for out-of-bounds access --
    emitter.label(&null_label);
    emitter.instruction("movz x0, #0xFFFE");                                    // load lowest 16 bits of null sentinel
    emitter.instruction("movk x0, #0xFFFF, lsl #16");                           // insert bits 16-31 of null sentinel
    emitter.instruction("movk x0, #0xFFFF, lsl #32");                           // insert bits 32-47 of null sentinel
    emitter.instruction("movk x0, #0x7FFF, lsl #48");                           // insert bits 48-63 of null sentinel
    emitter.label(&ok_label);

    elem_ty
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
        PhpType::Str | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Callable => {}
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
            coerce_null_to_zero(emitter, &lt);
            // -- short-circuit AND: skip right side if left is falsy --
            emitter.instruction("cmp x0, #0");                                  // test if left operand is falsy
            emitter.instruction(&format!("b.eq {}", end_label));                // short-circuit: left is false so result is 0
            let rt = emit_expr(right, emitter, ctx, data);
            coerce_null_to_zero(emitter, &rt);
            // -- evaluate right operand truthiness --
            emitter.instruction("cmp x0, #0");                                  // test if right operand is falsy
            emitter.instruction("cset x0, ne");                                 // result=1 if right is truthy, 0 if falsy
            emitter.label(&end_label);
            return PhpType::Bool;
        }
        BinOp::Or => {
            let end_label = ctx.next_label("or_end");
            let lt = emit_expr(left, emitter, ctx, data);
            coerce_null_to_zero(emitter, &lt);
            // -- short-circuit OR: skip right side if left is truthy --
            emitter.instruction("cmp x0, #0");                                  // test if left operand is truthy
            emitter.instruction(&format!("b.ne {}", end_label));                // short-circuit: left is true, skip right
            let rt = emit_expr(right, emitter, ctx, data);
            coerce_null_to_zero(emitter, &rt);
            emitter.label(&end_label);
            // -- normalize final value to boolean 0 or 1 --
            emitter.instruction("cmp x0, #0");                                  // test whichever operand survived
            emitter.instruction("cset x0, ne");                                 // normalize to 1 if truthy, 0 if falsy
            return PhpType::Bool;
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
            return PhpType::Float;
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
                        // -- float modulo: a - floor(a/b) * b --
                        emitter.instruction("fdiv d2, d1, d0");                 // d2 = left / right
                        emitter.instruction("frintm d2, d2");                   // d2 = floor(left / right)
                        emitter.instruction("fmsub d0, d2, d0, d1");            // d0 = left - floor(l/r)*right
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
        BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq => {
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
                BinOp::Eq => "eq",
                BinOp::NotEq => "ne",
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
            return emit_strict_compare(left, op, right, emitter, ctx, data);
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
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor
        | BinOp::ShiftLeft | BinOp::ShiftRight => {
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
            return emit_null_coalesce(left, right, emitter, ctx, data);
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

    // Get function sig to check for default parameter values and variadic
    let sig = ctx.functions.get(name).cloned();
    let is_variadic = sig.as_ref().map(|s| s.variadic.is_some()).unwrap_or(false);

    // Determine how many regular (non-variadic) params the function has
    let regular_param_count = if is_variadic {
        // The last param in sig.params is the variadic array param — don't count it
        sig.as_ref().map(|s| s.params.len().saturating_sub(1)).unwrap_or(0)
    } else {
        sig.as_ref().map(|s| s.params.len()).unwrap_or(args.len())
    };

    // Separate regular args from variadic args
    // Also detect spread args: func(...$arr) passes array directly as variadic
    let mut regular_args: Vec<&Expr> = Vec::new();
    let mut variadic_args: Vec<&Expr> = Vec::new();
    let mut spread_arg: Option<&Expr> = None;

    for (i, arg) in args.iter().enumerate() {
        if let ExprKind::Spread(inner) = &arg.kind {
            // Spread arg — the inner expr should be an array
            spread_arg = Some(inner.as_ref());
        } else if is_variadic && i >= regular_param_count {
            variadic_args.push(arg);
        } else {
            regular_args.push(arg);
        }
    }

    // Build full argument list: regular args + defaults for missing regular params
    let mut all_args: Vec<&Expr> = regular_args;
    let mut default_exprs: Vec<Expr> = Vec::new();

    if let Some(ref s) = sig {
        for i in all_args.len()..regular_param_count {
            if let Some(Some(default)) = s.defaults.get(i) {
                default_exprs.push(default.clone());
            }
        }
    }
    let default_refs: Vec<&Expr> = default_exprs.iter().collect();
    all_args.extend(default_refs);

    let ref_params = sig.as_ref().map(|s| s.ref_params.clone()).unwrap_or_default();

    // -- evaluate and push regular args onto stack --
    let mut arg_types = Vec::new();
    for (i, arg) in all_args.iter().enumerate() {
        let is_ref = ref_params.get(i).copied().unwrap_or(false);
        if is_ref {
            // For ref params, push the ADDRESS of the variable, not its value
            if let ExprKind::Variable(var_name) = &arg.kind {
                if ctx.global_vars.contains(var_name) {
                    let label = format!("_gvar_{}", var_name);
                    emitter.comment(&format!("ref arg: address of global ${}", var_name));
                    emitter.instruction(&format!("adrp x0, {}@PAGE", label));   // load page of global var
                    emitter.instruction(&format!("add x0, x0, {}@PAGEOFF", label)); // add page offset
                } else if ctx.in_main && ctx.all_global_var_names.contains(var_name) {
                    let var = ctx.variables.get(var_name).expect("undefined variable");
                    let offset = var.stack_offset;
                    emitter.comment(&format!("ref arg: address of ${}", var_name));
                    emitter.instruction(&format!("sub x0, x29, #{}", offset));  // compute address of local variable
                } else {
                    let var = ctx.variables.get(var_name).expect("undefined variable");
                    let offset = var.stack_offset;
                    emitter.comment(&format!("ref arg: address of ${}", var_name));
                    emitter.instruction(&format!("sub x0, x29, #{}", offset));  // compute address of local variable
                }
            } else {
                emit_expr(arg, emitter, ctx, data);
            }
            emitter.instruction("str x0, [sp, #-16]!");                         // push address onto stack
            arg_types.push(PhpType::Int);
        } else {
            let ty = emit_expr(arg, emitter, ctx, data);
            match &ty {
                PhpType::Bool | PhpType::Int | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Callable => {
                    emitter.instruction("str x0, [sp, #-16]!");                 // push int/bool/array-ptr/callable arg onto stack
                }
                PhpType::Float => {
                    emitter.instruction("str d0, [sp, #-16]!");                 // push float arg onto stack
                }
                PhpType::Str => {
                    emitter.instruction("stp x1, x2, [sp, #-16]!");             // push string ptr+len arg onto stack
                }
                PhpType::Void => {}
            }
            arg_types.push(ty);
        }
    }

    // -- build variadic array if this function has a variadic parameter --
    if is_variadic {
        if let Some(spread_expr) = spread_arg {
            // Spread: ...$arr — pass the array directly as the variadic param
            emitter.comment("spread array as variadic param");
            let _ty = emit_expr(spread_expr, emitter, ctx, data);
            // Array pointer is in x0
            emitter.instruction("str x0, [sp, #-16]!");                         // push variadic array pointer onto stack
        } else if variadic_args.is_empty() {
            // No variadic args — create an empty array
            emitter.comment("empty variadic array");
            emitter.instruction("mov x0, #8");                                  // initial capacity: 8 elements
            emitter.instruction("mov x1, #8");                                  // element size: 8 bytes
            emitter.instruction("bl __rt_array_new");                           // allocate empty array for variadic param
            emitter.instruction("str x0, [sp, #-16]!");                         // push empty variadic array onto stack
        } else {
            // Build array from variadic args
            let n = variadic_args.len();
            emitter.comment(&format!("build variadic array ({} elements)", n));
            // -- determine element type from first variadic arg --
            let first_elem_ty = match &variadic_args[0].kind {
                ExprKind::StringLiteral(_) => PhpType::Str,
                _ => PhpType::Int,
            };
            let es: usize = match &first_elem_ty {
                PhpType::Str => 16,
                _ => 8,
            };
            emitter.instruction(&format!(                                       // capacity: max of element count or 8
                "mov x0, #{}",
                std::cmp::max(n, 8)
            ));
            emitter.instruction(&format!("mov x1, #{}", es));                   // element size in bytes
            emitter.instruction("bl __rt_array_new");                           // allocate array for variadic args
            emitter.instruction("str x0, [sp, #-16]!");                         // save variadic array pointer on stack

            for (i, varg) in variadic_args.iter().enumerate() {
                let ty = emit_expr(varg, emitter, ctx, data);
                emitter.instruction("ldr x9, [sp]");                            // peek variadic array pointer from stack
                match &ty {
                    PhpType::Int | PhpType::Bool => {
                        emitter.instruction(&format!(                           // store int element at data offset
                            "str x0, [x9, #{}]",
                            24 + i * 8
                        ));
                    }
                    PhpType::Float => {
                        emitter.instruction(&format!(                           // store float element at data offset
                            "str d0, [x9, #{}]",
                            24 + i * 8
                        ));
                    }
                    PhpType::Str => {
                        emitter.instruction(&format!(                           // store string pointer at data offset
                            "str x1, [x9, #{}]",
                            24 + i * 16
                        ));
                        emitter.instruction(&format!(                           // store string length right after pointer
                            "str x2, [x9, #{}]",
                            24 + i * 16 + 8
                        ));
                    }
                    PhpType::Array(_) | PhpType::AssocArray { .. } => {
                        emitter.instruction(&format!(                           // store nested array pointer at data offset
                            "str x0, [x9, #{}]",
                            24 + i * 8
                        ));
                    }
                    _ => {}
                }
                // -- update array length --
                emitter.instruction(&format!("mov x10, #{}", i + 1));           // new length after adding this element
                emitter.instruction("str x10, [x9]");                           // write updated length to array header
            }
            // Array pointer is already on the stack (pushed earlier)
        }
        arg_types.push(PhpType::Array(Box::new(PhpType::Int)));
    }

    // Separate int and float register tracking
    let total_args = all_args.len() + if is_variadic { 1 } else { 0 };
    let mut assignments: Vec<(PhpType, usize, bool)> = Vec::new();
    let mut int_reg_idx = 0usize;
    let mut float_reg_idx = 0usize;
    for ty in &arg_types {
        if ty.is_float_reg() {
            assignments.push((ty.clone(), float_reg_idx, true));
            float_reg_idx += 1;
        } else {
            assignments.push((ty.clone(), int_reg_idx, false));
            int_reg_idx += ty.register_count();
        }
    }

    // -- pop arguments from stack into ABI registers in reverse order --
    for i in (0..total_args).rev() {
        let (ty, start_reg, is_float) = &assignments[i];
        match ty {
            PhpType::Bool | PhpType::Int | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Callable => {
                emitter.instruction(&format!(                                   // pop int/bool/array/callable arg into register
                    "ldr x{}, [sp], #16",
                    start_reg
                ));
            }
            PhpType::Float => {
                emitter.instruction(&format!(                                   // pop float arg into float register
                    "ldr d{}, [sp], #16",
                    start_reg
                ));
            }
            PhpType::Str => {
                emitter.instruction(&format!(                                   // pop string ptr+len into consecutive regs
                    "ldp x{}, x{}, [sp], #16",
                    start_reg,
                    start_reg + 1
                ));
            }
            PhpType::Void => {}
        }
        let _ = is_float;
    }

    // -- call the user-defined PHP function --
    emitter.instruction(&format!("bl _fn_{}", name));                           // branch-and-link to compiled PHP function

    ctx.functions
        .get(name)
        .map(|sig| sig.return_type.clone())
        .unwrap_or(PhpType::Void)
}

fn emit_closure(
    params: &[String],
    body: &[crate::parser::ast::Stmt],
    emitter: &mut Emitter,
    ctx: &mut Context,
    _data: &mut DataSection,
) -> PhpType {
    let closure_label = ctx.next_label("closure");

    // -- build a FunctionSig for the closure (params default to Int) --
    let param_types: Vec<(String, crate::types::PhpType)> = params
        .iter()
        .map(|p| (p.clone(), PhpType::Int))
        .collect();
    let defaults: Vec<Option<crate::parser::ast::Expr>> = params.iter().map(|_| None).collect();
    let ref_params: Vec<bool> = params.iter().map(|_| false).collect();
    let sig = crate::types::FunctionSig {
        params: param_types,
        defaults,
        return_type: PhpType::Int,
        ref_params,
        variadic: None,
    };

    // -- defer the closure body emission for later --
    ctx.deferred_closures.push(super::context::DeferredClosure {
        label: closure_label.clone(),
        params: params.to_vec(),
        body: body.to_vec(),
        sig,
    });

    emitter.comment("closure: load function address");
    // -- load closure function address into x0 --
    emitter.instruction(&format!("adrp x0, {}@PAGE", closure_label));           // load page base of closure function
    emitter.instruction(&format!("add x0, x0, {}@PAGEOFF", closure_label));     // add page offset to get exact closure address
    PhpType::Callable
}

fn emit_closure_call(
    var: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment(&format!("call ${}()", var));

    // -- evaluate arguments and push onto stack --
    let mut arg_types = Vec::new();
    for arg in args {
        let ty = emit_expr(arg, emitter, ctx, data);
        match &ty {
            PhpType::Bool | PhpType::Int | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Callable => {
                emitter.instruction("str x0, [sp, #-16]!");                     // push int/bool/array/callable arg onto stack
            }
            PhpType::Float => {
                emitter.instruction("str d0, [sp, #-16]!");                     // push float arg onto stack
            }
            PhpType::Str => {
                emitter.instruction("stp x1, x2, [sp, #-16]!");                 // push string ptr+len arg onto stack
            }
            PhpType::Void => {}
        }
        arg_types.push(ty);
    }

    // -- load the closure function address from the variable's stack slot --
    let var_info = ctx.variables.get(var).expect("undefined closure variable");
    let var_offset = var_info.stack_offset;
    emitter.instruction(&format!("ldur x9, [x29, #-{}]", var_offset));          // load closure function address from stack

    // -- save closure address while we pop args into registers --
    emitter.instruction("str x9, [sp, #-16]!");                                 // push closure address temporarily

    // -- pop arguments from stack into ABI registers in reverse order --
    // Separate int and float register tracking
    let mut assignments: Vec<(PhpType, usize, bool)> = Vec::new();
    let mut int_reg_idx = 0usize;
    let mut float_reg_idx = 0usize;
    for ty in &arg_types {
        if ty.is_float_reg() {
            assignments.push((ty.clone(), float_reg_idx, true));
            float_reg_idx += 1;
        } else {
            assignments.push((ty.clone(), int_reg_idx, false));
            int_reg_idx += ty.register_count();
        }
    }

    // Pop closure address first (it's on top of args stack)
    emitter.instruction("ldr x9, [sp], #16");                                   // pop closure function address into x9

    for i in (0..args.len()).rev() {
        let (ty, start_reg, _is_float) = &assignments[i];
        match ty {
            PhpType::Bool | PhpType::Int | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Callable => {
                emitter.instruction(&format!("ldr x{}, [sp], #16", start_reg)); // pop int/bool/array/callable arg into register
            }
            PhpType::Float => {
                emitter.instruction(&format!("ldr d{}, [sp], #16", start_reg)); // pop float arg into float register
            }
            PhpType::Str => {
                emitter.instruction(&format!(                                   // pop string ptr+len into consecutive regs
                    "ldp x{}, x{}, [sp], #16",
                    start_reg,
                    start_reg + 1
                ));
            }
            PhpType::Void => {}
        }
    }

    // -- indirect call through closure function address --
    emitter.instruction("blr x9");                                              // branch to closure via function pointer in x9
    PhpType::Int
}

fn emit_cast(
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
                    // -- truncate float to integer --
                    emitter.instruction("fcvtzs x0, d0");                       // convert double to signed 64-bit int (toward zero)
                }
                PhpType::Bool => {} // already 0/1 in x0
                PhpType::Void => {
                    // -- (int)null === 0 in PHP --
                    emitter.instruction("mov x0, #0");                          // null casts to integer zero
                }
                PhpType::Str => {
                    // -- parse string as integer --
                    emitter.instruction("bl __rt_atoi");                        // runtime: ASCII string to integer conversion
                }
                PhpType::Array(_) | PhpType::AssocArray { .. } => {
                    // -- (int)array returns element count --
                    emitter.instruction("ldr x0, [x0]");                        // load array length from header (first field)
                }
                PhpType::Callable => {} // callable address already in x0
            }
            PhpType::Int
        }
        CastType::Float => {
            match &src_ty {
                PhpType::Float => {}
                PhpType::Int | PhpType::Bool => {
                    // -- convert integer/bool to double-precision float --
                    emitter.instruction("scvtf d0, x0");                        // signed int to double conversion
                }
                PhpType::Void => {
                    // -- (float)null === 0.0 in PHP --
                    emitter.instruction("mov x0, #0");                          // load zero integer
                    emitter.instruction("scvtf d0, x0");                        // convert to 0.0 double
                }
                PhpType::Str => {
                    // -- parse string as float via libc atof --
                    emitter.instruction("bl __rt_cstr");                        // null-terminate string (x1=ptr, x2=len → x0=cstr)
                    emitter.instruction("bl _atof");                            // parse C string as double → d0=result
                }
                PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Callable => {
                    // -- unsupported source type: default to 0.0 --
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
                    // -- convert int/null to boolean --
                    emitter.instruction("cmp x0, #0");                          // test if value is zero
                    emitter.instruction("cset x0, ne");                         // x0=1 if nonzero (truthy), 0 if zero (falsy)
                }
                PhpType::Float => {
                    // -- convert float to boolean --
                    emitter.instruction("fcmp d0, #0.0");                       // compare float against zero
                    emitter.instruction("cset x0, ne");                         // x0=1 if nonzero, 0 if zero
                }
                PhpType::Str => {
                    // empty string or "0" → false, everything else → true
                    // -- convert string to boolean --
                    emitter.instruction("cmp x2, #0");                          // check if string length is zero
                    emitter.instruction("cset x0, ne");                         // x0=1 if non-empty, 0 if empty
                }
                PhpType::Array(_) | PhpType::AssocArray { .. } => {
                    // empty array → false
                    // -- convert array to boolean based on element count --
                    emitter.instruction("ldr x0, [x0]");                        // load array length from header
                    emitter.instruction("cmp x0, #0");                          // check if array is empty
                    emitter.instruction("cset x0, ne");                         // x0=1 if non-empty, 0 if empty
                }
                PhpType::Callable => {
                    // -- callable is always truthy --
                    emitter.instruction("cmp x0, #0");                          // test if callable address is zero
                    emitter.instruction("cset x0, ne");                         // x0=1 if nonzero (truthy)
                }
            }
            PhpType::Bool
        }
        CastType::Array => {
            // Wrap scalar in single-element array
            match &src_ty {
                PhpType::Array(_) | PhpType::AssocArray { .. } => { return src_ty; }
                PhpType::Int | PhpType::Bool | PhpType::Callable => {
                    // -- wrap scalar in a new single-element array --
                    emitter.instruction("str x0, [sp, #-16]!");                 // save scalar value during allocation
                    emitter.instruction("mov x0, #8");                          // initial capacity: 8 elements
                    emitter.instruction("mov x1, #8");                          // element size: 8 bytes
                    emitter.instruction("bl __rt_array_new");                   // allocate new array struct
                    emitter.instruction("ldr x1, [sp], #16");                   // pop saved scalar value
                    emitter.instruction("bl __rt_array_push_int");              // push scalar as first element
                }
                _ => {
                    // For other types, create empty array
                    // -- create empty array for unsupported cast source --
                    emitter.instruction("mov x0, #8");                          // initial capacity: 8 elements
                    emitter.instruction("mov x1, #8");                          // element size: 8 bytes
                    emitter.instruction("bl __rt_array_new");                   // allocate empty array struct
                }
            }
            PhpType::Array(Box::new(PhpType::Int))
        }
    }
}

fn emit_strict_compare(
    left: &Expr,
    op: &BinOp,
    right: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let is_eq = *op == BinOp::StrictEq;
    emitter.comment(if is_eq { "===" } else { "!==" });

    // Peek at compile-time types to decide the strategy
    let lt_peek = peek_expr_type(left, ctx);
    let rt_peek = peek_expr_type(right, ctx);

    let types_match = match (&lt_peek, &rt_peek) {
        (Some(l), Some(r)) => l == r,
        _ => true, // unknown → assume they might match, use save path
    };

    let lt = emit_expr(left, emitter, ctx, data);

    if types_match {
        // Types match (or unknown): save left, compare values
        // -- save left operand for strict comparison --
        match &lt {
            PhpType::Float => {
                emitter.instruction("str d0, [sp, #-16]!");                     // push left float for later comparison
            }
            PhpType::Str => {
                emitter.instruction("stp x1, x2, [sp, #-16]!");                 // push left string ptr+len for comparison
            }
            _ => {
                emitter.instruction("str x0, [sp, #-16]!");                     // push left int/bool/null for comparison
            }
        }

        let rt = emit_expr(right, emitter, ctx, data);

        if lt != rt {
            // Types turned out different at emit time (unknown peek case)
            // -- type mismatch: strict compare result is predetermined --
            emitter.instruction("add sp, sp, #16");                             // discard saved left operand from stack
            emitter.instruction(&format!(                                       // === yields false, !== yields true
                "mov x0, #{}",
                if is_eq { 0 } else { 1 }
            ));
            return PhpType::Bool;
        }

        match &lt {
            PhpType::Int | PhpType::Bool | PhpType::Void => {
                // -- strict compare integers/bools/nulls by value --
                emitter.instruction("ldr x1, [sp], #16");                       // pop saved left operand from stack
                emitter.instruction("cmp x1, x0");                              // compare left vs right values
                let cond = if is_eq { "eq" } else { "ne" };
                emitter.instruction(&format!("cset x0, {}", cond));             // set boolean result from comparison
            }
            PhpType::Float => {
                // -- strict compare floats --
                emitter.instruction("ldr d1, [sp], #16");                       // pop saved left float operand
                emitter.instruction("fcmp d1, d0");                             // compare two doubles, setting NZCV flags
                let cond = if is_eq { "eq" } else { "ne" };
                emitter.instruction(&format!("cset x0, {}", cond));             // set boolean result from float comparison
            }
            PhpType::Str => {
                // -- strict compare strings by content --
                emitter.instruction("mov x3, x1");                              // move right string pointer to 3rd arg
                emitter.instruction("mov x4, x2");                              // move right string length to 4th arg
                emitter.instruction("ldp x1, x2, [sp], #16");                   // pop left string ptr/len into 1st/2nd args
                emitter.instruction("bl __rt_str_eq");                          // runtime: byte-by-byte string comparison
                if !is_eq {
                    emitter.instruction("eor x0, x0, #1");                      // invert result for !== (XOR with 1)
                }
            }
            PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Callable => {
                // -- strict compare arrays/callables by reference (pointer equality) --
                emitter.instruction("ldr x1, [sp], #16");                       // pop saved left array/callable pointer
                emitter.instruction("cmp x1, x0");                              // compare pointers (reference equality)
                let cond = if is_eq { "eq" } else { "ne" };
                emitter.instruction(&format!("cset x0, {}", cond));             // set boolean result from pointer comparison
            }
        }
    } else {
        // Types known to differ: evaluate right for side effects, emit constant
        emit_expr(right, emitter, ctx, data);
        // -- types differ at compile time: strict result is predetermined --
        emitter.instruction(&format!(                                           // === always false, !== always true
            "mov x0, #{}",
            if is_eq { 0 } else { 1 }
        ));
    }

    PhpType::Bool
}

/// Peek at the compile-time type of an expression without emitting code.
/// Returns None if the type can't be determined cheaply (e.g., complex expressions).
fn peek_expr_type(expr: &Expr, ctx: &Context) -> Option<PhpType> {
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

fn emit_null_coalesce(
    value: &Expr,
    default: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("null coalesce ??");
    let val_ty = emit_expr(value, emitter, ctx, data);

    if val_ty == PhpType::Void {
        // Compile-time null: always use default
        return emit_expr(default, emitter, ctx, data);
    }

    // -- check if value is null sentinel at runtime --
    let end_label = ctx.next_label("nc_end");
    // -- build null sentinel value for comparison --
    emitter.instruction("movz x9, #0xFFFE");                                    // load lowest 16 bits of null sentinel
    emitter.instruction("movk x9, #0xFFFF, lsl #16");                           // insert bits 16-31 of null sentinel
    emitter.instruction("movk x9, #0xFFFF, lsl #32");                           // insert bits 32-47 of null sentinel
    emitter.instruction("movk x9, #0x7FFF, lsl #48");                           // insert bits 48-63 of null sentinel
    emitter.instruction("cmp x0, x9");                                          // compare value against null sentinel
    emitter.instruction(&format!("b.ne {}", end_label));                        // if not null, skip default and keep value

    // -- value is null, evaluate default expression --
    let def_ty = emit_expr(default, emitter, ctx, data);
    emitter.label(&end_label);

    // Return the type of the non-null side
    if val_ty == PhpType::Void { def_ty } else { val_ty }
}

/// Quick helper: store x0 to _gvar_NAME (for pre-increment etc. where value is in x0)
fn emit_global_store_inline(emitter: &mut Emitter, name: &str) {
    let label = format!("_gvar_{}", name);
    emitter.instruction(&format!("adrp x9, {}@PAGE", label));                   // load page of global var storage
    emitter.instruction(&format!("add x9, x9, {}@PAGEOFF", label));             // add page offset
    emitter.instruction("str x0, [x9]");                                        // store value to global storage
}

fn load_immediate(emitter: &mut Emitter, reg: &str, value: i64) {
    if value >= 0 && value <= 65535 {
        // -- small non-negative immediate fits in single MOV --
        emitter.instruction(&format!("mov {}, #{}", reg, value));               // load small immediate value directly
    } else if value < 0 && value >= -65536 {
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
