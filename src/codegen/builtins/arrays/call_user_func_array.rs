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
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            emitter.instruction(&format!("ldr x10, [{}, #-8]", array_reg));     // load the packed array kind word from the heap header
            emitter.instruction("mov x12, #0x80ff");                            // preserve the indexed-array kind and persistent COW flag
            emitter.instruction("and x10, x10, x12");                           // keep only the persistent indexed-array metadata bits
            emitter.instruction(&format!("mov x11, #{}", value_type_tag));      // materialize the runtime array value_type tag
            emitter.instruction("lsl x11, x11, #8");                            // move the value_type tag into the packed kind-word byte lane
            emitter.instruction("orr x10, x10, x11");                           // combine the heap kind with the array value_type tag
            emitter.instruction(&format!("str x10, [{}, #-8]", array_reg));     // persist the packed array kind word in the heap header
        }
        crate::codegen::platform::Arch::X86_64 => {
            emitter.instruction(&format!("mov r10, QWORD PTR [{} - 8]", array_reg)); // load the packed array kind word from the heap header
            emitter.instruction("mov r11, 0x80ff");                             // preserve the indexed-array kind and persistent COW flag
            emitter.instruction("and r10, r11");                                // keep only the persistent indexed-array metadata bits
            emitter.instruction(&format!("mov rcx, {}", value_type_tag));       // materialize the runtime array value_type tag
            emitter.instruction("shl rcx, 8");                                  // move the value_type tag into the packed kind-word byte lane
            emitter.instruction("or r10, rcx");                                 // combine the heap kind with the array value_type tag
            emitter.instruction(&format!("mov QWORD PTR [{} - 8], r10", array_reg)); // persist the packed array kind word in the heap header
        }
    }
}

pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("call_user_func_array()");
    let save_concat_before_args =
        emitter.target.arch == crate::codegen::platform::Arch::X86_64;
    if save_concat_before_args {
        crate::codegen::expr::save_concat_offset_before_nested_call(emitter, ctx);
    }
    let call_reg = abi::nested_call_reg(emitter);
    let result_reg = abi::int_result_reg(emitter);
    let (array_reg, len_reg, tail_count_reg, tail_index_reg, index_reg, offset_reg, data_reg, peek_reg, array_new_capacity_reg, array_new_elem_size_reg, len_store_reg) =
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => (
                "x20", "x21", "x22", "x23", "x24", "x25", "x26", "x9", "x0", "x1", "x10"
            ),
            crate::codegen::platform::Arch::X86_64 => (
                "r13", "r14", "r15", "rbx", "rcx", "r8", "r9", "r11", "rdi", "rsi", "r10"
            ),
        };

    // -- resolve callback function address and signature --
    let is_callable_expr = matches!(
        &args[0].kind,
        ExprKind::Closure { .. } | ExprKind::FirstClassCallable(_)
    );
    let sig = if is_callable_expr {
        emit_expr(&args[0], emitter, ctx, data);
        emitter.instruction(&format!("mov {}, {}", call_reg, result_reg));      // move the synthesized callback address into the nested-call scratch register
        ctx.deferred_closures
            .last()
            .expect("call_user_func_array: missing synthesized callable signature")
            .sig
            .clone()
    } else if let ExprKind::Variable(var_name) = &args[0].kind {
        let var = ctx.variables.get(var_name).expect("undefined callback variable");
        let offset = var.stack_offset;
        abi::load_at_offset(emitter, call_reg, offset);                          // load the callback address from the callable variable slot
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
        abi::emit_symbol_address(emitter, call_reg, &label);
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

    emitter.instruction(&format!("mov {}, {}", array_reg, abi::int_result_reg(emitter))); // preserve the callback-argument array pointer across element boxing
    abi::emit_load_from_address(emitter, len_reg, array_reg, 0);                // load callback-argument array length

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
            match emitter.target.arch {
                crate::codegen::platform::Arch::AArch64 => {
                    emitter.instruction(&format!("cmp {}, #{}", len_reg, i + 1)); // compare provided array length against required positional index
                }
                crate::codegen::platform::Arch::X86_64 => {
                    emitter.instruction(&format!("cmp {}, {}", len_reg, i + 1)); // compare provided array length against required positional index
                }
            }
            match emitter.target.arch {
                crate::codegen::platform::Arch::AArch64 => {
                    emitter.instruction(&format!("b.ge {}", load_label));       // load an explicit array element when present
                }
                crate::codegen::platform::Arch::X86_64 => {
                    emitter.instruction(&format!("jge {}", load_label));        // load an explicit array element when present
                }
            }
            let _ = args::push_expr_arg(default_expr, target_ty, emitter, ctx, data);
            abi::emit_jump(emitter, &done_label);
            emitter.label(&load_label);
            args::load_array_element_to_result(emitter, &elem_ty, array_reg, 24 + i * elem_size);
            let _ =
                args::push_loaded_array_element_arg(&elem_ty, target_ty, emitter, ctx, data);
            emitter.label(&done_label);
        } else {
            args::load_array_element_to_result(emitter, &elem_ty, array_reg, 24 + i * elem_size);
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
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction(&format!("cmp {}, #{}", len_reg, regular_param_count)); // compare provided array length against the fixed arity prefix
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction(&format!("cmp {}, {}", len_reg, regular_param_count)); // compare provided array length against the fixed arity prefix
            }
        }
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction(&format!("b.gt {}", build_label));          // build a tail array only when extra positional elements exist
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction(&format!("jg {}", build_label));            // build a tail array only when extra positional elements exist
            }
        }
        emitter.comment("empty variadic array for call_user_func_array()");
        abi::emit_load_int_immediate(emitter, array_new_capacity_reg, 4);
        abi::emit_load_int_immediate(emitter, array_new_elem_size_reg, variadic_elem_ty.stack_size() as i64);
        abi::emit_call_label(emitter, "__rt_array_new");
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // push the empty variadic array onto the temporary arg stack
        abi::emit_jump(emitter, &done_label);

        emitter.label(&build_label);
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction(&format!("sub {}, {}, #{}", tail_count_reg, len_reg, regular_param_count)); // compute the count of variadic tail arguments
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction(&format!("mov {}, {}", tail_count_reg, len_reg)); // seed the tail count from the provided array length
                emitter.instruction(&format!("sub {}, {}", tail_count_reg, regular_param_count)); // compute the count of variadic tail arguments
            }
        }
        emitter.instruction(&format!("mov {}, {}", array_new_capacity_reg, tail_count_reg)); // pass the exact tail argument count as the initial capacity
        abi::emit_load_int_immediate(emitter, array_new_elem_size_reg, variadic_elem_ty.stack_size() as i64);
        abi::emit_call_label(emitter, "__rt_array_new");
        abi::emit_push_reg(emitter, abi::int_result_reg(emitter));              // keep the variadic array pointer on the stack while filling it
        emitter.instruction(&format!("mov {}, {}", peek_reg, abi::int_result_reg(emitter))); // copy the variadic array pointer into a scratch register for metadata stamping
        emit_array_value_type_stamp(emitter, peek_reg, &variadic_elem_ty);      // stamp the array header with the variadic element runtime tag
        abi::emit_load_int_immediate(emitter, tail_index_reg, 0);
        let loop_label = ctx.next_label("cufa_variadic_loop");
        let loop_done_label = ctx.next_label("cufa_variadic_loop_done");
        emitter.label(&loop_label);
        emitter.instruction(&format!("cmp {}, {}", tail_index_reg, tail_count_reg)); // stop once every tail element has been copied
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction(&format!("b.ge {}", loop_done_label));      // exit the fill loop when the tail array is complete
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction(&format!("jge {}", loop_done_label));       // exit the fill loop when the tail array is complete
            }
        }
        emitter.instruction(&format!("mov {}, {}", index_reg, tail_index_reg)); // copy the tail index into a scratch register
        if regular_param_count > 0 {
            match emitter.target.arch {
                crate::codegen::platform::Arch::AArch64 => {
                    emitter.instruction(&format!("add {}, {}, #{}", index_reg, index_reg, regular_param_count)); // offset the tail index by the fixed-arity prefix length
                }
                crate::codegen::platform::Arch::X86_64 => {
                    emitter.instruction(&format!("add {}, {}", index_reg, regular_param_count)); // offset the tail index by the fixed-arity prefix length
                }
            }
        }
        emitter.instruction(&format!("mov {}, {}", data_reg, array_reg));       // start from the callback-argument array pointer before indexing into payload data
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction(&format!("add {}, {}, #24", data_reg, data_reg)); // skip the fixed array header before indexing variadic source elements
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction(&format!("add {}, 24", data_reg));          // skip the fixed array header before indexing variadic source elements
            }
        }
        match elem_ty.codegen_repr() {
            PhpType::Str => {
                match emitter.target.arch {
                    crate::codegen::platform::Arch::AArch64 => {
                        emitter.instruction(&format!("lsl {}, {}, #4", offset_reg, index_reg)); // compute the 16-byte source slot offset for a string element
                        emitter.instruction(&format!("add {}, {}, {}", data_reg, data_reg, offset_reg)); // advance to the selected source string element
                        let (ptr_reg, len_reg_out) = abi::string_result_regs(emitter);
                        abi::emit_load_from_address(emitter, ptr_reg, data_reg, 0);
                        abi::emit_load_from_address(emitter, len_reg_out, data_reg, 8);
                    }
                    crate::codegen::platform::Arch::X86_64 => {
                        emitter.instruction(&format!("mov {}, {}", offset_reg, index_reg)); // copy the element index before scaling to bytes
                        emitter.instruction(&format!("shl {}, 4", offset_reg)); // compute the 16-byte source slot offset for a string element
                        emitter.instruction(&format!("add {}, {}", data_reg, offset_reg)); // advance to the selected source string element
                        let (ptr_reg, len_reg_out) = abi::string_result_regs(emitter);
                        abi::emit_load_from_address(emitter, ptr_reg, data_reg, 0);
                        abi::emit_load_from_address(emitter, len_reg_out, data_reg, 8);
                    }
                }
            }
            PhpType::Float => {
                match emitter.target.arch {
                    crate::codegen::platform::Arch::AArch64 => {
                        emitter.instruction(&format!("lsl {}, {}, #3", offset_reg, index_reg)); // compute the 8-byte source slot offset for a float element
                        emitter.instruction(&format!("add {}, {}, {}", data_reg, data_reg, offset_reg)); // advance to the selected source float element
                    }
                    crate::codegen::platform::Arch::X86_64 => {
                        emitter.instruction(&format!("mov {}, {}", offset_reg, index_reg)); // copy the element index before scaling to bytes
                        emitter.instruction(&format!("shl {}, 3", offset_reg)); // compute the 8-byte source slot offset for a float element
                        emitter.instruction(&format!("add {}, {}", data_reg, offset_reg)); // advance to the selected source float element
                    }
                }
                abi::emit_load_from_address(emitter, abi::float_result_reg(emitter), data_reg, 0);
            }
            PhpType::Void => {}
            _ => {
                match emitter.target.arch {
                    crate::codegen::platform::Arch::AArch64 => {
                        emitter.instruction(&format!("lsl {}, {}, #3", offset_reg, index_reg)); // compute the 8-byte source slot offset for a scalar or boxed element
                        emitter.instruction(&format!("add {}, {}, {}", data_reg, data_reg, offset_reg)); // advance to the selected source scalar element
                    }
                    crate::codegen::platform::Arch::X86_64 => {
                        emitter.instruction(&format!("mov {}, {}", offset_reg, index_reg)); // copy the element index before scaling to bytes
                        emitter.instruction(&format!("shl {}, 3", offset_reg)); // compute the 8-byte source slot offset for a scalar or boxed element
                        emitter.instruction(&format!("add {}, {}", data_reg, offset_reg)); // advance to the selected source scalar element
                    }
                }
                abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), data_reg, 0);
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
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction(&format!("ldr {}, [sp]", peek_reg));        // reload the variadic array pointer from the stack
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction(&format!("mov {}, QWORD PTR [rsp]", peek_reg)); // reload the variadic array pointer from the stack
            }
        }
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
                let dest_reg = len_store_reg;
                emitter.instruction(&format!("mov {}, {}", dest_reg, peek_reg)); // point at the variadic array before skipping the header
                match emitter.target.arch {
                    crate::codegen::platform::Arch::AArch64 => {
                        emitter.instruction(&format!("add {}, {}, #24", dest_reg, dest_reg)); // point at the variadic array payload
                        emitter.instruction(&format!("lsl {}, {}, #3", offset_reg, tail_index_reg)); // compute the 8-byte destination slot offset
                        emitter.instruction(&format!("add {}, {}, {}", dest_reg, dest_reg, offset_reg)); // advance to the selected variadic destination slot
                    }
                    crate::codegen::platform::Arch::X86_64 => {
                        emitter.instruction(&format!("add {}, 24", dest_reg));  // point at the variadic array payload
                        emitter.instruction(&format!("mov {}, {}", offset_reg, tail_index_reg)); // copy the destination index before scaling
                        emitter.instruction(&format!("shl {}, 3", offset_reg)); // compute the 8-byte destination slot offset
                        emitter.instruction(&format!("add {}, {}", dest_reg, offset_reg)); // advance to the selected variadic destination slot
                    }
                }
                abi::emit_store_to_address(emitter, abi::int_result_reg(emitter), dest_reg, 0);
            }
            PhpType::Float => {
                let dest_reg = len_store_reg;
                emitter.instruction(&format!("mov {}, {}", dest_reg, peek_reg)); // point at the variadic array before skipping the header
                match emitter.target.arch {
                    crate::codegen::platform::Arch::AArch64 => {
                        emitter.instruction(&format!("add {}, {}, #24", dest_reg, dest_reg)); // point at the variadic array payload
                        emitter.instruction(&format!("lsl {}, {}, #3", offset_reg, tail_index_reg)); // compute the 8-byte destination slot offset
                        emitter.instruction(&format!("add {}, {}, {}", dest_reg, dest_reg, offset_reg)); // advance to the selected variadic destination slot
                    }
                    crate::codegen::platform::Arch::X86_64 => {
                        emitter.instruction(&format!("add {}, 24", dest_reg));  // point at the variadic array payload
                        emitter.instruction(&format!("mov {}, {}", offset_reg, tail_index_reg)); // copy the destination index before scaling
                        emitter.instruction(&format!("shl {}, 3", offset_reg)); // compute the 8-byte destination slot offset
                        emitter.instruction(&format!("add {}, {}", dest_reg, offset_reg)); // advance to the selected variadic destination slot
                    }
                }
                abi::emit_store_to_address(emitter, abi::float_result_reg(emitter), dest_reg, 0);
            }
            PhpType::Str => {
                let dest_reg = len_store_reg;
                let (ptr_reg, len_reg_out) = abi::string_result_regs(emitter);
                emitter.instruction(&format!("mov {}, {}", dest_reg, peek_reg)); // point at the variadic array before skipping the header
                match emitter.target.arch {
                    crate::codegen::platform::Arch::AArch64 => {
                        emitter.instruction(&format!("add {}, {}, #24", dest_reg, dest_reg)); // point at the variadic array payload
                        emitter.instruction(&format!("lsl {}, {}, #4", offset_reg, tail_index_reg)); // compute the 16-byte destination slot offset
                        emitter.instruction(&format!("add {}, {}, {}", dest_reg, dest_reg, offset_reg)); // advance to the selected variadic destination slot
                    }
                    crate::codegen::platform::Arch::X86_64 => {
                        emitter.instruction(&format!("add {}, 24", dest_reg));  // point at the variadic array payload
                        emitter.instruction(&format!("mov {}, {}", offset_reg, tail_index_reg)); // copy the destination index before scaling
                        emitter.instruction(&format!("shl {}, 4", offset_reg)); // compute the 16-byte destination slot offset
                        emitter.instruction(&format!("add {}, {}", dest_reg, offset_reg)); // advance to the selected variadic destination slot
                    }
                }
                abi::emit_store_to_address(emitter, ptr_reg, dest_reg, 0);
                abi::emit_store_to_address(emitter, len_reg_out, dest_reg, 8);
            }
            PhpType::Void => {}
        }
        match emitter.target.arch {
            crate::codegen::platform::Arch::AArch64 => {
                emitter.instruction(&format!("add {}, {}, #1", tail_index_reg, tail_index_reg)); // advance to the next tail element
            }
            crate::codegen::platform::Arch::X86_64 => {
                emitter.instruction(&format!("add {}, 1", tail_index_reg));     // advance to the next tail element
            }
        }
        abi::emit_store_to_address(emitter, tail_index_reg, peek_reg, 0);       // persist the updated variadic array length
        abi::emit_jump(emitter, &loop_label);
        emitter.label(&loop_done_label);
        emitter.label(&done_label);
        arg_types.push(PhpType::Array(Box::new(variadic_elem_ty)));
    }

    let assignments = abi::build_outgoing_arg_assignments_for_target(emitter.target, &arg_types, 0);
    let overflow_bytes = abi::materialize_outgoing_args(emitter, &assignments);

    let ret_ty = sig.return_type.clone();

    // -- call callback via the resolved address in x19 --
    if !save_concat_before_args {
        crate::codegen::expr::save_concat_offset_before_nested_call(emitter, ctx);
    }
    abi::emit_call_reg(emitter, call_reg);
    if save_concat_before_args {
        abi::emit_release_temporary_stack(emitter, overflow_bytes);
        crate::codegen::expr::restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
    } else {
        crate::codegen::expr::restore_concat_offset_after_nested_call(emitter, ctx, &ret_ty);
        abi::emit_release_temporary_stack(emitter, overflow_bytes);
    }

    Some(ret_ty)
}
