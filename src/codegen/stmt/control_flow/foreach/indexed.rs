use crate::codegen::context::{Context, HeapOwnership, LoopLabels};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::stmt::emit_stmt;
use crate::parser::ast::Stmt;
use crate::types::PhpType;

pub(crate) fn emit_indexed_foreach(
    key_var: &Option<String>,
    value_var: &str,
    body: &[Stmt],
    loop_start: &str,
    loop_end: &str,
    loop_cont: &str,
    elem_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.instruction("str x0, [sp, #-16]!");                                     // push array pointer onto stack
    emitter.instruction("ldr x9, [x0]");                                            // load array length from first field of array struct
    emitter.instruction("str x9, [sp, #-16]!");                                     // push array length onto stack
    emitter.instruction("str xzr, [sp, #-16]!");                                    // push initial loop index (0) onto stack

    emitter.label(loop_start);
    emitter.instruction("ldr x0, [sp]");                                            // load current loop index from top of stack
    emitter.instruction("ldr x1, [sp, #16]");                                       // load array length from stack (2 slots down)
    emitter.instruction("cmp x0, x1");                                              // compare index against array length
    emitter.instruction(&format!("b.ge {}", loop_end));                             // exit loop if index >= length

    if let Some(kv) = key_var {
        if let Some(kvar) = ctx.variables.get(kv) {
            let k_offset = kvar.stack_offset;
            crate::codegen::abi::store_at_offset_scratch(emitter, "x0", k_offset, "x10");
            ctx.update_var_type_and_ownership(kv, PhpType::Int, HeapOwnership::NonHeap);
        } else {
            emitter.comment(&format!("WARNING: undefined foreach key variable ${}", kv));
        }
    }

    emitter.instruction("ldr x9, [sp, #32]");                                       // load array pointer from stack (3 slots down)
    let val_var = match ctx.variables.get(value_var) {
        Some(v) => v,
        None => {
            emitter.comment(&format!("WARNING: undefined foreach value variable ${}", value_var));
            return;
        }
    };
    let val_offset = val_var.stack_offset;
    match elem_ty {
        PhpType::Int => {
            emitter.instruction("add x9, x9, #24");                                 // skip 24-byte array header to reach data
            emitter.instruction("ldr x0, [x9, x0, lsl #3]");                        // load int at data[index] (8 bytes per element)
            crate::codegen::abi::store_at_offset(emitter, "x0", val_offset);
        }
        PhpType::Str => {
            emitter.instruction("lsl x10, x0, #4");                                 // multiply index by 16 (string slot size)
            emitter.instruction("add x9, x9, x10");                                 // offset to the string slot in data region
            emitter.instruction("add x9, x9, #24");                                 // skip 24-byte array header
            emitter.instruction("ldr x1, [x9]");                                    // load string pointer from slot
            emitter.instruction("ldr x2, [x9, #8]");                                // load string length from slot+8
            crate::codegen::abi::store_at_offset(emitter, "x1", val_offset);
            crate::codegen::abi::store_at_offset(emitter, "x2", val_offset - 8);
        }
        PhpType::Mixed | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
            emitter.instruction("add x9, x9, #24");                                 // skip 24-byte array header to reach data
            emitter.instruction("ldr x0, [x9, x0, lsl #3]");                        // load nested array/object pointer at index
            crate::codegen::abi::store_at_offset(emitter, "x0", val_offset);
        }
        _ => {}
    }
    ctx.update_var_type_and_ownership(
        value_var,
        elem_ty.clone(),
        HeapOwnership::borrowed_alias_for_type(elem_ty),
    );

    ctx.loop_stack.push(LoopLabels {
        continue_label: loop_cont.to_string(),
        break_label: loop_end.to_string(),
        sp_adjust: 48,
    });
    for s in body {
        emit_stmt(s, emitter, ctx, data);
    }
    ctx.loop_stack.pop();

    emitter.label(loop_cont);
    emitter.instruction("ldr x0, [sp]");                                            // load current loop index from stack
    emitter.instruction("add x0, x0, #1");                                          // increment index by 1
    emitter.instruction("str x0, [sp]");                                            // write updated index back to stack
    emitter.instruction(&format!("b {}", loop_start));                              // jump back to loop condition check
    emitter.label(loop_end);
    emitter.instruction("add sp, sp, #48");                                         // deallocate 48 bytes (3 x 16-byte slots) from stack
}
