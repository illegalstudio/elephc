use crate::codegen::context::{Context, HeapOwnership, LoopLabels};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
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
    if emitter.target.arch == Arch::X86_64 {
        emit_indexed_foreach_linux_x86_64(
            key_var,
            value_var,
            body,
            loop_start,
            loop_end,
            loop_cont,
            elem_ty,
            emitter,
            ctx,
            data,
        );
        return;
    }

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

fn emit_indexed_foreach_linux_x86_64(
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
    crate::codegen::abi::emit_push_reg(emitter, "rax");                              // preserve the indexed-array pointer across the foreach loop state setup
    emitter.instruction("mov r10, QWORD PTR [rax]");                                 // load the indexed-array logical length before entering the foreach loop
    crate::codegen::abi::emit_push_reg(emitter, "r10");                              // preserve the indexed-array logical length in a dedicated foreach loop stack slot
    emitter.instruction("xor r10, r10");                                             // materialize the initial foreach loop index value 0 in a scratch register
    crate::codegen::abi::emit_push_reg(emitter, "r10");                              // preserve the current foreach loop index in a dedicated loop stack slot

    emitter.label(loop_start);
    emitter.instruction("mov rax, QWORD PTR [rsp]");                                 // load the current foreach loop index from the top temporary stack slot
    emitter.instruction("mov rdx, QWORD PTR [rsp + 16]");                            // load the indexed-array logical length from the second temporary stack slot
    emitter.instruction("cmp rax, rdx");                                             // compare the current foreach loop index against the indexed-array logical length
    emitter.instruction(&format!("jge {}", loop_end));                               // exit the foreach loop once the current index reaches the indexed-array logical length

    if let Some(kv) = key_var {
        if let Some(kvar) = ctx.variables.get(kv) {
            let k_offset = kvar.stack_offset;
            crate::codegen::abi::store_at_offset_scratch(emitter, "rax", k_offset, "r10"); // store the current foreach loop index into the loop key variable slot
            ctx.update_var_type_and_ownership(kv, PhpType::Int, HeapOwnership::NonHeap);
        } else {
            emitter.comment(&format!("WARNING: undefined foreach key variable ${}", kv));
        }
    }

    emitter.instruction("mov r11, QWORD PTR [rsp + 32]");                            // reload the indexed-array pointer from the preserved foreach loop stack slot
    let val_var = match ctx.variables.get(value_var) {
        Some(v) => v,
        None => {
            emitter.comment(&format!("WARNING: undefined foreach value variable ${}", value_var));
            return;
        }
    };
    let val_offset = val_var.stack_offset;
    match elem_ty {
        PhpType::Int | PhpType::Bool => {
            emitter.instruction("add r11, 24");                                      // skip the indexed-array header to reach the scalar payload base address
            emitter.instruction("mov rax, QWORD PTR [r11 + rax * 8]");               // load the current scalar foreach payload from the indexed-array data region
            crate::codegen::abi::store_at_offset(emitter, "rax", val_offset);
        }
        PhpType::Float => {
            emitter.instruction("add r11, 24");                                      // skip the indexed-array header to reach the floating-point payload base address
            emitter.instruction("movsd xmm0, QWORD PTR [r11 + rax * 8]");            // load the current floating-point foreach payload from the indexed-array data region
            crate::codegen::abi::store_at_offset(emitter, "xmm0", val_offset);
        }
        PhpType::Str => {
            emitter.instruction("mov r10, rax");                                     // copy the current foreach loop index before scaling it to the 16-byte string slot size
            emitter.instruction("shl r10, 4");                                       // scale the foreach loop index by the 16-byte string slot size
            emitter.instruction("add r11, r10");                                     // advance from the indexed-array base to the selected string slot
            emitter.instruction("add r11, 24");                                      // skip the indexed-array header to reach the selected string slot payload
            emitter.instruction("mov rax, QWORD PTR [r11]");                         // load the current foreach string pointer from the selected string slot
            emitter.instruction("mov rdx, QWORD PTR [r11 + 8]");                     // load the current foreach string length from the selected string slot
            crate::codegen::abi::store_at_offset(emitter, "rax", val_offset);
            crate::codegen::abi::store_at_offset(emitter, "rdx", val_offset - 8);
        }
        PhpType::Mixed | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
            emitter.instruction("add r11, 24");                                      // skip the indexed-array header to reach the pointer payload base address
            emitter.instruction("mov rax, QWORD PTR [r11 + rax * 8]");               // load the current pointer-like foreach payload from the indexed-array data region
            crate::codegen::abi::store_at_offset(emitter, "rax", val_offset);
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
    emitter.instruction("mov rax, QWORD PTR [rsp]");                                 // reload the current foreach loop index from the top temporary stack slot
    emitter.instruction("add rax, 1");                                               // advance the foreach loop index to the next indexed-array payload slot
    emitter.instruction("mov QWORD PTR [rsp], rax");                                 // persist the updated foreach loop index for the next iteration
    emitter.instruction(&format!("jmp {}", loop_start));                             // jump back to the indexed-array foreach loop condition
    emitter.label(loop_end);
    emitter.instruction("add rsp, 48");                                              // release the foreach loop index, length, and array-pointer temporary stack slots
}
