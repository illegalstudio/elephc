//! Purpose:
//! Lowers the checker's maybe-undefined locals (`CheckResult::maybe_undefined`): a hidden
//! "defined" flag per such local, cleared at frame entry, set by every store and cleared by
//! `unset`, and each recorded read answered as `flag ? value : null`.
//!
//! Called from:
//! - `crate::ir_lower::program` (installs the program's decisions for this lowering run),
//! - `crate::ir_lower::function` (frame entry), `LoweringContext` (stores), `expr` (reads),
//!   `expr::unset` (`unset`).
//!
//! Key details:
//! - The decisions live in a thread-local for the duration of one `program::lower` run, like
//!   `diagnostics`, so they need not be threaded through every function-lowering entry point.
//! - Only flagged locals pay anything; every other local keeps its native storage and reads.
//! - A plain read of an undefined local warns `Undefined variable $x` through `trigger_error`
//!   (so `@` and error handlers apply) and yields null; `isset`/`??`/`empty` probe silently.

use std::cell::RefCell;

use crate::ir_lower::context::LoweringContext;
use crate::span::Span;
use crate::types::{MaybeUndefinedLocals, PhpType};

thread_local! {
    static DECISIONS: RefCell<MaybeUndefinedLocals> = RefCell::new(MaybeUndefinedLocals::default());
}

/// Installs the checker's decisions for the lowering run on this thread.
pub(crate) fn install(decisions: &MaybeUndefinedLocals) {
    DECISIONS.with(|cell| *cell.borrow_mut() = decisions.clone());
}

/// Returns whether the read of `name` at `span` is flagged, and whether it warns.
pub(crate) fn read_decision(span: Span, name: &str) -> Option<bool> {
    DECISIONS.with(|cell| cell.borrow().reads.get(&(span, name.to_string())).copied())
}

/// Declares and clears the defined-flag of every flagged local of the current scope.
pub(crate) fn init_frame_flags(ctx: &mut LoweringContext<'_, '_>) {
    let mut names = DECISIONS.with(|cell| {
        cell.borrow()
            .by_scope
            .get(&ctx.loop_storage_scope)
            .map(|names| names.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default()
    });
    names.sort();
    for name in names {
        let flag = crate::names::generated_local_name(&format!("__elephc_defined_{}", name));
        // An arrow function receives the flag of a possibly undefined capture with it.
        if ctx
            .local_slots
            .get(&flag)
            .is_some_and(|slot| ctx.slot_is_initialized(*slot))
        {
            ctx.maybe_undefined_flags.insert(name, flag);
            continue;
        }
        ctx.declare_local(&flag, PhpType::Bool);
        // A local already bound on entry (a parameter, a capture) is defined.
        let defined = ctx
            .local_slots
            .get(&name)
            .is_some_and(|slot| ctx.slot_is_initialized(*slot));
        let value = crate::ir_lower::expr::emit_bool_literal(ctx, defined, None);
        ctx.store_local(&flag, value, PhpType::Bool, None);
        ctx.maybe_undefined_flags.insert(name, flag);
    }
}

/// Sets or clears `name`'s defined-flag, when it has one.
pub(crate) fn set_flag(ctx: &mut LoweringContext<'_, '_>, name: &str, defined: bool, span: Option<Span>) {
    let Some(flag) = ctx.maybe_undefined_flags.get(name).cloned() else {
        return;
    };
    let value = crate::ir_lower::expr::emit_bool_literal(ctx, defined, span);
    ctx.store_local(&flag, value, PhpType::Bool, span);
}

