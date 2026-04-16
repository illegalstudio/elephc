use crate::codegen::context::{Context, HeapOwnership, LoopLabels};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::stmt::emit_stmt;
use crate::parser::ast::Stmt;
use crate::types::PhpType;

pub(crate) fn emit_assoc_foreach(
    key_var: &Option<String>,
    value_var: &str,
    body: &[Stmt],
    loop_start: &str,
    loop_end: &str,
    loop_cont: &str,
    val_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    if emitter.target.arch == Arch::X86_64 {
        emit_assoc_foreach_linux_x86_64(
            key_var,
            value_var,
            body,
            loop_start,
            loop_end,
            loop_cont,
            val_ty,
            emitter,
            ctx,
            data,
        );
        return;
    }

    emitter.instruction("str x0, [sp, #-16]!");                                 // push hash table pointer
    emitter.instruction("str xzr, [sp, #-16]!");                                // push initial iterator cursor (0 = start from hash header head)

    emitter.label(loop_start);
    emitter.instruction("ldr x0, [sp, #16]");                                   // load hash table pointer
    emitter.instruction("ldr x1, [sp]");                                        // load current iterator cursor
    emitter.instruction("bl __rt_hash_iter_next");                              // x0=next_cursor(-1=done), x1=key_ptr, x2=key_len, x3=val_lo, x4=val_hi, x5=val_tag
    emitter.instruction("cmn x0, #1");                                          // compare x0 with -1 (end of iteration)
    emitter.instruction(&format!("b.eq {}", loop_end));                         // exit if done
    emitter.instruction("str x0, [sp]");                                        // store next iterator cursor

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
    match val_ty {
        PhpType::Int | PhpType::Bool => {
            crate::codegen::abi::store_at_offset_scratch(emitter, "x3", v_offset, "x10");
        }
        PhpType::Str => {
            crate::codegen::abi::store_at_offset_scratch(emitter, "x3", v_offset, "x10");
            crate::codegen::abi::store_at_offset_scratch(emitter, "x4", v_offset - 8, "x10");
        }
        PhpType::Mixed => {
            emitter.instruction("str x3, [sp, #-16]!");                         // save iterated value_lo across the decref of the previous loop value
            emitter.instruction("stp x4, x5, [sp, #-16]!");                     // save iterated value_hi and value_tag across the helper call
            crate::codegen::abi::load_at_offset_scratch(emitter, "x0", v_offset, "x10"); // load the previous boxed mixed loop value before overwrite
            emitter.instruction("bl __rt_decref_mixed");                        // release the previous owned mixed loop value if one exists
            emitter.instruction("ldp x4, x5, [sp], #16");                       // restore iterated value_hi and value_tag after decref
            emitter.instruction("ldr x3, [sp], #16");                           // restore iterated value_lo after decref
            emitter.instruction("cmp x5, #7");                                  // does this hash entry already store a boxed mixed value?
            let reuse_box = ctx.next_label("foreach_assoc_mixed_reuse");
            let store_box = ctx.next_label("foreach_assoc_mixed_store");
            emitter.instruction(&format!("b.eq {}", reuse_box));                // reuse existing mixed boxes instead of nesting them
            super::super::super::super::emit_box_runtime_payload_as_mixed(emitter, "x5", "x3", "x4"); // box the borrowed entry payload into an owned mixed cell
            emitter.instruction(&format!("b {}", store_box));                   // skip the mixed-box reuse path once boxing is done
            emitter.label(&reuse_box);
            emitter.instruction("mov x0, x3");                                  // x0 = existing boxed mixed pointer from the hash entry
            emitter.instruction("bl __rt_incref");                              // retain the shared mixed box for the foreach variable
            emitter.label(&store_box);
            crate::codegen::abi::store_at_offset_scratch(emitter, "x0", v_offset, "x10");
        }
        _ => {
            crate::codegen::abi::store_at_offset_scratch(emitter, "x3", v_offset, "x10");
        }
    }
    ctx.update_var_type_and_ownership(
        value_var,
        val_ty.clone(),
        if matches!(val_ty, PhpType::Mixed) {
            HeapOwnership::local_owner_for_type(val_ty)
        } else {
            HeapOwnership::borrowed_alias_for_type(val_ty)
        },
    );

    ctx.loop_stack.push(LoopLabels {
        continue_label: loop_cont.to_string(),
        break_label: loop_end.to_string(),
        sp_adjust: 32,
    });
    for s in body {
        emit_stmt(s, emitter, ctx, data);
    }
    ctx.loop_stack.pop();

    emitter.label(loop_cont);
    emitter.instruction(&format!("b {}", loop_start));                          // jump back to iterator
    emitter.label(loop_end);
    emitter.instruction("add sp, sp, #32");                                     // pop iter_index + hash_ptr
}

fn emit_assoc_foreach_linux_x86_64(
    key_var: &Option<String>,
    value_var: &str,
    body: &[Stmt],
    loop_start: &str,
    loop_end: &str,
    loop_cont: &str,
    val_ty: &PhpType,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    crate::codegen::abi::emit_push_reg(emitter, "rax");                                // preserve the associative-array hash-table pointer across the insertion-order iterator loop
    emitter.instruction("sub rsp, 16");                                         // reserve one temporary stack slot for the associative-array iterator cursor
    emitter.instruction("mov QWORD PTR [rsp], 0");                              // initialize the associative-array iterator cursor to the hash-header head sentinel

    emitter.label(loop_start);
    emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");                       // load the associative-array hash-table pointer for the next insertion-order iteration step
    emitter.instruction("mov rsi, QWORD PTR [rsp]");                            // load the current associative-array iterator cursor
    emitter.instruction("call __rt_hash_iter_next");                            // advance one associative-array insertion-order entry and return its key plus payload
    emitter.instruction("cmp rax, -1");                                         // has associative-array iteration reached the done sentinel?
    emitter.instruction(&format!("je {}", loop_end));                           // stop the foreach loop once the associative-array iterator is exhausted
    emitter.instruction("mov QWORD PTR [rsp], rax");                            // save the updated associative-array iterator cursor for the next loop step

    if let Some(kv) = key_var {
        if let Some(kvar) = ctx.variables.get(kv) {
            let k_offset = kvar.stack_offset;
            crate::codegen::abi::store_at_offset_scratch(emitter, "rdi", k_offset, "r10"); // store the associative-array foreach key pointer into the loop key variable slot
            crate::codegen::abi::store_at_offset_scratch(emitter, "rdx", k_offset - 8, "r10"); // store the associative-array foreach key length into the paired loop key variable slot
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
    match val_ty {
        PhpType::Int | PhpType::Bool => {
            crate::codegen::abi::store_at_offset_scratch(emitter, "rcx", v_offset, "r10"); // store the associative-array foreach scalar payload into the loop value slot
        }
        PhpType::Str => {
            crate::codegen::abi::store_at_offset_scratch(emitter, "rcx", v_offset, "r10"); // store the associative-array foreach string pointer into the loop value slot
            crate::codegen::abi::store_at_offset_scratch(emitter, "r8", v_offset - 8, "r10"); // store the associative-array foreach string length into the paired loop value slot
        }
        PhpType::Mixed => {
            crate::codegen::abi::emit_push_reg(emitter, "rcx");                           // preserve the associative-array foreach mixed low payload word across decref of the previous loop value
            crate::codegen::abi::emit_push_reg_pair(emitter, "r8", "r9");                 // preserve the associative-array foreach mixed high payload word and runtime tag across the decref helper
            crate::codegen::abi::load_at_offset_scratch(emitter, "rax", v_offset, "r10"); // load the previous boxed mixed foreach value before overwriting the loop variable
            emitter.instruction("call __rt_decref_mixed");                      // release the previous owned mixed foreach value if one exists
            crate::codegen::abi::emit_pop_reg_pair(emitter, "r8", "r9");                  // restore the associative-array foreach mixed high payload word and runtime tag after decref
            crate::codegen::abi::emit_pop_reg(emitter, "rcx");                            // restore the associative-array foreach mixed low payload word after decref
            emitter.instruction("cmp r9, 7");                                   // does this associative-array entry already store a boxed mixed value?
            let reuse_box = ctx.next_label("foreach_assoc_mixed_reuse");
            let store_box = ctx.next_label("foreach_assoc_mixed_store");
            emitter.instruction(&format!("je {}", reuse_box));                  // reuse existing mixed boxes instead of nesting them
            super::super::super::super::emit_box_runtime_payload_as_mixed(emitter, "r9", "rcx", "r8"); // box the borrowed associative-array payload into an owned mixed cell
            emitter.instruction(&format!("jmp {}", store_box));                 // skip the mixed-box reuse path once boxing is done
            emitter.label(&reuse_box);
            emitter.instruction("mov rax, rcx");                                // move the existing mixed box pointer into the incref helper input register
            emitter.instruction("call __rt_incref");                            // retain the shared mixed box for the foreach loop variable
            emitter.label(&store_box);
            crate::codegen::abi::store_at_offset_scratch(emitter, "rax", v_offset, "r10"); // store the owned mixed box pointer into the foreach loop variable slot
        }
        _ => {
            crate::codegen::abi::store_at_offset_scratch(emitter, "rcx", v_offset, "r10"); // store the associative-array foreach pointer-like payload into the loop value slot
        }
    }
    ctx.update_var_type_and_ownership(
        value_var,
        val_ty.clone(),
        if matches!(val_ty, PhpType::Mixed) {
            HeapOwnership::local_owner_for_type(val_ty)
        } else {
            HeapOwnership::borrowed_alias_for_type(val_ty)
        },
    );

    ctx.loop_stack.push(LoopLabels {
        continue_label: loop_cont.to_string(),
        break_label: loop_end.to_string(),
        sp_adjust: 32,
    });
    for s in body {
        emit_stmt(s, emitter, ctx, data);
    }
    ctx.loop_stack.pop();

    emitter.label(loop_cont);
    emitter.instruction(&format!("jmp {}", loop_start));                        // continue the associative-array foreach loop from the next insertion-order entry
    emitter.label(loop_end);
    emitter.instruction("add rsp, 32");                                         // drop the associative-array iterator cursor and preserved hash-table pointer stack slots
}
