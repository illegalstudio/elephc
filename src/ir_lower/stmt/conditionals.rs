//! Purpose:
//! If-chain lowering, the shared arm join for lazily evaluated expressions, and loop-entry
//! storage contracts.
//!
//! Called from:
//! - `crate::ir_lower::stmt`.
//! - `crate::ir_lower::expr` (ternary, `?:`, `??`, `&&`/`||`, `match`) through `ExprBranchJoin`.
//! - Nullsafe expression joins reuse the pointer-storage merge.
//!
//! Key details:
//! - Preserves statement ordering, CFG shape, EIR effects, and ownership contracts.

use super::*;
use std::collections::HashMap;

use crate::ir_lower::context::{IfArmExit, ReflectionBindings, StaticCallableBinding};
use crate::types::TypeEnv;

mod runtime_choice;
pub(super) use runtime_choice::lower_runtime_choice;

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
    let joined_reflection = ctx.reflection_bindings_snapshot();
    ctx.clear_static_callable_locals();
    ctx.restore_static_callable_locals(joined_callables);
    ctx.restore_reflection_bindings(joined_reflection);
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
    let split_static_callables = ctx.static_callable_locals_snapshot();
    let split_reflection = ctx.reflection_bindings_snapshot();
    // Interior-alias markers are a MAY fact, so the arms are unioned rather than sequenced.
    // Without this, `if (..) { $r = &$a[0]; } else { $r = &$o->p; }` would take whichever arm
    // was lowered last as the answer for both.
    let split_borrowed_refs = ctx.borrowed_element_ref_locals_snapshot();
    let split_reference_marker_arrays = ctx.reference_marker_array_locals_snapshot();

    ctx.builder.position_at_end(then_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    ctx.restore_local_types(split_types.clone());
    ctx.restore_static_callable_locals(split_static_callables.clone());
    ctx.restore_reflection_bindings(split_reflection.clone());
    lower_block(ctx, then_body);
    let then_initialized = ctx.initialized_slots_snapshot();
    let mut merge_reachable = false;
    let then_reachable = !ctx.builder.insertion_block_is_terminated();
    if then_reachable {
        merge_reachable = true;
        record_if_arm_exit(ctx, arms);
    }

    let then_borrowed_refs = ctx.borrowed_element_ref_locals_snapshot();
    let then_reference_marker_arrays = ctx.reference_marker_array_locals_snapshot();
    ctx.builder.position_at_end(else_block);
    ctx.restore_initialized_slots(split_initialized.clone());
    ctx.restore_local_types(split_types);
    ctx.restore_static_callable_locals(split_static_callables);
    ctx.restore_reflection_bindings(split_reflection);
    ctx.restore_borrowed_element_ref_locals(split_borrowed_refs);
    ctx.restore_reference_marker_array_locals(split_reference_marker_arrays);
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
        ctx.restore_reference_marker_array_locals(HashSet::new());
    }
    if then_reachable {
        ctx.merge_borrowed_element_ref_locals(&then_borrowed_refs);
        ctx.merge_reference_marker_array_locals(&then_reference_marker_arrays);
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
///
/// The arm's current block stays unterminated and becomes the edge's tail: the join appends its
/// conversions there and then branches to the merge. Every caller moves the builder to another
/// block right away, so nothing else is lowered into the tail meanwhile. A separate tail block
/// per edge doubled the blocks of every lazy `&&`/`||`, and dominator validation grows faster
/// than the block count.
pub(super) fn record_if_arm_exit(ctx: &mut LoweringContext<'_, '_>, arms: &mut Vec<IfArmExit>) {
    let tail = ctx
        .builder
        .insertion_block()
        .expect("a reachable arm has an insertion block");
    arms.push(pending_join_edge(ctx, tail));
}

/// Defers a jump's edge into a fresh tail and terminates the current block with the branch to it.
///
/// A `break`/`continue` ends its block, and lowering may still emit unreachable statements after
/// it into the same position, so its edge cannot leave the block open the way an arm does.
fn record_terminated_arm_exit(ctx: &mut LoweringContext<'_, '_>, arms: &mut Vec<IfArmExit>) {
    let tail = ctx.builder.create_named_block("join.edge", Vec::new());
    ctx.builder.terminate(Terminator::Br {
        target: tail,
        args: Vec::new(),
    });
    arms.push(pending_join_edge(ctx, tail));
}

/// Defers a `break`/`continue` edge whose target is a join point being lowered.
///
/// A `switch` exit, and the exit and latch of a loop whose body jumps, are joins like an `if`
/// merge: each `break`/`continue` reaches them with the facts its own path left, so it records
/// its edge for the join to reconcile instead of branching straight to the target. Returns
/// `false` when `target` is no such join point; the caller then branches.
pub(super) fn record_join_edge(ctx: &mut LoweringContext<'_, '_>, target: BlockId) -> bool {
    let Some(index) = ctx
        .join_edge_arms
        .iter()
        .rposition(|(join, _)| *join == target)
    else {
        return false;
    };
    let mut arms = std::mem::take(&mut ctx.join_edge_arms[index].1);
    record_terminated_arm_exit(ctx, &mut arms);
    ctx.join_edge_arms[index].1 = arms;
    true
}

/// Removes a join point registered by its owner and returns the edges recorded into it.
pub(super) fn take_join_edges(ctx: &mut LoweringContext<'_, '_>, join: BlockId) -> Vec<IfArmExit> {
    let index = ctx
        .join_edge_arms
        .iter()
        .rposition(|(block, _)| *block == join)
        .expect("a join point is taken by the construct that registered it");
    ctx.join_edge_arms.remove(index).1
}

/// Describes the still-unterminated block `tail` as a pending join edge carrying the current facts.
pub(super) fn pending_join_edge(ctx: &LoweringContext<'_, '_>, tail: BlockId) -> IfArmExit {
    IfArmExit {
        tail,
        types: ctx.local_types_snapshot(),
        initialized: ctx.initialized_slots_snapshot(),
        static_callables: ctx.static_callable_locals_snapshot(),
        reflection: ctx.reflection_bindings_snapshot(),
    }
}

/// Joins deferred edges into `merge` like the arms of an `if`, starting from the first edge's facts.
///
/// `finish_if_type_join` rewrites only the locals its arms disagree on and leaves every other
/// local as the context holds it. After an `if` that is the last arm's facts, but a `switch` body
/// or a loop exit is joined after other code was lowered, so the context may hold the facts of
/// another path: a local the last `switch` label assigned (`case ($o = null) === null:`) would
/// read as that label left it in an earlier body whose own edges all agree it still holds the
/// object. Initialization is a must-fact: only slots initialized on every incoming edge are
/// visible after the join. That intersection is restored after any type-join conversion stores.
/// A join no edge reaches is unreachable and leaves the context's facts alone.
pub(super) fn join_edges(
    ctx: &mut LoweringContext<'_, '_>,
    edges: Vec<IfArmExit>,
    merge: BlockId,
    span: Span,
    containers: ContainerJoin,
) {
    let Some(first) = edges.first() else {
        finish_type_join(ctx, edges, merge, span, containers);
        return;
    };
    ctx.restore_local_types(first.types.clone());
    ctx.restore_initialized_slots(first.initialized.clone());
    let mut initialized = first.initialized.clone();
    for edge in edges.iter().skip(1) {
        initialized.retain(|slot| edge.initialized.contains(slot));
    }
    finish_type_join(ctx, edges, merge, span, containers);
    ctx.restore_initialized_slots(initialized);
}

/// Whether a join reconciles array and hash layouts that its edges disagree on.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ContainerJoin {
    /// Convert each edge's container to the common layout, as an `if` merge does.
    Reconcile,
    /// Keep the first edge's container fact without converting. A loop's carried layouts are
    /// fixed by the checker's storage contracts and the `for` fixed point; an edge leaving the
    /// body early still holds the head's fact (`array<never>` for a `[]` the body appends to),
    /// and reconciling it widened the array to `array<mixed>` under a caller that reads the
    /// function's `array<string>` result, so `in_array(…, true)` matched nothing.
    KeepFirst,
}

/// The join points of a loop whose body may `break` or `continue` it.
///
/// A loop body is lowered once, so the facts after it are those of the path lowered last, and
/// a jump out of the middle of the body used to reach the exit (or the next iteration) with a
/// local the facts no longer described: `$v = 1; foreach (…) { if ($c) { $v = 2.5; break; } }`
/// read `$v` back as `int(2)` after the loop. A loop that may jump therefore gets two joins like
/// an `if` merge, boxing a local whose incoming edges disagree on its representation:
///
/// - the latch, reached by the end of the body and by every `continue`, which then enters the
///   next iteration (the `for` update or the `do while` condition is lowered from it);
/// - the exit, reached by the loop's own exit edge and by every `break`, including `break N` from
///   a nested loop or `switch`.
///
/// The loop's own exit edge leaves from the head, which the entry reaches as well as the latch,
/// so its facts are NOT the latch facts on the path that never iterates: after
/// `$o = new C; foreach ($xs as $x) { if (…) { break; } $o = null; }` the latch says `$o` is
/// `null` while an empty `$xs` leaves the object. Nothing may be loaded through such a fact, so a
/// local whose entry and latch facts disagree is retyped on that edge instead (`Mixed` storage,
/// or the shared pointer word), exactly like `apply_null_entry_boxing`, and the join then needs
/// no conversion that reads it.
///
/// A loop whose body cannot jump keeps its direct edges and lowers exactly as before.
pub(super) struct LoopJumpJoins {
    /// Block every `break` (and the loop's own exit edge) finally reaches.
    exit: BlockId,
    /// Empty block the loop's own exit edge targets, so that edge can be converted too.
    natural_exit: BlockId,
    /// Block every `continue` and the end of the body reach.
    latch: BlockId,
    /// Where the latch continues, or `None` when the latch is itself the next lowered block.
    after_latch: Option<BlockId>,
    /// Local facts on entry to the loop, which its own exit edge can still carry.
    entry_types: TypeEnv,
    /// Slots initialized on entry to the loop.
    entry_initialized: HashSet<LocalSlotId>,
}

impl LoopJumpJoins {
    /// Registers the exit and latch joins of a loop whose `body` may jump out of it.
    ///
    /// `continue_target` is where the next iteration starts. When `latch_is_target` is set, that
    /// block is lowered after the body (`for` update, `do while` condition) and is the latch
    /// itself; otherwise it was lowered before the body and a fresh latch branches to it.
    pub(super) fn begin(
        ctx: &mut LoweringContext<'_, '_>,
        body: &[Stmt],
        exit: BlockId,
        continue_target: BlockId,
        latch_is_target: bool,
    ) -> Option<Self> {
        if !crate::termination::block_may_jump_out_of_current_loop(body) {
            return None;
        }
        let natural_exit = ctx.builder.create_named_block("loop.done", Vec::new());
        let (latch, after_latch) = if latch_is_target {
            (continue_target, None)
        } else {
            (
                ctx.builder.create_named_block("loop.latch", Vec::new()),
                Some(continue_target),
            )
        };
        ctx.join_edge_arms.push((exit, Vec::new()));
        ctx.join_edge_arms.push((latch, Vec::new()));
        Some(Self {
            exit,
            natural_exit,
            latch,
            after_latch,
            entry_types: ctx.local_types_snapshot(),
            entry_initialized: ctx.initialized_slots_snapshot(),
        })
    }

    /// Returns the block the loop's own exit edge targets.
    pub(super) fn exit_target(joins: &Option<Self>, exit: BlockId) -> BlockId {
        joins.as_ref().map_or(exit, |joins| joins.natural_exit)
    }

    /// Returns the block a `continue` of this loop targets.
    pub(super) fn continue_target(joins: &Option<Self>, continue_target: BlockId) -> BlockId {
        joins.as_ref().map_or(continue_target, |joins| joins.latch)
    }

    /// Ends the body: joins its fall-through end and every `continue` into the latch.
    ///
    /// The builder is left in the latch with the joined facts. A latch that is not itself the
    /// next lowered block is terminated with the branch to the next iteration.
    pub(super) fn finish_latch(&self, ctx: &mut LoweringContext<'_, '_>, span: Span) {
        let falls_through = !ctx.builder.insertion_block_is_terminated();
        if falls_through {
            record_join_edge(ctx, self.latch);
        }
        let mut edges = take_join_edges(ctx, self.latch);
        // The body's end goes first: its facts are what the next iteration saw before these
        // joins existed, and the join keeps them for every local the edges do not disagree on.
        if falls_through {
            edges.rotate_right(1);
        }
        join_edges(ctx, edges, self.latch, span, ContainerJoin::KeepFirst);
        ctx.builder.position_at_end(self.latch);
        if let Some(target) = self.after_latch {
            branch_to(ctx, target);
        }
    }

    /// Joins the loop's own exit edge and every `break` into the exit and positions the builder
    /// there. The own exit edge carries the current facts, which are what reaching the exit
    /// without a jump implied before these joins existed.
    pub(super) fn finish_exit(self, ctx: &mut LoweringContext<'_, '_>, span: Span) {
        self.retype_entry_divergent_locals(ctx);
        let mut edges = vec![pending_join_edge(ctx, self.natural_exit)];
        edges.extend(take_join_edges(ctx, self.exit));
        join_edges(ctx, edges, self.exit, span, ContainerJoin::KeepFirst);
        ctx.builder.position_at_end(self.exit);
    }

    /// Retypes, without reading them, the locals whose entry and latch facts disagree.
    ///
    /// The current facts are the latch's. A local both sides hold in an ordinary frame slot gets
    /// the type every path can be read back through: the pointer word when both are `null` or a
    /// pointer, the inline nullable int when one side already is one and the other an int or
    /// `null`, otherwise `Mixed` for representations of different kinds. Widening the slot
    /// makes the backend box every store into it, so the exit reads whatever each path left.
    /// Arrays that differ only in layout keep the latch fact, which their storage contracts keep
    /// valid, as do locals some path leaves uninitialized.
    fn retype_entry_divergent_locals(&self, ctx: &mut LoweringContext<'_, '_>) {
        let mut names = self.entry_types.keys().cloned().collect::<Vec<_>>();
        names.sort();
        for name in names {
            let entry = &self.entry_types[&name];
            let Some(latch) = ctx.local_type_fact(&name).cloned() else {
                continue;
            };
            let (entry_repr, latch_repr) = (entry.codegen_repr(), latch.codegen_repr());
            if entry_repr == latch_repr
                || crate::names::is_generated_local_name(&name)
                || !ctx.local_is_plain_frame_slot(&name)
            {
                continue;
            }
            let Some(slot) = ctx.local_slots.get(&name).copied() else {
                continue;
            };
            if !self.entry_initialized.contains(&slot) || !ctx.slot_is_initialized(slot) {
                continue;
            }
            if let Some(pointer) = join_pointer_arm_types(&[entry.clone(), latch.clone()]) {
                ctx.set_local_type(&name, pointer);
                continue;
            }
            // An inline nullable int already holds both an int and `null`: keep that slot.
            let tagged_member =
                |ty: &PhpType| matches!(ty, PhpType::Int | PhpType::Void | PhpType::TaggedScalar);
            if tagged_member(&entry_repr) && tagged_member(&latch_repr) {
                if let Some(tagged) = [entry, &latch]
                    .into_iter()
                    .find(|ty| ty.codegen_repr() == PhpType::TaggedScalar)
                {
                    ctx.set_local_type(&name, tagged.clone());
                    continue;
                }
            }
            let array_like =
                |ty: &PhpType| matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. });
            if (array_like(&entry_repr) && array_like(&latch_repr))
                || std::mem::discriminant(&entry_repr) == std::mem::discriminant(&latch_repr)
            {
                continue;
            }
            ctx.set_local_type(&name, PhpType::Mixed);
        }
    }
}

/// Reconciles flow-sensitive types and indexed-array layouts on all incoming merge edges.
pub(super) fn finish_if_type_join(
    ctx: &mut LoweringContext<'_, '_>,
    arms: Vec<IfArmExit>,
    merge: BlockId,
    span: Span,
) {
    finish_type_join(ctx, arms, merge, span, ContainerJoin::Reconcile);
}

/// Reconciles flow-sensitive types on all incoming merge edges, and container layouts when asked.
fn finish_type_join(
    ctx: &mut LoweringContext<'_, '_>,
    arms: Vec<IfArmExit>,
    merge: BlockId,
    span: Span,
    containers: ContainerJoin,
) {
    if arms.len() < 2 {
        if let Some(arm) = arms.first() {
            ctx.restore_local_types(arm.types.clone());
            ctx.restore_static_callable_locals(arm.static_callables.clone());
            ctx.restore_reflection_bindings(arm.reflection.clone());
        } else {
            ctx.restore_static_callable_locals(HashMap::new());
            ctx.restore_reflection_bindings(ReflectionBindings::default());
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

    let (joined, edge_boxed) = join_arm_types(ctx, &arms, containers);
    let joined_callables = join_arm_static_callables(&arms);
    let mut joined_reflection = arms[0].reflection.clone();
    for arm in &arms[1..] {
        joined_reflection.intersect(&arm.reflection);
    }
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
    ctx.restore_reflection_bindings(joined_reflection);
}

/// The arms of a lazily evaluated expression (`?:`, `??`, `&&`, `||`, `match`), joined like `if`.
///
/// Such an expression runs at most one of its arms, so an assignment inside one arm must not
/// reach the facts below the merge as if every path had run it. `$c ? ($o = null) : 0` left `$o`
/// typed `null` after the ternary, and the next read materialized PHP's null for the object the
/// other path still held. Each arm now starts from the split point's facts, records its merge
/// edge instead of branching straight to the merge, and `finish` reconciles the arms exactly as
/// an `if` merge does, boxing a local whose arms disagree.
pub(crate) struct ExprBranchJoin {
    /// Flow-sensitive local types at the split point, where every arm starts.
    split_types: TypeEnv,
    /// Compile-time callable targets valid at the split point.
    split_static_callables: HashMap<String, StaticCallableBinding>,
    /// Reflection targets valid before either expression arm runs.
    split_reflection: ReflectionBindings,
    /// The arms that still reach the merge, in lowering order.
    arms: Vec<IfArmExit>,
}

impl ExprBranchJoin {
    /// Captures the flow facts at the split point, before the first arm is lowered.
    pub(crate) fn at_split(ctx: &LoweringContext<'_, '_>) -> Self {
        Self {
            split_types: ctx.local_types_snapshot(),
            split_static_callables: ctx.static_callable_locals_snapshot(),
            split_reflection: ctx.reflection_bindings_snapshot(),
            arms: Vec::new(),
        }
    }

    /// Starts lowering one arm from the split point's flow facts.
    pub(crate) fn enter_arm(&self, ctx: &mut LoweringContext<'_, '_>) {
        ctx.restore_local_types(self.split_types.clone());
        ctx.restore_static_callable_locals(self.split_static_callables.clone());
        ctx.restore_reflection_bindings(self.split_reflection.clone());
    }

    /// Ends the arm being lowered, deferring its merge edge when it still reaches the merge.
    pub(crate) fn leave_arm(&mut self, ctx: &mut LoweringContext<'_, '_>) {
        if !ctx.builder.insertion_block_is_terminated() {
            record_if_arm_exit(ctx, &mut self.arms);
        }
    }

    /// Joins every recorded arm into `merge` and leaves the builder positioned there.
    pub(crate) fn finish(self, ctx: &mut LoweringContext<'_, '_>, merge: BlockId, span: Span) {
        finish_if_type_join(ctx, self.arms, merge, span);
        ctx.builder.position_at_end(merge);
    }
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
    containers: ContainerJoin,
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
        if containers == ContainerJoin::KeepFirst
            && arm_types
                .iter()
                .all(|ty| matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }))
        {
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
/// loop contract. The box holds the SAME array or hash the arm left in the slot, so the store
/// keeps the hidden internal-pointer cursor: `next`/`end` before the `if` still hold after it.
///
/// Every edge-boxed local ends in a `Mixed` slot, so each arm's load becomes an owned unbox
/// whose reference the box does not take over. Array, hash and object loads are provisional
/// owners and `box_value_as_mixed` already releases them; a callable load is only one once the
/// slot is `Mixed`, which is not yet true for the first arm boxed, so that arm's descriptor
/// leaked once per pass through its edge. The release is added here instead, and builder
/// finalization prunes it should the slot ever stay `callable`.
fn box_arm_locals_as_mixed(ctx: &mut LoweringContext<'_, '_>, names: &[String], span: Span) {
    for name in names {
        let source = ctx.load_local(name, Some(span));
        let release_callable_view = !ctx.value_is_owning_temporary(source)
            && ctx.builder.value_php_type(source.value).codegen_repr() == PhpType::Callable;
        let boxed = ctx.box_value_as_mixed(source, PhpType::Mixed, Some(span));
        if release_callable_view {
            crate::ir_lower::ownership::release_if_owned(ctx, source, Some(span));
        }
        ctx.store_local_representation(name, boxed, PhpType::Mixed, Some(span));
    }
}

/// Joins arm facts that are all `null`, packed record addresses or pointers into one pointer type.
///
/// They are all one pointer word: a record address is the word a `ptr<T>` holds, and `null` is
/// address 0 in the pointer-word slot such a local gets (`pointer_word_storage` in
/// `crate::ir::builder`). The join is the arms' common pointer (targets merge through
/// `merge_local_pointer_targets`), as `?ptr<T>` when any arm can be `null`, so `=== null` after
/// the merge tests the address instead of trusting the last arm's fact. Returns `None` when the
/// arms already agree, when no arm holds a pointer, or when any arm holds something else.
pub(in crate::ir_lower) fn join_pointer_arm_types(arm_types: &[PhpType]) -> Option<PhpType> {
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

/// Boxes, before a loop, each local that enters it holding `null` and that the loop assigns.
///
/// The loop body is lowered once, against the entry facts, so a local the back edge carries a
/// new value into needs a head representation that holds both. The checker records that
/// contract (boxed `Mixed`, issue #562) when ITS environment says the local is `null`, but it
/// keeps a declared or earlier type across `$o = null`: after `$o = new C; $o = null;` it still
/// says `C`, no contract is recorded, and the body read the slot through lowering's `null` fact.
/// A pointer slot refused that load at compile time; once an `if` join in the body had boxed the
/// slot, the read became a constant `null` on every iteration. The same `Mixed` box, applied
/// from lowering's own fact, gives the header one representation for both paths.
///
/// The box is a storage retype, not a load and re-store: the `null` fact that selects the local
/// can be stale. A loop exit keeps its body's facts, so after `while ($k-- > 0) { $o = null; }`
/// `$o` is typed `null` on the path that never entered the loop too. Reading the slot through
/// that view materialized `null` for the object it still held, and storing the box made the
/// slot `Mixed`, which hid the read from the backend's pointer-slot `null` proof. Widening the
/// frame slot to `Mixed` instead makes the backend box every store into it, `null` and object
/// alike, so the head reads whatever each path really left there.
pub(super) fn apply_null_entry_boxing(
    ctx: &mut LoweringContext<'_, '_>,
    condition: Option<&Expr>,
    body: &[Stmt],
    update: Option<&Stmt>,
) {
    let mut names = crate::types::checker::loop_assigned_local_names(condition, body, update)
        .into_iter()
        .filter(|name| null_entry_local_needs_box(ctx, name))
        .collect::<Vec<_>>();
    names.sort();
    for name in names {
        ctx.set_local_type(&name, PhpType::Mixed);
    }
}

/// Returns whether an initialized plain frame local enters the loop with a `null` fact.
fn null_entry_local_needs_box(ctx: &LoweringContext<'_, '_>, name: &str) -> bool {
    if !ctx
        .local_type_fact(name)
        .is_some_and(|ty| ty.codegen_repr() == PhpType::Void)
    {
        return false;
    }
    let is_plain_frame_local = matches!(
        ctx.local_kinds.get(name).copied().unwrap_or(LocalKind::PhpLocal),
        LocalKind::PhpLocal
    ) && !ctx.is_ref_bound_local(name)
        && !ctx.local_uses_global_storage(name)
        && !crate::names::is_generated_local_name(name);
    is_plain_frame_local
        && ctx
            .local_slots
            .get(name)
            .is_some_and(|slot| ctx.slot_is_initialized(*slot))
}

/// Materializes the checker-recorded storage contract before entering a loop.
///
/// Indexed and associative arrays are promoted in place so existing elements use boxed payload
/// cells. A whole-value `Mixed` contract uses the retaining representation store, allowing
/// loop-carried container-kind changes to share the same fixed frame representation while the
/// boxed array or hash keeps its internal-pointer cursor. A `?ptr<T>` contract for a local that
/// enters holding `null` only retypes an ordinary frame slot, which becomes a pointer word; a
/// reference cell, a global or a static is boxed as before.
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
        let target_repr = target_ty.codegen_repr();
        match (&source_ty, &target_repr) {
            (PhpType::Array(_), PhpType::AssocArray { .. }) => {
                let source = ctx.load_local(&name, span);
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
                // Boxing keeps the variable bound to the same array or hash, so the internal
                // pointer `next`/`end` moved before the loop must survive the loop-entry box.
                let source = ctx.load_local(&name, span);
                let converted = ctx.box_value_as_mixed(source, target_ty.clone(), span);
                ctx.store_local_representation(&name, converted, target_ty, span);
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
}
