//! Purpose:
//! Paired acquire/release cancellation peephole: drop an `Acquire` whose result
//! is consumed only by a matching `Release`.
//!
//! Called from:
//! - `crate::ir_passes::peephole::Peephole::run` via `collect`.
//!
//! Key details:
//! - `Acquire` persists/increfs a value; `Release` frees/decrefs it. When the
//!   acquired result is used exactly once and that single use is its `Release`,
//!   the pair is refcount-neutral on every path (allocate-then-free or
//!   incref-then-decref with nothing observing the copy in between), and the
//!   acquired operand is untouched. Both instructions are neutralized.
//! - The single-use guard makes this safe regardless of how far apart the two
//!   ops are or which path the `Release` sits on: the value flows to exactly one
//!   `Release`, so removing both cannot leak or double-free.
//! - It does NOT make the raised refcount unobservable, which is why an `Acquire`
//!   carrying the lifetime-pin marker is skipped: such a pair exists precisely
//!   because something between the two ops may release the value's other owner
//!   (issue #580).

use crate::fast_hash::FastMap as HashMap;

use crate::ir::{Function, InstId, Op, Ownership, ValueId};
use crate::types::PhpType;

use super::super::rewrite::count_value_uses;
use super::Rewrites;

/// Collects single-use `Acquire`/`Release` pairs, neutralizing both and
/// redirecting the (now unused) acquired result to the acquired operand.
pub(super) fn collect(function: &Function, rewrites: &mut Rewrites) {
    let uses = count_value_uses(function);
    let release_of = release_targets(function);
    for (index, inst) in function.instructions.iter().enumerate() {
        if inst.op != Op::Acquire {
            continue;
        }
        // A lifetime pin is an acquire whose result is deliberately never read: it exists so
        // the value survives an interval in which its other owner may go away. Cancelling it
        // against its release would leave that interval running on freed storage, so the
        // marker opts the pair out of this rewrite entirely.
        if inst.immediate.is_some() {
            continue;
        }
        let Some(acquired) = inst.result else {
            continue;
        };
        if uses.get(&acquired).copied().unwrap_or(0) != 1 {
            continue;
        }
        let Some(&release_index) = release_of.get(&acquired) else {
            continue;
        };
        let Some(&source) = inst.operands.first() else {
            continue;
        };
        rewrites.rauw.insert(acquired, source);
        rewrites.nops.push(InstId::from_raw(index as u32));
        rewrites.nops.push(release_index);
    }
}

/// Collects owner transfers: `b = acquire a` followed, later in the same block, by
/// `release a`, where `a` is an owned temporary that nothing reads after the acquire but that
/// release (it may be filled before: `array_push a …`). The increment and the decrement cancel,
/// and `b` is `a` itself; both instructions are neutralized and `b`'s uses read `a`. Lowering
/// writes this shape wherever a fresh value is handed to a slot (a constructed container, a
/// cloned argument), and every one cost an incref and a decref call.
///
/// A use between the two would see one reference instead of two (a write there would no
/// longer separate), and a use in another block or a terminator cannot be ordered, so either
/// keeps the pair. Only containers, `Mixed` cells and objects qualify, whose acquire is a pure
/// reference increment: a string's acquire may copy a scratch buffer into the heap, and a
/// narrowed scalar's boxes it, so for those `b` is not `a`.
pub(super) fn collect_transfers(function: &Function, rewrites: &mut Rewrites) {
    let uses = count_value_uses(function);
    let mut taken: crate::fast_hash::FastSet<InstId> = rewrites.nops.iter().copied().collect();
    for block in &function.blocks {
        // Source value -> (position, id) of its first release in this block.
        let mut release_at: HashMap<ValueId, (usize, InstId)> = HashMap::default();
        // Value -> positions of the instructions of this block reading it, one per operand.
        let mut used_at: HashMap<ValueId, Vec<usize>> = HashMap::default();
        for (position, &inst_id) in block.instructions.iter().enumerate() {
            let Some(inst) = function.instruction(inst_id) else { continue };
            for &operand in &inst.operands {
                used_at.entry(operand).or_default().push(position);
            }
            if inst.op == Op::Release {
                if let Some(&released) = inst.operands.first() {
                    release_at.entry(released).or_insert((position, inst_id));
                }
            }
        }
        for (position, &inst_id) in block.instructions.iter().enumerate() {
            let Some(inst) = function.instruction(inst_id) else { continue };
            if inst.op != Op::Acquire || inst.immediate.is_some() || taken.contains(&inst_id) {
                continue;
            }
            let (Some(acquired), Some(&source)) = (inst.result, inst.operands.first()) else {
                continue;
            };
            if rewrites.rauw.contains_key(&acquired) {
                continue;
            }
            let Some(&(release_position, release_id)) = release_at.get(&source) else { continue };
            if release_position <= position || taken.contains(&release_id) {
                continue;
            }
            // Every use of the source is in this block, and none after the acquire but the
            // release: the earlier ones (filling it) plus the acquire and the release are all.
            let positions = used_at.get(&source).map(Vec::as_slice).unwrap_or(&[]);
            let earlier = positions.iter().filter(|&&at| at < position).count();
            let after = positions.iter().filter(|&&at| at > position).collect::<Vec<_>>();
            if after.as_slice() != [&release_position]
                || earlier + 2 != uses.get(&source).copied().unwrap_or(0)
            {
                continue;
            }
            let (Some(source_value), Some(acquired_value)) =
                (function.value(source), function.value(acquired))
            else {
                continue;
            };
            if source_value.ownership != Ownership::Owned
                || source_value.ir_type != acquired_value.ir_type
                || !acquire_is_a_reference_increment(&source_value.php_type)
            {
                continue;
            }
            #[cfg(test)]
            OWNER_TRANSFERS.with(|count| count.set(count.get() + 1));
            rewrites.rauw.insert(acquired, source);
            rewrites.nops.push(inst_id);
            rewrites.nops.push(release_id);
            taken.insert(inst_id);
            taken.insert(release_id);
        }
    }
}

/// Whether the codegen of an `Acquire` of this type is a plain reference increment.
fn acquire_is_a_reference_increment(php_type: &PhpType) -> bool {
    matches!(
        php_type.codegen_repr(),
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed | PhpType::Object(_)
    )
}

#[cfg(test)]
thread_local! {
    /// Owner transfers the peephole recognized on this thread.
    pub(crate) static OWNER_TRANSFERS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Maps each value released by a `Release` instruction to that instruction's id.
/// A value released more than once keeps the first mapping; the single-use guard
/// in `collect` means only single-release values are ever cancelled anyway.
fn release_targets(function: &Function) -> HashMap<ValueId, InstId> {
    let mut targets = HashMap::default();
    for (index, inst) in function.instructions.iter().enumerate() {
        if inst.op != Op::Release {
            continue;
        }
        if let Some(&released) = inst.operands.first() {
            targets.entry(released).or_insert(InstId::from_raw(index as u32));
        }
    }
    targets
}
