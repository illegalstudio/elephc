use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub(super) fn emit_array_assign_stmt(
    array: &str,
    index: &Expr,
    value: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment(&format!("${}[...] = ...", array));
    let var = match ctx.variables.get(array) {
        Some(v) => v,
        None => {
            emitter.comment(&format!("WARNING: undefined variable ${}", array));
            return;
        }
    };
    let offset = var.stack_offset;
    let is_ref = ctx.ref_params.contains(array);
    let is_assoc = matches!(&var.ty, PhpType::AssocArray { .. });
    let elem_ty = match &var.ty {
        PhpType::Array(t) => *t.clone(),
        PhpType::AssocArray { value: v, .. } => *v.clone(),
        PhpType::Buffer(t) => *t.clone(),
        _ => PhpType::Int,
    };

    if matches!(&var.ty, PhpType::Buffer(_)) {
        if is_ref {
            abi::load_at_offset(emitter, "x10", offset);                            // load ref slot that points at the buffer local
            emitter.instruction("ldr x10, [x10]");                               // dereference the ref slot to get the buffer header pointer
        } else {
            abi::load_at_offset(emitter, "x10", offset);                            // load the buffer header pointer from the local slot
        }
        emitter.instruction("str x10, [sp, #-16]!");                                // preserve the buffer pointer while evaluating the index
        emit_expr(index, emitter, ctx, data);
        emitter.instruction("str x0, [sp, #-16]!");                                 // preserve the computed element index across value evaluation
        let val_ty = emit_expr(value, emitter, ctx, data);
        match &val_ty {
            PhpType::Float => {
                emitter.instruction("str d0, [sp, #-16]!");                         // preserve the float payload across address computation
            }
            PhpType::Str => {
                emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve unsupported string payload for consistent stack cleanup
            }
            _ => {
                emitter.instruction("str x0, [sp, #-16]!");                         // preserve scalar/pointer payload across address computation
            }
        }
        emitter.instruction("ldr x9, [sp, #16]");                                   // reload the target index without disturbing the saved value
        emitter.instruction("ldr x10, [sp, #32]");                                  // reload the buffer header pointer without disturbing the saved value
        let bounds_ok = ctx.next_label("buffer_store_ok");
        emitter.instruction("cmp x9, #0");                                          // reject negative buffer indexes
        emitter.instruction("b.lt __rt_buffer_bounds_fail");                        // abort on negative buffer index
        emitter.instruction("ldr x11, [x10]");                                      // load the logical buffer length from the header
        emitter.instruction("cmp x9, x11");                                         // compare the target index against the logical length
        emitter.instruction(&format!("b.lo {}", bounds_ok));                        // continue once the write target is in bounds
        emitter.instruction("mov x0, x9");                                          // pass the out-of-bounds index to the runtime helper
        emitter.instruction("mov x1, x11");                                         // pass the logical buffer length to the runtime helper
        emitter.instruction("bl __rt_buffer_bounds_fail");                          // abort the program on invalid buffer writes
        emitter.label(&bounds_ok);
        emitter.instruction("ldr x12, [x10, #8]");                                  // load the element stride from the buffer header
        emitter.instruction("add x10, x10, #16");                                   // skip the buffer header to reach the payload base
        emitter.instruction("madd x10, x9, x12, x10");                              // compute payload base + index*stride for the target element
        match &elem_ty {
            PhpType::Float => {
                emitter.instruction("ldr d0, [sp], #16");                           // restore the float payload before the direct store
                emitter.instruction("str d0, [x10]");                               // store the float payload directly into the contiguous element slot
            }
            PhpType::Packed(_) => {
                emitter.comment("WARNING: packed buffer whole-element stores are not supported");
                emitter.instruction("add sp, sp, #16");                             // drop the preserved placeholder payload for unsupported packed stores
            }
            _ => {
                emitter.instruction("ldr x0, [sp], #16");                           // restore the scalar/pointer payload before the direct store
                emitter.instruction("str x0, [x10]");                               // store the scalar/pointer payload directly into the contiguous element slot
            }
        }
        emitter.instruction("add sp, sp, #32");                                     // drop the preserved index and buffer pointer slots
        return;
    }

    if is_assoc {
        if is_ref {
            abi::load_at_offset(emitter, "x9", offset);                             // load ref pointer
            emitter.instruction("ldr x0, [x9]");                                // dereference to get hash table pointer
        } else {
            abi::load_at_offset(emitter, "x0", offset);                             // load hash table pointer
        }
        emitter.instruction("str x0, [sp, #-16]!");                             // save hash table pointer
        emit_expr(index, emitter, ctx, data);
        emitter.instruction("stp x1, x2, [sp, #-16]!");                         // save key ptr/len
        let mut val_ty = emit_expr(value, emitter, ctx, data);
        if matches!(elem_ty, PhpType::Mixed) && val_ty != PhpType::Mixed {
            super::super::super::emit_box_current_value_as_mixed(emitter, &val_ty);
            val_ty = PhpType::Mixed;
        } else {
            super::super::retain_borrowed_heap_result(emitter, value, &val_ty);
        }
        let (val_lo, val_hi) = match &val_ty {
            PhpType::Int | PhpType::Bool => ("x0", "xzr"),
            PhpType::Str => {
                emitter.instruction("bl __rt_str_persist");                     // copy string to heap, x1=heap_ptr, x2=len
                ("x1", "x2")
            }
            PhpType::Float => {
                emitter.instruction("fmov x9, d0");                             // move float bits to integer register
                ("x9", "xzr")
            }
            _ => ("x0", "xzr"),
        };
        emitter.instruction(&format!("mov x3, {}", val_lo));                    // value_lo
        emitter.instruction(&format!("mov x4, {}", val_hi));                    // value_hi
        emitter.instruction(&format!("mov x5, #{}", super::super::super::runtime_value_tag(&val_ty))); //value_tag for this assoc entry
        emitter.instruction("ldp x1, x2, [sp], #16");                           // pop key ptr/len
        emitter.instruction("ldr x0, [sp], #16");                               // pop hash table pointer
        emitter.instruction("bl __rt_hash_set");                                // insert/update key-value pair (x0 = table)
        if is_ref {
            abi::load_at_offset(emitter, "x9", offset);                             // load ref pointer
            emitter.instruction("str x0, [x9]");                                // store new table ptr through ref
        } else {
            abi::store_at_offset(emitter, "x0", offset);                            // save possibly-new table pointer
        }
    } else {
        if is_ref {
            abi::load_at_offset(emitter, "x9", offset);                             // load ref pointer
            emitter.instruction("ldr x0, [x9]");                                // dereference to get array heap pointer
        } else {
            abi::load_at_offset(emitter, "x0", offset);                             // load array heap pointer from stack frame
        }
        emitter.instruction("bl __rt_array_ensure_unique");                     // split shared indexed arrays before direct indexed writes mutate storage
        if is_ref {
            abi::load_at_offset(emitter, "x13", offset);                            // load ref pointer
            emitter.instruction("str x0, [x13]");                               // persist the unique array pointer through the reference slot
        } else {
            abi::store_at_offset(emitter, "x0", offset);                            // persist the unique array pointer in the local slot
        }
        emitter.instruction("str x0, [sp, #-16]!");                             // push array pointer onto stack
        emit_expr(index, emitter, ctx, data);
        emitter.instruction("str x0, [sp, #-16]!");                             // push computed index onto stack
        let val_ty = emit_expr(value, emitter, ctx, data);
        super::super::retain_borrowed_heap_result(emitter, value, &val_ty);
        match &val_ty {
            PhpType::Str => {
                emitter.instruction("stp x1, x2, [sp, #-16]!");                 // preserve string pointer/length across growth helpers
            }
            PhpType::Float => {
                emitter.instruction("fmov x12, d0");                            // move float bits into an integer register for stack preservation
                emitter.instruction("str x12, [sp, #-16]!");                    // preserve float bits across growth helpers
            }
            _ => {
                emitter.instruction("str x0, [sp, #-16]!");                     // preserve scalar or heap pointer value across growth helpers
            }
        }
        let effective_store_ty = if matches!(elem_ty, PhpType::Mixed) {
            PhpType::Mixed
        } else if elem_ty != val_ty {
            val_ty.clone()
        } else {
            elem_ty.clone()
        };
        if effective_store_ty != elem_ty {
            let updated_ty = PhpType::Array(Box::new(effective_store_ty.clone()));
            ctx.update_var_type_and_ownership(
                array,
                updated_ty.clone(),
                super::super::local_slot_ownership_after_store(&updated_ty),
            );
        }
        let stores_refcounted_pointer = matches!(
            effective_store_ty,
            PhpType::Mixed | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_)
        );
        emitter.instruction("ldr x9, [sp, #16]");                               // reload index without disturbing the preserved value on top of the stack
        emitter.instruction("ldr x10, [sp, #32]");                              // reload array pointer without disturbing the preserved value on top of the stack
        emitter.instruction("ldr x11, [x10]");                                  // load the original array length before any growth or extension
        emitter.instruction("ldr x12, [x10, #8]");                              // load the current array capacity before any growth
        let grow_check = ctx.next_label("array_assign_grow_check");
        let grow_ready = ctx.next_label("array_assign_grow_ready");
        emitter.label(&grow_check);
        emitter.instruction("cmp x9, x12");                                     // does the target index fit within the current capacity?
        emitter.instruction(&format!("b.lo {}", grow_ready));                   // skip growth once the target slot is addressable
        emitter.instruction("str x9, [sp, #-16]!");                             // preserve the target index across the growth helper
        emitter.instruction("mov x0, x10");                                     // move the current array pointer into the growth helper argument register
        emitter.instruction("bl __rt_array_grow");                              // grow the indexed array until the target slot fits
        emitter.instruction("mov x10, x0");                                     // keep the possibly-reallocated array pointer in x10
        emitter.instruction("ldr x9, [sp], #16");                               // restore the target index after growth
        emitter.instruction("ldr x12, [x10, #8]");                              // reload the new array capacity after growth
        emitter.instruction(&format!("b {}", grow_check));                      // continue growing until the target slot fits
        emitter.label(&grow_ready);
        if is_ref {
            abi::load_at_offset(emitter, "x13", offset);                            // load ref pointer
            emitter.instruction("str x10, [x13]");                              // store the possibly-grown array pointer through the ref
        } else {
            abi::store_at_offset(emitter, "x10", offset);                           // save possibly-grown array pointer
        }
        match &val_ty {
            PhpType::Str => {
                emitter.instruction("ldp x1, x2, [sp], #16");                   // restore string pointer/length after growth helpers
            }
            PhpType::Float => {
                emitter.instruction("ldr x12, [sp], #16");                      // restore preserved float bits after growth helpers
                emitter.instruction("fmov d0, x12");                            // move preserved float bits back into the float result register
            }
            _ => {
                emitter.instruction("ldr x0, [sp], #16");                       // restore scalar or heap pointer value after growth helpers
            }
        }
        emitter.instruction("add sp, sp, #32");                                 // drop the original saved index and array pointer after they have been restored
        let skip_normalize = ctx.next_label("array_assign_skip_normalize");
        emitter.instruction("cmp x11, #0");                                     // is this the first indexed write into the array?
        emitter.instruction(&format!("b.ne {}", skip_normalize));               // keep the existing storage layout once the array already has elements
        match &effective_store_ty {
            PhpType::Str => {
                emitter.instruction("mov x12, #16");                            // string arrays need 16-byte slots for ptr+len payloads
                emitter.instruction("str x12, [x10, #16]");                     // persist the string slot size in the array header
                emitter.instruction("ldr x12, [x10, #-8]");                     // load the packed array kind word from the heap header
                emitter.instruction("mov x14, #0x80ff");                        // preserve the indexed-array kind and persistent COW flag
                emitter.instruction("and x12, x12, x14");                       // keep only the persistent indexed-array metadata bits
                emitter.instruction("mov x13, #1");                             // runtime value_type 1 = string
                emitter.instruction("lsl x13, x13, #8");                        // move the value_type tag into the packed kind-word byte lane
                emitter.instruction("orr x12, x12, x13");                       // combine heap kind + string value_type tag
                emitter.instruction("str x12, [x10, #-8]");                     // persist the string-oriented packed kind word
            }
            PhpType::Mixed | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
                emitter.instruction("mov x12, #8");                             // nested heap pointers use ordinary 8-byte slots
                emitter.instruction("str x12, [x10, #16]");                     // persist the pointer-sized slot width in the array header
            }
            _ => {
                emitter.instruction("mov x12, #8");                             // scalar indexed arrays use ordinary 8-byte slots
                emitter.instruction("str x12, [x10, #16]");                     // persist the scalar slot width in the array header
                emitter.instruction("ldr x12, [x10, #-8]");                     // load the packed array kind word from the heap header
                emitter.instruction("mov x14, #0x80ff");                        // preserve the indexed-array kind and persistent COW flag
                emitter.instruction("and x12, x12, x14");                       // clear stale value_type bits while keeping the persistent container metadata
                emitter.instruction("str x12, [x10, #-8]");                     // persist the scalar-oriented packed kind word
            }
        }
        emitter.label(&skip_normalize);
        if stores_refcounted_pointer {
            emitter.instruction("cmp x9, x11");                                 // check whether this write overwrites an existing slot from the original array
            let skip_release = ctx.next_label("array_assign_skip_release");
            emitter.instruction(&format!("b.hs {}", skip_release));             // skip release for writes past current length
            emitter.instruction("stp x0, x9, [sp, #-16]!");                     // preserve new nested pointer and index across decref call
            emitter.instruction("str x10, [sp, #-16]!");                        // preserve array pointer across decref call
            emitter.instruction("add x12, x10, #24");                           // compute base of array data region
            emitter.instruction("ldr x0, [x12, x9, lsl #3]");                   // load previous nested pointer from slot
            abi::emit_decref_if_refcounted(emitter, &elem_ty);
            emitter.instruction("ldr x10, [sp], #16");                          // restore array pointer after decref
            emitter.instruction("ldp x0, x9, [sp], #16");                       // restore new nested pointer and index after decref
            emitter.label(&skip_release);
            super::super::stamp_indexed_array_value_type(emitter, "x10", &val_ty);
            emitter.instruction("add x12, x10, #24");                           // compute base of array data region
            emitter.instruction("str x0, [x12, x9, lsl #3]");                   // store pointer at data[index]
        } else {
            match &effective_store_ty {
                PhpType::Int | PhpType::Bool | PhpType::Callable => {
                    emitter.instruction("add x12, x10, #24");                   // compute base of the scalar data region without clobbering the array pointer
                    emitter.instruction("str x0, [x12, x9, lsl #3]");           // store int-like payload at data[index]
                }
                PhpType::Float => {
                    emitter.instruction("fmov x12, d0");                        // move float bits into an integer register for storage
                    emitter.instruction("add x13, x10, #24");                   // skip 24-byte array header
                    emitter.instruction("str x12, [x13, x9, lsl #3]");          // store float bits at data[index]
                }
                PhpType::Str => {
                    emitter.instruction("cmp x9, x11");                         // check whether this write overwrites an existing string slot
                    let skip_release = ctx.next_label("array_assign_skip_release");
                    emitter.instruction(&format!("b.hs {}", skip_release));     // skip release for writes past current length
                    emitter.instruction("stp x1, x2, [sp, #-16]!");             // preserve new string ptr/len across old-string release
                    emitter.instruction("stp x9, x10, [sp, #-16]!");            // preserve index and array pointer across old-string release
                    emitter.instruction("lsl x12, x9, #4");                     // multiply index by 16 for string slots
                    emitter.instruction("add x12, x10, x12");                   // offset into array data region
                    emitter.instruction("add x12, x12, #24");                   // skip 24-byte array header
                    emitter.instruction("ldr x0, [x12]");                       // load previous string pointer from slot
                    emitter.instruction("bl __rt_heap_free_safe");              // release the overwritten string storage before replacing it
                    emitter.instruction("ldp x9, x10, [sp], #16");              // restore index and array pointer after old-string release
                    emitter.instruction("ldp x1, x2, [sp], #16");               // restore new string ptr/len after old-string release
                    emitter.label(&skip_release);
                    super::super::stamp_indexed_array_value_type(emitter, "x10", &val_ty);
                    emitter.instruction("lsl x12, x9, #4");                     // multiply index by 16 without clobbering the logical index register
                    emitter.instruction("add x12, x10, x12");                   // offset into array data region without clobbering the array pointer
                    emitter.instruction("add x12, x12, #24");                   // skip 24-byte array header
                    emitter.instruction("str x1, [x12]");                       // store string pointer at slot
                    emitter.instruction("str x2, [x12, #8]");                   // store string length at slot+8
                }
                _ => {}
            }
        }
        let skip_extend = ctx.next_label("array_assign_skip_extend");
        let extend_loop = ctx.next_label("array_assign_extend_loop");
        let extend_store_len = ctx.next_label("array_assign_store_len");
        emitter.instruction("cmp x9, x11");                                     // does this assignment extend the array beyond its original length?
        emitter.instruction(&format!("b.lo {}", skip_extend));                  // existing slots already keep the current array length
        emitter.instruction("mov x12, x11");                                    // start zero-filling at the previous logical end of the array
        emitter.label(&extend_loop);
        emitter.instruction("cmp x12, x9");                                     // have we filled every gap slot before the target index?
        emitter.instruction(&format!("b.ge {}", extend_store_len));             // stop zero-filling once we reach the target index
        match &effective_store_ty {
            PhpType::Str => {
                emitter.instruction("lsl x13, x12, #4");                        // multiply the gap index by 16 for string slots
                emitter.instruction("add x13, x10, x13");                       // offset into the string data region
                emitter.instruction("add x13, x13, #24");                       // skip the 24-byte array header
                emitter.instruction("str xzr, [x13]");                          // initialize the gap string pointer to null
                emitter.instruction("str xzr, [x13, #8]");                      // initialize the gap string length to zero
            }
            _ => {
                emitter.instruction("add x13, x10, #24");                       // compute the base of the pointer/scalar data region
                emitter.instruction("str xzr, [x13, x12, lsl #3]");             // initialize the gap slot to zero/null
            }
        }
        emitter.instruction("add x12, x12, #1");                                // advance to the next gap slot
        emitter.instruction(&format!("b {}", extend_loop));                     // continue zero-filling until the target index is reached
        emitter.label(&extend_store_len);
        emitter.instruction("add x12, x9, #1");                                 // new length = highest written index + 1
        emitter.instruction("str x12, [x10]");                                  // persist the extended logical length in the array header
        emitter.label(&skip_extend);
        let _ = val_ty;
    }
}
