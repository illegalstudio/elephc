use super::super::super::abi;
use super::super::super::context::Context;
use super::super::super::data_section::DataSection;
use super::super::super::emit::Emitter;
use super::super::super::expr::{coerce_result_to_type, emit_expr};
use crate::codegen::platform::Arch;
use crate::names::static_property_symbol;
use crate::parser::ast::{Expr, StaticReceiver};
use crate::types::PhpType;

#[derive(Clone)]
struct StaticPropertyBranch {
    class_id: u64,
    declaring_class: String,
}

pub(crate) fn emit_static_property_assign_stmt(
    receiver: &StaticReceiver,
    property: &str,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment(&format!("::${} = ...", property));

    let Some((class_name, declaring_class, prop_ty, declared)) =
        resolve_static_property(receiver, property, ctx, emitter)
    else {
        return;
    };
    let branches = dynamic_static_property_branches(receiver, property, &declaring_class, ctx);
    let class_id_saved = emit_and_push_called_class_id_if_needed(&branches, emitter, ctx);

    let mut val_ty = emit_expr(value, emitter, ctx, data);
    let boxed_to_mixed = declared
        && matches!(prop_ty, PhpType::Mixed | PhpType::Union(_))
        && !matches!(val_ty, PhpType::Mixed | PhpType::Union(_));
    if declared {
        coerce_result_to_type(emitter, ctx, data, &val_ty, &prop_ty);
        val_ty = prop_ty.clone();
    }
    if !boxed_to_mixed {
        super::super::helpers::retain_borrowed_heap_result(emitter, value, &val_ty);
    }

    emitter.comment(&format!("store {}::${}", class_name, property));
    if class_id_saved {
        let class_id_reg = class_id_work_reg(emitter);
        abi::emit_pop_reg(emitter, class_id_reg);                              // restore the late-bound called class id for static property storage dispatch
        emit_dynamic_store_result_to_static_property(
            property,
            class_id_reg,
            &declaring_class,
            &branches,
            &val_ty,
            true,
            emitter,
            ctx,
        );
    } else {
        let symbol = static_property_symbol(&declaring_class, property);
        abi::emit_store_result_to_symbol(emitter, &symbol, &val_ty, true);
    }
}

pub(crate) fn emit_static_property_array_push_stmt(
    receiver: &StaticReceiver,
    property: &str,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment(&format!("::${}[] = ...", property));

    let Some((_, declaring_class, prop_ty, _)) =
        resolve_static_property(receiver, property, ctx, emitter)
    else {
        return;
    };
    let elem_ty = match &prop_ty {
        PhpType::Array(elem_ty) => *elem_ty.clone(),
        _ => {
            emitter.comment("WARNING: static property array push on non-array property");
            return;
        }
    };
    let branches = dynamic_static_property_branches(receiver, property, &declaring_class, ctx);
    let class_id_saved = emit_and_push_called_class_id_if_needed(&branches, emitter, ctx);
    if class_id_saved {
        let class_id_reg = class_id_work_reg(emitter);
        abi::emit_pop_reg(emitter, class_id_reg);                              // reload the called class id before selecting the static array slot
        abi::emit_push_reg(emitter, class_id_reg);                             // keep the called class id available for the final static array store
        emit_dynamic_load_static_property_reg(
            property,
            class_id_reg,
            &declaring_class,
            &branches,
            abi::int_result_reg(emitter),
            emitter,
            ctx,
        );
    } else {
        let symbol = static_property_symbol(&declaring_class, property);
        abi::emit_load_symbol_to_reg(emitter, abi::int_result_reg(emitter), &symbol, 0);
    }
    emit_ensure_indexed_array_pointer(&elem_ty, emitter, ctx);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                 // preserve the static array pointer while evaluating the appended value

    let mut val_ty = emit_expr(value, emitter, ctx, data);
    if matches!(elem_ty, PhpType::Mixed) && !matches!(val_ty, PhpType::Mixed | PhpType::Union(_)) {
        crate::codegen::emit_box_current_value_as_mixed(emitter, &val_ty);
        val_ty = PhpType::Mixed;
    } else {
        super::super::helpers::retain_borrowed_heap_result(emitter, value, &val_ty);
    }

    let array_reg = abi::symbol_scratch_reg(emitter);
    abi::emit_pop_reg(emitter, array_reg);                                      // restore the static array pointer after evaluating the appended value
    emit_array_push_runtime_call(array_reg, &val_ty, emitter);
    if class_id_saved {
        let class_id_reg = class_id_work_reg(emitter);
        abi::emit_pop_reg(emitter, class_id_reg);                              // restore the called class id for the late-bound static array store
        emit_dynamic_store_reg_to_static_property(
            property,
            class_id_reg,
            abi::int_result_reg(emitter),
            &declaring_class,
            &branches,
            emitter,
            ctx,
        );
    } else {
        let symbol = static_property_symbol(&declaring_class, property);
        abi::emit_store_reg_to_symbol(emitter, abi::int_result_reg(emitter), &symbol, 0);
    }
}

pub(crate) fn emit_static_property_array_assign_stmt(
    receiver: &StaticReceiver,
    property: &str,
    index: &Expr,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment(&format!("::${}[...] = ...", property));

    let Some((_, declaring_class, prop_ty, _)) =
        resolve_static_property(receiver, property, ctx, emitter)
    else {
        return;
    };
    let elem_ty = match &prop_ty {
        PhpType::Array(elem_ty) => *elem_ty.clone(),
        _ => {
            emitter.comment("WARNING: static property array assign on non-array property");
            return;
        }
    };
    let branches = dynamic_static_property_branches(receiver, property, &declaring_class, ctx);
    let class_id_saved = emit_and_push_called_class_id_if_needed(&branches, emitter, ctx);
    if class_id_saved {
        let class_id_reg = class_id_work_reg(emitter);
        abi::emit_pop_reg(emitter, class_id_reg);                              // reload the called class id before selecting the static array slot
        abi::emit_push_reg(emitter, class_id_reg);                             // keep the called class id available for later static array stores
        emit_dynamic_load_static_property_reg(
            property,
            class_id_reg,
            &declaring_class,
            &branches,
            abi::int_result_reg(emitter),
            emitter,
            ctx,
        );
    } else {
        let symbol = static_property_symbol(&declaring_class, property);
        abi::emit_load_symbol_to_reg(emitter, abi::int_result_reg(emitter), &symbol, 0);
    }
    emit_ensure_indexed_array_pointer(&elem_ty, emitter, ctx);
    emit_static_indexed_array_assign(
        property,
        &declaring_class,
        &branches,
        class_id_saved,
        &elem_ty,
        index,
        value,
        emitter,
        ctx,
        data,
    );
}

fn emit_static_indexed_array_assign(
    property: &str,
    declaring_class: &str,
    branches: &[StaticPropertyBranch],
    class_id_saved: bool,
    elem_ty: &PhpType,
    index: &Expr,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("bl __rt_array_ensure_unique");                 // split shared static arrays before mutating indexed storage
            publish_static_array_pointer(
                property,
                declaring_class,
                branches,
                class_id_saved,
                0,
                "x0",
                emitter,
                ctx,
            );
            abi::emit_push_reg(emitter, "x0");                                  // preserve the unique static array pointer while evaluating the index
            emit_expr(index, emitter, ctx, data);
            abi::emit_push_reg(emitter, "x0");                                  // preserve the target index while evaluating the assigned value
            let val_ty = prepare_static_array_assign_value(value, emitter, ctx, data, elem_ty);
            let state = StaticIndexedAssignState::new(elem_ty, &val_ty);
            emitter.instruction("ldr x9, [sp, #16]");                           // reload the target static-array index without disturbing the saved value
            emitter.instruction("ldr x10, [sp, #32]");                          // reload the static array pointer without disturbing the saved value
            emitter.instruction("ldr x11, [x10]");                              // load the original static array length before growth
            grow_static_indexed_array_until_ready_aarch64(emitter, ctx);
            abi::emit_push_reg(emitter, "x9");                                  // preserve the target index while publishing the possibly-grown static array pointer
            publish_static_array_pointer(
                property,
                declaring_class,
                branches,
                class_id_saved,
                64,
                "x10",
                emitter,
                ctx,
            );
            abi::emit_pop_reg(emitter, "x9");                                   // restore the target index after publishing the static array pointer
            restore_static_array_assign_value_aarch64(emitter, &val_ty);
            normalize_static_indexed_array_layout_aarch64(&state, emitter, ctx);
            store_static_indexed_array_value_aarch64(&state, emitter, ctx);
            extend_static_indexed_array_if_needed_aarch64(&state, emitter, ctx);
            let drop_bytes = if class_id_saved { 48 } else { 32 };
            emitter.instruction(&format!("add sp, sp, #{}", drop_bytes));       // drop preserved static-array dispatch, pointer, and index slots
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, rax");                                // pass the static array pointer to the uniqueness helper
            abi::emit_call_label(emitter, "__rt_array_ensure_unique");
            publish_static_array_pointer(
                property,
                declaring_class,
                branches,
                class_id_saved,
                0,
                "rax",
                emitter,
                ctx,
            );
            abi::emit_push_reg(emitter, "rax");                                 // preserve the unique static array pointer while evaluating the index
            emit_expr(index, emitter, ctx, data);
            abi::emit_push_reg(emitter, "rax");                                 // preserve the target index while evaluating the assigned value
            let val_ty = prepare_static_array_assign_value(value, emitter, ctx, data, elem_ty);
            let state = StaticIndexedAssignState::new(elem_ty, &val_ty);
            emitter.instruction("mov r9, QWORD PTR [rsp + 16]");                // reload the target static-array index without disturbing the saved value
            emitter.instruction("mov r10, QWORD PTR [rsp + 32]");               // reload the static array pointer without disturbing the saved value
            emitter.instruction("mov r11, QWORD PTR [r10]");                    // load the original static array length before growth
            grow_static_indexed_array_until_ready_x86_64(emitter, ctx);
            publish_static_array_pointer(
                property,
                declaring_class,
                branches,
                class_id_saved,
                48,
                "r10",
                emitter,
                ctx,
            );
            restore_static_array_assign_value_x86_64(emitter, &val_ty);
            normalize_static_indexed_array_layout_x86_64(&state, emitter, ctx);
            store_static_indexed_array_value_x86_64(&state, emitter, ctx);
            extend_static_indexed_array_if_needed_x86_64(&state, emitter, ctx);
            let drop_bytes = if class_id_saved { 48 } else { 32 };
            emitter.instruction(&format!("add rsp, {}", drop_bytes));           // drop preserved static-array dispatch, pointer, and index slots
        }
    }
}

fn publish_static_array_pointer(
    property: &str,
    declaring_class: &str,
    branches: &[StaticPropertyBranch],
    class_id_saved: bool,
    class_id_stack_offset: usize,
    source_reg: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    if class_id_saved {
        let class_id_reg = class_id_work_reg(emitter);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("ldr {}, [sp, #{}]", class_id_reg, class_id_stack_offset)); // reload the called class id from the static array temporary stack
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov {}, QWORD PTR [rsp + {}]", class_id_reg, class_id_stack_offset)); // reload the called class id from the static array temporary stack
            }
        }
        emit_dynamic_store_reg_to_static_property(
            property,
            class_id_reg,
            source_reg,
            declaring_class,
            branches,
            emitter,
            ctx,
        );
    } else {
        let symbol = static_property_symbol(declaring_class, property);
        abi::emit_store_reg_to_symbol(emitter, source_reg, &symbol, 0);
    }
}

fn emit_array_push_runtime_call(array_reg: &str, val_ty: &PhpType, emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => match val_ty {
            PhpType::Int | PhpType::Bool => {
                emitter.instruction("mov x1, x0");                              // move the appended scalar payload into the runtime value register
                emitter.instruction(&format!("mov x0, {}", array_reg));         // move the static array pointer into the runtime receiver register
                emitter.instruction("bl __rt_array_push_int");                  // append the scalar payload and return the possibly-grown static array
            }
            PhpType::Float => {
                emitter.instruction("fmov x1, d0");                             // move the appended float payload bits into the runtime value register
                emitter.instruction(&format!("mov x0, {}", array_reg));         // move the static array pointer into the runtime receiver register
                emitter.instruction("bl __rt_array_push_int");                  // append the float payload bits and return the possibly-grown static array
            }
            PhpType::Str => {
                emitter.instruction(&format!("mov x0, {}", array_reg));         // move the static array pointer into the runtime receiver register
                emitter.instruction("bl __rt_array_push_str");                  // persist and append the string payload into the static array
            }
            PhpType::Callable => {
                emitter.instruction("mov x1, x0");                              // move the callable pointer bits into the runtime value register
                emitter.instruction(&format!("mov x0, {}", array_reg));         // move the static array pointer into the runtime receiver register
                emitter.instruction("bl __rt_array_push_int");                  // append the callable pointer bits as a scalar slot
            }
            PhpType::Mixed | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
                emitter.instruction("mov x1, x0");                              // move the retained heap payload into the runtime value register
                emitter.instruction(&format!("mov x0, {}", array_reg));         // move the static array pointer into the runtime receiver register
                emitter.instruction("bl __rt_array_push_refcounted");           // append the retained heap payload into the static array
            }
            _ => emitter.comment("WARNING: unsupported static property array push payload"),
        },
        Arch::X86_64 => match val_ty {
            PhpType::Int | PhpType::Bool => {
                emitter.instruction("mov rsi, rax");                            // move the appended scalar payload into the SysV value register
                emitter.instruction(&format!("mov rdi, {}", array_reg));        // move the static array pointer into the SysV receiver register
                abi::emit_call_label(emitter, "__rt_array_push_int");
            }
            PhpType::Float => {
                emitter.instruction("movq rsi, xmm0");                          // move the appended float payload bits into the SysV value register
                emitter.instruction(&format!("mov rdi, {}", array_reg));        // move the static array pointer into the SysV receiver register
                abi::emit_call_label(emitter, "__rt_array_push_int");
            }
            PhpType::Str => {
                emitter.instruction("mov rsi, rax");                            // move the appended string pointer into the SysV payload register
                emitter.instruction(&format!("mov rdi, {}", array_reg));        // move the static array pointer into the SysV receiver register
                abi::emit_call_label(emitter, "__rt_array_push_str");
            }
            PhpType::Callable => {
                emitter.instruction("mov rsi, rax");                            // move the callable pointer bits into the SysV value register
                emitter.instruction(&format!("mov rdi, {}", array_reg));        // move the static array pointer into the SysV receiver register
                abi::emit_call_label(emitter, "__rt_array_push_int");
            }
            PhpType::Mixed | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
                emitter.instruction("mov rsi, rax");                            // move the retained heap payload into the SysV value register
                emitter.instruction(&format!("mov rdi, {}", array_reg));        // move the static array pointer into the SysV receiver register
                abi::emit_call_label(emitter, "__rt_array_push_refcounted");
            }
            _ => emitter.comment("WARNING: unsupported static property array push payload"),
        },
    }
}

struct StaticIndexedAssignState {
    val_ty: PhpType,
    effective_store_ty: PhpType,
    stores_refcounted_pointer: bool,
}

impl StaticIndexedAssignState {
    fn new(elem_ty: &PhpType, val_ty: &PhpType) -> Self {
        let effective_store_ty = if matches!(elem_ty, PhpType::Mixed) {
            PhpType::Mixed
        } else if elem_ty != val_ty {
            val_ty.clone()
        } else {
            elem_ty.clone()
        };
        let stores_refcounted_pointer = matches!(
            effective_store_ty,
            PhpType::Mixed | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_)
        );
        Self {
            val_ty: val_ty.clone(),
            effective_store_ty,
            stores_refcounted_pointer,
        }
    }
}

fn prepare_static_array_assign_value(
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    elem_ty: &PhpType,
) -> PhpType {
    let mut val_ty = emit_expr(value, emitter, ctx, data);
    if matches!(elem_ty, PhpType::Mixed) && !matches!(val_ty, PhpType::Mixed | PhpType::Union(_)) {
        crate::codegen::emit_box_current_value_as_mixed(emitter, &val_ty);
        val_ty = PhpType::Mixed;
    } else {
        super::super::helpers::retain_borrowed_heap_result(emitter, value, &val_ty);
    }
    match &val_ty {
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);
        }
        PhpType::Float => abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter)),
        _ => abi::emit_push_reg(emitter, abi::int_result_reg(emitter)),
    }
    val_ty
}

fn restore_static_array_assign_value_aarch64(emitter: &mut Emitter, val_ty: &PhpType) {
    match val_ty {
        PhpType::Str => abi::emit_pop_reg_pair(emitter, "x1", "x2"),
        PhpType::Float => abi::emit_pop_float_reg(emitter, abi::float_result_reg(emitter)),
        _ => abi::emit_pop_reg(emitter, "x0"),
    }
}

fn restore_static_array_assign_value_x86_64(emitter: &mut Emitter, val_ty: &PhpType) {
    match val_ty {
        PhpType::Str => abi::emit_pop_reg_pair(emitter, "rax", "rdx"),
        PhpType::Float => abi::emit_pop_float_reg(emitter, "xmm0"),
        _ => abi::emit_pop_reg(emitter, "rax"),
    }
}

fn grow_static_indexed_array_until_ready_aarch64(emitter: &mut Emitter, ctx: &mut Context) {
    emitter.instruction("ldr x12, [x10, #8]");                                  // load the current static array capacity before growth checks
    let grow_check = ctx.next_label("static_array_assign_grow_check");
    let grow_ready = ctx.next_label("static_array_assign_grow_ready");
    emitter.label(&grow_check);
    emitter.instruction("cmp x9, x12");                                         // does the target index fit in the current static array capacity?
    emitter.instruction(&format!("b.lo {}", grow_ready));                       // skip growth once the target slot is addressable
    emitter.instruction("str x9, [sp, #-16]!");                                 // preserve the target index across the growth helper
    emitter.instruction("mov x0, x10");                                         // pass the current static array pointer to the growth helper
    emitter.instruction("bl __rt_array_grow");                                  // grow the static array until the indexed slot fits
    emitter.instruction("mov x10, x0");                                         // keep the possibly-reallocated static array pointer in the working register
    emitter.instruction("ldr x9, [sp], #16");                                   // restore the target index after growth
    emitter.instruction("ldr x12, [x10, #8]");                                  // reload capacity after growth so the loop converges
    emitter.instruction(&format!("b {}", grow_check));                          // continue growing until the indexed slot is addressable
    emitter.label(&grow_ready);
}

fn grow_static_indexed_array_until_ready_x86_64(emitter: &mut Emitter, ctx: &mut Context) {
    let grow_check = ctx.next_label("static_array_assign_grow_check");
    let grow_ready = ctx.next_label("static_array_assign_grow_ready");
    emitter.label(&grow_check);
    emitter.instruction("mov r12, QWORD PTR [r10 + 8]");                        // load the current static array capacity before growth checks
    emitter.instruction("cmp r9, r12");                                         // does the target index fit in the current static array capacity?
    emitter.instruction(&format!("jb {}", grow_ready));                         // skip growth once the target slot is addressable
    abi::emit_push_reg(emitter, "r9");                                          // preserve the target index across the growth helper
    emitter.instruction("mov rdi, r10");                                        // pass the current static array pointer to the growth helper
    abi::emit_call_label(emitter, "__rt_array_grow");
    emitter.instruction("mov r10, rax");                                        // keep the possibly-reallocated static array pointer in the working register
    abi::emit_pop_reg(emitter, "r9");                                           // restore the target index after growth
    emitter.instruction(&format!("jmp {}", grow_check));                        // continue growing until the indexed slot is addressable
    emitter.label(&grow_ready);
}

fn normalize_static_indexed_array_layout_aarch64(
    state: &StaticIndexedAssignState,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let skip_normalize = ctx.next_label("static_array_assign_skip_normalize");
    emitter.instruction("cmp x11, #0");                                         // is this the first indexed write into the static array?
    emitter.instruction(&format!("b.ne {}", skip_normalize));                   // keep the existing static array layout once it already has elements
    match &state.effective_store_ty {
        PhpType::Str => {
            emitter.instruction("mov x12, #16");                                // string static arrays use 16-byte pointer-plus-length slots
            emitter.instruction("str x12, [x10, #16]");                         // persist the string slot width in the static array header
        }
        _ => {
            emitter.instruction("mov x12, #8");                                 // scalar and pointer static arrays use 8-byte slots
            emitter.instruction("str x12, [x10, #16]");                         // persist the pointer-sized slot width in the static array header
        }
    }
    emitter.label(&skip_normalize);
}

fn normalize_static_indexed_array_layout_x86_64(
    state: &StaticIndexedAssignState,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let skip_normalize = ctx.next_label("static_array_assign_skip_normalize");
    emitter.instruction("cmp r11, 0");                                          // is this the first indexed write into the static array?
    emitter.instruction(&format!("jne {}", skip_normalize));                    // keep the existing static array layout once it already has elements
    match &state.effective_store_ty {
        PhpType::Str => {
            emitter.instruction("mov r12, 16");                                 // string static arrays use 16-byte pointer-plus-length slots
            emitter.instruction("mov QWORD PTR [r10 + 16], r12");               // persist the string slot width in the static array header
        }
        _ => {
            emitter.instruction("mov r12, 8");                                  // scalar and pointer static arrays use 8-byte slots
            emitter.instruction("mov QWORD PTR [r10 + 16], r12");               // persist the pointer-sized slot width in the static array header
        }
    }
    emitter.label(&skip_normalize);
}

fn store_static_indexed_array_value_aarch64(
    state: &StaticIndexedAssignState,
    emitter: &mut Emitter,
    _ctx: &mut Context,
) {
    if state.stores_refcounted_pointer {
        super::super::helpers::stamp_indexed_array_value_type(emitter, "x10", &state.val_ty);
        emitter.instruction("add x12, x10, #24");                               // compute the base of the static array pointer data region
        emitter.instruction("str x0, [x12, x9, lsl #3]");                       // store the retained heap pointer in the addressed static array slot
        return;
    }
    match &state.effective_store_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            emitter.instruction("add x12, x10, #24");                           // compute the base of the static array scalar data region
            emitter.instruction("str x0, [x12, x9, lsl #3]");                   // store the scalar payload in the addressed static array slot
        }
        PhpType::Float => {
            emitter.instruction("fmov x12, d0");                                // move the floating-point payload bits into an integer scratch register
            emitter.instruction("add x13, x10, #24");                           // compute the base of the static array scalar data region
            emitter.instruction("str x12, [x13, x9, lsl #3]");                  // store the floating-point payload bits in the addressed static array slot
        }
        PhpType::Str => {
            super::super::helpers::stamp_indexed_array_value_type(emitter, "x10", &state.val_ty);
            emitter.instruction("lsl x12, x9, #4");                             // convert the string index into its 16-byte slot offset
            emitter.instruction("add x12, x10, x12");                           // compute the addressed static array string slot
            emitter.instruction("add x12, x12, #24");                           // skip the static array header to reach payload storage
            emitter.instruction("str x1, [x12]");                               // store the new string pointer in the static array slot
            emitter.instruction("str x2, [x12, #8]");                           // store the new string length in the static array slot
        }
        _ => {}
    }
}

fn store_static_indexed_array_value_x86_64(
    state: &StaticIndexedAssignState,
    emitter: &mut Emitter,
    _ctx: &mut Context,
) {
    if state.stores_refcounted_pointer {
        abi::emit_push_reg(emitter, "rax");                                     // preserve the retained heap pointer across value-type stamping
        super::super::helpers::stamp_indexed_array_value_type(emitter, "r10", &state.val_ty);
        abi::emit_pop_reg(emitter, "rax");                                      // restore the retained heap pointer after value-type stamping
        emitter.instruction("mov QWORD PTR [r10 + 24 + r9 * 8], rax");          // store the retained heap pointer in the addressed static array slot
        return;
    }
    match &state.effective_store_ty {
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            emitter.instruction("mov QWORD PTR [r10 + 24 + r9 * 8], rax");      // store the scalar payload in the addressed static array slot
        }
        PhpType::Float => {
            emitter.instruction("movq r12, xmm0");                              // move the floating-point payload bits into an integer scratch register
            emitter.instruction("mov QWORD PTR [r10 + 24 + r9 * 8], r12");      // store the floating-point payload bits in the addressed static array slot
        }
        PhpType::Str => {
            abi::emit_push_reg_pair(emitter, "rax", "rdx");                    // preserve the string payload across value-type stamping
            super::super::helpers::stamp_indexed_array_value_type(emitter, "r10", &state.val_ty);
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");                     // restore the string payload after value-type stamping
            emitter.instruction("mov rcx, r9");                                 // copy the target index before scaling it into a string-slot offset
            emitter.instruction("shl rcx, 4");                                  // convert the target index into a 16-byte static array string-slot offset
            emitter.instruction("lea rcx, [r10 + rcx + 24]");                   // compute the addressed static array string slot
            emitter.instruction("mov QWORD PTR [rcx], rax");                    // store the new string pointer in the static array slot
            emitter.instruction("mov QWORD PTR [rcx + 8], rdx");                // store the new string length in the static array slot
        }
        _ => {}
    }
}

fn extend_static_indexed_array_if_needed_aarch64(
    _state: &StaticIndexedAssignState,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let skip_extend = ctx.next_label("static_array_assign_skip_extend");
    emitter.instruction("ldr x11, [x10]");                                      // reload the current static array logical length after storing the value
    emitter.instruction("cmp x9, x11");                                         // does this indexed write extend the static array?
    emitter.instruction(&format!("b.lo {}", skip_extend));                      // keep the current length for overwrites inside the static array
    emitter.instruction("add x12, x9, #1");                                     // compute the extended static array length
    emitter.instruction("str x12, [x10]");                                      // persist the extended static array length
    emitter.label(&skip_extend);
}

fn extend_static_indexed_array_if_needed_x86_64(
    _state: &StaticIndexedAssignState,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let skip_extend = ctx.next_label("static_array_assign_skip_extend");
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // reload the current static array logical length after storing the value
    emitter.instruction("cmp r9, r11");                                         // does this indexed write extend the static array?
    emitter.instruction(&format!("jb {}", skip_extend));                        // keep the current length for overwrites inside the static array
    emitter.instruction("lea r12, [r9 + 1]");                                   // compute the extended static array length
    emitter.instruction("mov QWORD PTR [r10], r12");                            // persist the extended static array length
    emitter.label(&skip_extend);
}

fn emit_ensure_indexed_array_pointer(elem_ty: &PhpType, emitter: &mut Emitter, ctx: &mut Context) {
    let ready = ctx.next_label("static_array_ready");
    let elem_size = if matches!(elem_ty.codegen_repr(), PhpType::Str) { 16 } else { 8 };
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #0");                                  // does the static array slot already point at heap storage?
            emitter.instruction(&format!("b.ne {}", ready));                    // reuse existing static array storage when it is already initialized
            emitter.instruction("mov x0, #4");                                  // use a small default capacity for an implicitly-created static array
            emitter.instruction(&format!("mov x1, #{}", elem_size));            // choose the element slot width for the implicit static array
            emitter.instruction("bl __rt_array_new");                           // allocate the implicit indexed array for the static property
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 0");                                  // does the static array slot already point at heap storage?
            emitter.instruction(&format!("jne {}", ready));                     // reuse existing static array storage when it is already initialized
            emitter.instruction("mov rdi, 4");                                  // use a small default capacity for an implicitly-created static array
            emitter.instruction(&format!("mov rsi, {}", elem_size));            // choose the element slot width for the implicit static array
            abi::emit_call_label(emitter, "__rt_array_new");
        }
    }
    emitter.label(&ready);
}

fn emit_and_push_called_class_id_if_needed(
    branches: &[StaticPropertyBranch],
    emitter: &mut Emitter,
    ctx: &Context,
) -> bool {
    if branches.is_empty() {
        return false;
    }
    let class_id_reg = class_id_work_reg(emitter);
    if !emit_called_class_id_into(emitter, ctx, class_id_reg) {
        emitter.comment("WARNING: missing forwarded called class id");
        return false;
    }
    abi::emit_push_reg(emitter, class_id_reg);                                  // preserve the called class id across value evaluation
    true
}

fn emit_called_class_id_into(emitter: &mut Emitter, ctx: &Context, dest: &str) -> bool {
    if let Some(var) = ctx.variables.get("__elephc_called_class_id") {
        abi::load_at_offset(emitter, abi::int_result_reg(emitter), var.stack_offset); // load the forwarded called-class id from the current static method frame
    } else if let Some(var) = ctx.variables.get("this") {
        abi::load_at_offset(emitter, abi::int_result_reg(emitter), var.stack_offset); // load $this so its runtime class id can drive late static storage
        abi::emit_load_from_address(
            emitter,
            abi::int_result_reg(emitter),
            abi::int_result_reg(emitter),
            0,
        );
    } else {
        return false;
    }
    emitter.instruction(&format!("mov {}, {}", dest, abi::int_result_reg(emitter))); // copy the called class id into a scratch register for branch dispatch
    true
}

fn emit_dynamic_load_static_property_reg(
    property: &str,
    class_id_reg: &str,
    fallback_declaring_class: &str,
    branches: &[StaticPropertyBranch],
    dest_reg: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let done = ctx.next_label("static_prop_load_done");
    let mut labels = Vec::new();
    for branch in branches {
        let label = ctx.next_label("static_prop_load_branch");
        emit_branch_if_class_id_matches(emitter, class_id_reg, branch.class_id, &label);
        labels.push((label, branch));
    }
    let fallback_symbol = static_property_symbol(fallback_declaring_class, property);
    abi::emit_load_symbol_to_reg(emitter, dest_reg, &fallback_symbol, 0);
    emit_jump(emitter, &done);
    for (label, branch) in labels {
        emitter.label(&label);
        let symbol = static_property_symbol(&branch.declaring_class, property);
        abi::emit_load_symbol_to_reg(emitter, dest_reg, &symbol, 0);
        emit_jump(emitter, &done);
    }
    emitter.label(&done);
}

fn emit_dynamic_store_result_to_static_property(
    property: &str,
    class_id_reg: &str,
    fallback_declaring_class: &str,
    branches: &[StaticPropertyBranch],
    ty: &PhpType,
    release_previous: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let done = ctx.next_label("static_prop_store_done");
    let mut labels = Vec::new();
    for branch in branches {
        let label = ctx.next_label("static_prop_store_branch");
        emit_branch_if_class_id_matches(emitter, class_id_reg, branch.class_id, &label);
        labels.push((label, branch));
    }
    let fallback_symbol = static_property_symbol(fallback_declaring_class, property);
    abi::emit_store_result_to_symbol(emitter, &fallback_symbol, ty, release_previous);
    emit_jump(emitter, &done);
    for (label, branch) in labels {
        emitter.label(&label);
        let symbol = static_property_symbol(&branch.declaring_class, property);
        abi::emit_store_result_to_symbol(emitter, &symbol, ty, release_previous);
        emit_jump(emitter, &done);
    }
    emitter.label(&done);
}

fn emit_dynamic_store_reg_to_static_property(
    property: &str,
    class_id_reg: &str,
    source_reg: &str,
    fallback_declaring_class: &str,
    branches: &[StaticPropertyBranch],
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let done = ctx.next_label("static_prop_store_done");
    let mut labels = Vec::new();
    for branch in branches {
        let label = ctx.next_label("static_prop_store_branch");
        emit_branch_if_class_id_matches(emitter, class_id_reg, branch.class_id, &label);
        labels.push((label, branch));
    }
    let fallback_symbol = static_property_symbol(fallback_declaring_class, property);
    abi::emit_store_reg_to_symbol(emitter, source_reg, &fallback_symbol, 0);
    emit_jump(emitter, &done);
    for (label, branch) in labels {
        emitter.label(&label);
        let symbol = static_property_symbol(&branch.declaring_class, property);
        abi::emit_store_reg_to_symbol(emitter, source_reg, &symbol, 0);
        emit_jump(emitter, &done);
    }
    emitter.label(&done);
}

fn emit_branch_if_class_id_matches(
    emitter: &mut Emitter,
    class_id_reg: &str,
    class_id: u64,
    label: &str,
) {
    let compare_reg = class_id_compare_reg(emitter);
    abi::emit_load_int_immediate(emitter, compare_reg, class_id as i64);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp {}, {}", class_id_reg, compare_reg)); // compare the runtime called class id to a redeclared static property owner
            emitter.instruction(&format!("b.eq {}", label));                   // use this static property slot when the called class id matches
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp {}, {}", class_id_reg, compare_reg)); // compare the runtime called class id to a redeclared static property owner
            emitter.instruction(&format!("je {}", label));                     // use this static property slot when the called class id matches
        }
    }
}

fn emit_jump(emitter: &mut Emitter, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("b {}", label));                       // jump to the end of the static property dispatch chain
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("jmp {}", label));                     // jump to the end of the static property dispatch chain
        }
    }
}

fn class_id_work_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x13",
        Arch::X86_64 => "r13",
    }
}

fn class_id_compare_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x14",
        Arch::X86_64 => "r14",
    }
}

fn dynamic_static_property_branches(
    receiver: &StaticReceiver,
    property: &str,
    fallback_declaring_class: &str,
    ctx: &Context,
) -> Vec<StaticPropertyBranch> {
    if !matches!(receiver, StaticReceiver::Static) {
        return Vec::new();
    }
    let Some(base_class) = ctx.current_class.as_deref() else {
        return Vec::new();
    };
    let mut branches = Vec::new();
    for (class_name, class_info) in &ctx.classes {
        if !is_same_or_descendant(class_name, base_class, ctx) {
            continue;
        }
        let Some(declaring_class) = class_info.static_property_declaring_classes.get(property) else {
            continue;
        };
        if declaring_class == fallback_declaring_class {
            continue;
        }
        branches.push(StaticPropertyBranch {
            class_id: class_info.class_id,
            declaring_class: declaring_class.clone(),
        });
    }
    branches.sort_by_key(|branch| branch.class_id);
    branches.dedup_by_key(|branch| branch.class_id);
    branches
}

fn is_same_or_descendant(class_name: &str, ancestor: &str, ctx: &Context) -> bool {
    let mut cursor = Some(class_name);
    while let Some(name) = cursor {
        if name == ancestor {
            return true;
        }
        cursor = ctx
            .classes
            .get(name)
            .and_then(|class_info| class_info.parent.as_deref());
    }
    false
}

fn resolve_static_property(
    receiver: &StaticReceiver,
    property: &str,
    ctx: &Context,
    emitter: &mut Emitter,
) -> Option<(String, String, PhpType, bool)> {
    let class_name = match receiver {
        StaticReceiver::Named(class_name) => class_name.as_str().to_string(),
        StaticReceiver::Self_ | StaticReceiver::Static => match &ctx.current_class {
            Some(class_name) => class_name.clone(),
            None => {
                emitter.comment("WARNING: self::/static:: used outside class scope");
                return None;
            }
        },
        StaticReceiver::Parent => {
            let current_class = match &ctx.current_class {
                Some(class_name) => class_name.clone(),
                None => {
                    emitter.comment("WARNING: parent:: used outside class scope");
                    return None;
                }
            };
            match ctx.classes.get(&current_class).and_then(|info| info.parent.clone()) {
                Some(parent_name) => parent_name,
                None => {
                    emitter.comment(&format!("WARNING: class {} has no parent", current_class));
                    return None;
                }
            }
        }
    };

    let class_info = match ctx.classes.get(&class_name) {
        Some(class_info) => class_info,
        None => {
            emitter.comment(&format!("WARNING: undefined class {}", class_name));
            return None;
        }
    };
    let prop_ty = match class_info
        .static_properties
        .iter()
        .find(|(name, _)| name == property)
        .map(|(_, ty)| ty.clone())
    {
        Some(prop_ty) => prop_ty,
        None => {
            emitter.comment(&format!(
                "WARNING: undefined static property {}::${}",
                class_name, property
            ));
            return None;
        }
    };
    let declaring_class = class_info
        .static_property_declaring_classes
        .get(property)
        .cloned()
        .unwrap_or_else(|| class_name.clone());
    let declared = class_info.declared_static_properties.contains(property);
    Some((class_name, declaring_class, prop_ty, declared))
}
