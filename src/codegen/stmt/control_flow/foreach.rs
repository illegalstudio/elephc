use crate::codegen::context::{Context, HeapOwnership, LoopLabels};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::{Expr, Stmt};
use crate::types::PhpType;

pub(super) fn emit_foreach_stmt(
    array: &Expr,
    key_var: &Option<String>,
    value_var: &str,
    body: &[Stmt],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let loop_start = ctx.next_label("foreach_start");
    let loop_end = ctx.next_label("foreach_end");
    let loop_cont = ctx.next_label("foreach_cont");

    emitter.blank();
    emitter.comment("foreach");

    let arr_ty = emit_expr(array, emitter, ctx, data);

    if let PhpType::AssocArray { value, .. } = &arr_ty {
        let val_ty = *value.clone();
        emitter.instruction("str x0, [sp, #-16]!");                             // push hash table pointer
        emitter.instruction("str xzr, [sp, #-16]!");                            // push initial iterator cursor (0 = start from hash header head)

        emitter.label(&loop_start);
        emitter.instruction("ldr x0, [sp, #16]");                               // load hash table pointer
        emitter.instruction("ldr x1, [sp]");                                    // load current iterator cursor
        emitter.instruction("bl __rt_hash_iter_next");                          // x0=next_cursor(-1=done), x1=key_ptr, x2=key_len, x3=val_lo, x4=val_hi
        emitter.instruction("cmn x0, #1");                                      // compare x0 with -1 (end of iteration)
        emitter.instruction(&format!("b.eq {}", loop_end));                     // exit if done
        emitter.instruction("str x0, [sp]");                                    // store next iterator cursor

        if let Some(kv) = key_var {
            if let Some(kvar) = ctx.variables.get(kv) {
                let k_offset = kvar.stack_offset;
                crate::codegen::abi::store_at_offset_scratch(emitter, "x1", k_offset, "x10"); // store key ptr
                crate::codegen::abi::store_at_offset_scratch(emitter, "x2", k_offset - 8, "x10"); // store key len
                ctx.update_var_type_and_ownership(
                    kv,
                    PhpType::Str,
                    HeapOwnership::borrowed_alias_for_type(&PhpType::Str),
                );
            } else {
                emitter.comment(&format!("WARNING: undefined foreach key variable ${}", kv));
            }
        }

        let val_var_info = match ctx.variables.get(value_var) {
            Some(v) => v,
            None => {
                emitter.comment(&format!("WARNING: undefined foreach value variable ${}", value_var));
                return;
            }
        };
        let v_offset = val_var_info.stack_offset;
        match &val_ty {
            PhpType::Int | PhpType::Bool => {
                crate::codegen::abi::store_at_offset_scratch(emitter, "x3", v_offset, "x10");
            }
            PhpType::Str => {
                crate::codegen::abi::store_at_offset_scratch(emitter, "x3", v_offset, "x10");
                crate::codegen::abi::store_at_offset_scratch(emitter, "x4", v_offset - 8, "x10");
            }
            _ => {
                crate::codegen::abi::store_at_offset_scratch(emitter, "x3", v_offset, "x10");
            }
        }
        ctx.update_var_type_and_ownership(
            value_var,
            val_ty.clone(),
            HeapOwnership::borrowed_alias_for_type(&val_ty),
        );

        ctx.loop_stack.push(LoopLabels {
            continue_label: loop_cont.clone(),
            break_label: loop_end.clone(),
            sp_adjust: 32,
        });
        for s in body {
            super::super::emit_stmt(s, emitter, ctx, data);
        }
        ctx.loop_stack.pop();

        emitter.label(&loop_cont);
        emitter.instruction(&format!("b {}", loop_start));                      // jump back to iterator
        emitter.label(&loop_end);
        emitter.instruction("add sp, sp, #32");                                 // pop iter_index + hash_ptr
    } else {
        let elem_ty = match &arr_ty {
            PhpType::Array(t) => *t.clone(),
            _ => PhpType::Int,
        };
        emitter.instruction("str x0, [sp, #-16]!");                             // push array pointer onto stack
        emitter.instruction("ldr x9, [x0]");                                    // load array length from first field of array struct
        emitter.instruction("str x9, [sp, #-16]!");                             // push array length onto stack
        emitter.instruction("str xzr, [sp, #-16]!");                            // push initial loop index (0) onto stack

        emitter.label(&loop_start);
        emitter.instruction("ldr x0, [sp]");                                    // load current loop index from top of stack
        emitter.instruction("ldr x1, [sp, #16]");                               // load array length from stack (2 slots down)
        emitter.instruction("cmp x0, x1");                                      // compare index against array length
        emitter.instruction(&format!("b.ge {}", loop_end));                     // exit loop if index >= length

        if let Some(kv) = key_var {
            if let Some(kvar) = ctx.variables.get(kv) {
                let k_offset = kvar.stack_offset;
                crate::codegen::abi::store_at_offset_scratch(emitter, "x0", k_offset, "x10");
                ctx.update_var_type_and_ownership(kv, PhpType::Int, HeapOwnership::NonHeap);
            } else {
                emitter.comment(&format!("WARNING: undefined foreach key variable ${}", kv));
            }
        }

        emitter.instruction("ldr x9, [sp, #32]");                               // load array pointer from stack (3 slots down)
        let val_var = match ctx.variables.get(value_var) {
            Some(v) => v,
            None => {
                emitter.comment(&format!("WARNING: undefined foreach value variable ${}", value_var));
                return;
            }
        };
        let val_offset = val_var.stack_offset;
        match &elem_ty {
            PhpType::Int => {
                emitter.instruction("add x9, x9, #24");                         // skip 24-byte array header to reach data
                emitter.instruction("ldr x0, [x9, x0, lsl #3]");                // load int at data[index] (8 bytes per element)
                crate::codegen::abi::store_at_offset(emitter, "x0", val_offset);
            }
            PhpType::Str => {
                emitter.instruction("lsl x10, x0, #4");                         // multiply index by 16 (string slot size)
                emitter.instruction("add x9, x9, x10");                         // offset to the string slot in data region
                emitter.instruction("add x9, x9, #24");                         // skip 24-byte array header
                emitter.instruction("ldr x1, [x9]");                            // load string pointer from slot
                emitter.instruction("ldr x2, [x9, #8]");                        // load string length from slot+8
                crate::codegen::abi::store_at_offset(emitter, "x1", val_offset);
                crate::codegen::abi::store_at_offset(emitter, "x2", val_offset - 8);
            }
            PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
                emitter.instruction("add x9, x9, #24");                         // skip 24-byte array header to reach data
                emitter.instruction("ldr x0, [x9, x0, lsl #3]");                // load nested array/object pointer at index
                crate::codegen::abi::store_at_offset(emitter, "x0", val_offset);
            }
            _ => {}
        }
        ctx.update_var_type_and_ownership(
            value_var,
            elem_ty.clone(),
            HeapOwnership::borrowed_alias_for_type(&elem_ty),
        );

        ctx.loop_stack.push(LoopLabels {
            continue_label: loop_cont.clone(),
            break_label: loop_end.clone(),
            sp_adjust: 48,
        });
        for s in body {
            super::super::emit_stmt(s, emitter, ctx, data);
        }
        ctx.loop_stack.pop();

        emitter.label(&loop_cont);
        emitter.instruction("ldr x0, [sp]");                                    // load current loop index from stack
        emitter.instruction("add x0, x0, #1");                                  // increment index by 1
        emitter.instruction("str x0, [sp]");                                    // write updated index back to stack
        emitter.instruction(&format!("b {}", loop_start));                      // jump back to loop condition check
        emitter.label(&loop_end);
        emitter.instruction("add sp, sp, #48");                                 // deallocate 48 bytes (3 x 16-byte slots) from stack
    }
}
