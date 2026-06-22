//! Purpose:
//! Lowers indexed array mutation through static property storage.
//! Works with static property symbols and class metadata instead of local frame slots.
//!
//! Called from:
//! - `crate::codegen::stmt::assignments::static_properties`
//!
//! Key details:
//! - Late-bound receivers and visibility checks must match PHP inheritance semantics before storage is updated.

use super::super::late_bound::{self, StaticPropertyBranch};
use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{coerce_result_to_type, emit_expr};
use crate::codegen::platform::Arch;
use crate::codegen::stmt::helpers;
use crate::names::static_property_symbol;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Lowers indexed array assignment to a static property: `$prop[$index] = value`.
/// Handles array growth, slot layout normalization, and dispatches to late-bound storage when `class_id_saved` is true.
pub(super) fn emit_static_indexed_array_assign(
    property: &str,
    declaring_class: &str,
    branches: &[StaticPropertyBranch],
    class_id_saved: bool,
    prop_ty: &PhpType,
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
                prop_ty,
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
                prop_ty,
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
                prop_ty,
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
                prop_ty,
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

/// Publishes the static array pointer to the appropriate storage after a mutation.
/// When `class_id_saved`, reloads the class ID from the stack offset and dispatches via late-bound store;
/// otherwise stores directly to the static property symbol.
fn publish_static_array_pointer(
    property: &str,
    declaring_class: &str,
    branches: &[StaticPropertyBranch],
    class_id_saved: bool,
    class_id_stack_offset: usize,
    prop_ty: &PhpType,
    source_reg: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    if class_id_saved {
        let class_id_reg = late_bound::class_id_work_reg(emitter);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("ldr {}, [sp, #{}]", class_id_reg, class_id_stack_offset)); //reload the called class id from the static array temporary stack
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov {}, QWORD PTR [rsp + {}]", class_id_reg, class_id_stack_offset)); //reload the called class id from the static array temporary stack
            }
        }
        late_bound::emit_dynamic_store_reg_to_static_property(
            property,
            class_id_reg,
            source_reg,
            declaring_class,
            branches,
            prop_ty,
            emitter,
            ctx,
        );
    } else {
        let symbol = static_property_symbol(declaring_class, property);
        abi::emit_store_reg_to_symbol(emitter, source_reg, &symbol, 0);
        late_bound::clear_uninitialized_marker_after_static_store(emitter, &symbol, prop_ty);
    }
}

/// Holds the value type and store representation for a static indexed array assignment.
/// `effective_store_ty` determines slot width and handling; `stores_refcounted_pointer` indicates heap-allocated values.
struct StaticIndexedAssignState {
    val_ty: PhpType,
    effective_store_ty: PhpType,
    stores_refcounted_pointer: bool,
}

impl StaticIndexedAssignState {
    /// Constructs state for a static indexed array assignment.
    /// `elem_ty` is the declared element type of the static property array;
    /// `val_ty` is the resolved type of the assigned value expression.
    /// Derives `effective_store_ty` from the relationship between `elem_ty` and `val_ty`
    /// (using `val_ty` when types differ, otherwise `elem_ty`; `Mixed` is preserved).
    /// `stores_refcounted_pointer` is true when the effective store type requires a heap pointer.
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

/// Prepares the assigned value: emits the expression, coerces to `elem_ty` if needed, boxes iterables for Mixed containers,
/// and pushes the value onto the stack in ABI-appropriate form (register pair for strings, single register otherwise).
fn prepare_static_array_assign_value(
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    elem_ty: &PhpType,
) -> PhpType {
    let mut val_ty = emit_expr(value, emitter, ctx, data);
    if matches!(val_ty, PhpType::Mixed | PhpType::Union(_))
        && !matches!(elem_ty, PhpType::Mixed | PhpType::Union(_))
        && crate::codegen::expr::can_coerce_result_to_type(&val_ty, elem_ty)
    {
        let release_mixed_after_coerce =
            helpers::should_release_owned_mixed_after_coerce(value, &val_ty, elem_ty);
        if release_mixed_after_coerce {
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
        }
        coerce_result_to_type(emitter, ctx, data, &val_ty, elem_ty);
        if release_mixed_after_coerce {
            helpers::release_preserved_mixed_after_coercion(emitter, elem_ty);
        }
        val_ty = elem_ty.clone();
    }
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

/// Restores the assigned value from the stack into ABI result registers after index evaluation on ARM64.
/// Strings use a register pair (x1, x2), floats use d0, and other types use x0.
fn restore_static_array_assign_value_aarch64(emitter: &mut Emitter, val_ty: &PhpType) {
    match val_ty {
        PhpType::Str => abi::emit_pop_reg_pair(emitter, "x1", "x2"),
        PhpType::Float => abi::emit_pop_float_reg(emitter, abi::float_result_reg(emitter)),
        _ => abi::emit_pop_reg(emitter, "x0"),
    }
}

/// Restores the assigned value from the stack into ABI result registers after index evaluation on x86_64.
/// Strings use a register pair (rax, rdx), floats use xmm0, and other types use rax.
fn restore_static_array_assign_value_x86_64(emitter: &mut Emitter, val_ty: &PhpType) {
    match val_ty {
        PhpType::Str => abi::emit_pop_reg_pair(emitter, "rax", "rdx"),
        PhpType::Float => abi::emit_pop_float_reg(emitter, "xmm0"),
        _ => abi::emit_pop_reg(emitter, "rax"),
    }
}

/// Grows the static array until the target index is within capacity on ARM64.
/// Loads capacity, loops with `__rt_array_grow` as needed, and restores the target index after reallocation.
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

/// Grows the static array until the target index is within capacity on x86_64.
/// Loads capacity, loops with `__rt_array_grow` as needed, and restores the target index after reallocation.
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

/// Normalizes the static array slot width on first write for ARM64.
/// String elements use 16-byte slots; scalar and pointer types use 8-byte slots. Skipped when the array already has elements.
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

/// Normalizes the static array slot width on first write for x86_64.
/// String elements use 16-byte slots; scalar and pointer types use 8-byte slots. Skipped when the array already has elements.
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

/// Stores the assigned value into the static array slot at the target index on ARM64.
/// For refcounted types, stamps the value type and stores the pointer; for strings, stores pointer and length separately.
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

/// Stores the assigned value into the static array slot at the target index on x86_64.
/// For refcounted types, stamps the value type and stores the pointer; for strings, stores pointer and length separately.
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

/// Extends the static array logical length if the target index is at or beyond the current length on ARM64.
/// Skipped for overwrites inside the existing array bounds.
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

/// Extends the static array logical length if the target index is at or beyond the current length on x86_64.
/// Skipped for overwrites inside the existing array bounds.
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
