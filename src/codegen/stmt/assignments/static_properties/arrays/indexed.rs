use super::super::late_bound::{self, StaticPropertyBranch};
use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::codegen::stmt::helpers;
use crate::names::static_property_symbol;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub(super) fn emit_static_indexed_array_assign(
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
        let class_id_reg = late_bound::class_id_work_reg(emitter);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("ldr {}, [sp, #{}]", class_id_reg, class_id_stack_offset)); // reload the called class id from the static array temporary stack
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov {}, QWORD PTR [rsp + {}]", class_id_reg, class_id_stack_offset)); // reload the called class id from the static array temporary stack
            }
        }
        late_bound::emit_dynamic_store_reg_to_static_property(
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
    let boxed_iterable =
        crate::codegen::emit_box_iterable_value_for_mixed_container(emitter, &mut val_ty);
    if !boxed_iterable
        && matches!(elem_ty, PhpType::Mixed)
        && !matches!(val_ty, PhpType::Mixed | PhpType::Union(_))
    {
        crate::codegen::emit_box_current_value_as_mixed(emitter, &val_ty);
        val_ty = PhpType::Mixed;
    } else if !boxed_iterable {
        helpers::retain_borrowed_heap_result(emitter, value, &val_ty);
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
        helpers::stamp_indexed_array_value_type(emitter, "x10", &state.val_ty);
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
            helpers::stamp_indexed_array_value_type(emitter, "x10", &state.val_ty);
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
        helpers::stamp_indexed_array_value_type(emitter, "r10", &state.val_ty);
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
            helpers::stamp_indexed_array_value_type(emitter, "r10", &state.val_ty);
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
