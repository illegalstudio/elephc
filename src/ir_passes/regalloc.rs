//! Purpose:
//! Linear-scan register allocator (Poletto-Sarkar) over EIR functions. Assigns
//! registers to non-overlapping value live intervals, spilling to the stack
//! under register pressure, with separate integer and float pools.
//!
//! Called from:
//! - `crate::codegen_ir` per function, before instruction lowering.
//!
//! Key details:
//! - Two register classes are allocated. Values whose live range never crosses a
//!   clobber point (call-free, per `crate::ir_passes::clobber`) prefer
//!   caller-saved registers, which need no prologue save/restore. Values that
//!   live across a clobber point use callee-saved registers, which survive
//!   calls. Both classes are disjoint from the scratch/result registers the
//!   instruction emitters use.
//! - The spill heuristic is use-weighted: under pressure the rarely-used,
//!   furthest-reaching interval is evicted first, keeping hot values in
//!   registers.
//! - First cut: only single-word `NonHeap` scalars (`I64`, `F64`) are
//!   register-eligible, and never block parameters or branch arguments, which
//!   stay in stack slots so the existing block-parameter moves are unchanged.
//!   Generators and functions with exception handlers fall back to all-spilled.

use std::collections::{HashMap, HashSet};

use crate::codegen::platform::{Arch, Target};
use crate::ir::{Function, IrType, Op, Ownership, Terminator, ValueId};
use crate::ir_passes::allocation::Allocation;
use crate::ir_passes::intervals::{build_intervals, LiveInterval};
use crate::ir_passes::liveness::compute_liveness;

/// Computes a register allocation for `func` on `target`.
///
/// Runs liveness and interval analysis, then a linear scan that assigns
/// callee-saved registers to eligible intervals and spills the longest-lived
/// interval when a pool is exhausted. Generators and functions containing
/// exception handlers conservatively fall back to all-spilled.
pub fn allocate_registers(func: &Function, target: Target) -> Allocation {
    if func.flags.is_generator || has_exception_handlers(func) {
        return Allocation::all_spilled();
    }

    let liveness = compute_liveness(func);
    let intervals = build_intervals(func, &liveness);
    let ineligible = ineligible_values(func);

    let eligible: Vec<LiveInterval> = intervals
        .into_iter()
        .filter(|iv| is_eligible(func, iv, &ineligible))
        .collect();

    scan(&eligible, target)
}

/// Returns true when the function contains any exception handler. Such
/// functions are skipped because a thrown exception may clobber registers
/// before reaching a handler in this first cut.
fn has_exception_handlers(func: &Function) -> bool {
    func.instructions
        .iter()
        .any(|inst| inst.op == Op::TryPushHandler)
}

/// Collects values that must stay in stack slots regardless of their type:
/// block parameters and values passed as branch arguments. These feed the
/// slot-based block-parameter moves, which read them from their slots.
fn ineligible_values(func: &Function) -> HashSet<ValueId> {
    let mut ineligible = HashSet::new();
    for block in &func.blocks {
        for param in &block.params {
            ineligible.insert(*param);
        }
        if let Some(term) = &block.terminator {
            for arg in terminator_branch_args(term) {
                ineligible.insert(arg);
            }
        }
    }
    ineligible
}

/// Returns the values a terminator passes as block-parameter arguments. These
/// are distinct from condition/scrutinee/return uses, which are ordinary uses.
fn terminator_branch_args(term: &Terminator) -> Vec<ValueId> {
    match term {
        Terminator::Br { args, .. } => args.clone(),
        Terminator::CondBr {
            then_args,
            else_args,
            ..
        } => then_args.iter().chain(else_args).copied().collect(),
        Terminator::Switch {
            cases,
            default_args,
            ..
        } => cases
            .iter()
            .flat_map(|case| case.args.iter().copied())
            .chain(default_args.iter().copied())
            .collect(),
        Terminator::GeneratorSuspend { resume_args, .. } => resume_args.clone(),
        Terminator::Return { .. }
        | Terminator::Throw { .. }
        | Terminator::Fatal { .. }
        | Terminator::Unreachable => Vec::new(),
    }
}

/// Returns true when an interval's value can live in a register: a single-word
/// non-heap scalar that is not a block parameter or branch argument.
fn is_eligible(func: &Function, iv: &LiveInterval, ineligible: &HashSet<ValueId>) -> bool {
    if ineligible.contains(&iv.value) {
        return false;
    }
    if !matches!(iv.ir_type, IrType::I64 | IrType::F64) {
        return false;
    }
    func.value(iv.value)
        .map(|value| value.ownership == Ownership::NonHeap)
        .unwrap_or(false)
}

/// Which of the four register pools an active interval drew its register from.
/// Determines the free list a register returns to on expiry, and whether the
/// function must save/restore the register (only callee-saved pools).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PoolKind {
    /// Caller-saved integer pool (`x12`–`x15` / `rsi`,`rdi`,`r8`,`r9`).
    CallerInt,
    /// Callee-saved integer pool (`x21`–`x28` / `rbx`).
    CalleeInt,
    /// Caller-saved float pool (`d16`–`d23` / `xmm2`–`xmm7`).
    CallerFloat,
    /// Callee-saved float pool (`d8`–`d14`; empty on x86_64).
    CalleeFloat,
}

impl PoolKind {
    /// Returns true when registers from this pool are callee-saved and so must
    /// be preserved across the function via prologue/epilogue save/restore.
    fn is_callee_saved(self) -> bool {
        matches!(self, PoolKind::CalleeInt | PoolKind::CalleeFloat)
    }
}

/// An interval currently holding a register during the scan.
struct ActiveInterval {
    /// Linear position where the interval ends.
    end: u32,
    /// Register the interval occupies.
    reg: &'static str,
    /// Value the interval describes.
    value: ValueId,
    /// Use count, used to choose the cheapest interval to spill.
    weight: u32,
    /// Pool the register came from, so it returns to the right free list.
    pool: PoolKind,
}

/// Mutable free lists for the four register pools during a scan.
struct FreePools {
    caller_int: Vec<&'static str>,
    callee_int: Vec<&'static str>,
    caller_float: Vec<&'static str>,
    callee_float: Vec<&'static str>,
}

impl FreePools {
    /// Builds the four free lists for `target`, ordered so `pop` hands out the
    /// lowest-numbered register first.
    fn new(target: Target) -> Self {
        let rev = |regs: &[&'static str]| -> Vec<&'static str> { regs.iter().rev().copied().collect() };
        Self {
            caller_int: rev(caller_int_pool(target)),
            callee_int: rev(callee_int_pool(target)),
            caller_float: rev(caller_float_pool(target)),
            callee_float: rev(callee_float_pool(target)),
        }
    }

    /// Returns the free list for a pool kind.
    fn list_mut(&mut self, pool: PoolKind) -> &mut Vec<&'static str> {
        match pool {
            PoolKind::CallerInt => &mut self.caller_int,
            PoolKind::CalleeInt => &mut self.callee_int,
            PoolKind::CallerFloat => &mut self.caller_float,
            PoolKind::CalleeFloat => &mut self.callee_float,
        }
    }
}

/// Returns the pools an interval may draw from, most-preferred first.
///
/// Call-free intervals prefer caller-saved registers, which need no save or
/// restore, and fall back to callee-saved under pressure. Intervals that live
/// across a clobber point may only use callee-saved registers, which survive
/// calls and any caller-saved register the lowering touches.
fn candidate_pools(iv: &LiveInterval) -> &'static [PoolKind] {
    let is_float = iv.ir_type == IrType::F64;
    match (is_float, iv.call_free) {
        (false, true) => &[PoolKind::CallerInt, PoolKind::CalleeInt],
        (false, false) => &[PoolKind::CalleeInt],
        (true, true) => &[PoolKind::CallerFloat, PoolKind::CalleeFloat],
        (true, false) => &[PoolKind::CalleeFloat],
    }
}

/// Runs the linear scan over already-eligible, start-sorted intervals.
///
/// Maintains four per-pool free lists (caller/callee × int/float) and an active
/// set. Each interval is assigned from its preferred pool; on exhaustion it
/// spills using a use-weighted heuristic that keeps frequently-used values in
/// registers.
fn scan(eligible: &[LiveInterval], target: Target) -> Allocation {
    let mut free = FreePools::new(target);
    let mut active: Vec<ActiveInterval> = Vec::new();
    let mut assignments: HashMap<ValueId, &'static str> = HashMap::new();
    let mut used: Vec<&'static str> = Vec::new();

    for iv in eligible {
        expire_old_intervals(&mut active, iv.start, &mut free);

        let pools = candidate_pools(iv);
        if let Some(pool) = pools.iter().copied().find(|&p| !free.list_mut(p).is_empty()) {
            let reg = free.list_mut(pool).pop().expect("pool checked non-empty");
            if pool.is_callee_saved() {
                used.push(reg);
            }
            assignments.insert(iv.value, reg);
            active.push(ActiveInterval {
                end: iv.end,
                reg,
                value: iv.value,
                weight: iv.weight,
                pool,
            });
        } else {
            spill_at_interval(iv, pools, &mut active, &mut assignments, &mut used);
        }
    }

    Allocation::from_assignments(assignments, used)
}

/// Frees registers held by intervals that end at or before `position`, so the
/// register can be reused by the interval starting there.
fn expire_old_intervals(active: &mut Vec<ActiveInterval>, position: u32, free: &mut FreePools) {
    active.retain(|a| {
        if a.end <= position {
            free.list_mut(a.pool).push(a.reg);
            false
        } else {
            true
        }
    });
}

/// Resolves register pressure for `current` when all its candidate pools are
/// exhausted.
///
/// Among `current` and every active interval that holds a register in one of
/// `current`'s candidate pools, the cheapest interval to spill is chosen: the
/// one with the lowest use weight, breaking ties toward the furthest end so the
/// freed register stays available longest (the classic furthest-use rule). If
/// `current` is the cheapest, it stays spilled; otherwise the victim is evicted
/// and `current` takes its register.
fn spill_at_interval(
    current: &LiveInterval,
    pools: &[PoolKind],
    active: &mut Vec<ActiveInterval>,
    assignments: &mut HashMap<ValueId, &'static str>,
    used: &mut Vec<&'static str>,
) {
    // The interior victim with the best (lowest) spill cost among matching-pool
    // actives, if any beats `current`.
    let victim = active
        .iter()
        .enumerate()
        .filter(|(_, a)| pools.contains(&a.pool))
        .min_by(|(_, a), (_, b)| spill_cost(a.weight, a.end).cmp(&spill_cost(b.weight, b.end)));

    let Some((index, a)) = victim else {
        // No matching-pool active to steal from; `current` stays spilled.
        return;
    };

    // Keep `current` spilled when it is itself the cheapest to spill.
    if spill_cost(current.weight, current.end) <= spill_cost(a.weight, a.end) {
        return;
    }

    let reg = a.reg;
    let pool = a.pool;
    assignments.remove(&a.value);
    active.remove(index);
    if pool.is_callee_saved() {
        // The register was already clobbered when first assigned; ensure it is
        // recorded so the prologue/epilogue preserves it for `current` too.
        used.push(reg);
    }
    assignments.insert(current.value, reg);
    active.push(ActiveInterval {
        end: current.end,
        reg,
        value: current.value,
        weight: current.weight,
        pool,
    });
}

/// Returns a spill-cost key for an interval: lower is cheaper to spill.
///
/// Ordered by use weight ascending (rarely-used values spill first), then by
/// furthest end (`Reverse`), so among equally-used intervals the one that would
/// otherwise block a register the longest is the cheaper victim. Spilling the
/// furthest-reaching interval frees the register for the longest stretch and
/// minimizes future pressure — the classic furthest-use rule applied as the
/// tie-breaker under the use-weighted primary criterion.
fn spill_cost(weight: u32, end: u32) -> (u32, std::cmp::Reverse<u32>) {
    (weight, std::cmp::Reverse(end))
}

/// Returns the integer callee-saved register pool for `target`, excluding the
/// frame pointer, scratch registers, and registers the emitters use inline.
fn callee_int_pool(target: Target) -> &'static [&'static str] {
    match target.arch {
        Arch::AArch64 => &["x21", "x22", "x23", "x24", "x25", "x26", "x27", "x28"],
        // Only rbx is reliably preserved across the hand-written x86_64 runtime
        // routines and shared heap-marker codegen; r14/r15 are used there as
        // scratch without ABI-compliant save/restore, so they are not allocated.
        Arch::X86_64 => &["rbx"],
    }
}

/// Returns the float callee-saved register pool for `target`. SysV x86_64 has
/// no callee-saved XMM registers, so float values that live across a clobber
/// point are never register-allocated there and must stay spilled.
fn callee_float_pool(target: Target) -> &'static [&'static str] {
    match target.arch {
        Arch::AArch64 => &["d8", "d9", "d10", "d11", "d12", "d13", "d14"],
        Arch::X86_64 => &[],
    }
}

/// Returns the integer caller-saved register pool for `target`.
///
/// These registers are clobbered by any call, so they may only hold values
/// whose live range never crosses a clobber point. The audited volatile-safe
/// op lowerings (see `crate::ir_passes::clobber`) never touch them, so a
/// call-free value keeps its register intact with no save/restore.
fn caller_int_pool(target: Target) -> &'static [&'static str] {
    match target.arch {
        // x12–x15 are caller-saved temporaries the volatile-safe lowerings never use.
        Arch::AArch64 => &["x12", "x13", "x14", "x15"],
        // rsi/rdi/r8/r9 are caller-saved argument registers, untouched by the
        // volatile-safe lowerings (which use rax/rdx/rcx/r10/r11 only).
        Arch::X86_64 => &["rsi", "rdi", "r8", "r9"],
    }
}

/// Returns the float caller-saved register pool for `target`.
///
/// On x86_64, where there are no callee-saved XMM registers, this is the only
/// way float values are register-allocated at all, enabled for call-free
/// intervals. The volatile-safe float lowerings use only d0/d1 and xmm0/xmm1.
fn caller_float_pool(target: Target) -> &'static [&'static str] {
    match target.arch {
        // d16–d23 are caller-saved vector registers never used by the backend
        // as arguments, results, or scratch.
        Arch::AArch64 => &["d16", "d17", "d18", "d19", "d20", "d21", "d22", "d23"],
        Arch::X86_64 => &["xmm2", "xmm3", "xmm4", "xmm5", "xmm6", "xmm7"],
    }
}
