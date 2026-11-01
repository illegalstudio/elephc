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

use crate::ir::{IrHeapKind, IrType, Op};
use crate::ir_lower::context::{LoweredValue, LoweringContext};
use crate::ir_lower::expr::lower_expr;
use crate::parser::ast::{
    Expr, ExprKind, StaticReceiver, Stmt, StmtKind, NESTED_APPEND_TEMP_PREFIX,
};
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
    target: &'a Expr,
    base: BaseKind<'a>,
    index: &'a Expr,
}
/// What the nested append's outer container is.
///
/// The distinction is not cosmetic. `Op::SlotDetach` republishes the container pointer through
/// `source_load_local_slot`, which only resolves a LOCAL slot — `__rt_hash_set` may rehash the
/// table and hand back a new pointer, and on a property base that pointer would never make it back
/// into the property, leaving it stale. So a property base gets the auto-vivification (which is the
/// data-loss fix) but NOT the detach (which is the O(n^2) fix). Correctness first; the property
/// base stays quadratic until the detach can republish through a property store.
enum BaseKind<'a> {
    Local(&'a str),
    Property {
        object: &'a Expr,
        property: &'a str,
    },
    StaticProperty {
        receiver: &'a StaticReceiver,
        property: &'a str,
    },
    /// A bucket below another array access, such as `$o->items[$first][$second][]`.
    ///
    /// The parser's generic read/push/write-back desugaring cannot safely read this bucket
    /// before it has established every missing parent: that ordinary read emits undefined-key
    /// warnings. The dedicated path probes it with PHP's quiet `isset` semantics first, then
    /// creates the bucket through the existing nested-write lowering only when absent.
    Nested,
}

/// Returns the nested-append group a synthetic body encodes, or `None` to fall back to
/// today's lowering.
///
/// The scope gate is deliberately narrow: a plain variable base whose checker type is an indexed
/// or associative array. Property bases and static-property bases are gated after loading their
/// IR value. Other shapes keep the existing read/copy/write-back path.
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
                    ExprKind::Variable(base) => {
                        (name.as_str(), BaseKind::Local(base.as_str()), index.as_ref())
                    }
                    ExprKind::PropertyAccess { object, property } => (
                        name.as_str(),
                        BaseKind::Property {
                            object: object.as_ref(),
                            property: property.as_str(),
                        },
                        index.as_ref(),
                    ),
                    ExprKind::StaticPropertyAccess { receiver, property } => (
                        name.as_str(),
                        BaseKind::StaticProperty {
                            receiver,
                            property: property.as_str(),
                        },
                        index.as_ref(),
                    ),
                    ExprKind::ArrayAccess { .. } => {
                        (name.as_str(), BaseKind::Nested, index.as_ref())
                    }
                    _ => return None,
                },
                _ => return None,
            }
        }
        _ => return None,
    };
    let value_target = match &triple[0].kind {
        StmtKind::Assign { value, .. } => value,
        _ => unreachable!("nested append group was matched from an assignment"),
    };

    match &triple[1].kind {
        StmtKind::ArrayPush { array, .. } if array == temp => {}
        _ => return None,
    }

    // The write-back must target the SAME container the read came from, and hand back the very
    // temporary the push mutated. Both were built by the parser from one stabilized target, so
    // matching the shape and the names is enough — the `Synthetic` wrapper and the reserved temp
    // prefix already prove the provenance.
    match (&triple[2].kind, &base) {
        (StmtKind::ArrayAssign { array, value, .. }, BaseKind::Local(base_name))
            if array == base_name =>
        {
            match &value.kind {
                ExprKind::Variable(name) if name == temp => {}
                _ => return None,
            }
        }
        (
            StmtKind::PropertyArrayAssign {
                property, value, ..
            },
            BaseKind::Property {
                property: base_property,
                ..
            },
        ) if property == base_property => match &value.kind {
            ExprKind::Variable(name) if name == temp => {}
            _ => return None,
        },
        (
            StmtKind::StaticPropertyArrayAssign {
                property, value, ..
            },
            BaseKind::StaticProperty {
                property: base_property,
                ..
            },
        ) if property == base_property => match &value.kind {
            ExprKind::Variable(name) if name == temp => {}
            _ => return None,
        },
        (StmtKind::NestedArrayAssign { target: write_target, value }, BaseKind::Nested)
            if write_target == value_target =>
        {
            match &value.kind {
                ExprKind::Variable(name) if name == temp => {}
                _ => return None,
            }
        }
        _ => return None,
    }

    // A local container must be an indexed- or associative-array local the checker knows about;
    // anything else falls open to ordinary lowering. A property container is gated at lowering
    // time instead, on the IR type of the loaded property.
    if let BaseKind::Local(name) = base {
        let ty = ctx.local_type(name);
        let supported = matches!(&ty, PhpType::Array(_) | PhpType::AssocArray { .. })
            || matches!(&ty, PhpType::Object(class) if class.eq_ignore_ascii_case("WeakMap"));
        if !ctx.has_local_slot(name)
            || !supported
        {
            return None;
        }
    }

    Some(NestedAppendGroup {
        prefix,
        read: &triple[0],
        push: &triple[1],
        write_back: &triple[2],
        target: value_target,
        base,
        index,
    })
}

/// Fuses an array-root append whose parser-generated writeback matches the captured read.
pub(super) fn try_lower_array_root(
    ctx: &mut LoweringContext<'_, '_>, body: &[Stmt], span: Span,
) -> bool {
    if body.len() < 3 { return false; }
    let (prefix, triple) = body.split_at(body.len() - 3);
    let StmtKind::Assign { name: temp, value: target } = &triple[0].kind else { return false; };
    if !temp.starts_with(NESTED_APPEND_TEMP_PREFIX) { return false; }
    let ExprKind::ArrayAccess { array, .. } = &target.kind else { return false; };
    let StmtKind::ArrayPush { array: pushed, value } = &triple[1].kind else { return false; };
    let (stored, matches_target) = match &triple[2].kind {
        StmtKind::NestedArrayAssign { target: written, value } => (value, written == target),
        StmtKind::ArrayAssign { array: written, index, value } => (value, matches!(
            &array.kind, ExprKind::Variable(name) if name == written
        ) && matches!(&target.kind, ExprKind::ArrayAccess { index: read, .. } if read.as_ref() == index)),
        StmtKind::StaticPropertyArrayAssign { receiver, property, index, value } => (value, matches!(
            &array.kind, ExprKind::StaticPropertyAccess { receiver: read, property: name }
                if read == receiver && name == property
        ) && matches!(&target.kind, ExprKind::ArrayAccess { index: read, .. } if read.as_ref() == index)),
        _ => return false,
    };
    if pushed != temp || !matches_target
        || !matches!(&stored.kind, ExprKind::Variable(name) if name == temp)
    { return false; }
    let mut root = target;
    while let ExprKind::ArrayAccess { array, .. } = &root.kind { root = array; }
    let supported = match &root.kind {
        ExprKind::Variable(name) => ctx.has_local_slot(name) && matches!(
            ctx.local_type(name).codegen_repr(), PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed
        ),
        ExprKind::StaticPropertyAccess { .. } =>
            super::nested_array_writes::nested_target_has_static_array_root(ctx, target)
                && (matches!(array.kind, ExprKind::ArrayAccess { .. })
                    || super::nested_array_writes::static_root_supports_attached_fetch(ctx, root)),
        _ => false,
    };
    if !supported { return false; }
    for stmt in prefix { super::lower_stmt(ctx, stmt); }
    super::nested_plain_writes::lower_append(ctx, target, value, span);
    true
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
    if let (StmtKind::Assign { value: target, .. }, StmtKind::ArrayPush { value, .. }) =
        (&group.read.kind, &group.push.kind)
    {
        if crate::ir_lower::expr::lower_weak_map_append(ctx, target, value, span) { return; }
    }

    if matches!(group.base, BaseKind::Nested) {
        if !nested_target_has_safe_quiet_probe(ctx, group.target) {
            super::lower_stmt(ctx, group.read);
            super::lower_stmt(ctx, group.push);
            super::lower_stmt(ctx, group.write_back);
            return;
        }
        lower_deep_nested_append(ctx, group, span);
        return;
    }

    // Load the container and probe the key. For a local this is a `LoadLocal`; for a property it is
    // an ordinary property read, which is pure and can be replayed.
    let container = load_container(ctx, group, span);
    let container_is_hash = matches!(container.ir_type, IrType::Heap(IrHeapKind::Hash));
    let container_is_array = matches!(container.ir_type, IrType::Heap(IrHeapKind::Array));
    if !container_is_hash && !container_is_array {
        // Anything else (a `Mixed` property, an object) is out of scope: fall open to today's
        // lowering, which is what the parser's desugar already produces. Keep key evaluation in
        // that path so unsupported receivers do not evaluate it twice.
        //
        // The probe read above still produced a value, and for a `Mixed` property it is an OWNED
        // box (`prop_get` retains one). Nothing below consumes it, so it is retired here; left
        // alone it leaked one box per append (`$this->tags[$name][] = $attributes`, Symfony's
        // `Definition::addTag`).
        //
        // Only an OWNING read is retired. A static property load BORROWS the slot's box, and
        // releasing it dropped the static's own reference: after the merge a typed static `array`
        // reaches this path as a boxed php-array, and `self::$b[$k][] = $v` freed the bucket it
        // was about to append to (`count()` then read an int, siblings vanished).
        if ctx.value_is_owning_temporary(container) {
            crate::ir_lower::ownership::release_if_owned(ctx, container, Some(span));
        }
        super::lower_stmt(ctx, group.read);
        super::lower_stmt(ctx, group.push);
        super::lower_stmt(ctx, group.write_back);
        return;
    }
    let key = lower_expr(ctx, group.index);
    let key_is_dynamic = nested_append_key_is_dynamic(ctx, &key);
    let present = if container_is_hash {
        ctx.emit_value(
            Op::HashIsset,
            vec![container.value, key.value],
            None,
            PhpType::Bool,
            Op::HashIsset.default_effects(),
            Some(span),
        )
    } else if key_is_dynamic {
        lower_dynamic_array_append_probe(ctx, container.value, key.value, span)
    } else {
        ctx.emit_value(
            Op::ArrayIsset,
            vec![container.value, key.value],
            None,
            PhpType::Bool,
            Op::ArrayIsset.default_effects(),
            Some(span),
        )
    };
    // A property read yields an owned box when the property is Mixed-valued; retire it too.
    if matches!(group.base, BaseKind::Local(_)) || ctx.value_is_owning_temporary(container) {
        crate::ir_lower::ownership::release_if_owned(ctx, container, Some(span));
    }
    if ctx.value_is_owning_temporary(key) {
        crate::ir_lower::ownership::release_if_owned(ctx, key, Some(span));
    } else {
        super::release_persisted_string_operand(ctx, key, span);
    }

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
    let vivify = vivify_stmt(group, span);
    super::lower_stmt(ctx, &vivify);
    ctx.builder.terminate(crate::ir::Terminator::Br {
        target: body_block,
        args: Vec::new(),
    });

    // -- body: the slot now certainly holds an array --
    ctx.builder.position_at_end(body_block);
    ctx.restore_initialized_slots(split_initialized);
    // The bucket is about to be read and appended to in place, so it must belong to this container
    // alone. When the container is shared with a copy (`$copy = $o->items;`) or a clone, the bucket
    // cell is reachable through the other holder too and an in-place append would show there.
    // The for-write parent fetch is what a nested ELEMENT write already uses: it splits a shared
    // container, republishes it, and replaces a shared bucket cell with a private one.
    unshare_bucket_for_write(ctx, group, span);
    super::lower_stmt(ctx, group.read);

    // Hand the container slot's reference to the temporary, so the push below sees a uniquely-owned
    // bucket and mutates it in place instead of copy-on-write cloning it — O(n^2) -> O(n). This
    // cannot free the bucket: the read above already took a reference, so the count it drops is at
    // least two.
    //
    // ONLY for an unaliased local base. `Op::SlotDetach` republishes the (possibly rehashed)
    // container pointer through the local's raw storage. A local bound by `$ref =& $obj->$name`
    // is semantically a property base even though its AST shape is a local: detaching through it
    // can reinterpret a runtime-promoted hash as an indexed array before the ref-cell writeback.
    // Such aliases keep the correct (quadratic) copy-on-write path.
    if let BaseKind::Local(name) = &group.base {
        let local_ty = ctx.local_type(name).codegen_repr();
        // Mixed-slot indexed locals can be runtime-promoted hashes or carry invoker markers.
        // The raw indexed SlotDetach writer cannot republish either shape safely. Preserve
        // the ordinary read/push/write-back COW path for them, including reference variadics.
        let polymorphic_indexed = matches!(&local_ty,
            PhpType::Array(element) if element.codegen_repr() == PhpType::Mixed)
            && (ctx.is_ref_bound_local(name)
                || super::index_expr_key_type(ctx, group.index).codegen_repr() != PhpType::Int);
        // `SlotDetach`'s indexed fast path accepts only a raw integer offset. A runtime Mixed key
        // may normalize to a string and promote the array to hash storage; feeding the boxed cell
        // pointer to that helper makes it grow until the fixed heap is exhausted. Keep the normal
        // copy-on-write path for dynamic keys, which already dispatches through the mixed-key
        // runtime helpers.
        let can_detach_indexed_slot = !container_is_array || !key_is_dynamic;
        if !ctx.is_ref_bound_local(name)
            && can_detach_indexed_slot
            && !polymorphic_indexed
            && matches!(local_ty, PhpType::Array(_) | PhpType::AssocArray { .. })
        {
            detach_array_bucket(ctx, group, name, span);
        }
    }

    super::lower_stmt(ctx, group.push);
    super::lower_stmt(ctx, group.write_back);
}

/// Detaches only actual arrays, preserving a selected scalar until its append error is caught.
/// Boxed buckets retain the in-place fast path without removing strings before RHS evaluation.
fn detach_array_bucket(
    ctx: &mut LoweringContext<'_, '_>, group: &NestedAppendGroup<'_>, name: &str, span: Span,
) {
    let StmtKind::Assign { name: temp, .. } = &group.read.kind else { return; };
    let bucket_ty = ctx.local_type(temp).codegen_repr();
    let merge = if bucket_ty == PhpType::Mixed {
        let bucket = ctx.load_local(temp, Some(span));
        let is_array = ctx.emit_value(
            Op::TypePredicate, vec![bucket.value],
            Some(crate::ir::Immediate::TypePredicate(crate::ir::PhpTypePredicate::Array)),
            PhpType::Bool, Op::TypePredicate.default_effects(), Some(span),
        );
        let detach = ctx.builder.create_named_block("napp.detach", Vec::new());
        let append = ctx.builder.create_named_block("napp.append", Vec::new());
        ctx.builder.terminate(crate::ir::Terminator::CondBr {
            cond: is_array.value, then_target: detach, then_args: Vec::new(),
            else_target: append, else_args: Vec::new(),
        });
        ctx.builder.position_at_end(detach);
        Some(append)
    } else if matches!(bucket_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        None
    } else {
        return;
    };
    // Vivification can republish the container; reload it and the replayable or hoisted key.
    let container = ctx.load_local(name, Some(span));
    let key = lower_expr(ctx, group.index);
    ctx.emit_void(
        Op::SlotDetach, vec![container.value, key.value], None,
        Op::SlotDetach.default_effects(), Some(span),
    );
    if let Some(append) = merge {
        ctx.builder.terminate(crate::ir::Terminator::Br { target: append, args: Vec::new() });
        ctx.builder.position_at_end(append);
    }
}


/// Makes the bucket the append is about to mutate private to its container (see the call site).
///
/// Only the shapes the for-write parent fetch lowers concretely take part -- a property whose
/// storage is a Mixed-valued array, a local of the same kind -- and the rest keep their lowering.
fn unshare_bucket_for_write(
    ctx: &mut LoweringContext<'_, '_>,
    group: &NestedAppendGroup<'_>,
    span: Span,
) {
    let parent = match &group.base {
        BaseKind::Property { object, property } => {
            let access = Expr::new(
                ExprKind::PropertyAccess {
                    object: Box::new((*object).clone()),
                    property: (*property).to_string(),
                },
                span,
            );
            super::nested_array_writes::lower_property_parent_fetch_for_write(
                ctx,
                object,
                property,
                group.index,
                &access,
            )
        }
        BaseKind::Local(name) => {
            let mixed_elements = matches!(
                ctx.local_type(name).codegen_repr(),
                PhpType::Array(elem) if elem.codegen_repr() == PhpType::Mixed
            ) || matches!(
                ctx.local_type(name).codegen_repr(),
                PhpType::AssocArray { value, .. } if value.codegen_repr() == PhpType::Mixed
            );
            if !mixed_elements || ctx.is_ref_bound_local(name) {
                return;
            }
            let access = Expr::new(ExprKind::Variable((*name).to_string()), span);
            super::nested_array_writes::lower_local_parent_fetch_for_write(
                ctx,
                name,
                group.index,
                &access,
                false,
            )
        }
        BaseKind::StaticProperty { .. } | BaseKind::Nested => None,
    };
    if let Some(parent) = parent {
        crate::ir_lower::ownership::release_if_owned(ctx, parent, Some(span));
    }
}

/// Lowers an append whose bucket has more than one array-access parent.
///
/// A parser-generated group for `$object->items[$first][$second][] = $value` starts with a
/// normal read of `$object->items[$first][$second]`. Unlike a direct property bucket, that read
/// cannot be fused into a one-level `ArrayIsset`/`HashIsset` probe. Reuse `isset`'s quiet,
/// recursive fetch semantics instead. The parser has stabilized every effectful subexpression in
/// the target, and the guarded root is a native array slot, so the probe does not duplicate a
/// PHP-visible operation. It does not warn for legal missing write parents and preserves PHP's
/// distinction between a missing/null bucket (which must be vivified) and a scalar bucket (which
/// must retain the ordinary append error).
fn lower_deep_nested_append(
    ctx: &mut LoweringContext<'_, '_>,
    group: &NestedAppendGroup<'_>,
    span: Span,
) {
    let Some(present) = crate::ir_lower::expr::lower_synthesized_isset_for_write_target(
        ctx,
        group.target,
    )
    else {
        // The synthetic target has one ordinary positional operand, so the lazy lowering is
        // expected to accept it. Preserve the pre-existing desugar if a future AST shape makes
        // that assumption false rather than partially lowering the group.
        super::lower_stmt(ctx, group.read);
        super::lower_stmt(ctx, group.push);
        super::lower_stmt(ctx, group.write_back);
        return;
    };

    let split_initialized = ctx.initialized_slots_snapshot();
    let vivify_block = ctx
        .builder
        .create_named_block("napp.deep.vivify", Vec::new());
    let body_block = ctx.builder.create_named_block("napp.deep.body", Vec::new());
    ctx.builder.terminate(crate::ir::Terminator::CondBr {
        cond: present.value,
        then_target: body_block,
        then_args: Vec::new(),
        else_target: vivify_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(vivify_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    let empty = Expr::new(ExprKind::ArrayLiteral(Vec::new()), span);
    super::lower_nested_array_assign(ctx, group.target, &empty, span);
    ctx.builder.terminate(crate::ir::Terminator::Br {
        target: body_block,
        args: Vec::new(),
    });

    ctx.builder.position_at_end(body_block);
    ctx.restore_initialized_slots(split_initialized);
    super::lower_stmt(ctx, group.read);
    super::lower_stmt(ctx, group.push);
    super::lower_stmt(ctx, group.write_back);
}

/// Returns whether a nested target can be probed quietly without adding magic-method calls.
///
/// The direct one-parent lowering already has precise native probes for statically-known
/// containers. This broader path is intentionally narrower than PHP's entire indirect-write
/// surface: an unknown `Mixed`, `ArrayAccess`, or virtual property retains the original desugar
/// until it has a similarly exact write-context implementation.
fn nested_target_has_safe_quiet_probe(ctx: &LoweringContext<'_, '_>, target: &Expr) -> bool {
    match &target.kind {
        ExprKind::ArrayAccess { array, .. } => nested_target_has_safe_quiet_probe(ctx, array),
        ExprKind::Variable(name) => matches!(
            ctx.local_type(name).codegen_repr(),
            PhpType::Array(_) | PhpType::AssocArray { .. }
        ),
        ExprKind::PropertyAccess { object, property } => {
            crate::ir_lower::expr::property_access_is_declared_native_array(ctx, object, property)
        }
        _ => false,
    }
}

/// Returns whether a nested append key needs PHP's runtime mixed-key normalization.
fn nested_append_key_is_dynamic(
    ctx: &LoweringContext<'_, '_>,
    key: &LoweredValue,
) -> bool {
    if matches!(key.ir_type, IrType::Heap(IrHeapKind::Mixed)) {
        return true;
    }
    matches!(
        ctx.builder.value_php_type(key.value).codegen_repr(),
        PhpType::Str | PhpType::Mixed | PhpType::Union(_)
    )
}

/// Probes a packed array with a dynamic PHP key without forcing the key through integer storage.
fn lower_dynamic_array_append_probe(
    ctx: &mut LoweringContext<'_, '_>,
    array: crate::ir::ValueId,
    key: crate::ir::ValueId,
    span: Span,
) -> LoweredValue {
    let value = ctx.emit_value(
        Op::ArrayGetMixedKeySilent,
        vec![array, key],
        None,
        PhpType::Mixed,
        Op::ArrayGetMixedKeySilent.default_effects(),
        Some(span),
    );
    let is_null = ctx.emit_value(
        Op::IsNull,
        vec![value.value],
        None,
        PhpType::Bool,
        Op::IsNull.default_effects(),
        Some(span),
    );
    crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    let zero = ctx.emit_value(
        Op::ConstI64,
        Vec::new(),
        Some(crate::ir::Immediate::I64(0)),
        PhpType::Int,
        Op::ConstI64.default_effects(),
        Some(span),
    );
    ctx.emit_value(
        Op::ICmp,
        vec![is_null.value, zero.value],
        Some(crate::ir::Immediate::CmpPredicate(crate::ir::CmpPredicate::Eq)),
        PhpType::Bool,
        Op::ICmp.default_effects(),
        Some(span),
    )
}

/// Loads the nested append's outer container as a value.
fn load_container(
    ctx: &mut LoweringContext<'_, '_>,
    group: &NestedAppendGroup<'_>,
    span: Span,
) -> LoweredValue {
    match &group.base {
        BaseKind::Local(name) => ctx.load_local(name, Some(span)),
        BaseKind::Property { object, property } => {
            let access = Expr::new(
                ExprKind::PropertyAccess {
                    object: Box::new((*object).clone()),
                    property: (*property).to_string(),
                },
                span,
            );
            lower_expr(ctx, &access)
        }
        BaseKind::StaticProperty { receiver, property } => {
            let access = Expr::new(
                ExprKind::StaticPropertyAccess {
                    receiver: (*receiver).clone(),
                    property: (*property).to_string(),
                },
                span,
            );
            lower_expr(ctx, &access)
        }
        BaseKind::Nested => unreachable!("deep nested append does not load a direct container"),
    }
}

/// Builds the statement that auto-vivifies the missing inner array.
///
/// It writes an empty array into the container through the SAME write-back lowering the group's own
/// write-back uses. That is not a stylistic choice: the append temporary's checker type is the
/// container's VALUE type (typically `Mixed`), so assigning a bare `Array(Never)` literal straight
/// into it would bypass the boxing the container's storage expects. The bucket then looked fine
/// until it outgrew its initial capacity, at which point growing it read a malformed header and
/// segfaulted.
fn vivify_stmt(group: &NestedAppendGroup<'_>, span: Span) -> Stmt {
    let empty = Expr::new(ExprKind::ArrayLiteral(Vec::new()), span);
    let kind = match &group.base {
        BaseKind::Local(name) => StmtKind::ArrayAssign {
            array: (*name).to_string(),
            index: group.index.clone(),
            value: empty,
        },
        BaseKind::Property { object, property } => StmtKind::PropertyArrayAssign {
            object: Box::new((*object).clone()),
            property: (*property).to_string(),
            index: group.index.clone(),
            value: empty,
        },
        BaseKind::StaticProperty { receiver, property } => StmtKind::StaticPropertyArrayAssign {
            receiver: (*receiver).clone(),
            property: (*property).to_string(),
            index: group.index.clone(),
            value: empty,
        },
        BaseKind::Nested => unreachable!("deep nested append vivifies through NestedArrayAssign"),
    };
    Stmt::new(kind, span)
}
