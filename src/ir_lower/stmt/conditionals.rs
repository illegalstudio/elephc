//! Purpose:
//! If-chain lowering and loop-entry storage contracts.
//!
//! Called from:
//! - `crate::ir_lower::stmt`.
//!
//! Key details:
//! - Preserves statement ordering, CFG shape, EIR effects, and ownership contracts.

use super::*;
use std::collections::HashMap;

use crate::ir_lower::context::StaticCallableBinding;
use crate::types::TypeEnv;

/// One reachable arm of an `if` chain together with its deferred merge edge.
struct IfArmExit {
    /// Empty block filled after every sibling arm has been lowered.
    tail: BlockId,
    /// Flow-sensitive local types at the end of this arm.
    types: TypeEnv,
    /// Definitely-initialized slots at the end of this arm.
    initialized: HashSet<LocalSlotId>,
    /// Compile-time callable targets that remain valid at the end of this arm.
    static_callables: HashMap<String, StaticCallableBinding>,
}

/// Lowers an `if` / `elseif` / `else` chain and joins all reachable arm types once.
pub(super) fn lower_if(
    ctx: &mut LoweringContext<'_, '_>,
    condition: &Expr,
    then_body: &[Stmt],
    elseif_clauses: &[(Expr, Vec<Stmt>)],
    else_body: Option<&[Stmt]>,
    span: Span,
) {
    let merge = ctx.builder.create_named_block("if.merge", Vec::new());
    let mut arms = Vec::new();
    let merge_reachable = lower_if_chain(
        ctx,
        condition,
        then_body,
        elseif_clauses,
        else_body,
        merge,
        &mut arms,
        span,
    );
    finish_if_type_join(ctx, arms, merge, span);
    ctx.builder.position_at_end(merge);
    if !merge_reachable {
        ctx.builder.terminate(Terminator::Unreachable);
    }
    let joined_callables = ctx.static_callable_locals_snapshot();
    ctx.clear_static_callable_locals();
    ctx.restore_static_callable_locals(joined_callables);
}

/// Recursively emits one condition node and records every reachable arm against one shared merge.
#[allow(clippy::too_many_arguments)]
fn lower_if_chain(
    ctx: &mut LoweringContext<'_, '_>,
    condition: &Expr,
    then_body: &[Stmt],
    elseif_clauses: &[(Expr, Vec<Stmt>)],
    else_body: Option<&[Stmt]>,
    merge: BlockId,
    arms: &mut Vec<IfArmExit>,
    span: Span,
) -> bool {
    let cond_value = lower_expr(ctx, condition);
    let cond_value = ctx.truthy_consuming(cond_value, Some(condition.span));
    let split_initialized = ctx.initialized_slots_snapshot();
    let split_types = ctx.local_types_snapshot();
    let split_static_callables = ctx.static_callable_locals_snapshot();
    // Interior-alias markers are a MAY fact, so the arms are unioned rather than sequenced.
    // Without this, `if (..) { $r = &$a[0]; } else { $r = &$o->p; }` would take whichever arm
    // was lowered last as the answer for both.
    let split_borrowed_refs = ctx.borrowed_element_ref_locals_snapshot();
    let then_block = ctx.builder.create_named_block("if.then", Vec::new());
    let else_block = ctx.builder.create_named_block("if.else", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: cond_value.value,
        then_target: then_block,
        then_args: Vec::new(),
        else_target: else_block,
        else_args: Vec::new(),
    });

    ctx.builder.position_at_end(then_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    ctx.restore_local_types(split_types.clone());
    ctx.restore_static_callable_locals(split_static_callables.clone());
    lower_block(ctx, then_body);
    let then_initialized = ctx.initialized_slots_snapshot();
    let mut merge_reachable = false;
    let then_reachable = !ctx.builder.insertion_block_is_terminated();
    if then_reachable {
        merge_reachable = true;
        record_if_arm_exit(ctx, arms);
    }

    let then_borrowed_refs = ctx.borrowed_element_ref_locals_snapshot();
    ctx.builder.position_at_end(else_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    ctx.restore_local_types(split_types);
    ctx.restore_static_callable_locals(split_static_callables);
    ctx.restore_borrowed_element_ref_locals(split_borrowed_refs);
    let else_reachable =
        if let Some(((next_condition, next_body), rest)) = elseif_clauses.split_first() {
            lower_if_chain(
                ctx,
                next_condition,
                next_body,
                rest,
                else_body,
                merge,
                arms,
                span,
            )
        } else if let Some(else_body) = else_body {
            lower_block(ctx, else_body);
            if !ctx.builder.insertion_block_is_terminated() {
                record_if_arm_exit(ctx, arms);
                true
            } else {
                false
            }
        } else {
            lower_noop(ctx, span);
            if !ctx.builder.insertion_block_is_terminated() {
                record_if_arm_exit(ctx, arms);
                true
            } else {
                false
            }
        };
    merge_reachable |= else_reachable;
    if !else_reachable {
        ctx.restore_borrowed_element_ref_locals(HashSet::new());
    }
    if then_reachable {
        ctx.merge_borrowed_element_ref_locals(&then_borrowed_refs);
    }
    let else_initialized = ctx.initialized_slots_snapshot();
    ctx.restore_initialized_slots(merge_initialized_slots(
        &split_initialized,
        then_initialized,
        then_reachable,
        else_initialized,
        else_reachable,
    ));
    merge_reachable
}

/// Defers one reachable arm's merge edge so representation conversions can be inserted later.
fn record_if_arm_exit(ctx: &mut LoweringContext<'_, '_>, arms: &mut Vec<IfArmExit>) {
    let tail = ctx.builder.create_named_block("if.arm", Vec::new());
    ctx.builder.terminate(Terminator::Br {
        target: tail,
        args: Vec::new(),
    });
    arms.push(IfArmExit {
        tail,
        types: ctx.local_types_snapshot(),
        initialized: ctx.initialized_slots_snapshot(),
        static_callables: ctx.static_callable_locals_snapshot(),
    });
}

/// Reconciles flow-sensitive types and indexed-array layouts on all incoming merge edges.
fn finish_if_type_join(
    ctx: &mut LoweringContext<'_, '_>,
    arms: Vec<IfArmExit>,
    merge: BlockId,
    span: Span,
) {
    if arms.len() < 2 {
        if let Some(arm) = arms.first() {
            ctx.restore_local_types(arm.types.clone());
            ctx.restore_static_callable_locals(arm.static_callables.clone());
        } else {
            ctx.restore_static_callable_locals(HashMap::new());
        }
        for arm in &arms {
            ctx.builder.position_at_end(arm.tail);
            ctx.builder.terminate(Terminator::Br {
                target: merge,
                args: Vec::new(),
            });
        }
        return;
    }

    let (joined, edge_boxed) = join_arm_types(ctx, &arms);
    let joined_callables = join_arm_static_callables(&arms);
    let saved_types = ctx.local_types_snapshot();
    for arm in &arms {
        ctx.restore_local_types(arm.types.clone());
        let hash_conversions = arm_hash_conversions(arm, &joined);
        let conversions = arm_conversions(arm, &joined);
        let mixed_conversions = arm_mixed_conversions(arm, &edge_boxed);
        ctx.builder.position_at_end(arm.tail);
        widen_arm_containers_to_hash(ctx, &hash_conversions, span);
        widen_indexed_arrays_to_mixed(ctx, &conversions, span);
        box_arm_locals_as_mixed(ctx, &mixed_conversions, span);
        ctx.builder.terminate(Terminator::Br {
            target: merge,
            args: Vec::new(),
        });
    }
    ctx.restore_local_types(saved_types);
    for (name, ty) in joined {
        if matches!(ty.codegen_repr(), PhpType::AssocArray { .. }) {
            ctx.set_retyped_container_local_type(&name, ty);
        } else {
            ctx.set_local_type(&name, ty);
        }
    }
    ctx.restore_static_callable_locals(joined_callables);
}

/// Intersects static callable facts across every reachable arm of an `if` join.
fn join_arm_static_callables(
    arms: &[IfArmExit],
) -> HashMap<String, StaticCallableBinding> {
    let Some(first) = arms.first() else {
        return HashMap::new();
    };
    first
        .static_callables
        .iter()
        .filter(|(name, target)| {
            arms.iter().skip(1).all(|arm| {
                arm.static_callables.get(name.as_str()) == Some(*target)
            })
        })
        .map(|(name, target)| (name.clone(), target.clone()))
        .collect()
}

/// Computes the common post-merge type facts that every reachable arm can represent safely.
///
/// Returns the joined facts together with the locals whose arms must each box their value on the
/// merge edge (see [`scalar_divergence_join`]): those are joined to `Mixed` while their frame slot
/// is not yet boxed storage, so the join alone would not make the slot hold a `Mixed` cell.
fn join_arm_types(
    ctx: &LoweringContext<'_, '_>,
    arms: &[IfArmExit],
) -> (TypeEnv, HashSet<String>) {
    let Some(first) = arms.first() else {
        return (TypeEnv::new(), HashSet::new());
    };
    let mut names = first.types.keys().cloned().collect::<Vec<_>>();
    names.sort();

    let mut joined = TypeEnv::new();
    let mut edge_boxed = HashSet::new();
    'names: for name in names {
        let mut arm_types = Vec::with_capacity(arms.len());
        for arm in arms {
            let Some(arm_type) = arm.types.get(&name) else {
                continue 'names;
            };
            arm_types.push(arm_type.codegen_repr());
        }
        if arm_types.windows(2).all(|pair| pair[0] == pair[1]) {
            continue;
        }
        if arm_types.iter().any(|ty| *ty == PhpType::Mixed) {
            joined.insert(name, PhpType::Mixed);
            continue;
        }

        if arm_types
            .iter()
            .all(|ty| matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }))
            && arm_types
                .iter()
                .any(|ty| matches!(ty, PhpType::AssocArray { .. }))
        {
            if !arms
                .iter()
                .all(|arm| local_slot_is_convertible(ctx, &name, &arm.initialized))
            {
                continue;
            }
            joined.insert(
                name,
                PhpType::AssocArray {
                    key: Box::new(PhpType::Int),
                    value: Box::new(PhpType::Mixed),
                },
            );
            continue;
        }

        if !arm_types.iter().all(|ty| matches!(ty, PhpType::Array(_))) {
            match scalar_divergence_join(ctx, &name, arms, &arm_types) {
                ScalarDivergenceJoin::Keep => {}
                ScalarDivergenceJoin::Boxed => {
                    joined.insert(name, PhpType::Mixed);
                }
                ScalarDivergenceJoin::BoxOnEdges => {
                    joined.insert(name.clone(), PhpType::Mixed);
                    edge_boxed.insert(name);
                }
            }
            continue;
        }
        if !arms
            .iter()
            .all(|arm| local_slot_is_convertible(ctx, &name, &arm.initialized))
        {
            continue;
        }
        joined.insert(name, PhpType::Array(Box::new(PhpType::Mixed)));
    }
    (joined, edge_boxed)
}

/// How an `if` merge reconciles a local whose arms disagree on a non-container representation.
enum ScalarDivergenceJoin {
    /// Leave the merge with the last lowered arm's fact, as before this join existed.
    Keep,
    /// Join to `Mixed`: the frame slot is already boxed storage, so every arm's value is a cell.
    Boxed,
    /// Join to `Mixed` and box each arm's value on its merge edge first.
    BoxOnEdges,
}

/// Decides the merge of a local whose arms end with different, not-all-array representations.
///
/// `$m = null; if ($c) { $m = "s"; } return $m;` ends one arm with `$m` typed `string` and the
/// other with it typed `null`. The slot the two share was widened by the arm's store, so it can
/// hold both, but the logical type after the merge used to be whichever arm was lowered LAST —
/// here the `null` fall-through — and every read below the `if` then loaded the slot as `null`
/// and returned `NULL` for the string (issue #771). Named functions only escaped it because DCE's
/// tail-sinking copies a trailing `return $m;` into both arms; a closure body, the top-level
/// program, or any code that does not end the function right after the `if` read the stale fact.
///
/// The only type every arm can be read back through is `Mixed`, which is also what the checker's
/// own union for the name lowers to (`PhpType::codegen_repr`). When the slot is already boxed
/// storage — the usual case, since `widened_local_storage_type` widens `string`-over-`null`,
/// `float`-over-`int` and the like to `Mixed` — each arm's value already sits in a cell and only
/// the fact changes. The storage rules that do NOT widen to `Mixed` are the lossy ones for this
/// purpose: `int`/`bool`/`null` share one scalar word, and a nullable pointer slot holds `null` as
/// a zero pointer, so neither can tell the arms apart after the merge. Those are boxed on each
/// merge edge, which widens the slot to `Mixed` through the ordinary retaining store.
///
/// Arms that agree on the representation KIND (two object classes, say) keep the previous
/// behaviour: their storage is one shape and their difference is a class fact, not a value loss.
/// So do locals whose storage is not an ordinary frame slot (reference-bound, program-global,
/// static) and locals some arm leaves uninitialized, which have no single slot value to box.
fn scalar_divergence_join(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    arms: &[IfArmExit],
    arm_types: &[PhpType],
) -> ScalarDivergenceJoin {
    let Some(first) = arm_types.first() else {
        return ScalarDivergenceJoin::Keep;
    };
    if arm_types
        .iter()
        .all(|ty| std::mem::discriminant(ty) == std::mem::discriminant(first))
    {
        return ScalarDivergenceJoin::Keep;
    }
    if arm_types
        .iter()
        .all(|ty| matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }))
    {
        return ScalarDivergenceJoin::Keep;
    }
    let is_plain_frame_local = matches!(
        ctx.local_kinds.get(name).copied().unwrap_or(LocalKind::PhpLocal),
        LocalKind::PhpLocal
    ) && !ctx.is_ref_bound_local(name)
        && !ctx.local_uses_global_storage(name);
    let Some(slot) = ctx.local_slots.get(name).copied() else {
        return ScalarDivergenceJoin::Keep;
    };
    if !is_plain_frame_local || !arms.iter().all(|arm| arm.initialized.contains(&slot)) {
        return ScalarDivergenceJoin::Keep;
    }
    if ctx.builder.local_php_type(slot).codegen_repr() == PhpType::Mixed {
        ScalarDivergenceJoin::Boxed
    } else {
        ScalarDivergenceJoin::BoxOnEdges
    }
}

/// Returns the edge-boxed locals this arm still holds unboxed, in a deterministic order.
fn arm_mixed_conversions(arm: &IfArmExit, edge_boxed: &HashSet<String>) -> Vec<String> {
    let mut names = edge_boxed
        .iter()
        .filter(|name| {
            arm.types
                .get(name.as_str())
                .is_some_and(|ty| ty.codegen_repr() != PhpType::Mixed)
        })
        .cloned()
        .collect::<Vec<_>>();
    names.sort();
    names
}

/// Re-stores one arm's scalar-divergent locals as boxed `Mixed` cells before the merge.
///
/// The load reads the arm's own view of the slot, the box takes its own reference or copy of
/// the payload, and the retaining store widens the slot to `Mixed` and retires the previous
/// occupant — the same materialization `apply_loop_storage_contracts` uses for a `Mixed`
/// loop contract.
fn box_arm_locals_as_mixed(ctx: &mut LoweringContext<'_, '_>, names: &[String], span: Span) {
    for name in names {
        let source = ctx.load_local(name, Some(span));
        let boxed = ctx.box_value_as_mixed(source, PhpType::Mixed, Some(span));
        ctx.store_local(name, boxed, PhpType::Mixed, Some(span));
    }
}

/// Returns indexed-array locals whose current arm needs boxing before entering the merge.
fn arm_conversions(arm: &IfArmExit, joined: &TypeEnv) -> Vec<String> {
    let mut names = joined
        .keys()
        .filter(|name| {
            if !matches!(joined.get(name.as_str()).map(PhpType::codegen_repr), Some(PhpType::Array(_))) {
                return false;
            }
            matches!(
                arm.types.get(name.as_str()).map(PhpType::codegen_repr),
                Some(PhpType::Array(element)) if element.codegen_repr() != PhpType::Mixed
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    names.sort();
    names
}

/// Returns array-like locals whose arm edge must adopt the joined associative representation.
fn arm_hash_conversions(arm: &IfArmExit, joined: &TypeEnv) -> Vec<String> {
    let mut names = joined
        .iter()
        .filter(|(name, joined_ty)| {
            matches!(joined_ty.codegen_repr(), PhpType::AssocArray { .. })
                && arm
                    .types
                    .get(name.as_str())
                    .is_some_and(|arm_ty| arm_ty.codegen_repr() != joined_ty.codegen_repr())
        })
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    names.sort();
    names
}

/// Converts one arm's array-like locals to boxed-value hash storage before the merge.
fn widen_arm_containers_to_hash(
    ctx: &mut LoweringContext<'_, '_>,
    names: &[String],
    span: Span,
) {
    let target = PhpType::AssocArray {
        key: Box::new(PhpType::Int),
        value: Box::new(PhpType::Mixed),
    };
    for name in names {
        let source = ctx.load_local(name, Some(span));
        let op = match ctx.local_type(name).codegen_repr() {
            PhpType::Array(_) => Op::ArrayToHash,
            PhpType::AssocArray { .. } => Op::HashToMixed,
            _ => continue,
        };
        let converted = ctx.emit_value(
            op,
            vec![source.value],
            None,
            target.clone(),
            op.default_effects(),
            Some(span),
        );
        if op == Op::ArrayToHash {
            ctx.store_retyped_container_local(name, converted, target.clone(), Some(span));
        } else {
            ctx.store_mutated_local(name, converted, target.clone(), Some(span));
        }
    }
}

/// Returns whether one arm can safely convert the named local's array storage in place.
fn local_slot_is_convertible(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    initialized: &HashSet<LocalSlotId>,
) -> bool {
    repr_fixpoint::local_slot_kind_is_convertible(ctx, name)
        && ctx
            .local_slots
            .get(name)
            .is_some_and(|slot| initialized.contains(slot))
}

/// Boxes indexed-array elements on an arm edge so all paths agree at the merge.
fn widen_indexed_arrays_to_mixed(ctx: &mut LoweringContext<'_, '_>, names: &[String], span: Span) {
    let mixed_array_ty = PhpType::Array(Box::new(PhpType::Mixed));
    for name in names {
        let array = ctx.load_local(name, Some(span));
        let converted = ctx.emit_value(
            Op::ArrayToMixed,
            vec![array.value],
            None,
            mixed_array_ty.clone(),
            Op::ArrayToMixed.default_effects(),
            Some(span),
        );
        ctx.store_mutated_local(name, converted, mixed_array_ty.clone(), Some(span));
    }
}

/// Merges definitely-initialized locals from the reachable branches of an `if`.
pub(super) fn merge_initialized_slots(
    split_initialized: &HashSet<LocalSlotId>,
    then_initialized: HashSet<LocalSlotId>,
    then_reachable: bool,
    else_initialized: HashSet<LocalSlotId>,
    else_reachable: bool,
) -> HashSet<LocalSlotId> {
    match (then_reachable, else_reachable) {
        (true, true) => then_initialized
            .intersection(&else_initialized)
            .copied()
            .collect(),
        (true, false) => then_initialized,
        (false, true) => else_initialized,
        (false, false) => split_initialized.clone(),
    }
}

/// Lowers a residual `ifdef`; normally the conditional pass removes these first.
pub(super) fn lower_ifdef(
    ctx: &mut LoweringContext<'_, '_>,
    _symbol: &str,
    then_body: &[Stmt],
    else_body: Option<&[Stmt]>,
    _span: Span,
) {
    if !then_body.is_empty() {
        lower_block(ctx, then_body);
    } else if let Some(else_body) = else_body {
        lower_block(ctx, else_body);
    }
    ctx.clear_static_callable_locals();
}

/// Materializes the checker-recorded storage contract before entering a loop.
///
/// Indexed and associative arrays are promoted in place so existing elements use boxed payload
/// cells. A whole-value `Mixed` contract uses the ordinary retaining store, allowing loop-carried
/// container-kind changes to share the same fixed frame representation.
pub(super) fn apply_loop_storage_contracts(
    ctx: &mut LoweringContext<'_, '_>,
    loop_span: Span,
    span: Option<Span>,
) {
    let contracts = ctx
        .loop_storage_types
        .get(&(ctx.loop_storage_scope.clone(), loop_span))
        .cloned()
        .unwrap_or_default();
    for (name, target_ty) in contracts {
        if !ctx.local_slots.contains_key(&name) {
            continue;
        }
        let source_ty = ctx.local_type(&name).codegen_repr();
        if source_ty == target_ty.codegen_repr() {
            ctx.set_local_type(&name, target_ty);
            continue;
        }
        let source = ctx.load_local(&name, span);
        let target_repr = target_ty.codegen_repr();
        match (&source_ty, &target_repr) {
            (PhpType::Array(_), PhpType::AssocArray { .. }) => {
                let converted = ctx.emit_value(
                    Op::ArrayToHash,
                    vec![source.value],
                    None,
                    target_ty.clone(),
                    Op::ArrayToHash.default_effects(),
                    span,
                );
                ctx.store_retyped_container_local(&name, converted, target_ty, span);
            }
            (PhpType::Array(_), PhpType::Array(target_element))
                if target_element.codegen_repr() == PhpType::Mixed =>
            {
                let converted = ctx.emit_value(
                    Op::ArrayToMixed,
                    vec![source.value],
                    None,
                    target_ty.clone(),
                    Op::ArrayToMixed.default_effects(),
                    span,
                );
                ctx.store_mutated_local(&name, converted, target_ty, span);
            }
            (
                PhpType::AssocArray { .. },
                PhpType::AssocArray {
                    value: target_value,
                    ..
                },
            ) if target_value.codegen_repr() == PhpType::Mixed => {
                let converted = ctx.emit_value(
                    Op::HashToMixed,
                    vec![source.value],
                    None,
                    target_ty.clone(),
                    Op::HashToMixed.default_effects(),
                    span,
                );
                ctx.store_mutated_local(&name, converted, target_ty, span);
            }
            (_, PhpType::Mixed) => {
                let converted = ctx.box_value_as_mixed(source, target_ty.clone(), span);
                ctx.store_local(&name, converted, target_ty, span);
            }
            // The contract cannot be materialized for the representation this local actually
            // holds — the only remaining shapes disagree on container kind (an `AssocArray`
            // local against an `Array(Mixed)` contract, or a non-container local). Re-declaring
            // the type here would leave the slot holding a hash while every later read is typed
            // `array<mixed>`, so the write-site promotion reads storage it has already released.
            // Leave the local alone and let its own assignment path convert it, mirroring the
            // heap-kind guard the pre-fixed-point widening applied before promoting.
            _ => {}
        }
    }
}
