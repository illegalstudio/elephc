//! Purpose:
//! Rewrites `natsort()` and `natcasesort()` over a gradual array into the `uasort()` call they
//! are defined as.
//!
//! Called from:
//! - `crate::ir_lower::expr::compat_preludes`, for a call whose operand is gradual (a `mixed`
//!   value, a hash, or an array of gradual elements).
//!
//! Key details:
//! - The native sort walks an indexed array of known element layout. php's `natsort()` is a
//!   key-preserving stable sort by `strnatcmp()`, `natcasesort()` by `strnatcasecmp()`, which is
//!   exactly `uasort()` with that comparator over the elements' string values -- and `uasort()`
//!   already has a gradual path.
//! - The rewrite happens AT THE CALL SITE, on the caller's own variable. Routing through a
//!   by-reference helper function instead re-typed the caller's array: the helper converted a
//!   `array<string, string>` hash's elements to boxed values the caller still read as raw
//!   strings, so `implode()` after the sort printed empty strings.
//! - `strnatcmp()`/`strnatcasecmp()` and the gradual `uasort()` helper are preludes injected when
//!   the program names `natsort`/`natcasesort` (`crate::backend_gap_prelude::inject_if_used`).
//! - Symfony's `debug:container` sorts the service ids a private `: array` method returned; the
//!   checker refused that operand, so the command could not be compiled.
//! - Built as AST, never parsed from PHP source.

use crate::parser::ast::{CastType, Expr, ExprKind};
use crate::span::Span;
use crate::synthetic_class::{closure, e_call, e_cast, e_var, s_return};

/// Returns `uasort($array, static fn ($a, $b) => compare((string) $a, (string) $b))` for a
/// `natsort()` (`case_insensitive` false) or `natcasesort()` call on `array`.
pub(crate) fn uasort_call(array: &Expr, case_insensitive: bool, span: Span) -> Expr {
    let compare = if case_insensitive { "strnatcasecmp" } else { "strnatcmp" };
    let as_string = |name: &str| e_cast(CastType::String, e_var(name));
    let comparator = closure()
        .param_untyped("a")
        .param_untyped("b")
        .body(vec![s_return(e_call(compare, vec![as_string("a"), as_string("b")]))])
        .build();
    let mut call = e_call("uasort", vec![array.clone(), comparator]);
    call.span = span;
    if let ExprKind::FunctionCall { args, .. } = &mut call.kind {
        for arg in args.iter_mut() {
            if arg.span == Span::dummy() {
                arg.span = span;
            }
        }
    }
    call
}
