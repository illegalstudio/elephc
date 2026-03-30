use super::super::context::{Context, LoopLabels};
use super::super::data_section::DataSection;
use super::super::emit::Emitter;
use super::super::expr::emit_expr;
use crate::parser::ast::{Expr, Stmt};
use crate::types::PhpType;

pub(super) fn emit_if_stmt(
    condition: &Expr,
    then_body: &[Stmt],
    elseif_clauses: &[(Expr, Vec<Stmt>)],
    else_body: &Option<Vec<Stmt>>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let end_label = ctx.next_label("if_end");

    emitter.blank();
    emitter.comment("if");
    let cond_ty = emit_expr(condition, emitter, ctx, data);
    super::super::expr::coerce_to_truthiness(emitter, ctx, &cond_ty);
    let mut next_label = ctx.next_label("if_else");
    emitter.instruction("cmp x0, #0");                                          // test if condition result is zero (falsy)
    emitter.instruction(&format!("b.eq {}", next_label));                       // branch to else/elseif if condition is false

    for s in then_body {
        super::emit_stmt(s, emitter, ctx, data);
    }
    emitter.instruction(&format!("b {}", end_label));                           // unconditional jump past all else/elseif branches

    for (cond, body) in elseif_clauses {
        emitter.label(&next_label);
        emitter.comment("elseif");
        let cond_ty = emit_expr(cond, emitter, ctx, data);
        super::super::expr::coerce_to_truthiness(emitter, ctx, &cond_ty);
        next_label = ctx.next_label("if_else");
        emitter.instruction("cmp x0, #0");                                      // test if elseif condition is zero (falsy)
        emitter.instruction(&format!("b.eq {}", next_label));                   // branch to next elseif/else if condition is false

        for s in body {
            super::emit_stmt(s, emitter, ctx, data);
        }
        emitter.instruction(&format!("b {}", end_label));                       // unconditional jump past remaining branches
    }

    emitter.label(&next_label);
    if let Some(body) = else_body {
        emitter.comment("else");
        for s in body {
            super::emit_stmt(s, emitter, ctx, data);
        }
    }

    emitter.label(&end_label);
}

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
        emitter.instruction("str xzr, [sp, #-16]!");                            // push initial iterator index (0)

        emitter.label(&loop_start);
        emitter.instruction("ldr x0, [sp, #16]");                               // load hash table pointer
        emitter.instruction("ldr x1, [sp]");                                    // load current iterator index
        emitter.instruction("bl __rt_hash_iter_next");                          // x0=next_idx(-1=done), x1=key_ptr, x2=key_len, x3=val_lo, x4=val_hi
        emitter.instruction("cmn x0, #1");                                      // compare x0 with -1 (end of iteration)
        emitter.instruction(&format!("b.eq {}", loop_end));                     // exit if done
        emitter.instruction("str x0, [sp]");                                    // store new iterator index

        if let Some(kv) = key_var {
            if let Some(kvar) = ctx.variables.get(kv) {
                let k_offset = kvar.stack_offset;
                super::super::abi::store_at_offset_scratch(emitter, "x1", k_offset, "x10"); // store key ptr
                super::super::abi::store_at_offset_scratch(emitter, "x2", k_offset - 8, "x10"); // store key len
                ctx.update_var_type_and_ownership(
                    kv,
                    PhpType::Str,
                    super::HeapOwnership::borrowed_alias_for_type(&PhpType::Str),
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
                super::super::abi::store_at_offset_scratch(emitter, "x3", v_offset, "x10");
            }
            PhpType::Str => {
                super::super::abi::store_at_offset_scratch(emitter, "x3", v_offset, "x10");
                super::super::abi::store_at_offset_scratch(emitter, "x4", v_offset - 8, "x10");
            }
            _ => {
                super::super::abi::store_at_offset_scratch(emitter, "x3", v_offset, "x10");
            }
        }
        ctx.update_var_type_and_ownership(
            value_var,
            val_ty.clone(),
            super::HeapOwnership::borrowed_alias_for_type(&val_ty),
        );

        ctx.loop_stack.push(LoopLabels {
            continue_label: loop_cont.clone(),
            break_label: loop_end.clone(),
            sp_adjust: 32,
        });
        for s in body {
            super::emit_stmt(s, emitter, ctx, data);
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
                super::super::abi::store_at_offset_scratch(emitter, "x0", k_offset, "x10");
                ctx.update_var_type_and_ownership(kv, PhpType::Int, super::HeapOwnership::NonHeap);
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
                super::super::abi::store_at_offset(emitter, "x0", val_offset);
            }
            PhpType::Str => {
                emitter.instruction("lsl x10, x0, #4");                         // multiply index by 16 (string slot size)
                emitter.instruction("add x9, x9, x10");                         // offset to the string slot in data region
                emitter.instruction("add x9, x9, #24");                         // skip 24-byte array header
                emitter.instruction("ldr x1, [x9]");                            // load string pointer from slot
                emitter.instruction("ldr x2, [x9, #8]");                        // load string length from slot+8
                super::super::abi::store_at_offset(emitter, "x1", val_offset);
                super::super::abi::store_at_offset(emitter, "x2", val_offset - 8);
            }
            PhpType::Array(_) | PhpType::AssocArray { .. } => {
                emitter.instruction("add x9, x9, #24");                         // skip 24-byte array header to reach data
                emitter.instruction("ldr x0, [x9, x0, lsl #3]");                // load nested array pointer at index
                super::super::abi::store_at_offset(emitter, "x0", val_offset);
            }
            _ => {}
        }
        ctx.update_var_type_and_ownership(
            value_var,
            elem_ty.clone(),
            super::HeapOwnership::borrowed_alias_for_type(&elem_ty),
        );

        ctx.loop_stack.push(LoopLabels {
            continue_label: loop_cont.clone(),
            break_label: loop_end.clone(),
            sp_adjust: 48,
        });
        for s in body {
            super::emit_stmt(s, emitter, ctx, data);
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

pub(super) fn emit_do_while_stmt(
    body: &[Stmt],
    condition: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let loop_start = ctx.next_label("dowhile_start");
    let loop_end = ctx.next_label("dowhile_end");
    let loop_cond = ctx.next_label("dowhile_cond");

    emitter.blank();
    emitter.comment("do...while");
    emitter.label(&loop_start);

    ctx.loop_stack.push(LoopLabels {
        continue_label: loop_cond.clone(),
        break_label: loop_end.clone(),
        sp_adjust: 0,
    });
    for s in body {
        super::emit_stmt(s, emitter, ctx, data);
    }
    ctx.loop_stack.pop();

    emitter.label(&loop_cond);
    let cond_ty = emit_expr(condition, emitter, ctx, data);
    super::super::expr::coerce_to_truthiness(emitter, ctx, &cond_ty);
    emitter.instruction("cmp x0, #0");                                          // test if do-while condition is zero (falsy)
    emitter.instruction(&format!("b.ne {}", loop_start));                       // loop back to start if condition is nonzero (truthy)
    emitter.label(&loop_end);
}

pub(super) fn emit_while_stmt(
    condition: &Expr,
    body: &[Stmt],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let loop_start = ctx.next_label("while_start");
    let loop_end = ctx.next_label("while_end");

    emitter.blank();
    emitter.comment("while");
    emitter.label(&loop_start);
    let cond_ty = emit_expr(condition, emitter, ctx, data);
    super::super::expr::coerce_to_truthiness(emitter, ctx, &cond_ty);
    emitter.instruction("cmp x0, #0");                                          // test if while condition is zero (falsy)
    emitter.instruction(&format!("b.eq {}", loop_end));                         // exit loop if condition is false

    ctx.loop_stack.push(LoopLabels {
        continue_label: loop_start.clone(),
        break_label: loop_end.clone(),
        sp_adjust: 0,
    });
    for s in body {
        super::emit_stmt(s, emitter, ctx, data);
    }
    ctx.loop_stack.pop();

    emitter.instruction(&format!("b {}", loop_start));                          // unconditional branch back to loop start
    emitter.label(&loop_end);
}

pub(super) fn emit_for_stmt(
    init: &Option<Box<Stmt>>,
    condition: &Option<Expr>,
    update: &Option<Box<Stmt>>,
    body: &[Stmt],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let loop_start = ctx.next_label("for_start");
    let loop_continue = ctx.next_label("for_cont");
    let loop_end = ctx.next_label("for_end");

    emitter.blank();
    emitter.comment("for");

    if let Some(s) = init {
        super::emit_stmt(s, emitter, ctx, data);
    }

    emitter.label(&loop_start);

    if let Some(cond) = condition {
        let cond_ty = emit_expr(cond, emitter, ctx, data);
        super::super::expr::coerce_to_truthiness(emitter, ctx, &cond_ty);
        emitter.instruction("cmp x0, #0");                                      // test if for-loop condition is zero (falsy)
        emitter.instruction(&format!("b.eq {}", loop_end));                     // exit loop if condition is false
    }

    ctx.loop_stack.push(LoopLabels {
        continue_label: loop_continue.clone(),
        break_label: loop_end.clone(),
        sp_adjust: 0,
    });
    for s in body {
        super::emit_stmt(s, emitter, ctx, data);
    }
    ctx.loop_stack.pop();

    emitter.label(&loop_continue);
    if let Some(s) = update {
        super::emit_stmt(s, emitter, ctx, data);
    }
    emitter.instruction(&format!("b {}", loop_start));                          // unconditional branch back to loop start
    emitter.label(&loop_end);
}

pub(super) fn emit_break_stmt(emitter: &mut Emitter, ctx: &Context) {
    let labels = ctx
        .loop_stack
        .last()
        .expect("codegen bug: break statement outside loop (should have been caught by type checker)");
    emitter.instruction(&format!("b {}", labels.break_label));                  // unconditional branch to loop exit label
}

pub(super) fn emit_return_stmt(
    expr: &Option<Expr>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment("return");
    if let Some(e) = expr {
        let ty = emit_expr(e, emitter, ctx, data);
        super::retain_borrowed_heap_result(emitter, e, &ty);
    }
    if let Some(label) = &ctx.return_label {
        let sp_total: usize = ctx.loop_stack.iter().map(|l| l.sp_adjust).sum();
        if sp_total > 0 {
            emitter.instruction(&format!("add sp, sp, #{}", sp_total));         // pop switch subjects before returning
        }
        emitter.instruction(&format!("b {}", label));                           // branch to function epilogue for stack cleanup and ret
    }
}

pub(super) fn emit_continue_stmt(emitter: &mut Emitter, ctx: &Context) {
    let labels = ctx
        .loop_stack
        .last()
        .expect("codegen bug: continue statement outside loop (should have been caught by type checker)");
    emitter.instruction(&format!("b {}", labels.continue_label));               // unconditional branch to loop continue label
}

pub(super) fn emit_switch_stmt(
    subject: &Expr,
    cases: &[(Vec<Expr>, Vec<Stmt>)],
    default: &Option<Vec<Stmt>>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let switch_end = ctx.next_label("switch_end");
    emitter.blank();
    emitter.comment("switch");

    let subj_ty = emit_expr(subject, emitter, ctx, data);
    match &subj_ty {
        PhpType::Str => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // save string subject
        }
        _ => {
            emitter.instruction("str x0, [sp, #-16]!");                         // save int/bool subject
        }
    }

    let mut body_labels = Vec::new();
    for (i, (values, _)) in cases.iter().enumerate() {
        let body_label = ctx.next_label(&format!("case_{}", i));
        for val in values {
            let val_ty = emit_expr(val, emitter, ctx, data);
            match &subj_ty {
                PhpType::Str => {
                    emitter.instruction("mov x3, x1");                          // pattern ptr
                    emitter.instruction("mov x4, x2");                          // pattern len
                    emitter.instruction("ldp x1, x2, [sp]");                    // peek subject string
                    emitter.instruction("bl __rt_str_eq");                      // compare → x0=1 if equal
                }
                _ => {
                    emitter.instruction("ldr x9, [sp]");                        // peek subject
                    emitter.instruction("cmp x9, x0");                          // compare
                    emitter.instruction("cset x0, eq");                         // x0=1 if equal
                }
            }
            emitter.instruction(&format!("cbnz x0, {}", body_label));           // jump to case body if match
            let _ = val_ty;
        }
        body_labels.push(body_label);
    }

    let default_label = ctx.next_label("switch_default");
    if default.is_some() {
        emitter.instruction(&format!("b {}", default_label));                   // jump to default case
    } else {
        emitter.instruction(&format!("b {}", switch_end));                      // jump to end (no default)
    }

    ctx.loop_stack.push(LoopLabels {
        continue_label: switch_end.clone(),
        break_label: switch_end.clone(),
        sp_adjust: 16,
    });
    for (i, (_, body)) in cases.iter().enumerate() {
        emitter.label(&body_labels[i]);
        for s in body {
            super::emit_stmt(s, emitter, ctx, data);
        }
    }

    if let Some(def_body) = default {
        emitter.label(&default_label);
        for s in def_body {
            super::emit_stmt(s, emitter, ctx, data);
        }
    }

    ctx.loop_stack.pop();
    emitter.label(&switch_end);
    emitter.instruction("add sp, sp, #16");                                     // pop saved subject
}
