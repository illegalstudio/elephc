//! Purpose:
//! Applies resolver-recorded namespace fallbacks during the second pre-check target fold.
//!
//! Called from:
//! - `fold_constants_for_target`, `fold_expr`, and first-class callable folding.
//!
//! Key details:
//! - The symbol snapshot includes autoloaded functions and surviving conditional declarations.
//! - Other folding passes do not reconsider function bindings.

use super::*;
use crate::name_resolver::FunctionFallbacks;

thread_local! {
    static ACTIVE_FALLBACKS: RefCell<Option<FunctionFallbacks>> = const { RefCell::new(None) };
}

/// Runs the second fold with the post-pruning symbol inventory, restoring nested context on exit.
pub(super) fn fold_after_pruning(program: Program) -> Program {
    ACTIVE_FALLBACKS.with(|slot| {
        let previous = slot.replace(Some(FunctionFallbacks::new(&program)));
        let folded = fold_block(program);
        slot.replace(previous);
        folded
    })
}

/// Rebinds a direct call only when the complete program has no declaration for its local name.
pub(super) fn resolve_call(name: &Name, args: &[Expr], span: Span) -> Option<Expr> {
    ACTIVE_FALLBACKS.with(|slot| slot.borrow().as_ref()?.resolve_call(name, args, span))
}

/// Rebinds first-class callable names using the same surviving declaration inventory as calls.
pub(super) fn resolve_name(name: Name) -> Name {
    ACTIVE_FALLBACKS.with(|slot| {
        slot.borrow().as_ref().and_then(|fallbacks| fallbacks.resolve_name(&name)).unwrap_or(name)
    })
}
