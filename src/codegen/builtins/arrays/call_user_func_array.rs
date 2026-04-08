use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::expr::calls::args;
use crate::codegen::abi;
use crate::names::function_symbol;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

fn emit_array_value_type_stamp(emitter: &mut Emitter, array_reg: &str, elem_ty: &PhpType) {
    let value_type_tag = match elem_ty {
        PhpType::Float => 2,
        PhpType::Bool => 3,
        PhpType::Str => 1,
        PhpType::Array(_) => 4,
        PhpType::AssocArray { .. } => 5,
        PhpType::Object(_) => 6,
        PhpType::Mixed => 7,
        PhpType::Union(_) => 7,
        PhpType::Void => 8,
        _ => return,
    };
    emitter.instruction(&format!("ldr x10, [{}, #-8]", array_reg));             // load the packed array kind word from the heap header
    emitter.instruction("mov x12, #0x80ff");                                    // preserve the indexed-array kind and persistent COW flag
    emitter.instruction("and x10, x10, x12");                                   // keep only the persistent indexed-array metadata bits
    emitter.instruction(&format!("mov x11, #{}", value_type_tag));              // materialize the runtime array value_type tag
    emitter.instruction("lsl x11, x11, #8");                                    // move the value_type tag into the packed kind-word byte lane
    emitter.instruction("orr x10, x10, x11");                                   // combine the heap kind with the array value_type tag
    emitter.instruction(&format!("str x10, [{}, #-8]", array_reg));             // persist the packed array kind word in the heap header
}

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("call_user_func_array()");

    // -- resolve callback function address and signature --
    let is_callable_expr = matches!(
        &args[0].kind,
        ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_)
    );
    let sig = if is_callable_expr {
        emit_expr(&args[0], emitter, ctx, data);
        emitter.instruction("mov x19, x0");                                     // move synthesized callback address to x19
        ctx.deferred_closures
            .last()
            .expect("call_user_func_array: missing synthesized callable signature")
            .sig
            .clone()
    } else if let ExprKind::Variable(var_name) = &args[0].kind {
        let var = ctx.variables.get(var_name).expect("undefined callback variable");
        let offset = var.stack_offset;
        abi::load_at_offset(emitter, "x19", offset);                                // load callback address from callable variable
        ctx.closure_sigs
            .get(var_name)
            .expect("call_user_func_array: callable variable signature not found")
            .clone()
    } else {
        let func_name = match &args[0].kind {
            ExprKind::StringLiteral(name) => name.clone(),
            _ => panic!("call_user_func_array() callback must be a string literal, callable expression, or callable variable"),
        };
        let label = function_symbol(&func_name);
        emitter.adrp("x19", &format!("{}", label));              // load page address of callback function
        emitter.add_lo12("x19", "x19", &format!("{}", label));       // resolve full address of callback
        ctx.functions
            .get(&func_name)
            .expect("call_user_func_array: function not found")
            .clone()
    };

    // Evaluate the array argument (second arg)
    let arr_ty = emit_expr(&args[1], emitter, ctx, data);

    // Determine element type and size from the array type
    let elem_ty = match &arr_ty {
        PhpType::Array(t) => *t.clone(),
        PhpType::AssocArray { value, .. } => *value.clone(),
        _ => PhpType::Int,
    };
    let elem_size = args::array_element_stride(&elem_ty);

    emitter.instruction("mov x20, x0");                                         // preserve the callback-argument array pointer across element boxing
    emitter.instruction("ldr x21, [x20]");                                      // load callback-argument array length

    // -- extract elements from array and push them as regular call arguments --
    let mut arg_types = Vec::new();
    let regular_param_count = if sig.variadic.is_some() {
        sig.params.len().saturating_sub(1)
    } else {
        sig.params.len()
    };
    for i in 0..regular_param_count {
        let has_default = sig.defaults.get(i).and_then(|d| d.as_ref()).is_some();
        let target_ty = if args::declared_target_ty(Some(&sig), i).is_some() || has_default {
            sig.params.get(i).map(|(_, ty)| ty)
        } else {
            None
        };
        let pushed_ty = target_ty
            .map(PhpType::codegen_repr)
            .unwrap_or_else(|| elem_ty.codegen_repr());

        if let Some(default_expr) = sig.defaults.get(i).and_then(|d| d.as_ref()) {
            let load_label = ctx.next_label("cufa_load_arg");
            let done_label = ctx.next_label("cufa_arg_done");
            emitter.instruction(&format!("cmp x21, #{}", i + 1));               // compare provided array length against required positional index
            emitter.instruction(&format!("b.ge {}", load_label));               // load an explicit array element when present
            let _ = args::push_expr_arg(default_expr, target_ty, emitter, ctx, data);
            emitter.instruction(&format!("b {}", done_label));                  // skip the explicit-element path after pushing the default
            emitter.label(&load_label);
            emitter.instruction("add x9, x20, #24");                            // point x9 at the callback-argument array payload
            args::load_array_element_to_result(emitter, &elem_ty, "x9", i * elem_size);
            let _ =
                args::push_loaded_array_element_arg(&elem_ty, target_ty, emitter, ctx, data);
            emitter.label(&done_label);
        } else {
            emitter.instruction("add x9, x20, #24");                            // point x9 at the callback-argument array payload
            args::load_array_element_to_result(emitter, &elem_ty, "x9", i * elem_size);
            let _ =
                args::push_loaded_array_element_arg(&elem_ty, target_ty, emitter, ctx, data);
        }
        arg_types.push(pushed_ty);
    }

    if sig.variadic.is_some() {
        let variadic_elem_ty = sig
            .params
            .last()
            .and_then(|(_, ty)| match ty {
                PhpType::Array(elem) => Some((**elem).clone()),
                _ => None,
            })
            .unwrap_or_else(|| elem_ty.clone());
        let build_label = ctx.next_label("cufa_build_variadic");
        let done_label = ctx.next_label("cufa_variadic_done");
        emitter.instruction(&format!("cmp x21, #{}", regular_param_count));     // compare provided array length against the fixed arity prefix
        emitter.instruction(&format!("b.gt {}", build_label));                  // build a tail array only when extra positional elements exist
        emitter.comment("empty variadic array for call_user_func_array()");
        emitter.instruction("mov x0, #4");                                      // initial capacity: 4 (grows dynamically)
        emitter.instruction(&format!("mov x1, #{}", variadic_elem_ty.stack_size())); //element size in bytes for the variadic array
        emitter.instruction("bl __rt_array_new");                               // allocate an empty variadic array
        emitter.instruction("str x0, [sp, #-16]!");                             // push the empty variadic array onto the stack
        emitter.instruction(&format!("b {}", done_label));                      // skip the non-empty variadic array builder

        emitter.label(&build_label);
        emitter.instruction(&format!("sub x22, x21, #{}", regular_param_count)); //compute the count of variadic tail arguments
        emitter.instruction("mov x0, x22");                                     // pass the exact tail argument count as the initial capacity
        emitter.instruction(&format!("mov x1, #{}", variadic_elem_ty.stack_size())); //pass the variadic element size in bytes
        emitter.instruction("bl __rt_array_new");                               // allocate the variadic array storage
        emitter.instruction("str x0, [sp, #-16]!");                             // keep the variadic array pointer on the stack while filling it
        emitter.instruction("mov x9, x0");                                      // copy the variadic array pointer into a scratch register for metadata stamping
        emit_array_value_type_stamp(emitter, "x9", &variadic_elem_ty);           // stamp the array header with the variadic element runtime tag
        emitter.instruction("mov x23, #0");                                     // start the variadic tail index at zero
        let loop_label = ctx.next_label("cufa_variadic_loop");
        let loop_done_label = ctx.next_label("cufa_variadic_loop_done");
        emitter.label(&loop_label);
        emitter.instruction("cmp x23, x22");                                    // stop once every tail element has been copied
        emitter.instruction(&format!("b.ge {}", loop_done_label));              // exit the fill loop when the tail array is complete
        emitter.instruction("add x24, x23, #0");                                // copy the tail index into a scratch register
        if regular_param_count > 0 {
            emitter.instruction(&format!("add x24, x24, #{}", regular_param_count)); //offset the tail index by the fixed-arity prefix length
        }
        emitter.instruction("add x26, x20, #24");                               // point x26 at the callback-argument array payload
        match elem_ty.codegen_repr() {
            PhpType::Str => {
                emitter.instruction("lsl x25, x24, #4");                        // compute the 16-byte source slot offset for a string element
                emitter.instruction("add x26, x26, x25");                       // advance x26 to the selected source string element
                emitter.instruction("ldp x1, x2, [x26]");                       // load the source string pointer and length pair
            }
            PhpType::Float => {
                emitter.instruction("lsl x25, x24, #3");                        // compute the 8-byte source slot offset for a float element
                emitter.instruction("add x26, x26, x25");                       // advance x26 to the selected source float element
                emitter.instruction("ldr d0, [x26]");                           // load the source float element
            }
            PhpType::Void => {}
            _ => {
                emitter.instruction("lsl x25, x24, #3");                        // compute the 8-byte source slot offset for a scalar or boxed element
                emitter.instruction("add x26, x26, x25");                       // advance x26 to the selected source scalar element
                emitter.instruction("ldr x0, [x26]");                           // load the source scalar or boxed element
            }
        }
        let (stored_ty, boxed_to_mixed) = args::coerce_current_value_to_target(
            emitter,
            ctx,
            data,
            &elem_ty,
            Some(&variadic_elem_ty),
        );
        if !boxed_to_mixed {
            abi::emit_incref_if_refcounted(emitter, &elem_ty.codegen_repr());   // retain refcounted tail elements copied into the new variadic array
        }
        emitter.instruction("ldr x9, [sp]");                                    // reload the variadic array pointer from the stack
        match stored_ty {
            PhpType::Int
            | PhpType::Bool
            | PhpType::Callable
            | PhpType::Mixed
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Buffer(_)
            | PhpType::Object(_)
            | PhpType::Packed(_)
            | PhpType::Pointer(_)
            | PhpType::Union(_) => {
                emitter.instruction("add x10, x9, #24");                        // point x10 at the variadic array payload
                emitter.instruction("lsl x11, x23, #3");                        // compute the 8-byte destination slot offset
                emitter.instruction("add x10, x10, x11");                       // advance to the selected variadic destination slot
                emitter.instruction("str x0, [x10]");                           // store the scalar or boxed tail element
            }
            PhpType::Float => {
                emitter.instruction("add x10, x9, #24");                        // point x10 at the variadic array payload
                emitter.instruction("lsl x11, x23, #3");                        // compute the 8-byte destination slot offset
                emitter.instruction("add x10, x10, x11");                       // advance to the selected variadic destination slot
                emitter.instruction("str d0, [x10]");                           // store the float tail element
            }
            PhpType::Str => {
                emitter.instruction("add x10, x9, #24");                        // point x10 at the variadic array payload
                emitter.instruction("lsl x11, x23, #4");                        // compute the 16-byte destination slot offset
                emitter.instruction("add x10, x10, x11");                       // advance to the selected variadic destination slot
                emitter.instruction("stp x1, x2, [x10]");                       // store the string pointer and length pair
            }
            PhpType::Void => {}
        }
        emitter.instruction("add x23, x23, #1");                                // advance to the next tail element
        emitter.instruction("str x23, [x9]");                                   // persist the updated variadic array length
        emitter.instruction(&format!("b {}", loop_label));                      // continue filling the variadic array
        emitter.label(&loop_done_label);
        emitter.label(&done_label);
        arg_types.push(PhpType::Array(Box::new(variadic_elem_ty)));
    }

    let assignments = abi::build_outgoing_arg_assignments(&arg_types, 0);
    let overflow_bytes = abi::materialize_outgoing_args(emitter, &assignments);

    let ret_ty = sig.return_type.clone();

    // -- call callback via the resolved address in x19 --
    crate::codegen::expr::save_concat_offset_before_nested_call(emitter);
    emitter.instruction("blr x19");                                             // call callback via indirect branch
    crate::codegen::expr::restore_concat_offset_after_nested_call(emitter, &ret_ty);
    if overflow_bytes > 0 {
        emitter.instruction(&format!("add sp, sp, #{}", overflow_bytes));       // drop spilled stack callback arguments after the indirect call returns
    }

    Some(ret_ty)
}
