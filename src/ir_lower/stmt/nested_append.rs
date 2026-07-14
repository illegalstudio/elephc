//! Purpose:
//! Recognizes the statement group a nested append (`$a[$k][] = $v`) desugars to, and lowers it
//! as a fused, auto-vivifying, in-place append instead of a read/copy/write-back.
//!
//! Called from:
//! - `crate::ir_lower::stmt::lower_stmt()`, on `StmtKind::Synthetic`.
//!
//! Key details:
//! - The parser eagerly desugars EVERY nested append, at parse time, into a
//!   read / push / write-back triple wrapped in a `StmtKind::Synthetic`
//!   (`crate::parser::stmt::assign::postfix::lower_nested_append_assignment`). Neither the
//!   checker nor IR lowering ever sees a "nested append" node. That desugar has two defects
//!   this module fixes, and it is the only place they can be fixed without a new AST node:
//!
//!   1. **It loses data.** The first push into any bucket is a *miss*: nothing auto-vivifies
//!      the missing inner array the way PHP does. On a `Mixed` bucket the miss reads back a
//!      boxed null and the append silently drops the value; on a concretely-typed bucket it
//!      reads back the missing-key sentinel. `$g = []; $g["k"][] = 1;` printed `count() == 0`.
//!   2. **It is quadratic.** The read leaves the bucket owned twice (the container slot and the
//!      temporary), so the push copy-on-write clones the whole bucket — O(length) per push,
//!      O(n^2) over a growing bucket. `Op::SlotDetach` nulls the slot between the read and the
//!      push, dropping the count back to one so the append mutates in place.
//!
//! - Recognition has two locks that PHP source cannot forge: the `StmtKind::Synthetic` wrapper
//!   (no surface syntax) and the temporary's reserved `NESTED_APPEND_TEMP_PREFIX` (not a legal
//!   PHP identifier). The prefix is what distinguishes this group from the `.=` / `+=` desugars,
//!   which emit the same statement shapes from the same lowerer.
//! - Anything it does not recognize **fails open** to `lower_block`, i.e. to today's lowering,
//!   bit for bit. That is deliberate: the scope gate below is narrow on purpose.

use crate::ir::Op;
use crate::ir_lower::context::LoweringContext;
use crate::ir_lower::expr::lower_expr;
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind, NESTED_APPEND_TEMP_PREFIX};
use crate::span::Span;
use crate::types::PhpType;

/// A `StmtKind::Synthetic` body proven to be a nested append this module can fuse.
///
/// `prefix` holds the stabilization assignments the parser hoists when the key expression is
/// not replayable (`$a[f()][] = 1` yields more than three statements), so the group is matched
/// as a *suffix*, never by a length check.
pub(super) struct NestedAppendGroup<'a> {
    prefix: &'a [Stmt],
    read: &'a Stmt,
    push: &'a Stmt,
    write_back: &'a Stmt,
    base: &'a str,
    index: &'a Expr,
}

/// Returns the nested-append group a synthetic body encodes, or `None` to fall back to
/// today's lowering.
///
/// The scope gate is deliberately narrow: a plain variable base whose checker type is an
/// associative array. Indexed bases, property bases and static-property bases keep the existing
/// read/copy/write-back path — correct, just slow (and, for a missing key, still lossy).
pub(super) fn recognize<'a>(
    ctx: &LoweringContext<'_, '_>,
    body: &'a [Stmt],
) -> Option<NestedAppendGroup<'a>> {
    if body.len() < 3 {
        return None;
    }
    let split = body.len() - 3;
    let (prefix, triple) = body.split_at(split);

    let (temp, base, index) = match &triple[0].kind {
        StmtKind::Assign { name, value } if name.starts_with(NESTED_APPEND_TEMP_PREFIX) => {
            match &value.kind {
                ExprKind::ArrayAccess { array, index } => match &array.kind {
                    ExprKind::Variable(base) => (name.as_str(), base.as_str(), index.as_ref()),
                    _ => return None,
                },
                _ => return None,
            }
        }
        _ => return None,
    };

    match &triple[1].kind {
        StmtKind::ArrayPush { array, .. } if array == temp => {}
        _ => return None,
    }

    match &triple[2].kind {
        StmtKind::ArrayAssign { array, value, .. } if array == base => match &value.kind {
            ExprKind::Variable(name) if name == temp => {}
            _ => return None,
        },
        _ => return None,
    }

    // The container must be a real associative-array local the checker knows about; anything
    // else (an indexed array, a Mixed local, an undeclared name) falls open.
    if !ctx.has_local_slot(base) || !matches!(ctx.local_type(base), PhpType::AssocArray { .. }) {
        return None;
    }

    Some(NestedAppendGroup {
        prefix,
        read: &triple[0],
        push: &triple[1],
        write_back: &triple[2],
        base,
        index,
    })
}

/// Lowers a recognized nested append: vivify if missing, then read, detach, push in place,
/// write back.
///
/// Only the *vivification* is conditional. Everything after it is the very straight-line
/// sequence the parser already emits, lowered by the ordinary statement lowerings — so the
/// fused path inherits every existing decision about element typing, Mixed boxing, and the
/// hash-versus-indexed refcount asymmetry (`__rt_hash_set` *consumes* the value it stores;
/// `__rt_array_set_refcounted` *retains* it). Re-deriving any of that by hand would leak on one
/// side and double-free on the other.
///
/// The vivification writes an empty array into the slot through the same `StmtKind::ArrayAssign`
/// lowering the write-back uses, rather than assigning `[]` straight into the append temporary.
/// That is not a stylistic choice: the temporary's checker type is the container's *value* type
/// (typically `Mixed`), so assigning a bare `Array(Never)` literal into it bypasses the boxing
/// the container's storage expects. The bucket then looked fine until it outgrew its initial
/// capacity, at which point growing it read the malformed header and segfaulted.
pub(super) fn lower(ctx: &mut LoweringContext<'_, '_>, group: &NestedAppendGroup<'_>, span: Span) {
    for stmt in group.prefix {
        super::lower_stmt(ctx, stmt);
    }

    let container = ctx.load_local(group.base, Some(span));
    let key = lower_expr(ctx, group.index);
    let present = ctx.emit_value(
        Op::HashIsset,
        vec![container.value, key.value],
        None,
        PhpType::Bool,
        Op::HashIsset.default_effects(),
        Some(span),
    );

    // Snapshot the definitely-initialized locals before the split and restore them at the head of
    // the arm, exactly as `lower_if_chain` does; the vivify arm initializes nothing new, so the
    // merge simply inherits the pre-split set.
    let split_initialized = ctx.initialized_slots_snapshot();
    let vivify_block = ctx.builder.create_named_block("napp.vivify", Vec::new());
    let body_block = ctx.builder.create_named_block("napp.body", Vec::new());
    ctx.builder.terminate(crate::ir::Terminator::CondBr {
        cond: present.value,
        then_target: body_block,
        then_args: Vec::new(),
        else_target: vivify_block,
        else_args: Vec::new(),
    });

    // -- vivify: PHP creates the missing inner array; nothing else does --
    ctx.builder.position_at_end(vivify_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    let vivify = Stmt::new(
        StmtKind::ArrayAssign {
            array: group.base.to_string(),
            index: group.index.clone(),
            value: Expr::new(ExprKind::ArrayLiteral(Vec::new()), span),
        },
        span,
    );
    super::lower_stmt(ctx, &vivify);
    ctx.builder.terminate(crate::ir::Terminator::Br {
        target: body_block,
        args: Vec::new(),
    });

    // -- body: the slot now certainly holds an array --
    ctx.builder.position_at_end(body_block);
    ctx.restore_initialized_slots(split_initialized);
    super::lower_stmt(ctx, group.read);
    // Re-load the container and key rather than reusing the values above: those were emitted in a
    // predecessor block, and the vivification may have republished a grown or copy-on-write-split
    // container pointer into the local. A `LoadLocal` is pure, and the key expression is either
    // replayable or already hoisted into a prefix temporary, so neither is evaluated twice
    // observably.
    let container = ctx.load_local(group.base, Some(span));
    let key = lower_expr(ctx, group.index);
    // Hand the slot's reference to the temporary, so the push below sees a uniquely-owned bucket
    // and mutates it in place instead of copy-on-write cloning it. This cannot free the bucket:
    // the read above already took a reference, so the count it drops is at least two.
    ctx.emit_void(
        Op::SlotDetach,
        vec![container.value, key.value],
        None,
        Op::SlotDetach.default_effects(),
        Some(span),
    );
    super::lower_stmt(ctx, group.push);
    super::lower_stmt(ctx, group.write_back);
}
