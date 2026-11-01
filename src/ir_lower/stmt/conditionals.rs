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
    if let Some(result) = crate::ir_lower::expr::statically_known_instanceof_result(ctx, condition)
    {
        let condition_value = lower_expr(ctx, condition);
        let _ = ctx.truthy_consuming(condition_value, Some(condition.span));
        if result {
            lower_block(ctx, then_body);
        } else if let Some(((next_condition, next_body), rest)) = elseif_clauses.split_first() {
            lower_if(
                ctx,
                next_condition,
                next_body,
                rest,
                else_body,
                span,
            );
        } else if let Some(else_body) = else_body {
            lower_block(ctx, else_body);
        }
        ctx.clear_static_callable_locals();
        return;
    }
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
    let then_block = ctx.builder.create_named_block("if.then", Vec::new());
    let else_block = ctx.builder.create_named_block("if.else", Vec::new());
    lower_condition(ctx, condition, then_block, else_block);
    let split_initialized = ctx.initialized_slots_snapshot();
    let split_types = ctx.local_types_snapshot();
    let split_try_handler_stack = ctx.try_handler_stack.clone();
    let split_static_callables = ctx.static_callable_locals_snapshot();
    // Interior-alias markers are a MAY fact, so the arms are unioned rather than sequenced.
    // Without this, `if (..) { $r = &$a[0]; } else { $r = &$o->p; }` would take whichever arm
    // was lowered last as the answer for both.
    let split_borrowed_refs = ctx.borrowed_element_ref_locals_snapshot();

    ctx.builder.position_at_end(then_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    ctx.restore_local_types(split_types.clone());
    ctx.try_handler_stack = split_try_handler_stack.clone();
    ctx.restore_static_callable_locals(split_static_callables.clone());
    apply_instanceof_branch_narrowing(ctx, condition, true);
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
    ctx.try_handler_stack = split_try_handler_stack.clone();
    ctx.restore_static_callable_locals(split_static_callables);
    ctx.restore_borrowed_element_ref_locals(split_borrowed_refs);
    apply_instanceof_branch_narrowing(ctx, condition, false);
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
    ctx.try_handler_stack = split_try_handler_stack;
    merge_reachable
}

/// Applies the positive nominal fact implied by a static `instanceof` branch to one local.
pub(in crate::ir_lower) fn apply_instanceof_branch_narrowing(
    ctx: &mut LoweringContext<'_, '_>,
    condition: &Expr,
    branch_matches: bool,
) {
    if let Some((name, ty)) =
        crate::ir_lower::expr::instanceof_branch_local_type(ctx, condition, branch_matches)
    {
        ctx.set_local_logical_type(&name, ty);
    }
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

    let joined = join_arm_types(ctx, &arms);
    let joined_callables = join_arm_static_callables(&arms);
    // The merge resumes from a REACHABLE arm's facts, overlaid with the joins. The types current
    // here belong to whichever branch was lowered last, and when that branch ended in a `throw`
    // they are the pre-branch split: `$t = []; if (…) { foreach (…) { $t[] = …; } } elseif (…)
    // { $t[] = …; } else { throw …; }` left `$t` typed `array<never>` after the chain -- both
    // arms agreed on `array<string>`, so `join_arm_types` recorded no join -- and every later
    // `in_array()` on it was compiled for an array that is always empty.
    let saved_types = arms
        .last()
        .map(|arm| arm.types.clone())
        .unwrap_or_else(|| ctx.local_types_snapshot());
    for arm in &arms {
        ctx.restore_local_types(arm.types.clone());
        let conversions = arm_conversions(arm, &joined);
        ctx.builder.position_at_end(arm.tail);
        convert_arm_arrays(ctx, &conversions, span);
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
fn join_arm_types(ctx: &LoweringContext<'_, '_>, arms: &[IfArmExit]) -> TypeEnv {
    let Some(first) = arms.first() else {
        return TypeEnv::default();
    };
    let mut names = first.types.keys().cloned().collect::<Vec<_>>();
    names.sort();

    let mut joined = TypeEnv::default();
    'names: for name in names {
        let mut arm_types = Vec::with_capacity(arms.len());
        let mut raw_arm_types = Vec::with_capacity(arms.len());
        for arm in arms {
            let Some(arm_type) = arm.types.get(&name) else {
                continue 'names;
            };
            arm_types.push(arm_type.codegen_repr());
            raw_arm_types.push(arm_type.clone());
        }
        // A reference cell or a global keeps the `null` an arm stored in its own representation,
        // so only an ordinary frame slot joins to the pointer word.
        if ctx.local_is_plain_frame_slot(&name) {
            if let Some(pointer) = join_pointer_arm_types(&raw_arm_types) {
                joined.insert(name, pointer);
                continue;
            }
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
            .all(|arm_type| matches!(arm_type, PhpType::Array(_)))
        {
            // An arm that left the array at `array<never>` added no element to it: `[]` has no
            // element to contradict the type another arm gave it. `$names = []; foreach (… as $c)
            // { if (…) { $names[] = "e.$c"; } }` joined `array<string>` with `array<never>` as
            // boxed `array<mixed>`, while the checker -- which types the function's return from
            // the same pushes -- promised the CALLER `array<string>`; the caller then read each
            // boxed cell pointer as a string (garbage, or a multi-gigabyte persist request).
            // Joining at the one concrete element keeps both layers on the same representation,
            // and the empty arm needs no conversion to be read at it.
            if let Some(element) = shared_filled_array_element(&arm_types) {
                joined.insert(name, PhpType::Array(Box::new(element)));
                continue;
            }
            if !arms
                .iter()
                .all(|arm| local_slot_is_convertible(ctx, &name, &arm.initialized))
            {
                continue;
            }
            joined.insert(name, PhpType::Array(Box::new(PhpType::Mixed)));
            continue;
        }

        // One arm promoted the array to hash storage (`$calls[$id] = …` with a string key) and
        // another left it indexed. The frame slot rule below would join them as boxed `Mixed`,
        // but the slot -- and a by-reference parameter's cell above all -- holds the raw array
        // or hash pointer, never a Mixed box: `++$calls[$id][0]` after the branch then handed
        // that pointer to the Mixed write path and every increment went nowhere. Joining at the
        // hash both arms can reach keeps the payload honest; the indexed arms convert on their
        // edge with the same `ArrayToHash` the promoting assignment already applied to the slot.
        if arm_types.iter().all(|arm_type| match arm_type {
            PhpType::Array(_) => true,
            PhpType::AssocArray { value, .. } => value.codegen_repr() == PhpType::Mixed,
            _ => false,
        }) && arms
            .iter()
            .all(|arm| local_slot_is_hash_convertible(ctx, &name, &arm.initialized))
        {
            joined.insert(
                name,
                PhpType::AssocArray {
                    key: Box::new(PhpType::Mixed),
                    value: Box::new(PhpType::Mixed),
                },
            );
            continue;
        }

        // Any other indexed/associative mix whose slots convert in place joins at a hash of
        // boxed values: indexed arms promote, associative arms box their values.
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
            // Keys keep their storage through the conversion, so the join may only claim `int`
            // keys when every associative arm has them. `$known = array_flip([]); if (!$known) {
            // $known += ['a' => …]; }` joined `array<never, int>` with `array<mixed, mixed>` at
            // `array<int, mixed>`, and `array_keys($known)` then read its string keys as ints.
            let int_keys = arm_types.iter().all(|ty| match ty {
                PhpType::AssocArray { key, .. } => {
                    matches!(key.codegen_repr(), PhpType::Int | PhpType::Never)
                }
                _ => true,
            });
            joined.insert(
                name,
                PhpType::AssocArray {
                    key: Box::new(if int_keys { PhpType::Int } else { PhpType::Mixed }),
                    value: Box::new(PhpType::Mixed),
                },
            );
            continue;
        }

        // The arms disagree and no in-place array conversion reconciles them:
        // `$i = \strlen('ab'); if (…) { $i = 'si'; }` leaves `int` on one edge and `string` on
        // the other. Falling through with NO joined fact left the merge block reading the
        // LAST arm's fact — the untaken `else`'s `int` — while the store in the `then` arm had
        // already widened the frame slot to boxed `Mixed` (`Builder::widen_local_storage_type`).
        // The `echo` below the branch was then lowered as an `int` load of a boxed slot holding
        // the string, so `__rt_mixed_cast_int` printed `0` where PHP prints `si` — and only when
        // something below the branch kept the tail from being duplicated into the arms, which is
        // what made the miscompile look order-dependent.
        //
        // The slot's frame storage type is the join every edge already agreed to: every store
        // widened it, so it can represent what any arm left in the slot, and reading at it makes
        // the guard's narrowing an unbox APPLIED TO THE LOADED VALUE inside the arm — where the
        // runtime tag check belongs. This is the contract `join_switch_type_envs` has always used
        // for a switch's edges; the `if` join is the outlier being brought into line.
        //
        // A name with no frame slot is left alone: its home is not this frame (an extern C
        // global bypasses the slot entirely, `store_local` types it from the symbol), so the
        // storage type this rule reads does not exist and the pre-existing behaviour stands.
        if let Some(slot) = ctx.local_slots.get(&name) {
            joined.insert(name, ctx.builder.local_php_type(*slot));
        }
    }
    joined
}

/// Joins arm facts that are all `null`, packed record addresses or pointers into one pointer type.
///
/// They are all one pointer word: a record address is the word a `ptr<T>` holds, and `null` is
/// address 0 in the pointer-word slot such a local gets (`pointer_word_storage` in
/// `crate::ir::builder`). The join is the arms' common pointer (targets merge through
/// `merge_local_pointer_targets`), as `?ptr<T>` when any arm can be `null`, so `=== null` after
/// the merge tests the address instead of trusting the last arm's fact. Returns `None` when the
/// arms already agree, when no arm holds a pointer, or when any arm holds something else.
fn join_pointer_arm_types(arm_types: &[PhpType]) -> Option<PhpType> {
    if arm_types.windows(2).all(|pair| pair[0] == pair[1]) {
        return None;
    }
    let mut pointer: Option<PhpType> = None;
    let mut nullable = false;
    for arm_type in arm_types {
        let arm_pointer = match arm_type {
            PhpType::Void | PhpType::Never => {
                nullable = true;
                continue;
            }
            PhpType::Pointer(_) => arm_type.clone(),
            PhpType::Packed(class) => PhpType::Pointer(Some(class.clone())),
            PhpType::Union(members) => {
                nullable = true;
                crate::types::nullable_pointer_union_member(members)?
            }
            _ => return None,
        };
        pointer = Some(match pointer {
            Some(previous) => crate::types::merge_local_pointer_targets(&previous, &arm_pointer),
            None => arm_pointer,
        });
    }
    let pointer = pointer?;
    Some(if nullable {
        PhpType::Union(vec![pointer, PhpType::Void])
    } else {
        pointer
    })
}

/// Returns the element type every non-empty arm agrees on, when at least one arm left the array
/// empty (`array<never>`) and the others all hold the same concrete, unboxed element.
fn shared_filled_array_element(arm_types: &[PhpType]) -> Option<PhpType> {
    let mut shared: Option<PhpType> = None;
    let mut saw_empty = false;
    for arm_type in arm_types {
        let PhpType::Array(element) = arm_type else {
            return None;
        };
        let element = element.codegen_repr();
        if array_write_storage::is_empty_indexed_array_element(&element) {
            saw_empty = true;
            continue;
        }
        if element == PhpType::Mixed {
            return None;
        }
        match &shared {
            None => shared = Some(element),
            Some(existing) if *existing == element => {}
            Some(_) => return None,
        }
    }
    if saw_empty {
        shared
    } else {
        None
    }
}

/// Returns indexed-array locals whose current arm needs converting before entering the merge,
/// each with the joined type it converts to.
fn arm_conversions(arm: &IfArmExit, joined: &TypeEnv) -> Vec<(String, PhpType)> {
    let mut names = joined
        .iter()
        .filter(|(name, target)| {
            let element = match arm.types.get(name.as_str()).map(PhpType::codegen_repr) {
                Some(PhpType::Array(element)) => element,
                Some(PhpType::AssocArray { value, .. }) => {
                    return matches!(target, PhpType::AssocArray { .. })
                        && value.codegen_repr() != PhpType::Mixed;
                }
                _ => return false,
            };
            // A join at a concrete element (an empty arm meeting a filled one) reads every
            // arm's array as it already is; only a boxed or hash target converts.
            if let PhpType::Array(target_element) = target.codegen_repr() {
                if target_element.codegen_repr() != PhpType::Mixed {
                    return false;
                }
            }
            matches!(target, PhpType::AssocArray { .. })
                || element.codegen_repr() != PhpType::Mixed
        })
        .map(|(name, target)| (name.clone(), target.clone()))
        .collect::<Vec<_>>();
    names.sort_by(|left, right| left.0.cmp(&right.0));
    names
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

/// Returns whether every arm can promote the named local to hash storage on its edge.
///
/// Unlike boxing elements in place, promotion is what a string-keyed write already does to the
/// slot -- a by-reference cell included -- so reference binding does not rule it out.
fn local_slot_is_hash_convertible(
    ctx: &LoweringContext<'_, '_>,
    name: &str,
    initialized: &HashSet<LocalSlotId>,
) -> bool {
    matches!(
        ctx.local_kinds.get(name).copied().unwrap_or(LocalKind::PhpLocal),
        LocalKind::PhpLocal | LocalKind::StaticLocal
    ) && !ctx.local_uses_global_storage(name)
        && ctx
            .local_slots
            .get(name)
            .is_some_and(|slot| initialized.contains(slot))
}

/// Converts array locals on an arm edge so all paths agree at the merge: boxing the elements
/// for an indexed join, promoting to hash storage (or boxing a hash's values) for an
/// associative one.
fn convert_arm_arrays(
    ctx: &mut LoweringContext<'_, '_>,
    conversions: &[(String, PhpType)],
    span: Span,
) {
    for (name, target) in conversions {
        // A static local's boxed slot already holds any array, so its logical array type converts
        // nothing. One narrowed on `null === $s` kept an array type over its `mixed` storage, and
        // the conversion reached the backend as `array_to_mixed` over a `mixed` operand
        // (DeepClone's `static $setTrace`). Checked before loading: a boxed load holds a reference.
        // A by-reference parameter's slot is `mixed` too, but its cell holds the raw container
        // and still converts.
        if ctx.local_kinds.get(name).copied() == Some(LocalKind::StaticLocal)
            && ctx
                .local_slots
                .get(name)
                .is_some_and(|slot| ctx.builder.local_php_type(*slot).codegen_repr() == PhpType::Mixed)
        {
            continue;
        }
        let source_is_hash = matches!(ctx.local_type(name).codegen_repr(), PhpType::AssocArray { .. });
        let array = ctx.load_local(name, Some(span));
        let op = if source_is_hash {
            Op::HashToMixed
        } else if matches!(target, PhpType::AssocArray { .. }) {
            Op::ArrayToHash
        } else {
            Op::ArrayToMixed
        };
        // Program-global storage can be a boxed `mixed` that already holds any list: `$argv`,
        // promoted to a global because a function body runs `eval()`, loaded as `mixed` and
        // reached the backend as `array_to_mixed` over a `mixed` operand (Symfony's
        // `bin/console`). The load is a borrowed read, so dropping it releases nothing.
        if op == Op::ArrayToMixed
            && ctx.builder.value_php_type(array.value).codegen_repr() == PhpType::Mixed
        {
            continue;
        }
        // The join makes this local `mixed` for the rest of the function, so its frame slot is a
        // boxed cell whatever it held when this arm was lowered. Declaring that BEFORE the store is
        // what lets the store release the cell it overwrites: the later widening arrived after this
        // edge was lowered, the edge saw a raw-array slot, and every pass through it left the
        // previous box -- and the array it held -- behind (`$r = new ReflectionMethod(..); $ps =
        // $r->getParameters();` in a loop leaked 176 blocks per iteration).
        let join_is_boxed = matches!(
            target.codegen_repr(),
            PhpType::Mixed | PhpType::Union(_)
        );
        let target = if op == Op::ArrayToMixed {
            PhpType::Array(Box::new(PhpType::Mixed))
        } else {
            target.clone()
        };
        let converted = ctx.emit_value(
            op.clone(),
            vec![array.value],
            None,
            target.clone(),
            op.default_effects(),
            Some(span),
        );
        if join_is_boxed {
            if let Some(slot) = ctx.local_slots.get(name).copied() {
                ctx.builder.widen_local_storage_type(slot, PhpType::Mixed);
            }
        }
        ctx.store_mutated_local(name, converted, target, Some(span));
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
/// cells. Nullable scalars receive a tagged null sentinel. A whole-value `Mixed` contract uses the
/// ordinary retaining store, allowing loop-carried container-kind changes to share the same fixed
/// frame representation. A `?ptr<T>` contract for a local that enters holding `null` only retypes
/// an ordinary frame slot, which becomes a pointer word; a reference cell, a global or a static is
/// boxed as before.
pub(super) fn apply_loop_storage_contracts(
    ctx: &mut LoweringContext<'_, '_>,
    loop_span: Span,
    span: Option<Span>,
) -> HashSet<String> {
    // Locals the contract boxes although they entered the loop as a plain scalar or string.
    let mut scalar_entry_boxed = HashSet::new();
    // Name order, not the map's: each local converts independently, but the conversions are
    // emitted in this order, and a per-process hash seed made the same source lower to
    // differently ordered code on every run.
    let mut contracts: Vec<(String, PhpType)> = ctx
        .loop_storage_types
        .get(&(ctx.loop_storage_scope.clone(), loop_span))
        .map(|contracts| {
            contracts
                .iter()
                .map(|(name, ty)| (name.clone(), ty.clone()))
                .collect()
        })
        .unwrap_or_default();
    contracts.sort_by(|left, right| left.0.cmp(&right.0));
    for (name, target_ty) in contracts {
        if !ctx.local_slots.contains_key(&name) {
            continue;
        }
        // A slot whose frame storage is already a boxed cell (a `static $x`, a whole-scope
        // `mixed` widening) holds any representation the loop stores, so there is nothing to
        // convert: loading it yields the box, and an array conversion of that box is refused.
        // league/uri's `Option::cases()` fills `static $assoc` from `$assoc = []` in a foreach.
        if ctx.local_storage_type(&name).codegen_repr() == PhpType::Mixed
            && !matches!(target_ty.codegen_repr(), PhpType::Mixed)
            && matches!(
                ctx.local_type(&name).codegen_repr(),
                PhpType::Array(_) | PhpType::AssocArray { .. }
            )
        {
            continue;
        }
        let source_ty = ctx.local_type(&name).codegen_repr();
        if source_ty == target_ty.codegen_repr() {
            ctx.set_local_type(&name, target_ty);
            continue;
        }
        let target_repr = target_ty.codegen_repr();
        if matches!(
            source_ty,
            PhpType::Bool | PhpType::Int | PhpType::Float | PhpType::Str
        ) && target_repr == PhpType::Mixed
        {
            scalar_entry_boxed.insert(name.clone());
        }
        if matches!(source_ty, PhpType::Void | PhpType::Never)
            && target_repr == PhpType::TaggedScalar
        {
            let converted = ctx.emit_value(
                Op::ConstNull,
                Vec::new(),
                None,
                target_ty.clone(),
                Op::ConstNull.default_effects(),
                span,
            );
            ctx.store_local(&name, converted, target_ty, span);
            continue;
        }
        // Each converting arm loads the local itself. A load taken before the match was left
        // unused by the arm that converts nothing, and from a boxed (`mixed`) slot that load holds
        // its own reference: the hash stayed shared, so the loop's first write copied it and
        // leaked the original once per call.
        match (&source_ty, &target_repr) {
            (PhpType::Array(_), PhpType::AssocArray { .. }) => {
                let source = ctx.load_local(&name, span);
                // The conversion builds a NEW hash, so it has to consume the slot's array: a
                // plain `ArrayToHash` kept it, and `$s = []; foreach ($keys as $k) { $s[$k] = 1; }`
                // leaked the empty array and, through the boxed slot, the whole hash per call.
                let span_or_zero = span.unwrap_or_else(|| Span::new(0, 0));
                let converted =
                    ctx.emit_local_array_to_hash_move(&name, source, target_ty.clone(), span_or_zero);
                ctx.store_retyped_container_local(&name, converted, target_ty, span);
            }
            (PhpType::Array(_), PhpType::Array(target_element))
                if target_element.codegen_repr() == PhpType::Mixed =>
            {
                let source = ctx.load_local(&name, span);
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
                let source = ctx.load_local(&name, span);
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
                let source = ctx.load_local(&name, span);
                let converted = ctx.box_value_as_mixed(source, target_ty.clone(), span);
                ctx.store_local(&name, converted, target_ty, span);
                // This store makes the slot's frame storage boxed for the whole function, so the
                // `string` load above is an unboxing COPY (`__rt_mixed_cast_string`) -- and
                // boxing a loaded string copies it again. Nothing else owns that first copy:
                // `$r = $s; foreach ($ps as $p) { $r = $p; }` leaked one copy of `$s` per call.
                if source_ty == PhpType::Str
                    && ctx.builder.value_defining_op(source.value) == Some(Op::LoadLocal)
                {
                    crate::ir_lower::ownership::release_if_owned(ctx, source, span);
                }
            }
            // A local that enters holding `null` (or a packed record address) and is assigned
            // pointers in the loop is one pointer word (`pointer_word_storage` in
            // `crate::ir::builder`): the retype widens its slot to that word, where the `null`
            // already stored is address 0, so the value needs no conversion.
            (PhpType::Void | PhpType::Packed(_), PhpType::Pointer(_))
                if ctx.local_is_plain_frame_slot(&name) =>
            {
                ctx.set_local_type(&name, target_ty);
            }
            // A reference cell, a global or a static keeps the `null` it holds in its own
            // representation, which a retype cannot turn into address 0: such a local keeps the
            // boxed whole-value contract a null-entry loop local had before pointer contracts.
            (PhpType::Void, PhpType::Pointer(_)) => {
                let source = ctx.load_local(&name, span);
                let converted = ctx.box_value_as_mixed(source, PhpType::Mixed, span);
                ctx.store_local(&name, converted, PhpType::Mixed, span);
            }
            // The contract cannot be materialized for the representation this local actually
            // holds — the only remaining shapes disagree on container kind (an `AssocArray`
            // local against an `Array(Mixed)` contract, or a non-container local). Re-declaring
            // the type here would leave the slot holding a hash while every later read is typed
            // `array<mixed>`, so the write-site promotion reads storage it has already released.
            // Avoid loading it as well: unboxing a concrete container from Mixed frame storage
            // acquires an owner even when the unused load appears borrowed in EIR.
            // Leave the local alone and let its own assignment path convert it, mirroring the
            // heap-kind guard the pre-fixed-point widening applied before promoting.
            _ => {}
        }
    }
    scalar_entry_boxed
}

