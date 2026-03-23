use super::abi;
use super::context::{Context, LoopLabels};
use crate::types::PhpType;
use super::data_section::DataSection;
use super::emit::Emitter;
use super::expr::emit_expr;
use crate::parser::ast::{Stmt, StmtKind};

pub fn emit_stmt(
    stmt: &Stmt,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    match &stmt.kind {
        StmtKind::Echo(expr) => {
            emitter.blank();
            emitter.comment("echo");
            let ty = emit_expr(expr, emitter, ctx, data);
            match &ty {
                PhpType::Void => {
                    // null — don't print anything
                }
                PhpType::Bool => {
                    // echo false → nothing, echo true → "1"
                    let skip_label = ctx.next_label("echo_skip_false");
                    // -- skip echo if boolean value is false --
                    emitter.instruction(&format!("cbz x0, {}", skip_label));    // branch to skip label if x0 is zero (false)
                    abi::emit_write_stdout(emitter, &ty);
                    emitter.label(&skip_label);
                }
                PhpType::Int => {
                    // Runtime null check
                    let skip_label = ctx.next_label("echo_skip_null");
                    // -- build the null sentinel value 0x7FFFFFFFFFFFFFFFE in x9 --
                    emitter.instruction("movz x9, #0xFFFE");                    // load lowest 16 bits of null sentinel into x9
                    emitter.instruction("movk x9, #0xFFFF, lsl #16");           // insert bits 16-31 of null sentinel
                    emitter.instruction("movk x9, #0xFFFF, lsl #32");           // insert bits 32-47 of null sentinel
                    emitter.instruction("movk x9, #0x7FFF, lsl #48");           // insert bits 48-63 of null sentinel
                    // -- compare value against null sentinel and skip echo if null --
                    emitter.instruction("cmp x0, x9");                          // compare integer value against null sentinel
                    emitter.instruction(&format!("b.eq {}", skip_label));       // skip echo if value is the null sentinel
                    abi::emit_write_stdout(emitter, &ty);
                    emitter.label(&skip_label);
                }
                PhpType::Float => {
                    abi::emit_write_stdout(emitter, &ty);
                }
                _ => {
                    abi::emit_write_stdout(emitter, &ty);
                }
            }
        }
        StmtKind::Assign { name, value } => {
            emitter.blank();
            emitter.comment(&format!("${} = ...", name));
            let ty = emit_expr(value, emitter, ctx, data);

            let var = ctx.variables.get(name).expect("variable not pre-allocated");
            let offset = var.stack_offset;

            abi::emit_store(emitter, &ty, offset);

            // Update variable type if it changed (e.g. int /= produces float)
            if var.ty != ty {
                ctx.variables.get_mut(name).unwrap().ty = ty;
            }
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            let end_label = ctx.next_label("if_end");

            // Evaluate condition
            emitter.blank();
            emitter.comment("if");
            let cond_ty = emit_expr(condition, emitter, ctx, data);
            super::expr::coerce_null_to_zero(emitter, &cond_ty);
            let mut next_label = ctx.next_label("if_else");
            // -- test if condition and branch to else/elseif --
            emitter.instruction("cmp x0, #0");                                  // test if condition result is zero (falsy)
            emitter.instruction(&format!("b.eq {}", next_label));               // branch to else/elseif if condition is false

            // then body
            for s in then_body {
                emit_stmt(s, emitter, ctx, data);
            }
            // -- skip remaining branches after then-body executes --
            emitter.instruction(&format!("b {}", end_label));                   // unconditional jump past all else/elseif branches

            // elseif clauses
            for (cond, body) in elseif_clauses {
                emitter.label(&next_label);
                emitter.comment("elseif");
                let cond_ty = emit_expr(cond, emitter, ctx, data);
                super::expr::coerce_null_to_zero(emitter, &cond_ty);
                next_label = ctx.next_label("if_else");
                // -- test elseif condition and branch to next branch --
                emitter.instruction("cmp x0, #0");                              // test if elseif condition is zero (falsy)
                emitter.instruction(&format!("b.eq {}", next_label));           // branch to next elseif/else if condition is false

                for s in body {
                    emit_stmt(s, emitter, ctx, data);
                }
                // -- skip remaining branches after elseif-body executes --
                emitter.instruction(&format!("b {}", end_label));               // unconditional jump past remaining branches
            }

            // else body (or fall-through label)
            emitter.label(&next_label);
            if let Some(body) = else_body {
                emitter.comment("else");
                for s in body {
                    emit_stmt(s, emitter, ctx, data);
                }
            }

            emitter.label(&end_label);
        }
        StmtKind::ArrayAssign { array, index, value } => {
            emitter.blank();
            emitter.comment(&format!("${}[...] = ...", array));
            let var = ctx.variables.get(array).expect("undefined variable");
            let offset = var.stack_offset;
            let is_assoc = matches!(&var.ty, PhpType::AssocArray { .. });
            let elem_ty = match &var.ty {
                PhpType::Array(t) => *t.clone(),
                PhpType::AssocArray { value: v, .. } => *v.clone(),
                _ => PhpType::Int,
            };

            if is_assoc {
                // -- associative array assignment: $map[$key] = $value --
                emitter.instruction(&format!("ldur x0, [x29, #-{}]", offset));  // load hash table pointer
                emitter.instruction("str x0, [sp, #-16]!");                     // save hash table pointer
                // Evaluate key (string)
                emit_expr(index, emitter, ctx, data);
                emitter.instruction("stp x1, x2, [sp, #-16]!");                 // save key ptr/len
                // Evaluate value
                let val_ty = emit_expr(value, emitter, ctx, data);
                // -- prepare hash_set args --
                let (val_lo, val_hi) = match &val_ty {
                    PhpType::Int | PhpType::Bool => ("x0", "xzr"),
                    PhpType::Str => ("x1", "x2"),
                    PhpType::Float => {
                        emitter.instruction("fmov x9, d0");                     // move float bits to integer register
                        ("x9", "xzr")
                    }
                    _ => ("x0", "xzr"),
                };
                emitter.instruction(&format!("mov x3, {}", val_lo));            // value_lo
                emitter.instruction(&format!("mov x4, {}", val_hi));            // value_hi
                emitter.instruction("ldp x1, x2, [sp], #16");                   // pop key ptr/len
                emitter.instruction("ldr x0, [sp], #16");                       // pop hash table pointer
                emitter.instruction("bl __rt_hash_set");                        // insert/update key-value pair
            } else {
                // -- indexed array assignment (existing logic) --
                // -- load array base pointer from local variable slot --
                emitter.instruction(&format!("ldur x0, [x29, #-{}]", offset));  // load array heap pointer from stack frame
                emitter.instruction("str x0, [sp, #-16]!");                     // push array pointer onto stack
                // Evaluate index
                emit_expr(index, emitter, ctx, data);
                emitter.instruction("str x0, [sp, #-16]!");                     // push computed index onto stack
                // Evaluate value
                let val_ty = emit_expr(value, emitter, ctx, data);
                // -- pop saved index and array pointer back into registers --
                emitter.instruction("ldr x9, [sp], #16");                       // pop index value from stack into x9
                emitter.instruction("ldr x10, [sp], #16");                      // pop array pointer from stack into x10
                match &elem_ty {
                    PhpType::Int => {
                        // -- store integer value at array[index] --
                        emitter.instruction("add x10, x10, #24");               // skip 24-byte array header
                        emitter.instruction("str x0, [x10, x9, lsl #3]");       // store int at data[index]
                    }
                    PhpType::Str => {
                        // -- store string (ptr+len pair) at array[index] --
                        emitter.instruction("lsl x9, x9, #4");                  // multiply index by 16
                        emitter.instruction("add x10, x10, x9");                // offset into array data region
                        emitter.instruction("add x10, x10, #24");               // skip 24-byte array header
                        emitter.instruction("str x1, [x10]");                   // store string pointer at slot
                        emitter.instruction("str x2, [x10, #8]");               // store string length at slot+8
                    }
                    _ => {}
                }
                let _ = val_ty;
            }
        }
        StmtKind::ArrayPush { array, value } => {
            emitter.blank();
            emitter.comment(&format!("${}[] = ...", array));
            let var = ctx.variables.get(array).expect("undefined variable");
            let offset = var.stack_offset;
            let elem_ty = match &var.ty {
                PhpType::Array(t) => *t.clone(),
                _ => PhpType::Int,
            };
            // -- load array pointer and save it before evaluating the value --
            emitter.instruction(&format!("ldur x0, [x29, #-{}]", offset));      // load array heap pointer from stack frame
            emitter.instruction("str x0, [sp, #-16]!");                         // push array pointer onto stack to preserve it
            // Evaluate value
            emit_expr(value, emitter, ctx, data);
            emitter.instruction("ldr x9, [sp], #16");                           // pop saved array pointer into x9
            match &elem_ty {
                PhpType::Int => {
                    // -- call runtime to append integer to array --
                    emitter.instruction("mov x1, x0");                          // move value to x1 (second arg for runtime call)
                    emitter.instruction("mov x0, x9");                          // move array pointer to x0 (first arg)
                    emitter.instruction("bl __rt_array_push_int");              // call runtime: append integer to dynamic array
                }
                PhpType::Str => {
                    // -- call runtime to append string to array --
                    emitter.instruction("mov x0, x9");                          // move array pointer to x0 (first arg)
                    emitter.instruction("bl __rt_array_push_str");              // call runtime: append string (x1=ptr, x2=len) to array
                }
                _ => {}
            }
        }
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            body,
        } => {
            let loop_start = ctx.next_label("foreach_start");
            let loop_end = ctx.next_label("foreach_end");
            let loop_cont = ctx.next_label("foreach_cont");

            emitter.blank();
            emitter.comment("foreach");

            // Evaluate array
            let arr_ty = emit_expr(array, emitter, ctx, data);

            if let PhpType::AssocArray { value, .. } = &arr_ty {
                // -- foreach over associative array using hash iterator --
                let val_ty = *value.clone();
                // Stack: [hash_ptr:16][iter_index:16]
                emitter.instruction("str x0, [sp, #-16]!");                     // push hash table pointer
                emitter.instruction("str xzr, [sp, #-16]!");                    // push initial iterator index (0)

                emitter.label(&loop_start);
                // -- call hash_iter_next to get next entry --
                emitter.instruction("ldr x0, [sp, #16]");                       // load hash table pointer
                emitter.instruction("ldr x1, [sp]");                            // load current iterator index
                emitter.instruction("bl __rt_hash_iter_next");                  // x0=next_idx(-1=done), x1=key_ptr, x2=key_len, x3=val_lo, x4=val_hi

                // -- check if iteration is done --
                emitter.instruction("cmn x0, #1");                              // compare x0 with -1 (end of iteration)
                emitter.instruction(&format!("b.eq {}", loop_end));             // exit if done

                // -- save updated index --
                emitter.instruction("str x0, [sp]");                            // store new iterator index

                // -- store key into $key_var if present --
                if let Some(kv) = key_var {
                    let kvar = ctx.variables.get(kv).expect("foreach key var");
                    let k_offset = kvar.stack_offset;
                    // key is a string: x1=ptr, x2=len
                    emitter.instruction(&format!("stur x1, [x29, #-{}]", k_offset)); // store key ptr
                    emitter.instruction(&format!("stur x2, [x29, #-{}]", k_offset - 8)); // store key len
                }

                // -- store value into $value_var --
                let val_var_info = ctx.variables.get(value_var).expect("foreach val var");
                let v_offset = val_var_info.stack_offset;
                match &val_ty {
                    PhpType::Int | PhpType::Bool => {
                        emitter.instruction(&format!("stur x3, [x29, #-{}]", v_offset)); // store int value
                    }
                    PhpType::Str => {
                        emitter.instruction(&format!("stur x3, [x29, #-{}]", v_offset)); // store string ptr
                        emitter.instruction(&format!("stur x4, [x29, #-{}]", v_offset - 8)); // store string len
                    }
                    _ => {
                        emitter.instruction(&format!("stur x3, [x29, #-{}]", v_offset)); // store value
                    }
                }

                ctx.loop_stack.push(LoopLabels {
                    continue_label: loop_cont.clone(),
                    break_label: loop_end.clone(),
                });

                for s in body {
                    emit_stmt(s, emitter, ctx, data);
                }

                ctx.loop_stack.pop();

                emitter.label(&loop_cont);
                emitter.instruction(&format!("b {}", loop_start));              // jump back to iterator
                emitter.label(&loop_end);
                emitter.instruction("add sp, sp, #32");                         // pop iter_index + hash_ptr
            } else {
                // -- foreach over indexed array (existing logic) --
                let elem_ty = match &arr_ty {
                    PhpType::Array(t) => *t.clone(),
                    _ => PhpType::Int,
                };
                // -- save array metadata on stack for iteration --
                emitter.instruction("str x0, [sp, #-16]!");                     // push array pointer onto stack
                emitter.instruction("ldr x9, [x0]");                            // load array length from first field of array struct
                emitter.instruction("str x9, [sp, #-16]!");                     // push array length onto stack
                emitter.instruction("str xzr, [sp, #-16]!");                    // push initial loop index (0) onto stack

                emitter.label(&loop_start);
                // -- load loop index and array length, check bounds --
                emitter.instruction("ldr x0, [sp]");                            // load current loop index from top of stack
                emitter.instruction("ldr x1, [sp, #16]");                       // load array length from stack (2 slots down)
                emitter.instruction("cmp x0, x1");                              // compare index against array length
                emitter.instruction(&format!("b.ge {}", loop_end));             // exit loop if index >= length

                // -- store index into $key_var if present --
                if let Some(kv) = key_var {
                    let kvar = ctx.variables.get(kv).expect("foreach key var");
                    let k_offset = kvar.stack_offset;
                    emitter.instruction(&format!("stur x0, [x29, #-{}]", k_offset)); // store index as key
                }

                // -- load element at current index into the loop variable --
                emitter.instruction("ldr x9, [sp, #32]");                       // load array pointer from stack (3 slots down)
                let val_var = ctx.variables.get(value_var).expect("foreach var");
                let val_offset = val_var.stack_offset;
                match &elem_ty {
                    PhpType::Int => {
                        // -- load integer element and store into $value_var --
                        emitter.instruction("add x9, x9, #24");                 // skip 24-byte array header to reach data
                        emitter.instruction("ldr x0, [x9, x0, lsl #3]");        // load int at data[index] (8 bytes per element)
                        emitter.instruction(&format!("stur x0, [x29, #-{}]", val_offset)); // store value into $value_var's stack slot
                    }
                    PhpType::Str => {
                        // -- load string element (ptr+len) and store into $value_var --
                        emitter.instruction("lsl x10, x0, #4");                 // multiply index by 16 (string slot size)
                        emitter.instruction("add x9, x9, x10");                 // offset to the string slot in data region
                        emitter.instruction("add x9, x9, #24");                 // skip 24-byte array header
                        emitter.instruction("ldr x1, [x9]");                    // load string pointer from slot
                        emitter.instruction("ldr x2, [x9, #8]");                // load string length from slot+8
                        emitter.instruction(&format!("stur x1, [x29, #-{}]", val_offset)); // store string pointer into $value_var
                        emitter.instruction(&format!("stur x2, [x29, #-{}]", val_offset - 8)); // store string length into $value_var+8
                    }
                    _ => {}
                }

                ctx.loop_stack.push(LoopLabels {
                    continue_label: loop_cont.clone(),
                    break_label: loop_end.clone(),
                });

                for s in body {
                    emit_stmt(s, emitter, ctx, data);
                }

                ctx.loop_stack.pop();

                // -- increment loop index and jump back to condition check --
                emitter.label(&loop_cont);
                emitter.instruction("ldr x0, [sp]");                            // load current loop index from stack
                emitter.instruction("add x0, x0, #1");                          // increment index by 1
                emitter.instruction("str x0, [sp]");                            // write updated index back to stack
                emitter.instruction(&format!("b {}", loop_start));              // jump back to loop condition check

                emitter.label(&loop_end);
                // -- clean up the 3 stack slots (index, length, array ptr) --
                emitter.instruction("add sp, sp, #48");                         // deallocate 48 bytes (3 x 16-byte slots) from stack
            }
        }
        StmtKind::DoWhile { body, condition } => {
            let loop_start = ctx.next_label("dowhile_start");
            let loop_end = ctx.next_label("dowhile_end");
            let loop_cond = ctx.next_label("dowhile_cond");

            emitter.blank();
            emitter.comment("do...while");
            emitter.label(&loop_start);

            ctx.loop_stack.push(LoopLabels {
                continue_label: loop_cond.clone(),
                break_label: loop_end.clone(),
            });

            for s in body {
                emit_stmt(s, emitter, ctx, data);
            }

            ctx.loop_stack.pop();

            // -- evaluate do-while condition and loop back if true --
            emitter.label(&loop_cond);
            let cond_ty = emit_expr(condition, emitter, ctx, data);
            super::expr::coerce_null_to_zero(emitter, &cond_ty);
            emitter.instruction("cmp x0, #0");                                  // test if do-while condition is zero (falsy)
            emitter.instruction(&format!("b.ne {}", loop_start));               // loop back to start if condition is nonzero (truthy)
            emitter.label(&loop_end);
        }
        StmtKind::While { condition, body } => {
            let loop_start = ctx.next_label("while_start");
            let loop_end = ctx.next_label("while_end");

            emitter.blank();
            emitter.comment("while");
            emitter.label(&loop_start);
            let cond_ty = emit_expr(condition, emitter, ctx, data);
            super::expr::coerce_null_to_zero(emitter, &cond_ty);
            // -- test while condition and exit loop if false --
            emitter.instruction("cmp x0, #0");                                  // test if while condition is zero (falsy)
            emitter.instruction(&format!("b.eq {}", loop_end));                 // exit loop if condition is false

            ctx.loop_stack.push(LoopLabels {
                continue_label: loop_start.clone(),
                break_label: loop_end.clone(),
            });

            for s in body {
                emit_stmt(s, emitter, ctx, data);
            }

            ctx.loop_stack.pop();

            // -- jump back to re-evaluate the while condition --
            emitter.instruction(&format!("b {}", loop_start));                  // unconditional branch back to loop start
            emitter.label(&loop_end);
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            let loop_start = ctx.next_label("for_start");
            let loop_continue = ctx.next_label("for_cont");
            let loop_end = ctx.next_label("for_end");

            emitter.blank();
            emitter.comment("for");

            // Init
            if let Some(s) = init {
                emit_stmt(s, emitter, ctx, data);
            }

            emitter.label(&loop_start);

            // Condition
            if let Some(cond) = condition {
                let cond_ty = emit_expr(cond, emitter, ctx, data);
                super::expr::coerce_null_to_zero(emitter, &cond_ty);
                // -- test for-loop condition and exit if false --
                emitter.instruction("cmp x0, #0");                              // test if for-loop condition is zero (falsy)
                emitter.instruction(&format!("b.eq {}", loop_end));             // exit loop if condition is false
            }

            ctx.loop_stack.push(LoopLabels {
                continue_label: loop_continue.clone(),
                break_label: loop_end.clone(),
            });

            // Body
            for s in body {
                emit_stmt(s, emitter, ctx, data);
            }

            ctx.loop_stack.pop();

            // Update + loop back
            emitter.label(&loop_continue);
            if let Some(s) = update {
                emit_stmt(s, emitter, ctx, data);
            }
            // -- jump back to re-evaluate the for-loop condition --
            emitter.instruction(&format!("b {}", loop_start));                  // unconditional branch back to loop start
            emitter.label(&loop_end);
        }
        StmtKind::Break => {
            let labels = ctx.loop_stack.last().expect("break outside loop");
            // -- break: jump out of the current loop --
            emitter.instruction(&format!("b {}", labels.break_label));          // unconditional branch to loop exit label
        }
        StmtKind::FunctionDecl { .. } => {
            // Emitted separately in codegen/mod.rs
        }
        StmtKind::Return(expr) => {
            emitter.blank();
            emitter.comment("return");
            if let Some(e) = expr {
                emit_expr(e, emitter, ctx, data);
            }
            if let Some(label) = &ctx.return_label {
                // -- jump to function epilogue to restore frame and return --
                emitter.instruction(&format!("b {}", label));                   // branch to function epilogue for stack cleanup and ret
            }
        }
        StmtKind::ExprStmt(expr) => {
            emitter.blank();
            emit_expr(expr, emitter, ctx, data);
            // result discarded
        }
        StmtKind::Continue => {
            let labels = ctx.loop_stack.last().expect("continue outside loop");
            // -- continue: jump to next iteration of the current loop --
            emitter.instruction(&format!("b {}", labels.continue_label));       // unconditional branch to loop continue label
        }
        StmtKind::Switch { subject, cases, default } => {
            let switch_end = ctx.next_label("switch_end");
            emitter.blank();
            emitter.comment("switch");

            // -- evaluate subject expression --
            let subj_ty = emit_expr(subject, emitter, ctx, data);
            match &subj_ty {
                PhpType::Str => {
                    emitter.instruction("stp x1, x2, [sp, #-16]!");             // save string subject
                }
                _ => {
                    emitter.instruction("str x0, [sp, #-16]!");                 // save int/bool subject
                }
            }

            // -- generate jump table: compare subject to each case value --
            let mut body_labels = Vec::new();
            for (i, (values, _)) in cases.iter().enumerate() {
                let body_label = ctx.next_label(&format!("case_{}", i));
                for val in values {
                    let val_ty = emit_expr(val, emitter, ctx, data);
                    match &subj_ty {
                        PhpType::Str => {
                            emitter.instruction("mov x3, x1");                  // pattern ptr
                            emitter.instruction("mov x4, x2");                  // pattern len
                            emitter.instruction("ldp x1, x2, [sp]");            // peek subject string
                            emitter.instruction("bl __rt_str_eq");              // compare → x0=1 if equal
                        }
                        _ => {
                            emitter.instruction("ldr x9, [sp]");                // peek subject
                            emitter.instruction("cmp x9, x0");                  // compare
                            emitter.instruction("cset x0, eq");                 // x0=1 if equal
                        }
                    }
                    emitter.instruction(&format!("cbnz x0, {}", body_label));   // jump to case body if match
                    let _ = val_ty;
                }
                body_labels.push(body_label);
            }

            // -- no case matched: jump to default or end --
            let default_label = ctx.next_label("switch_default");
            if default.is_some() {
                emitter.instruction(&format!("b {}", default_label));           // jump to default case
            } else {
                emitter.instruction(&format!("b {}", switch_end));              // jump to end (no default)
            }

            // -- emit case bodies (fall-through semantics) --
            ctx.loop_stack.push(LoopLabels {
                continue_label: switch_end.clone(),
                break_label: switch_end.clone(),
            });

            for (i, (_, body)) in cases.iter().enumerate() {
                emitter.label(&body_labels[i]);
                for s in body {
                    emit_stmt(s, emitter, ctx, data);
                }
                // No implicit break — fall through to next case
            }

            // -- default body --
            if let Some(def_body) = default {
                emitter.label(&default_label);
                for s in def_body {
                    emit_stmt(s, emitter, ctx, data);
                }
            }

            ctx.loop_stack.pop();
            emitter.label(&switch_end);
            // -- clean up saved subject --
            emitter.instruction("add sp, sp, #16");                             // pop saved subject
        }
        StmtKind::Include { .. } => {
            // Should have been resolved before codegen
            panic!("Unresolved include statement in codegen");
        }
    }
}
