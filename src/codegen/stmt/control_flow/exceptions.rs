use crate::codegen::abi;
use crate::codegen::context::{Context, FinallyContext};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::{CatchClause, Expr, Stmt};
use crate::types::PhpType;

const PENDING_RETURN: u64 = 1;
const PENDING_BRANCH: u64 = 2;
const PENDING_RETHROW: u64 = 3;

pub(super) fn emit_throw_stmt(
    expr: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    emitter.blank();
    emitter.comment("throw");
    let thrown_ty = emit_expr(expr, emitter, ctx, data);
    super::super::retain_borrowed_heap_result(emitter, expr, &thrown_ty);
    emitter.adrp("x9", "_exc_value");                            // load page of the current exception slot
    emitter.add_lo12("x9", "x9", "_exc_value");                      // resolve the current exception slot address
    emitter.instruction("str x0, [x9]");                                        // publish the thrown object pointer as the active exception
    emitter.instruction("bl __rt_throw_current");                               // unwind to the nearest active exception handler
}

pub(super) fn emit_try_stmt(
    try_body: &[Stmt],
    catches: &[CatchClause],
    finally_body: &Option<Vec<Stmt>>,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    let handler_offset = ctx.next_try_slot();
    let handler_resume = ctx.next_label("try_resume");
    let normal_done = ctx.next_label("try_normal_done");
    let end_label = ctx.next_label("try_end");
    let finally_label = finally_body
        .as_ref()
        .map(|_| ctx.next_label("try_finally"));
    let catch_end_label = ctx.next_label("try_catch_end");

    emitter.blank();
    emitter.comment("try");
    emit_try_handler_push(emitter, ctx, handler_offset);
    emit_handler_jmpbuf_address(emitter, handler_offset, "x0");
    emitter.bl_c("setjmp");                                          // snapshot the current stack/register state for this try handler
    emitter.instruction(&format!("cbnz x0, {}", handler_resume));               // resume at catch dispatch after a longjmp into this handler

    if let Some(label) = &finally_label {
        ctx.finally_stack.push(FinallyContext {
            entry_label: label.clone(),
        });
    }

    for stmt in try_body {
        super::super::emit_stmt(stmt, emitter, ctx, data);
    }

    if finally_label.is_some() {
        ctx.finally_stack.pop();
    }

    emit_try_handler_pop(emitter, handler_offset);
    if let Some(label) = &finally_label {
        emitter.instruction(&format!("b {}", label));                           // run finally after the try body completes normally
    } else {
        emitter.instruction(&format!("b {}", end_label));                       // skip catch dispatch after a normal try-body completion
    }

    emitter.label(&handler_resume);
    emit_try_handler_pop(emitter, handler_offset);

    if catches.is_empty() {
        if finally_label.is_some() {
            emit_set_pending_action(emitter, ctx, PENDING_RETHROW, None, false);
            let finally_entry = finally_label
                .as_ref()
                .expect("codegen bug: missing finally label");
            emitter.instruction(&format!("b {}", finally_entry));               // defer rethrow until after finally
        } else {
            emitter.instruction("bl __rt_rethrow_current");                     // propagate an uncaught exception to the next enclosing try
        }
    } else {
        for catch_clause in catches {
            let catch_label = ctx.next_label("catch_body");
            let next_catch_label = ctx.next_label("catch_next");
            for (idx, catch_type) in catch_clause.exception_types.iter().enumerate() {
                let (catch_id, catch_kind) = resolve_catch_match_target(ctx, catch_type.as_str());
                let mismatch_label = if idx + 1 == catch_clause.exception_types.len() {
                    next_catch_label.clone()
                } else {
                    ctx.next_label("catch_type_next")
                };

                emitter.adrp("x9", "_exc_value");                // load page of the current exception slot
                emitter.add_lo12("x9", "x9", "_exc_value");          // resolve the current exception slot address
                emitter.instruction("ldr x0, [x9]");                            // load the active exception object for catch matching
                emitter.instruction(&format!("mov x1, #{}", catch_id));         // materialize the catch target id for runtime matching
                emitter.instruction(&format!("mov x2, #{}", catch_kind));       // tell the runtime whether this catch target is a class or interface
                emitter.instruction("bl __rt_exception_matches");               // test whether the current exception matches this catch target
                emitter.instruction(&format!("cbz x0, {}", mismatch_label));    // move to the next type in this catch clause when it does not match
                emitter.instruction(&format!("b {}", catch_label));             // jump into the shared catch body once any type matches
                if idx + 1 != catch_clause.exception_types.len() {
                    emitter.label(&mismatch_label);
                }
            }

            emitter.label(&catch_label);
            bind_catch_variable(catch_clause, emitter, ctx);
            for stmt in &catch_clause.body {
                super::super::emit_stmt(stmt, emitter, ctx, data);
            }
            if let Some(label) = &finally_label {
                emitter.instruction(&format!("b {}", label));                   // run finally after the matching catch body completes
            } else {
                emitter.instruction(&format!("b {}", catch_end_label));         // leave the try/catch after the matching catch completes
            }
            emitter.label(&next_catch_label);
        }

        if let Some(label) = &finally_label {
            emit_set_pending_action(emitter, ctx, PENDING_RETHROW, None, false);
            emitter.instruction(&format!("b {}", label));                       // no catch matched, so run finally before rethrowing
        } else {
            emitter.instruction("bl __rt_rethrow_current");                     // no catch matched and there is no finally to run first
        }
    }

    emitter.label(&catch_end_label);
    emitter.instruction(&format!("b {}", end_label));                           // join point after try/catch when no finally is present

    if let Some(label) = finally_label {
        let dispatch_return = ctx.next_label("finally_dispatch_return");
        let dispatch_branch = ctx.next_label("finally_dispatch_branch");
        let dispatch_rethrow = ctx.next_label("finally_dispatch_rethrow");
        emitter.label(&label);
        if let Some(body) = finally_body {
            for stmt in body {
                super::super::emit_stmt(stmt, emitter, ctx, data);
            }
        }
        emit_finally_dispatch(
            emitter,
            ctx,
            &normal_done,
            &dispatch_return,
            &dispatch_branch,
            &dispatch_rethrow,
        );
        emitter.label(&normal_done);
    }

    emitter.label(&end_label);
}

pub(super) fn emit_branch_through_finally(emitter: &mut Emitter, ctx: &Context, target_label: &str) {
    emit_set_pending_action(emitter, ctx, PENDING_BRANCH, Some(target_label), false);
    let finally_entry = &ctx
        .finally_stack
        .last()
        .expect("codegen bug: pending branch requested without an active finally")
        .entry_label;
    emitter.instruction(&format!("b {}", finally_entry));                       // transfer control to the innermost finally before branching onward
}

pub(super) fn emit_return_through_finally(emitter: &mut Emitter, ctx: &Context) {
    emit_set_pending_action(
        emitter,
        ctx,
        PENDING_RETURN,
        ctx.return_label.as_deref(),
        true,
    );
    let finally_entry = &ctx
        .finally_stack
        .last()
        .expect("codegen bug: pending return requested without an active finally")
        .entry_label;
    emitter.instruction(&format!("b {}", finally_entry));                       // transfer control to the innermost finally before returning
}

fn emit_set_pending_action(
    emitter: &mut Emitter,
    ctx: &Context,
    action: u64,
    target_label: Option<&str>,
    preserve_return: bool,
) {
    let action_offset = ctx
        .pending_action_offset
        .expect("codegen bug: missing pending-action slot");
    let target_offset = ctx
        .pending_target_offset
        .expect("codegen bug: missing pending-target slot");

    emitter.instruction(&format!("mov x10, #{}", action));                      // materialize the pending control-flow action kind
    abi::store_at_offset(emitter, "x10", action_offset);                          // record the pending action kind for finally dispatch
    if let Some(label) = target_label {
        emit_label_address(emitter, label, "x10");
        abi::store_at_offset(emitter, "x10", target_offset);                      // record the post-finally branch target address
    }
    if preserve_return {
        let return_offset = ctx
            .pending_return_value_offset
            .expect("codegen bug: missing pending return spill slot");
        abi::emit_preserve_return_value(emitter, &ctx.return_type, return_offset);
    }
}

fn emit_finally_dispatch(
    emitter: &mut Emitter,
    ctx: &Context,
    normal_resume_label: &str,
    dispatch_return: &str,
    dispatch_branch: &str,
    dispatch_rethrow: &str,
) {
    let action_offset = ctx
        .pending_action_offset
        .expect("codegen bug: missing pending-action slot");
    let target_offset = ctx
        .pending_target_offset
        .expect("codegen bug: missing pending-target slot");
    let parent_finally = ctx.finally_stack.last().map(|info| info.entry_label.clone());

    abi::load_at_offset(emitter, "x10", action_offset);                           // reload the pending control-flow action after finally body execution
    emitter.instruction(&format!("cbz x10, {}", normal_resume_label));          // ordinary finally completion falls through to the local continuation
    if let Some(parent_label) = parent_finally {
        emitter.instruction(&format!("b {}", parent_label));                    // outer finally blocks must observe the same pending action first
        return;
    }

    emitter.instruction("cmp x10, #1");                                         // is the pending action a return?
    emitter.instruction(&format!("b.eq {}", dispatch_return));                  // restore the return value then continue to the return target
    emitter.instruction("cmp x10, #2");                                         // is the pending action a branch (break/continue)?
    emitter.instruction(&format!("b.eq {}", dispatch_branch));                  // jump to the recorded target after finally
    emitter.instruction("cmp x10, #3");                                         // is the pending action an exception rethrow?
    emitter.instruction(&format!("b.eq {}", dispatch_rethrow));                 // resume propagating the current exception after finally
    emitter.instruction(&format!("b {}", normal_resume_label));                 // unknown action kinds fall back to ordinary completion

    emitter.label(&dispatch_return);
    restore_pending_return_value(emitter, ctx);
    abi::store_at_offset(emitter, "xzr", action_offset);                           // clear the pending action before leaving finally
    abi::load_at_offset(emitter, "x10", target_offset);                            // reload the recorded return target address
    emitter.instruction("br x10");                                              // branch indirectly to the function epilogue

    emitter.label(&dispatch_branch);
    abi::store_at_offset(emitter, "xzr", action_offset);                           // clear the pending action before leaving finally
    abi::load_at_offset(emitter, "x10", target_offset);                            // reload the recorded branch target address
    emitter.instruction("br x10");                                              // branch indirectly to the loop target after finally

    emitter.label(&dispatch_rethrow);
    abi::store_at_offset(emitter, "xzr", action_offset);                           // clear the pending action before resuming exception propagation
    emitter.instruction("bl __rt_rethrow_current");                             // keep unwinding the current exception after finally
}

fn restore_pending_return_value(emitter: &mut Emitter, ctx: &Context) {
    let return_offset = ctx
        .pending_return_value_offset
        .expect("codegen bug: missing pending return spill slot");
    abi::emit_restore_return_value(emitter, &ctx.return_type, return_offset);
}

fn emit_try_handler_push(emitter: &mut Emitter, ctx: &Context, handler_offset: usize) {
    let activation_prev_offset = ctx
        .activation_prev_offset
        .expect("codegen bug: missing activation prev slot");

    emitter.comment("push exception handler");
    emitter.adrp("x9", "_exc_handler_top");                      // load page of the exception-handler stack top
    emitter.add_lo12("x9", "x9", "_exc_handler_top");                // resolve the exception-handler stack top address
    emitter.instruction("ldr x10, [x9]");                                       // load the previous exception handler pointer
    abi::store_at_offset(emitter, "x10", handler_offset);                          // save the previous handler pointer in this try slot
    emitter.instruction(&format!("sub x10, x29, #{}", activation_prev_offset)); // x10 = address of the current activation record
    abi::store_at_offset(emitter, "x10", handler_offset - 8);                      // remember which activation frame should survive this catch
    emitter.instruction(&format!("sub x10, x29, #{}", handler_offset));         // x10 = address of this try slot's handler header
    emitter.adrp("x9", "_exc_handler_top");                      // reload page of the exception-handler stack top after stack-slot stores may clobber x9
    emitter.add_lo12("x9", "x9", "_exc_handler_top");                // resolve the exception-handler stack top address again
    emitter.instruction("str x10, [x9]");                                       // publish this handler as the current exception target
}

fn emit_try_handler_pop(emitter: &mut Emitter, handler_offset: usize) {
    emitter.comment("pop exception handler");
    emitter.adrp("x9", "_exc_handler_top");                      // load page of the exception-handler stack top
    emitter.add_lo12("x9", "x9", "_exc_handler_top");                // resolve the exception-handler stack top address
    abi::load_at_offset(emitter, "x10", handler_offset);                           // reload the previous handler pointer from this try slot
    emitter.adrp("x9", "_exc_handler_top");                      // reload page of the exception-handler stack top after the load helper may clobber x9
    emitter.add_lo12("x9", "x9", "_exc_handler_top");                // resolve the exception-handler stack top address again
    emitter.instruction("str x10, [x9]");                                       // restore the previous exception handler pointer
}

fn emit_handler_jmpbuf_address(emitter: &mut Emitter, handler_offset: usize, dest_reg: &str) {
    emitter.instruction(&format!("sub {}, x29, #{}", dest_reg, handler_offset - 16)); //compute the jmp_buf base address inside this try slot
}

fn bind_catch_variable(catch_clause: &CatchClause, emitter: &mut Emitter, ctx: &Context) {
    let Some(variable) = &catch_clause.variable else {
        return;
    };
    let var = ctx
        .variables
        .get(variable)
        .expect("codegen bug: catch variable was not pre-allocated");

    emitter.comment(&format!("bind catch ${}", variable));
    if matches!(var.ty, PhpType::Str) {
        abi::load_at_offset(emitter, "x0", var.stack_offset);                   // load the previous string pointer before overwriting the catch variable
        emitter.instruction("bl __rt_heap_free_safe");                          // release the previous owned string value in the catch slot
    } else if var.ty.is_refcounted() {
        abi::load_at_offset(emitter, "x0", var.stack_offset);                   // load the previous heap-backed catch-slot value before overwriting it
        abi::emit_decref_if_refcounted(emitter, &var.ty);                       // release the previous owned heap value in the catch slot
    }
    emitter.adrp("x9", "_exc_value");                            // load page of the current exception slot
    emitter.add_lo12("x9", "x9", "_exc_value");                      // resolve the current exception slot address
    emitter.instruction("ldr x0, [x9]");                                        // load the current exception object pointer for the matching catch
    emitter.instruction("str xzr, [x9]");                                       // clear the global current exception slot now that catch owns it
    abi::emit_store(emitter, &var.ty, var.stack_offset);                        // move the caught exception into the catch variable slot
}

fn emit_label_address(emitter: &mut Emitter, label: &str, dest_reg: &str) {
    emitter.adrp(dest_reg, &format!("{}", label));                               // load page of the local control-flow target label
    emitter.add_lo12(dest_reg, dest_reg, &format!("{}", label));                // resolve the local control-flow target address
}

fn resolve_catch_match_target(ctx: &Context, raw_name: &str) -> (u64, u64) {
    let resolved_name = match raw_name {
        "self" => ctx.current_class.as_deref().unwrap_or(raw_name),
        "parent" => ctx
            .current_class
            .as_ref()
            .and_then(|class_name| ctx.classes.get(class_name))
            .and_then(|class_info| class_info.parent.as_deref())
            .unwrap_or(raw_name),
        _ => raw_name,
    };
    if let Some(class_info) = ctx.classes.get(resolved_name) {
        (class_info.class_id, 0)
    } else if let Some(interface_info) = ctx.interfaces.get(resolved_name) {
        (interface_info.interface_id, 1)
    } else {
        panic!(
            "codegen bug: unresolved catch target after type checking: {}",
            resolved_name
        )
    }
}
