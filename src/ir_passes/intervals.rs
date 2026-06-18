//! Purpose:
//! Builds linear-program-order live intervals for EIR values from per-block
//! liveness. Intervals are the input to the linear-scan register allocator.
//!
//! Called from:
//! - `crate::ir_passes::regalloc` (register allocation core).
//!
//! Key details:
//! - Blocks are numbered in reverse postorder. Each block reserves a position
//!   for its parameters (block entry), one per instruction, and one for its
//!   terminator. Intervals are contiguous `[start, end]` ranges (classic
//!   Poletto-Sarkar): a value is conservatively considered live across any hole
//!   between its definition and its last use.

use std::collections::HashMap;

use crate::ir::{BlockId, Function, InstId, IrType, ValueDef, ValueId};
use crate::ir_passes::cfg::reverse_postorder;
use crate::ir_passes::clobber::{op_is_volatile_safe, terminator_is_volatile_safe};
use crate::ir_passes::liveness::{terminator_uses, LivenessInfo};

/// A value's contiguous live range in linear program order: live from `start`
/// (its definition point) through `end` (its last use or last live-out point).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveInterval {
    /// The value this interval describes.
    pub value: ValueId,
    /// The value's IR type, used by the allocator to pick a register pool.
    pub ir_type: IrType,
    /// Linear position of the value's definition.
    pub start: u32,
    /// Linear position of the value's last use or last live-out edge.
    pub end: u32,
    /// Number of times the value is used (as an instruction operand or
    /// terminator use). Drives the spill heuristic: frequently-used values are
    /// cheaper to keep in registers and more expensive to spill.
    pub weight: u32,
    /// True when no instruction or terminator that could clobber a caller-saved
    /// register executes while the value is live (strictly after its definition,
    /// up to and including its last use). Such intervals may use caller-saved
    /// registers, which need no prologue save/restore.
    pub call_free: bool,
}

/// Linear position numbering for a function: where each block's parameters,
/// instructions, and terminator sit on the single linear axis the allocator
/// scans.
struct LinearNumbering {
    /// Position assigned to a block's entry (where its parameters are defined).
    block_start: HashMap<BlockId, u32>,
    /// Position assigned to a block's terminator (its last live-out point).
    block_end: HashMap<BlockId, u32>,
    /// Position assigned to each instruction.
    inst_pos: HashMap<InstId, u32>,
    /// Blocks in reverse postorder, the order positions were assigned in.
    order: Vec<BlockId>,
}

/// Numbers blocks in reverse postorder. Each block reserves one position for
/// its entry/parameters, one per instruction, and one for its terminator, so
/// definitions precede their dominated uses on the linear axis.
fn number_positions(func: &Function) -> LinearNumbering {
    let order = reverse_postorder(func);
    let mut block_start = HashMap::new();
    let mut block_end = HashMap::new();
    let mut inst_pos = HashMap::new();
    let mut pos = 0u32;

    for &block_id in &order {
        let block = func.block(block_id).expect("ordered block exists");
        block_start.insert(block_id, pos);
        pos += 1;
        for inst_id in &block.instructions {
            inst_pos.insert(*inst_id, pos);
            pos += 1;
        }
        block_end.insert(block_id, pos);
        pos += 1;
    }

    LinearNumbering {
        block_start,
        block_end,
        inst_pos,
        order,
    }
}

/// Builds one contiguous live interval per value defined in a reachable block.
///
/// The start is the value's definition position. The end is the maximum of its
/// use positions (instruction operands and terminator uses) and the terminator
/// position of every block where the value is live-out, so values that stay
/// live across edges and loop back-edges span the intervening positions.
pub fn build_intervals(func: &Function, liveness: &LivenessInfo) -> Vec<LiveInterval> {
    let numbering = number_positions(func);

    let mut starts: HashMap<ValueId, u32> = HashMap::new();
    let mut ends: HashMap<ValueId, u32> = HashMap::new();

    // Seed each value with its definition position. Values defined in blocks
    // unreachable from entry have no position and are skipped entirely.
    for (index, value) in func.values.iter().enumerate() {
        let value_id = ValueId::from_raw(index as u32);
        let def_pos = match &value.def {
            ValueDef::BlockParam { block, .. } => numbering.block_start.get(block).copied(),
            ValueDef::Instruction { inst, .. } => numbering.inst_pos.get(inst).copied(),
        };
        if let Some(def_pos) = def_pos {
            starts.insert(value_id, def_pos);
            ends.insert(value_id, def_pos);
        }
    }

    let mut weights: HashMap<ValueId, u32> = HashMap::new();
    let mut clobbers: Vec<u32> = Vec::new();

    let extend = |value: ValueId, position: u32, ends: &mut HashMap<ValueId, u32>| {
        if let Some(end) = ends.get_mut(&value) {
            if position > *end {
                *end = position;
            }
        }
    };

    for &block_id in &numbering.order {
        let block = func.block(block_id).expect("ordered block exists");

        for inst_id in &block.instructions {
            let position = numbering.inst_pos[inst_id];
            let inst = func.instruction(*inst_id).expect("valid instruction");
            for operand in &inst.operands {
                extend(*operand, position, &mut ends);
                *weights.entry(*operand).or_insert(0) += 1;
            }
            if !op_is_volatile_safe(inst.op) {
                clobbers.push(position);
            }
        }

        let terminator_position = numbering.block_end[&block_id];
        if let Some(term) = &block.terminator {
            for value in terminator_uses(term) {
                extend(value, terminator_position, &mut ends);
                *weights.entry(value).or_insert(0) += 1;
            }
            if !terminator_is_volatile_safe(term) {
                clobbers.push(terminator_position);
            }
        }
        for value in liveness.live_out_of(block_id) {
            extend(*value, terminator_position, &mut ends);
        }
    }

    clobbers.sort_unstable();

    let mut intervals: Vec<LiveInterval> = starts
        .into_iter()
        .map(|(value, start)| {
            let end = ends[&value].max(start);
            LiveInterval {
                value,
                ir_type: func.value(value).expect("value exists").ir_type,
                start,
                end,
                weight: weights.get(&value).copied().unwrap_or(0),
                call_free: is_call_free(&clobbers, start, end),
            }
        })
        .collect();
    intervals.sort_by_key(|iv| (iv.start, iv.value.as_raw()));
    intervals
}

/// Returns true when no clobber position lies in the closed range
/// `[start, end]`.
///
/// Both endpoints are included. A clobber at the definition position (`start`)
/// means the defining instruction is itself a clobber: a call-emitting op such
/// as `Call` stores its result into the value's register and only then runs its
/// trailing argument-cleanup calls, which clobber caller-saved registers and so
/// would corrupt the just-stored result. A clobber at the last-use position
/// (`end`) means a multi-operand clobbering op can call a runtime helper while
/// materializing an earlier operand, clobbering a caller-saved register that
/// still holds this value before it is read. Either case disqualifies the
/// interval from a caller-saved register.
fn is_call_free(clobbers: &[u32], start: u32, end: u32) -> bool {
    // First clobber with position >= start.
    let lo = clobbers.partition_point(|&pos| pos < start);
    // First clobber with position > end.
    let hi = clobbers.partition_point(|&pos| pos <= end);
    lo >= hi
}
