mod magic_set;
mod storage;
mod target;

use super::super::super::abi;
use super::super::super::context::Context;
use super::super::super::data_section::DataSection;
use super::super::super::emit::Emitter;
use super::super::super::expr::emit_expr;
use crate::parser::ast::Expr;

pub(crate) fn emit_property_assign_stmt(
    object: &Expr,
    property: &str,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment(&format!("->{}  = ...", property));

    let magic_set_class = magic_set::resolve_magic_set_target(object, property, ctx);
    let val_ty = emit_expr(value, emitter, ctx, data);
    if magic_set_class.is_none() {
        super::super::helpers::retain_borrowed_heap_result(emitter, value, &val_ty);
    }
    abi::emit_push_result_value(emitter, &val_ty);

    let obj_ty = emit_expr(object, emitter, ctx, data);
    let target = match target::resolve_property_assign_target(
        &obj_ty,
        property,
        magic_set_class.as_deref(),
        emitter,
        ctx,
    ) {
        target::PropertyAssignResolution::Resolved(target) => target,
        target::PropertyAssignResolution::UseMagicSet(class_name) => {
            magic_set::emit_magic_set_call(&class_name, property, &val_ty, emitter, ctx, data);
            return;
        }
        target::PropertyAssignResolution::Abort => return,
    };

    if target.needs_deref {
        abi::emit_call_label(emitter, "__rt_ptr_check_nonnull");
        emitter.comment(&format!(
            "store extern field {}::{} at offset {}",
            target.class_name, property, target.offset
        ));
    }

    let object_reg = abi::symbol_scratch_reg(emitter);
    emitter.instruction(&format!("mov {}, {}", object_reg, abi::int_result_reg(emitter))); // keep the object pointer in a scratch register while property storage is updated
    if !target.needs_deref {
        storage::release_previous_property_value(emitter, object_reg, &target.prop_ty, target.offset);
    }

    storage::store_property_value(emitter, object_reg, &val_ty, target.offset);
}

pub(crate) fn emit_property_array_push_stmt(
    object: &Expr,
    property: &str,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment(&format!("->{}[] = ...", property));

    let obj_ty = emit_expr(object, emitter, ctx, data);
    let target = match target::resolve_property_assign_target(&obj_ty, property, None, emitter, ctx) {
        target::PropertyAssignResolution::Resolved(target) => target,
        target::PropertyAssignResolution::UseMagicSet(_) | target::PropertyAssignResolution::Abort => {
            emitter.comment("WARNING: property array push requires a concrete array property");
            return;
        }
    };
    let elem_ty = match &target.prop_ty {
        crate::types::PhpType::Array(elem_ty) => *elem_ty.clone(),
        _ => {
            emitter.comment("WARNING: property array push on non-array property");
            return;
        }
    };

    if target.needs_deref {
        abi::emit_call_label(emitter, "__rt_ptr_check_nonnull");
        emitter.comment(&format!(
            "append to extern field {}::{} at offset {}",
            target.class_name, property, target.offset
        ));
    }

    let object_reg = abi::symbol_scratch_reg(emitter);
    emitter.instruction(&format!("mov {}, {}", object_reg, abi::int_result_reg(emitter))); // preserve the owning object pointer while the append helper evaluates the value and may reallocate the array
    abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), object_reg, target.offset);
    abi::emit_push_reg(emitter, object_reg);
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));

    let mut val_ty = emit_expr(value, emitter, ctx, data);
    if matches!(elem_ty, crate::types::PhpType::Mixed)
        && !matches!(val_ty, crate::types::PhpType::Mixed | crate::types::PhpType::Union(_))
    {
        crate::codegen::emit_box_current_value_as_mixed(emitter, &val_ty);
        val_ty = crate::types::PhpType::Mixed;
    } else {
        super::super::helpers::retain_borrowed_heap_result(emitter, value, &val_ty);
    }

    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            abi::emit_pop_reg(emitter, "x9");
            match &val_ty {
                crate::types::PhpType::Int | crate::types::PhpType::Bool => {
                    emitter.instruction("mov x1, x0");                          // move the appended scalar payload into the runtime helper value register
                    emitter.instruction("mov x0, x9");                          // move the current array pointer into the runtime helper receiver register
                    emitter.instruction("bl __rt_array_push_int");              // append the scalar payload and return the possibly-grown array pointer
                }
                crate::types::PhpType::Float => {
                    emitter.instruction("fmov x1, d0");                         // move the appended float payload bits into the runtime helper value register
                    emitter.instruction("mov x0, x9");                          // move the current array pointer into the runtime helper receiver register
                    emitter.instruction("bl __rt_array_push_int");              // append the float payload bits as an 8-byte scalar slot
                }
                crate::types::PhpType::Str => {
                    emitter.instruction("mov x0, x9");                          // move the current array pointer into the runtime helper receiver register
                    emitter.instruction("bl __rt_array_push_str");              // persist and append the string payload, returning the possibly-grown array pointer
                }
                crate::types::PhpType::Callable => {
                    emitter.instruction("mov x1, x0");                          // move the callable pointer bits into the runtime helper value register
                    emitter.instruction("mov x0, x9");                          // move the current array pointer into the runtime helper receiver register
                    emitter.instruction("bl __rt_array_push_int");              // append the callable pointer bits as a plain scalar slot
                }
                crate::types::PhpType::Mixed
                | crate::types::PhpType::Array(_)
                | crate::types::PhpType::AssocArray { .. }
                | crate::types::PhpType::Object(_) => {
                    emitter.instruction("mov x1, x0");                          // move the retained heap payload pointer into the runtime helper child register
                    emitter.instruction("mov x0, x9");                          // move the current array pointer into the runtime helper receiver register
                    emitter.instruction("bl __rt_array_push_refcounted");       // append the retained heap payload and return the possibly-grown array pointer
                }
                _ => {
                    emitter.comment("WARNING: unsupported property array push payload");
                    abi::emit_pop_reg(emitter, "x10");
                    return;
                }
            }
            abi::emit_pop_reg(emitter, "x10");
            abi::emit_store_to_address(emitter, "x0", "x10", target.offset);
        }
        crate::codegen::platform::Arch::X86_64 => {
            abi::emit_pop_reg(emitter, "r11");
            match &val_ty {
                crate::types::PhpType::Int | crate::types::PhpType::Bool => {
                    emitter.instruction("mov rsi, rax");                        // move the appended scalar payload into the SysV runtime helper value register
                    emitter.instruction("mov rdi, r11");                        // move the current array pointer into the SysV runtime helper receiver register
                    abi::emit_call_label(emitter, "__rt_array_push_int");
                }
                crate::types::PhpType::Float => {
                    emitter.instruction("movq rsi, xmm0");                      // move the appended float payload bits into the SysV runtime helper value register
                    emitter.instruction("mov rdi, r11");                        // move the current array pointer into the SysV runtime helper receiver register
                    abi::emit_call_label(emitter, "__rt_array_push_int");
                }
                crate::types::PhpType::Str => {
                    emitter.instruction("mov rsi, rax");                        // move the appended string pointer into the SysV runtime helper payload register
                    emitter.instruction("mov rdi, r11");                        // move the current array pointer into the SysV runtime helper receiver register
                    abi::emit_call_label(emitter, "__rt_array_push_str");
                }
                crate::types::PhpType::Callable => {
                    emitter.instruction("mov rsi, rax");                        // move the callable pointer bits into the SysV runtime helper value register
                    emitter.instruction("mov rdi, r11");                        // move the current array pointer into the SysV runtime helper receiver register
                    abi::emit_call_label(emitter, "__rt_array_push_int");
                }
                crate::types::PhpType::Mixed
                | crate::types::PhpType::Array(_)
                | crate::types::PhpType::AssocArray { .. }
                | crate::types::PhpType::Object(_) => {
                    emitter.instruction("mov rsi, rax");                        // move the retained heap payload pointer into the SysV runtime helper child register
                    emitter.instruction("mov rdi, r11");                        // move the current array pointer into the SysV runtime helper receiver register
                    abi::emit_call_label(emitter, "__rt_array_push_refcounted");
                }
                _ => {
                    emitter.comment("WARNING: unsupported property array push payload");
                    abi::emit_pop_reg(emitter, "r10");
                    return;
                }
            }
            abi::emit_pop_reg(emitter, "r10");
            abi::emit_store_to_address(emitter, "rax", "r10", target.offset);
        }
    }
}

pub(crate) fn emit_property_array_assign_stmt(
    object: &Expr,
    property: &str,
    index: &Expr,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment(&format!("->{}[...] = ...", property));

    let obj_ty = emit_expr(object, emitter, ctx, data);
    let target = match target::resolve_property_assign_target(&obj_ty, property, None, emitter, ctx) {
        target::PropertyAssignResolution::Resolved(target) => target,
        target::PropertyAssignResolution::UseMagicSet(_) | target::PropertyAssignResolution::Abort => {
            emitter.comment("WARNING: property array assign requires a concrete array property");
            return;
        }
    };
    let elem_ty = match &target.prop_ty {
        crate::types::PhpType::Array(elem_ty) => *elem_ty.clone(),
        _ => {
            emitter.comment("WARNING: property array assign on non-array property");
            return;
        }
    };

    if target.needs_deref {
        abi::emit_call_label(emitter, "__rt_ptr_check_nonnull");
        emitter.comment(&format!(
            "assign into extern field {}::{} at offset {}",
            target.class_name, property, target.offset
        ));
    }

    let object_reg = abi::symbol_scratch_reg(emitter);
    emitter.instruction(&format!("mov {}, {}", object_reg, abi::int_result_reg(emitter))); // preserve the owning object pointer while the indexed write evaluates the index/value and may reallocate the array
    abi::emit_load_from_address(emitter, abi::int_result_reg(emitter), object_reg, target.offset);
    match emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            abi::emit_push_reg(emitter, object_reg);
            emitter.instruction("bl __rt_array_ensure_unique");                 // split shared indexed arrays before mutating the property-backed array storage
            abi::emit_pop_reg(emitter, object_reg);
            abi::emit_store_to_address(emitter, "x0", object_reg, target.offset);
            abi::emit_push_reg(emitter, object_reg);
            abi::emit_push_reg(emitter, "x0");
            emit_expr(index, emitter, ctx, data);
            abi::emit_push_reg(emitter, "x0");
            let val_ty = prepare_property_array_assign_value(value, emitter, ctx, data, &elem_ty);
            let state = PropertyIndexedAssignState::new(&elem_ty, &val_ty);
            emitter.instruction("ldr x9, [sp, #16]");                           // reload the indexed target slot after preserving the assigned value on the temporary stack
            emitter.instruction("ldr x10, [sp, #32]");                          // reload the property-backed array pointer after preserving the assigned value on the temporary stack
            emitter.instruction("ldr x11, [x10]");                              // load the original logical length before growth so overwrites can be distinguished from extensions
            emitter.instruction("ldr x12, [x10, #8]");                          // load the current capacity before checking whether the target slot already fits
            let grow_check = ctx.next_label("prop_array_assign_grow_check");
            let grow_ready = ctx.next_label("prop_array_assign_grow_ready");
            emitter.label(&grow_check);
            emitter.instruction("cmp x9, x12");                                 // does the target index already fit within the current property-backed array capacity?
            emitter.instruction(&format!("b.lo {}", grow_ready));               // skip growth once the target indexed slot is already addressable
            emitter.instruction("str x9, [sp, #-16]!");                         // preserve the target index because the growth helper clobbers caller-saved registers
            emitter.instruction("mov x0, x10");                                 // pass the property-backed array pointer to the growth helper
            emitter.instruction("bl __rt_array_grow");                          // grow the indexed array until the requested slot fits
            emitter.instruction("mov x10, x0");                                 // keep the possibly-reallocated property-backed array pointer in the long-lived working register
            emitter.instruction("ldr x9, [sp], #16");                           // restore the target index after the growth helper returns
            emitter.instruction("ldr x12, [x10, #8]");                          // reload the capacity after growth so the loop converges on the new header values
            emitter.instruction(&format!("b {}", grow_check));                  // continue growing until the target property slot fits
            emitter.label(&grow_ready);
            emitter.instruction("ldr x13, [sp, #48]");                          // reload the preserved owning object pointer before publishing the possibly-grown array pointer back into the property slot
            abi::emit_store_to_address(emitter, "x10", "x13", target.offset);
            restore_property_array_assign_value_aarch64(emitter, &val_ty);
            normalize_property_indexed_array_layout_aarch64(&state, emitter, ctx);
            store_property_indexed_array_value_aarch64(&target.prop_ty, &state, emitter, ctx);
            extend_property_indexed_array_if_needed_aarch64(&state, emitter, ctx);
            emitter.instruction("add sp, sp, #48");                             // drop the preserved object pointer, array pointer, and index after completing the property-backed indexed write
        }
        crate::codegen::platform::Arch::X86_64 => {
            abi::emit_push_reg(emitter, object_reg);
            emitter.instruction("mov rdi, rax");                                // pass the property-backed array pointer to the x86_64 uniqueness helper before mutating indexed storage
            abi::emit_call_label(emitter, "__rt_array_ensure_unique");
            abi::emit_pop_reg(emitter, object_reg);
            abi::emit_store_to_address(emitter, "rax", object_reg, target.offset);
            abi::emit_push_reg(emitter, object_reg);
            abi::emit_push_reg(emitter, "rax");
            emit_expr(index, emitter, ctx, data);
            abi::emit_push_reg(emitter, "rax");
            let val_ty = prepare_property_array_assign_value(value, emitter, ctx, data, &elem_ty);
            let state = PropertyIndexedAssignState::new(&elem_ty, &val_ty);
            emitter.instruction("mov r9, QWORD PTR [rsp + 16]");                // reload the indexed target slot after preserving the assigned value on the temporary stack
            emitter.instruction("mov r10, QWORD PTR [rsp + 32]");               // reload the property-backed array pointer after preserving the assigned value on the temporary stack
            emitter.instruction("mov r11, QWORD PTR [r10]");                    // load the original logical length before growth so overwrites can be distinguished from extensions
            let grow_check = ctx.next_label("prop_array_assign_grow_check");
            let grow_ready = ctx.next_label("prop_array_assign_grow_ready");
            emitter.label(&grow_check);
            emitter.instruction("mov r12, QWORD PTR [r10 + 8]");                // load the current capacity before checking whether the target slot already fits
            emitter.instruction("cmp r9, r12");                                 // does the target index already fit within the current property-backed array capacity?
            emitter.instruction(&format!("jb {}", grow_ready));                 // skip growth once the target indexed slot is already addressable
            abi::emit_push_reg(emitter, "r9");
            emitter.instruction("mov rdi, r10");                                // pass the property-backed array pointer to the x86_64 growth helper
            abi::emit_call_label(emitter, "__rt_array_grow");
            emitter.instruction("mov r10, rax");                                // keep the possibly-reallocated property-backed array pointer in the long-lived working register
            abi::emit_pop_reg(emitter, "r9");
            emitter.instruction(&format!("jmp {}", grow_check));                // continue growing until the target property slot fits
            emitter.label(&grow_ready);
            emitter.instruction("mov r13, QWORD PTR [rsp + 48]");               // reload the preserved owning object pointer before publishing the possibly-grown array pointer back into the property slot
            abi::emit_store_to_address(emitter, "r10", "r13", target.offset);
            restore_property_array_assign_value_x86_64(emitter, &val_ty);
            normalize_property_indexed_array_layout_x86_64(&state, emitter, ctx);
            store_property_indexed_array_value_x86_64(&target.prop_ty, &state, emitter, ctx);
            extend_property_indexed_array_if_needed_x86_64(&state, emitter, ctx);
            emitter.instruction("add rsp, 48");                                 // drop the preserved object pointer, array pointer, and index after completing the property-backed indexed write
        }
    }
}

struct PropertyIndexedAssignState {
    val_ty: crate::types::PhpType,
    effective_store_ty: crate::types::PhpType,
    stores_refcounted_pointer: bool,
}

impl PropertyIndexedAssignState {
    fn new(elem_ty: &crate::types::PhpType, val_ty: &crate::types::PhpType) -> Self {
        let effective_store_ty = if matches!(elem_ty, crate::types::PhpType::Mixed) {
            crate::types::PhpType::Mixed
        } else if elem_ty != val_ty {
            val_ty.clone()
        } else {
            elem_ty.clone()
        };
        let stores_refcounted_pointer = matches!(
            effective_store_ty,
            crate::types::PhpType::Mixed
                | crate::types::PhpType::Array(_)
                | crate::types::PhpType::AssocArray { .. }
                | crate::types::PhpType::Object(_)
        );
        Self {
            val_ty: val_ty.clone(),
            effective_store_ty,
            stores_refcounted_pointer,
        }
    }
}

fn prepare_property_array_assign_value(
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
    elem_ty: &crate::types::PhpType,
) -> crate::types::PhpType {
    let mut val_ty = emit_expr(value, emitter, ctx, data);
    if matches!(elem_ty, crate::types::PhpType::Mixed)
        && !matches!(val_ty, crate::types::PhpType::Mixed | crate::types::PhpType::Union(_))
    {
        crate::codegen::emit_box_current_value_as_mixed(emitter, &val_ty);
        val_ty = crate::types::PhpType::Mixed;
    } else {
        super::super::helpers::retain_borrowed_heap_result(emitter, value, &val_ty);
    }
    match &val_ty {
        crate::types::PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
            abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);
        }
        crate::types::PhpType::Float => abi::emit_push_float_reg(emitter, abi::float_result_reg(emitter)),
        _ => abi::emit_push_reg(emitter, abi::int_result_reg(emitter)),
    }
    val_ty
}

fn restore_property_array_assign_value_aarch64(
    emitter: &mut Emitter,
    val_ty: &crate::types::PhpType,
) {
    match val_ty {
        crate::types::PhpType::Str => abi::emit_pop_reg_pair(emitter, "x1", "x2"),
        crate::types::PhpType::Float => abi::emit_pop_float_reg(emitter, abi::float_result_reg(emitter)),
        _ => abi::emit_pop_reg(emitter, "x0"),
    }
}

fn restore_property_array_assign_value_x86_64(
    emitter: &mut Emitter,
    val_ty: &crate::types::PhpType,
) {
    match val_ty {
        crate::types::PhpType::Str => abi::emit_pop_reg_pair(emitter, "rax", "rdx"),
        crate::types::PhpType::Float => abi::emit_pop_float_reg(emitter, "xmm0"),
        _ => abi::emit_pop_reg(emitter, "rax"),
    }
}

fn normalize_property_indexed_array_layout_aarch64(
    state: &PropertyIndexedAssignState,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let skip_normalize = ctx.next_label("prop_array_assign_skip_normalize");
    emitter.instruction("cmp x11, #0");                                         // is this the first indexed write into the property-backed array?
    emitter.instruction(&format!("b.ne {}", skip_normalize));                   // keep the existing slot layout once the property-backed array already has elements
    match &state.effective_store_ty {
        crate::types::PhpType::Str => {
            emitter.instruction("mov x12, #16");                                // string arrays need 16-byte pointer-plus-length slots
            emitter.instruction("str x12, [x10, #16]");                         // persist the string-slot width in the property-backed array header
            super::super::helpers::stamp_indexed_array_value_type(emitter, "x10", &state.val_ty);
        }
        crate::types::PhpType::Mixed
        | crate::types::PhpType::Array(_)
        | crate::types::PhpType::AssocArray { .. }
        | crate::types::PhpType::Object(_) => {
            emitter.instruction("mov x12, #8");                                 // nested heap pointers still use 8-byte slots in property-backed indexed arrays
            emitter.instruction("str x12, [x10, #16]");                         // persist the pointer-sized slot width in the property-backed array header
        }
        _ => {
            emitter.instruction("mov x12, #8");                                 // scalar indexed arrays use ordinary 8-byte slots
            emitter.instruction("str x12, [x10, #16]");                         // persist the scalar slot width in the property-backed array header
            emitter.instruction("ldr x12, [x10, #-8]");                         // load the packed kind word from the property-backed array heap header
            emitter.instruction("mov x14, #0x80ff");                            // preserve the indexed-array kind and persistent copy-on-write flag bits
            emitter.instruction("and x12, x12, x14");                           // clear stale value_type bits while keeping the stable container metadata
            emitter.instruction("str x12, [x10, #-8]");                         // persist the scalar-oriented packed kind word back into the heap header
        }
    }
    emitter.label(&skip_normalize);
}

fn normalize_property_indexed_array_layout_x86_64(
    state: &PropertyIndexedAssignState,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    let skip_normalize = ctx.next_label("prop_array_assign_skip_normalize");
    emitter.instruction("cmp r11, 0");                                          // is this the first indexed write into the property-backed array?
    emitter.instruction(&format!("jne {}", skip_normalize));                    // keep the existing slot layout once the property-backed array already has elements
    match &state.effective_store_ty {
        crate::types::PhpType::Str => {
            emitter.instruction("mov r12, 16");                                 // string arrays need 16-byte pointer-plus-length slots
            emitter.instruction("mov QWORD PTR [r10 + 16], r12");               // persist the string-slot width in the property-backed array header
            super::super::helpers::stamp_indexed_array_value_type(emitter, "r10", &state.val_ty);
        }
        crate::types::PhpType::Mixed
        | crate::types::PhpType::Array(_)
        | crate::types::PhpType::AssocArray { .. }
        | crate::types::PhpType::Object(_) => {
            emitter.instruction("mov r12, 8");                                  // nested heap pointers still use 8-byte slots in property-backed indexed arrays
            emitter.instruction("mov QWORD PTR [r10 + 16], r12");               // persist the pointer-sized slot width in the property-backed array header
        }
        _ => {
            emitter.instruction("mov r12, 8");                                  // scalar indexed arrays use ordinary 8-byte slots
            emitter.instruction("mov QWORD PTR [r10 + 16], r12");               // persist the scalar slot width in the property-backed array header
            emitter.instruction("mov r12, QWORD PTR [r10 - 8]");                // load the packed kind word from the property-backed array heap header
            emitter.instruction("mov r14, r12");                                // preserve the high x86_64 heap-marker bits while rewriting the low container metadata
            emitter.instruction("and r12, 0x80ff");                             // keep the low indexed-array kind and persistent copy-on-write flag bits while clearing stale value_type bits
            emitter.instruction("and r14, -65536");                             // keep the high x86_64 heap-marker bits while clearing the low container payload lane
            emitter.instruction("or r12, r14");                                 // combine the preserved heap marker bits with the stable scalar container metadata
            emitter.instruction("mov QWORD PTR [r10 - 8], r12");                // persist the scalar-oriented packed kind word back into the heap header
        }
    }
    emitter.label(&skip_normalize);
}

fn store_property_indexed_array_value_aarch64(
    elem_ty: &crate::types::PhpType,
    state: &PropertyIndexedAssignState,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    if state.stores_refcounted_pointer {
        emitter.instruction("cmp x9, x11");                                     // does this indexed write overwrite an existing property-backed slot from the original logical length?
        let skip_release = ctx.next_label("prop_array_assign_skip_release");
        emitter.instruction(&format!("b.hs {}", skip_release));                 // skip release work for writes that extend the property-backed array past its original logical length
        emitter.instruction("stp x0, x9, [sp, #-16]!");                         // preserve the new nested pointer and target index across the decref helper call
        emitter.instruction("str x10, [sp, #-16]!");                            // preserve the property-backed array pointer across the decref helper call
        emitter.instruction("add x12, x10, #24");                               // compute the base of the property-backed array data region
        emitter.instruction("ldr x0, [x12, x9, lsl #3]");                       // load the previous nested pointer from the overwritten property-backed array slot
        abi::emit_decref_if_refcounted(emitter, elem_ty);
        emitter.instruction("ldr x10, [sp], #16");                              // restore the property-backed array pointer after releasing the previous nested payload
        emitter.instruction("ldp x0, x9, [sp], #16");                           // restore the new nested pointer and target index after releasing the previous nested payload
        emitter.label(&skip_release);
        super::super::helpers::stamp_indexed_array_value_type(emitter, "x10", &state.val_ty);
        emitter.instruction("add x12, x10, #24");                               // compute the base of the property-backed array data region
        emitter.instruction("str x0, [x12, x9, lsl #3]");                       // store the new nested pointer in the addressed property-backed array slot
        return;
    }

    match &state.effective_store_ty {
        crate::types::PhpType::Int | crate::types::PhpType::Bool | crate::types::PhpType::Callable => {
            emitter.instruction("add x12, x10, #24");                           // compute the base of the scalar property-backed array data region
            emitter.instruction("str x0, [x12, x9, lsl #3]");                   // store the scalar payload in the addressed property-backed array slot
        }
        crate::types::PhpType::Float => {
            emitter.instruction("fmov x12, d0");                                // move the floating-point payload bits into an integer scratch register for property-backed indexed storage
            emitter.instruction("add x13, x10, #24");                           // compute the base of the property-backed array data region
            emitter.instruction("str x12, [x13, x9, lsl #3]");                  // store the floating-point payload bits in the addressed property-backed array slot
        }
        crate::types::PhpType::Str => {
            emitter.instruction("cmp x9, x11");                                 // does this indexed write overwrite an existing property-backed string slot?
            let skip_release = ctx.next_label("prop_array_assign_skip_release");
            emitter.instruction(&format!("b.hs {}", skip_release));             // skip release work for writes that extend the property-backed array past its original logical length
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve the new string pointer and length across the previous-string release helper call
            emitter.instruction("stp x9, x10, [sp, #-16]!");                    // preserve the target index and property-backed array pointer across the previous-string release helper call
            emitter.instruction("lsl x12, x9, #4");                             // convert the indexed string slot into its 16-byte byte offset
            emitter.instruction("add x12, x10, x12");                           // compute the address of the overwritten property-backed string slot
            emitter.instruction("add x12, x12, #24");                           // skip the array header to reach the string-slot payload
            emitter.instruction("ldr x0, [x12]");                               // load the previous string pointer from the overwritten property-backed array slot
            emitter.instruction("bl __rt_heap_free_safe");                      // release the previous owned string before replacing the property-backed array slot
            emitter.instruction("ldp x9, x10, [sp], #16");                      // restore the target index and property-backed array pointer after the previous-string release
            emitter.instruction("ldp x1, x2, [sp], #16");                       // restore the new string pointer and length after the previous-string release
            emitter.label(&skip_release);
            super::super::helpers::stamp_indexed_array_value_type(emitter, "x10", &state.val_ty);
            emitter.instruction("lsl x12, x9, #4");                             // convert the indexed string slot into its 16-byte byte offset
            emitter.instruction("add x12, x10, x12");                           // compute the address of the destination property-backed string slot
            emitter.instruction("add x12, x12, #24");                           // skip the array header to reach the string-slot payload
            emitter.instruction("str x1, [x12]");                               // store the new string pointer in the destination property-backed array slot
            emitter.instruction("str x2, [x12, #8]");                           // store the new string length in the destination property-backed array slot
        }
        _ => {}
    }
}

fn store_property_indexed_array_value_x86_64(
    elem_ty: &crate::types::PhpType,
    state: &PropertyIndexedAssignState,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    if state.stores_refcounted_pointer {
        emitter.instruction("cmp r9, r11");                                     // does this indexed write overwrite an existing property-backed slot from the original logical length?
        let skip_release = ctx.next_label("prop_array_assign_skip_release");
        emitter.instruction(&format!("jae {}", skip_release));                  // skip release work for writes that extend the property-backed array past its original logical length
        abi::emit_push_reg(emitter, "rax");
        abi::emit_push_reg(emitter, "r9");
        abi::emit_push_reg(emitter, "r10");
        emitter.instruction("mov rax, QWORD PTR [r10 + 24 + r9 * 8]");          // load the previous nested pointer from the overwritten property-backed array slot
        abi::emit_decref_if_refcounted(emitter, elem_ty);
        abi::emit_pop_reg(emitter, "r10");
        abi::emit_pop_reg(emitter, "r9");
        abi::emit_pop_reg(emitter, "rax");
        emitter.label(&skip_release);
        abi::emit_push_reg(emitter, "rax");
        super::super::helpers::stamp_indexed_array_value_type(emitter, "r10", &state.val_ty);
        abi::emit_pop_reg(emitter, "rax");
        emitter.instruction("mov QWORD PTR [r10 + 24 + r9 * 8], rax");          // store the new nested pointer in the addressed property-backed array slot
        return;
    }

    match &state.effective_store_ty {
        crate::types::PhpType::Int | crate::types::PhpType::Bool | crate::types::PhpType::Callable => {
            emitter.instruction("mov QWORD PTR [r10 + 24 + r9 * 8], rax");      // store the scalar payload directly into the addressed property-backed array slot
        }
        crate::types::PhpType::Float => {
            emitter.instruction("movq r12, xmm0");                              // move the floating-point payload bits into an integer scratch register for property-backed indexed storage
            emitter.instruction("mov QWORD PTR [r10 + 24 + r9 * 8], r12");      // store the floating-point payload bits in the addressed property-backed array slot
        }
        crate::types::PhpType::Str => {
            emitter.instruction("cmp r9, r11");                                 // does this indexed write overwrite an existing property-backed string slot?
            let skip_release = ctx.next_label("prop_array_assign_skip_release");
            emitter.instruction(&format!("jae {}", skip_release));              // skip release work for writes that extend the property-backed array past its original logical length
            abi::emit_push_reg_pair(emitter, "rax", "rdx");
            abi::emit_push_reg(emitter, "r9");
            abi::emit_push_reg(emitter, "r10");
            emitter.instruction("mov rcx, r9");                                 // copy the target index before scaling it into a 16-byte string-slot byte offset
            emitter.instruction("shl rcx, 4");                                  // convert the target index into the byte offset of the overwritten string slot
            emitter.instruction("lea rcx, [r10 + rcx + 24]");                   // compute the address of the overwritten property-backed string slot
            emitter.instruction("mov rax, QWORD PTR [rcx]");                    // load the previous string pointer from the overwritten property-backed array slot
            abi::emit_call_label(emitter, "__rt_heap_free_safe");
            abi::emit_pop_reg(emitter, "r10");
            abi::emit_pop_reg(emitter, "r9");
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");
            emitter.label(&skip_release);
            abi::emit_push_reg_pair(emitter, "rax", "rdx");
            super::super::helpers::stamp_indexed_array_value_type(emitter, "r10", &state.val_ty);
            abi::emit_pop_reg_pair(emitter, "rax", "rdx");
            emitter.instruction("mov rcx, r9");                                 // copy the target index before scaling it into a 16-byte string-slot byte offset
            emitter.instruction("shl rcx, 4");                                  // convert the target index into the byte offset of the destination string slot
            emitter.instruction("lea rcx, [r10 + rcx + 24]");                   // compute the address of the destination property-backed string slot
            emitter.instruction("mov QWORD PTR [rcx], rax");                    // store the new string pointer in the destination property-backed array slot
            emitter.instruction("mov QWORD PTR [rcx + 8], rdx");                // store the new string length in the destination property-backed array slot
        }
        _ => {}
    }
}

fn extend_property_indexed_array_if_needed_aarch64(
    state: &PropertyIndexedAssignState,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    emitter.instruction("ldr x11, [x10]");                                      // reload the current logical length after the indexed store path because helper calls may clobber caller-saved registers
    let skip_extend = ctx.next_label("prop_array_assign_skip_extend");
    let extend_loop = ctx.next_label("prop_array_assign_extend_loop");
    let extend_store_len = ctx.next_label("prop_array_assign_store_len");
    emitter.instruction("cmp x9, x11");                                         // does this indexed write extend the property-backed array beyond its current logical length?
    emitter.instruction(&format!("b.lo {}", skip_extend));                      // existing property-backed slots already keep the current logical length
    emitter.instruction("mov x12, x11");                                        // start zero-filling at the previous logical end of the property-backed array
    emitter.label(&extend_loop);
    emitter.instruction("cmp x12, x9");                                         // have we filled every property-backed gap slot before the target index?
    emitter.instruction(&format!("b.ge {}", extend_store_len));                 // stop zero-filling once we reach the target indexed slot
    match &state.effective_store_ty {
        crate::types::PhpType::Str => {
            emitter.instruction("lsl x13, x12, #4");                            // convert the gap index into the byte offset of the 16-byte string slot
            emitter.instruction("add x13, x10, x13");                           // compute the address of the property-backed string gap slot
            emitter.instruction("add x13, x13, #24");                           // skip the array header to reach the string-slot payload
            emitter.instruction("str xzr, [x13]");                              // initialize the gap string pointer to null
            emitter.instruction("str xzr, [x13, #8]");                          // initialize the gap string length to zero
        }
        _ => {
            emitter.instruction("add x13, x10, #24");                           // compute the base of the property-backed scalar or pointer data region
            emitter.instruction("str xzr, [x13, x12, lsl #3]");                 // initialize the property-backed gap slot to zero/null
        }
    }
    emitter.instruction("add x12, x12, #1");                                    // advance to the next gap slot that still needs zero-initialization
    emitter.instruction(&format!("b {}", extend_loop));                         // continue zero-filling until the target indexed slot is reached
    emitter.label(&extend_store_len);
    emitter.instruction("add x12, x9, #1");                                     // compute the new logical length as the highest written index plus one
    emitter.instruction("str x12, [x10]");                                      // persist the extended logical length in the property-backed array header
    emitter.label(&skip_extend);
}

fn extend_property_indexed_array_if_needed_x86_64(
    state: &PropertyIndexedAssignState,
    emitter: &mut Emitter,
    ctx: &mut Context,
) {
    emitter.instruction("mov r11, QWORD PTR [r10]");                            // reload the current logical length after the indexed store path because helper calls may clobber caller-saved registers
    let skip_extend = ctx.next_label("prop_array_assign_skip_extend");
    let extend_loop = ctx.next_label("prop_array_assign_extend_loop");
    let extend_store_len = ctx.next_label("prop_array_assign_store_len");
    emitter.instruction("cmp r9, r11");                                         // does this indexed write extend the property-backed array beyond its current logical length?
    emitter.instruction(&format!("jb {}", skip_extend));                        // existing property-backed slots already keep the current logical length
    emitter.instruction("mov r12, r11");                                        // start zero-filling at the previous logical end of the property-backed array
    emitter.label(&extend_loop);
    emitter.instruction("cmp r12, r9");                                         // have we filled every property-backed gap slot before the target index?
    emitter.instruction(&format!("jae {}", extend_store_len));                  // stop zero-filling once we reach the target indexed slot
    match &state.effective_store_ty {
        crate::types::PhpType::Str => {
            emitter.instruction("mov r13, r12");                                // copy the gap index before scaling it into a 16-byte string-slot byte offset
            emitter.instruction("shl r13, 4");                                  // convert the gap index into the byte offset of the property-backed string slot
            emitter.instruction("lea r13, [r10 + r13 + 24]");                   // compute the address of the property-backed string gap slot
            emitter.instruction("mov QWORD PTR [r13], 0");                      // initialize the gap string pointer to null
            emitter.instruction("mov QWORD PTR [r13 + 8], 0");                  // initialize the gap string length to zero
        }
        _ => {
            emitter.instruction("mov QWORD PTR [r10 + 24 + r12 * 8], 0");       // initialize the property-backed scalar or pointer gap slot to zero/null
        }
    }
    emitter.instruction("add r12, 1");                                          // advance to the next gap slot that still needs zero-initialization
    emitter.instruction(&format!("jmp {}", extend_loop));                       // continue zero-filling until the target indexed slot is reached
    emitter.label(&extend_store_len);
    emitter.instruction("lea r12, [r9 + 1]");                                   // compute the new logical length as the highest written index plus one
    emitter.instruction("mov QWORD PTR [r10], r12");                            // persist the extended logical length in the property-backed array header
    emitter.label(&skip_extend);
}
