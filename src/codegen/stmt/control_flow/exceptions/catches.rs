use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::parser::ast::CatchClause;

use super::finally;
use super::handlers;
use super::PENDING_RETHROW;

pub(super) fn emit_catch_dispatch(
    catches: &[CatchClause],
    finally_label: Option<&str>,
    catch_end_label: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    if catches.is_empty() {
        if let Some(label) = finally_label {
            finally::emit_set_pending_action(emitter, ctx, PENDING_RETHROW, None, false);
            abi::emit_jump(emitter, label);                                        // defer rethrow until after finally
        } else {
            abi::emit_call_label(emitter, "__rt_rethrow_current");                 // propagate an uncaught exception to the next enclosing try
        }
        return;
    }

    for catch_clause in catches {
        let catch_label = ctx.next_label("catch_body");
        let next_catch_label = ctx.next_label("catch_next");
        for (idx, catch_type) in catch_clause.exception_types.iter().enumerate() {
            let (catch_id, catch_kind) =
                handlers::resolve_catch_match_target(ctx, catch_type.as_str());
            let mismatch_label = if idx + 1 == catch_clause.exception_types.len() {
                next_catch_label.clone()
            } else {
                ctx.next_label("catch_type_next")
            };

            abi::emit_load_symbol_to_reg(emitter, abi::int_arg_reg_name(emitter.target, 0), "_exc_value", 0);
            abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 1), catch_id as i64); // materialize the catch target id for runtime matching
            abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 2), catch_kind as i64); // tell the runtime whether this catch target is a class or interface
            abi::emit_call_label(emitter, "__rt_exception_matches");               // test whether the current exception matches this catch target
            abi::emit_branch_if_int_result_zero(emitter, &mismatch_label);         // move to the next type in this catch clause when it does not match
            abi::emit_jump(emitter, &catch_label);                                 // jump into the shared catch body once any type matches
            if idx + 1 != catch_clause.exception_types.len() {
                emitter.label(&mismatch_label);
            }
        }

        emitter.label(&catch_label);
        handlers::bind_catch_variable(catch_clause, emitter, ctx);
        for stmt in &catch_clause.body {
            super::super::super::emit_stmt(stmt, emitter, ctx, data);
        }
        if let Some(label) = finally_label {
            abi::emit_jump(emitter, label);                                        // run finally after the matching catch body completes
        } else {
            abi::emit_jump(emitter, catch_end_label);                              // leave the try/catch after the matching catch completes
        }
        emitter.label(&next_catch_label);
    }

    if let Some(label) = finally_label {
        finally::emit_set_pending_action(emitter, ctx, PENDING_RETHROW, None, false);
        abi::emit_jump(emitter, label);                                            // no catch matched, so run finally before rethrowing
    } else {
        abi::emit_call_label(emitter, "__rt_rethrow_current");                     // no catch matched and there is no finally to run first
    }
}
