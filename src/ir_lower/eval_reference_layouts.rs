//! Purpose:
//! Refuses materialized eval scopes that could invalidate a native typed-array reference view.
//!
//! Called from:
//! - Eval call lowering before applying the materialized scope's local type barrier.
//!
//! Key details:
//! - Opaque source can access every visible alias; literal scope writes stay name-precise.
//! - Already boxed whole-local aliases keep their existing supported path.

use std::collections::BTreeSet;
use super::{context::LoweringContext, diagnostics};
use crate::{span::Span, types::PhpType};

/// Rejects raw typed-array aliases whose element layout eval cannot change behind native views.
pub(super) fn refuse_unboxed_array_aliases(
    ctx: &LoweringContext<'_, '_>, span: Span, writes: Option<&BTreeSet<String>>,
) {
    let mut locals = ctx.local_slots.iter().collect::<Vec<_>>();
    locals.sort_by(|(left, _), (right, _)| left.cmp(right));
    for (name, slot) in locals {
        if crate::names::is_generated_local_name(name)
            || writes.is_some_and(|writes| !writes.contains(name))
            || !ctx.is_ref_bound_local(name)
            || !(contains_unboxed_array(&ctx.local_type(name))
                || contains_unboxed_array(&ctx.builder.local_php_type(*slot)))
        {
            continue;
        }
        diagnostics::refuse(span, &format!(
            "Unsupported eval: native reference ${name} has unboxed array elements that cannot be normalized while aliases share its storage"
        ));
    }
}

/// Finds an indexed-array representation whose slots are narrower than boxed Mixed elements.
fn contains_unboxed_array(ty: &PhpType) -> bool {
    match ty {
        PhpType::Array(element) => element.codegen_repr() != PhpType::Mixed,
        PhpType::AssocArray { value, .. } => contains_unboxed_array(value),
        PhpType::Union(members) => members.iter().any(contains_unboxed_array),
        _ => false,
    }
}
