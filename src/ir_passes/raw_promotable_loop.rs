//! Purpose:
//! Keeps overflow-promotable integer accumulators and loop counters in raw registers.
//! After signed overflow the promoted local stays a raw `F64` for the rest of the loop.
//!
//! Called from:
//! - The EIR fixed-point driver after guarded counter versioning.
//!
//! Key details:
//! - The integer path is entered only when every carried value and mixed bound is a PHP int.
//! - The header test is a PHP relational compare, `==` or `!=` of such values, or an integer
//!   compare of values that are already raw. One side may be an overflow-checked bound that
//!   the header recomputes (`$i < 5 + $n`). A header parameter used after the loop becomes
//!   a parameter of the exit block. Integer header parameters (a counter proved in range by
//!   an earlier pass) get their own slot in the copy, and pure integer arithmetic over them
//!   is recomputed where the copy needs it. A counter kept in an `I64` local is stored to
//!   that same local.
//! - Add, subtract, and multiply use wrapping arithmetic plus `IOverflows` while both
//!   operands are still integers. On overflow the original operands become `f64` and that
//!   local stays float; other locals stay integers until they overflow too.
//! - A float and an integer operand use float arithmetic. The float never returns to int.
//! - The original mixed loop remains for a seed that is not an int. A promoted value is
//!   boxed once, at the loop exit.
//! - A call, extra store, or non-straight body keeps the original loop. So does any
//!   other loop value used after the exit, or more than four updated locals.
//! - The other side of an update may be an overflow-checked integer temporary such as
//!   `$i * $i`. It gets its own overflow test, and a float temporary promotes the local.
//! - Every update must read its own local, so a promoted local can never be reassigned an
//!   integer. A local becomes a float only when one of its own updates produced a float.
//! - The fast path reloads a carried local where the update runs. A read that the mixed
//!   loop takes on the other side of that local's store (`$sum + $i++`) keeps the loop mixed.
//! - A copy that would cost a local used by another loop its codegen frame cell is not made.
//! - The rewrite is target-neutral. Codegen fuses each integer overflow test into one branch.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use crate::ir::{
    BlockId, Builder, CmpPredicate, DataPool, Function, Immediate, InstId, IrHeapKind, IrType,
    LocalKind,
    LocalSlotId, MixedNumericOp, Op, Ownership, PhpTypePredicate, Terminator, ValueDef, ValueId,
};
use crate::codegen::stack_numeric::FrameCells;
use crate::types::PhpType;

use super::dominance::DominanceInfo;
use super::driver::IrPass;
use super::loops::{compute_loops, NaturalLoop};
use super::rewrite::defining_instruction;

/// Adds a raw integer copy of one straight-line overflow-promotable loop.
pub struct RawPromotableLoop;

impl IrPass for RawPromotableLoop {
    /// Names the pass in fixed-point validation errors.
    fn name(&self) -> &'static str {
        "raw_promotable_loop"
    }

    /// Skips functions that have no overflow-capable arithmetic to specialize.
    fn is_applicable(&self, function: &Function) -> bool {
        function.instructions.iter().any(|inst| {
            matches!(
                inst.op,
                Op::MixedNumericBinop
                    | Op::ICheckedAdd
                    | Op::ICheckedSub
                    | Op::ICheckedMul
                    | Op::StrIncDec
            )
        })
    }

    /// Rewrites every innermost matching loop, leaving each mixed copy in place.
    fn run(&self, function: &mut Function, data: &mut DataPool) -> bool {
        let mut changed = false;
        // Each rewrite adds blocks, so loops are found again before the next one. The
        // eligibility limits on function size end the search.
        while eligible(function, data) && rewrite_one(function, data) {
            changed = true;
        }
        changed
    }
}

/// Copies the first innermost loop that matches. Returns false when none does.
fn rewrite_one(function: &mut Function, data: &mut DataPool) -> bool {
    let dominance = super::dominance::compute_dominance(function);
    let loops = compute_loops(function, &dominance);
    // The frame-cell proof clones the function, so it only runs once a loop matches.
    let mut cells = None;
    let mut order: Vec<_> = loops.loops().iter().collect();
    order.sort_by_key(|lp| std::cmp::Reverse(lp.depth));
    for lp in order {
        if !innermost(&loops, lp) || marked(function, lp) {
            continue;
        }
        let Some(plan) = analyze(function, lp, &dominance) else {
            continue;
        };
        // The copy must not cost another loop of the function its frame cells.
        let (slots, values) = expendable_cells(function, &loops, lp, &plan);
        let mut trial = function.clone();
        apply(&mut trial, &plan);
        let before = cells.get_or_insert_with(|| frame_cells(function, data));
        if !before.kept_by(&frame_cells(&trial, data), &slots, &values) {
            continue;
        }
        *function = trial;
        return true;
    }
    false
}

/// Returns the frame cells the copy of `lp` may give up.
///
/// The mixed loop only runs after an overflow, so its own results may leave their cells.
/// An updated local may leave its cell when no other loop names it. A local that another
/// loop also uses must keep it, or that loop would pay a heap box on every iteration.
fn expendable_cells(
    function: &Function,
    loops: &super::loops::LoopInfo,
    lp: &NaturalLoop,
    plan: &Plan,
) -> (HashSet<LocalSlotId>, HashSet<ValueId>) {
    let mut slots: HashSet<LocalSlotId> = plan.updates.iter().map(|update| update.slot).collect();
    let mut values = HashSet::new();
    for block in &function.blocks {
        let inside = lp.contains(block.id);
        let other_loop = !inside && loops.innermost_loop(block.id).is_some();
        for id in &block.instructions {
            if inside {
                values.extend(function.instruction(*id).and_then(|inst| inst.result));
            }
            if other_loop {
                for slot in super::lazy_boxed_accumulator::slots_of(function, *id) {
                    slots.remove(&slot);
                }
            }
        }
    }
    (slots, values)
}

/// Runs codegen's frame-cell proof on `function` as codegen will see it.
///
/// The ownership finalizer runs first on a copy, because the proof only accepts a numeric
/// result once its assignment copy is gone. Codegen keeps proven numeric locals in frame
/// cells, and the proof needs a read and its user in one block. A rewrite that splits them
/// apart sends that local back to heap boxes, so both loop passes compare this proof
/// before and after each rewrite.
pub(super) fn frame_cells(function: &Function, data: &mut DataPool) -> FrameCells {
    let mut copy = function.clone();
    let finalizer = super::fresh_mixed_transfer::FreshMixedTransfer;
    if finalizer.is_applicable(&copy) {
        finalizer.run(&mut copy, data);
    }
    FrameCells::of(&copy)
}

/// Rejects scopes whose locals can be observed or unwound outside this proof.
pub(super) fn eligible(function: &Function, data: &DataPool) -> bool {
    if function.instructions.len() > 4096
        || function.locals.len() > 128
        || function.flags.is_generator
        || function.flags.is_fiber_wrapper
        || super::cfg::has_exception_handlers(function)
        || super::local_scope::has_dynamic_eval(function)
        || (function.flags.is_main && !super::bounded_mixed_counter::main_frame_is_private(function, data))
    {
        return false;
    }
    !function.locals.iter().any(|local| {
        matches!(
            local.kind,
            LocalKind::RefCell
                | LocalKind::ReturnRefCell
                | LocalKind::EvalContext
                | LocalKind::EvalScope
                | LocalKind::EvalGlobalScope
                | LocalKind::GeneratorState
        )
    })
}

/// Returns true when no other natural loop is nested inside `lp`.
fn innermost(loops: &super::loops::LoopInfo, lp: &NaturalLoop) -> bool {
    !loops
        .loops()
        .iter()
        .any(|inner| inner.header != lp.header && lp.contains(inner.header))
}

/// Returns true when this loop was already copied or is a cold mixed fallback.
fn marked(function: &Function, lp: &NaturalLoop) -> bool {
    let name = &function.blocks[lp.header.as_raw() as usize].name;
    // The guarded counter pass keeps `counter.slow` for a seed or bound its guard rejected.
    name.contains("rawint.") || name.contains("counter.slow")
}

/// Most carried locals one loop may update. Four keeps the copy at 32 integer updates.
const MAX_UPDATES: usize = 4;

/// One straight-line loop whose mixed locals can run as raw integers until overflow.
struct Plan {
    header: BlockId,
    preheader: BlockId,
    exit: BlockId,
    enter_on_then: bool,
    /// True for a `PhpRelCmp`, `==` or `!=` header test, false for an `ICmp` of raw integers.
    php_compare: bool,
    predicate: CmpPredicate,
    left: Operand,
    right: Operand,
    updates: Vec<Update>,
    header_args: Vec<ValueId>,
    /// The value each integer header parameter takes on the back edge, in parameter order.
    latch: Vec<Operand>,
    /// Pure integer computations of the loop, referenced by `Operand::Expr`.
    exprs: Vec<Expr>,
    /// Overflow-checked temporaries of the loop, referenced by `Operand::Checked`.
    checked: Vec<Checked>,
    /// Header parameters used after the loop, by index. The exit block receives them.
    exit_params: Vec<usize>,
    path: Vec<BlockId>,
}

/// One `ICheckedAdd`, `ICheckedSub`, or `ICheckedMul` of plain integers (`$i * $i`).
///
/// PHP yields an int, or the float of the same operation when the int overflows.
#[derive(Clone, Copy)]
struct Checked {
    op: MixedNumericOp,
    lhs: Operand,
    rhs: Operand,
}

/// One pure integer instruction of the loop that the copy recomputes at each use.
///
/// Its inputs never change inside one iteration: they are header parameters, values from
/// outside the loop, constants, or other such instructions.
struct Expr {
    op: Op,
    immediate: Option<Immediate>,
    operands: Vec<Operand>,
}

/// One store of the loop body, in source order.
struct Update {
    slot: LocalSlotId,
    kind: UpdateKind,
}

/// What a loop store writes.
#[derive(Clone, Copy)]
enum UpdateKind {
    /// A carried mixed local's one arithmetic store, which may promote to a float.
    Promote { op: MixedNumericOp, lhs: Operand, rhs: Operand },
    /// A plain integer stored to an `I64` local, such as a counter kept in a slot. The copy
    /// writes the same slot: no guard can fail once the copy runs.
    Assign { value: Operand },
}

/// An integer input of a fast-path compare or update.
#[derive(Clone, Copy)]
enum Operand {
    /// Mixed local this loop stores. The fast path keeps a parallel `I64` slot.
    Carried(LocalSlotId),
    Const(i64),
    /// `I64` local loaded inside the loop. The loop does not store it, or stores it with an
    /// `Assign` update.
    Scalar(LocalSlotId),
    /// Mixed local loaded inside the loop and not stored by it. It is unboxed once at entry.
    FrozenLocal(LocalSlotId),
    /// `I64` value defined outside the loop.
    Value(ValueId),
    /// Mixed value defined outside the loop. It is unboxed once at entry.
    FrozenValue(ValueId),
    /// Integer header parameter, by index. The copy keeps it in its own `I64` slot.
    Param(usize),
    /// Pure integer instruction of the loop, by index into `Plan::exprs`.
    Expr(usize),
    /// Overflow-checked integer temporary of the loop, by index into `Plan::checked`.
    /// Only the non-carried side of an update may be one.
    Checked(usize),
}

impl Operand {
    /// Returns true for a raw integer that cannot change or promote inside one iteration.
    fn is_plain(self) -> bool {
        !matches!(self, Operand::Carried(_) | Operand::Checked(_))
    }
}

/// A mixed seed checked and unboxed once before the fast header.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Guarded {
    Local(LocalSlotId),
    Value(ValueId),
}

/// Recognizes one header-tested straight-line loop of add, subtract, and multiply.
fn analyze(function: &Function, lp: &NaturalLoop, dominance: &DominanceInfo) -> Option<Plan> {
    let preheader = lp.preheader?;
    let header = function.block(lp.header)?;
    let Terminator::Br { target, args: header_args } = function.block(preheader)?.terminator.clone()? else {
        return None;
    };
    if target != lp.header {
        return None;
    }
    let Terminator::CondBr {
        cond,
        then_target,
        then_args,
        else_target,
        else_args,
    } = header.terminator.clone()?
    else {
        return None;
    };
    if !then_args.is_empty() || !else_args.is_empty() {
        return None;
    }
    let (body, exit, enter_on_then) =
        if lp.contains(then_target) && !lp.contains(else_target) {
            (then_target, else_target, true)
        } else if lp.contains(else_target) && !lp.contains(then_target) {
            (else_target, then_target, false)
        } else {
            return None;
        };
    let (path, latch_args) = straight_path(function, lp, body)?;
    // Every header parameter becomes an integer slot of the copy, so each one must be a
    // PHP int with one incoming value per edge.
    if header_args.len() != header.params.len() || latch_args.len() != header.params.len() {
        return None;
    }
    if !header.params.iter().all(|param| is_raw_int(function, *param)) {
        return None;
    }
    if path.iter().any(|block| !allowed_block(function, *block)) {
        return None;
    }
    let exit_params = exit_params(function, lp, &header.params, exit)?;
    let compare = defining_instruction(function, cond)?;
    let php_compare = match compare.op {
        Op::PhpRelCmp | Op::LooseEq | Op::LooseNotEq => true,
        Op::ICmp => false,
        _ => return None,
    };
    // Any other PHP compare on the path could warn or throw on the mixed loop.
    if compare.operands.len() != 2 || php_compares(function, &path) != usize::from(php_compare) {
        return None;
    }
    let def_block = match function.value(cond)?.def {
        ValueDef::Instruction { block, .. } => block,
        ValueDef::BlockParam { .. } => return None,
    };
    if def_block != lp.header {
        return None;
    }
    // `==` and `!=` of two ints, or of an int and a float, compare the numbers. `FCmp`
    // with `Eq` or `Ne` gives NaN the same answer PHP does.
    let predicate = match (compare.op, &compare.immediate) {
        (Op::LooseEq, None) => CmpPredicate::Eq,
        (Op::LooseNotEq, None) => CmpPredicate::Ne,
        (Op::PhpRelCmp, Some(Immediate::CmpPredicate(predicate @ (CmpPredicate::Slt | CmpPredicate::Sle | CmpPredicate::Sgt | CmpPredicate::Sge)))) => *predicate,
        (Op::ICmp, Some(Immediate::CmpPredicate(predicate))) => *predicate,
        _ => return None,
    };
    let stores = stores_in(function, &path, lp.header)?;
    // Each update can overflow on its own, so the copy has one block chain per reachable
    // set of promoted locals: up to `n * 2^(n-1)` integer updates for `n` stores.
    let stored: HashSet<_> = stores.iter().map(|store| store.0).collect();
    if stored.len() != stores.len() || header.params.len() > MAX_PARAMS {
        return None;
    }
    let carried: HashSet<_> =
        stored.iter().copied().filter(|slot| mixed_local(function, *slot)).collect();
    if carried.is_empty() || carried.len() > MAX_UPDATES {
        return None;
    }
    let positions = path_positions(function, &path);
    let store_at: HashMap<LocalSlotId, (usize, usize)> = stores
        .iter()
        .enumerate()
        .map(|(index, store)| (store.0, (index, store.2)))
        .collect();
    let ctx = ClassCtx {
        function,
        lp,
        preheader,
        dominance,
        carried: &carried,
        positions: &positions,
        store_at: &store_at,
        header_params: &header.params,
        pre_args: &header_args,
        latch_args: &latch_args,
        exprs: RefCell::new(Vec::new()),
        checked: RefCell::new(Vec::new()),
    };
    let updates = stores
        .iter()
        .enumerate()
        .map(|(index, (slot, value, _))| parse_update(&ctx, index, *slot, *value))
        .collect::<Option<Vec<_>>>()?;
    // The header compare runs before every update of its iteration.
    let left = classify(&ctx, compare.operands[0], 0)?;
    let right = classify(&ctx, compare.operands[1], 0)?;
    // An integer compare has no float form, so it cannot test a local that may promote.
    if !php_compare && [left, right].iter().any(|operand| matches!(operand, Operand::Carried(_))) {
        return None;
    }
    // One side may be a checked temporary (`$i < 5 + $n`), tested again on every iteration.
    let checked_sides = [left, right].iter().filter(|operand| matches!(operand, Operand::Checked(_))).count();
    if checked_sides > usize::from(php_compare) {
        return None;
    }
    // The back edge runs after every update. Its arguments are raw integers, never a
    // carried local, so `classify` accepts them at any position.
    let latch = latch_args
        .iter()
        .map(|arg| classify(&ctx, *arg, updates.len()).filter(|operand| operand.is_plain()))
        .collect::<Option<Vec<_>>>()?;
    let exprs = ctx.exprs.into_inner();
    let checked = ctx.checked.into_inner();
    let plan = Plan {
        header: lp.header,
        preheader,
        exit,
        enter_on_then,
        php_compare,
        predicate,
        left,
        right,
        updates,
        header_args,
        latch,
        exprs,
        checked,
        exit_params,
        path,
    };
    // A local whose address a call may take (a by-reference argument) needs no exclusion
    // here. Such a callee writes the slot only while its call runs, and the loop has no
    // call: `allowed_block` admits none. The copy reads the slots once before the first
    // iteration and stores them once at the exit, where the mixed loop stores them too.
    Some(plan)
}

impl Plan {
    /// Lists the carried mixed locals in update order.
    fn promotable(&self) -> impl Iterator<Item = LocalSlotId> + '_ {
        self.updates
            .iter()
            .filter(|update| matches!(update.kind, UpdateKind::Promote { .. }))
            .map(|update| update.slot)
    }

    /// Lists every operand the copy reads, including the inputs of recomputed instructions.
    fn operands(&self) -> impl Iterator<Item = Operand> + '_ {
        self.updates
            .iter()
            .flat_map(|update| match update.kind {
                UpdateKind::Promote { lhs, rhs, .. } => [lhs, rhs],
                UpdateKind::Assign { value } => [value, value],
            })
            .chain([self.left, self.right])
            .chain(self.latch.iter().copied())
            .chain(self.exprs.iter().flat_map(|expr| expr.operands.iter().copied()))
            .chain(self.checked.iter().flat_map(|checked| [checked.lhs, checked.rhs]))
    }
}

/// Most integer header parameters one loop may carry.
const MAX_PARAMS: usize = 8;

/// Returns true for a raw `I64` value that PHP sees as an int.
fn is_raw_int(function: &Function, value: ValueId) -> bool {
    function
        .value(value)
        .is_some_and(|value| value.ir_type == IrType::I64 && value.php_type == PhpType::Int)
}

/// Pure integer operations the copy may recompute. None of them traps, warns, or allocates.
fn recomputable(op: Op) -> bool {
    matches!(
        op,
        Op::IAdd | Op::ISub | Op::IMul | Op::INeg | Op::IBitAnd | Op::IBitOr | Op::IBitXor | Op::IBitNot
    )
}

/// Facts needed to classify an operand without cloning the function.
struct ClassCtx<'a> {
    function: &'a Function,
    lp: &'a NaturalLoop,
    preheader: BlockId,
    dominance: &'a DominanceInfo,
    carried: &'a HashSet<LocalSlotId>,
    /// Linear position of every instruction on the loop path, header first.
    positions: &'a HashMap<InstId, usize>,
    /// Update index and store position of each carried local.
    store_at: &'a HashMap<LocalSlotId, (usize, usize)>,
    header_params: &'a [ValueId],
    pre_args: &'a [ValueId],
    latch_args: &'a [ValueId],
    /// Recomputed instructions found so far, in the order `Operand::Expr` indexes them.
    exprs: RefCell<Vec<Expr>>,
    /// Checked temporaries found so far, in the order `Operand::Checked` indexes them.
    checked: RefCell<Vec<Checked>>,
}

/// Walks the unique in-loop path from `body` back to the header.
fn straight_path(function: &Function, lp: &NaturalLoop, body: BlockId) -> Option<(Vec<BlockId>, Vec<ValueId>)> {
    let mut path = vec![lp.header];
    let mut cursor = body;
    let mut seen = HashSet::from([lp.header]);
    let latch_args = loop {
        if !lp.contains(cursor) || !seen.insert(cursor) || path.len() > 4 {
            return None;
        }
        let block = function.block(cursor)?;
        if !block.params.is_empty() {
            return None;
        }
        path.push(cursor);
        match block.terminator.clone()? {
            Terminator::Br { target, args } if target == lp.header => break args,
            Terminator::Br { target, args } if args.is_empty() => cursor = target,
            _ => return None,
        }
    };
    (path.len() == lp.blocks.len()).then_some((path, latch_args))
}

/// Returns the values defined in the loop that a block outside it uses.
///
/// The fast copy reaches the exit without running the original blocks. Only an integer
/// header parameter can be handed over, as an argument of the exit block.
fn escaping_values(function: &Function, lp: &NaturalLoop) -> HashSet<ValueId> {
    let defined: HashSet<ValueId> = lp
        .blocks
        .iter()
        .flat_map(|id| {
            let block = &function.blocks[id.as_raw() as usize];
            block.params.iter().copied().chain(
                block
                    .instructions
                    .iter()
                    .filter_map(|inst| function.instructions[inst.as_raw() as usize].result),
            )
        })
        .collect();
    let mut used = HashSet::new();
    for block in function.blocks.iter().filter(|block| !lp.contains(block.id)) {
        for id in &block.instructions {
            used.extend(function.instructions[id.as_raw() as usize].operands.iter().copied());
        }
        if let Some(term) = &block.terminator {
            used.extend(term.used_values());
        }
    }
    used.retain(|value| defined.contains(value));
    used
}

/// Returns the header parameters used after the loop, by index, when the exit can take them.
///
/// The exit block receives them as new parameters, so it must have none yet and the loop
/// header must be its only predecessor. Any other escaping loop value rejects the loop.
fn exit_params(
    function: &Function,
    lp: &NaturalLoop,
    header_params: &[ValueId],
    exit: BlockId,
) -> Option<Vec<usize>> {
    let escaping = escaping_values(function, lp);
    if escaping.is_empty() {
        return Some(Vec::new());
    }
    let mut indexes = escaping
        .iter()
        .map(|value| header_params.iter().position(|param| param == value))
        .collect::<Option<Vec<_>>>()?;
    indexes.sort_unstable();
    let predecessors = super::cfg::predecessors(function);
    let only_from_header = predecessors[exit.as_raw() as usize].as_slice() == [lp.header];
    (function.block(exit)?.params.is_empty() && only_from_header).then_some(indexes)
}

/// Returns true when every instruction in the block is on the fast-path allowlist.
fn allowed_block(function: &Function, block: BlockId) -> bool {
    function.blocks[block.as_raw() as usize]
        .instructions
        .iter()
        .all(|id| allowed(function.instructions[id.as_raw() as usize].op))
}

/// Operations the fast path may skip because the mixed fallback still executes them.
fn allowed(op: Op) -> bool {
    matches!(
        op,
        Op::Nop
            | Op::ConcatReset
            | Op::LoadLocal
            | Op::StoreLocal
            | Op::ConstI64
            | Op::ConstBool
            | Op::Acquire
            | Op::Release
            | Op::Move
            | Op::Borrow
            | Op::MixedClone
            | Op::EnsureOwned
            | Op::MixedNumericBinop
            | Op::ICheckedAdd
            | Op::ICheckedSub
            | Op::ICheckedMul
            | Op::StrIncDec
            | Op::PhpRelCmp
            | Op::ReleaseLocalSlot
            | Op::MixedBox
            | Op::MixedUnbox
            | Op::ICmp
            | Op::LooseEq
            | Op::LooseNotEq
    ) || recomputable(op)
}

/// Counts PHP relational and loose equality compares on the loop path.
fn php_compares(function: &Function, path: &[BlockId]) -> usize {
    path.iter()
        .flat_map(|block| function.blocks[block.as_raw() as usize].instructions.iter())
        .filter(|id| {
            matches!(
                function.instructions[id.as_raw() as usize].op,
                Op::PhpRelCmp | Op::LooseEq | Op::LooseNotEq
            )
        })
        .count()
}

/// Numbers every instruction on the loop path in execution order, header first.
fn path_positions(function: &Function, path: &[BlockId]) -> HashMap<InstId, usize> {
    path.iter()
        .flat_map(|block| function.blocks[block.as_raw() as usize].instructions.iter().copied())
        .enumerate()
        .map(|(position, id)| (id, position))
        .collect()
}

/// Returns the loop's mixed and plain integer stores in source order with their path positions.
///
/// A store in the header rejects the loop. Positions match `path_positions`.
fn stores_in(
    function: &Function,
    path: &[BlockId],
    header: BlockId,
) -> Option<Vec<(LocalSlotId, ValueId, usize)>> {
    let mut stores = Vec::new();
    let mut position = 0usize;
    for block in path {
        for id in &function.blocks[block.as_raw() as usize].instructions {
            let at = position;
            position += 1;
            let inst = &function.instructions[id.as_raw() as usize];
            if inst.op != Op::StoreLocal {
                continue;
            }
            if *block == header {
                return None;
            }
            let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
                return None;
            };
            let Some(&value) = inst.operands.first() else {
                return None;
            };
            if !mixed_local(function, slot) && !raw_int_local(function, slot) {
                return None;
            }
            stores.push((slot, value, at));
        }
    }
    Some(stores)
}

/// Returns true for a plain `I64` PHP int local, such as a counter an earlier pass unboxed.
fn raw_int_local(function: &Function, slot: LocalSlotId) -> bool {
    function.locals.get(slot.as_raw() as usize).is_some_and(|local| {
        local.kind == LocalKind::PhpLocal
            && local.ir_type == IrType::I64
            && local.php_type == PhpType::Int
    })
}

/// Returns true for a private mixed PHP local that can hold an int or a promoted float.
pub(super) fn mixed_local(function: &Function, slot: LocalSlotId) -> bool {
    let Some(local) = function.locals.get(slot.as_raw() as usize) else {
        return false;
    };
    local.kind == LocalKind::PhpLocal
        && local.ir_type == IrType::Heap(IrHeapKind::Mixed)
        && local.php_type == PhpType::Mixed
        && slot.as_raw() as usize >= function.params.len()
        && !function.no_epilogue_cleanup_slots.contains(&slot)
        // A lazily boxed local keeps its value in a raw companion. Its slot may be stale.
        && !super::lazy_boxed_accumulator::is_lazy(function, slot)
}

/// Parses the stored add, subtract, multiply, or unit increment of update `index`.
///
/// A store to a plain integer local is an `Assign`. A carried mixed local must read `slot`
/// itself. Otherwise a promoted local could be reassigned an
/// integer on a later iteration, and the float states have no way back to an integer.
fn parse_update(ctx: &ClassCtx<'_>, index: usize, slot: LocalSlotId, value: ValueId) -> Option<Update> {
    if !ctx.carried.contains(&slot) {
        // An integer local takes any plain integer the copy can recompute.
        let value = classify(ctx, value, index).filter(|operand| operand.is_plain())?;
        return Some(Update { slot, kind: UpdateKind::Assign { value } });
    }
    let root = unwrap_owners(ctx.function, value);
    let inst = defining_instruction(ctx.function, root)?;
    let (op, lhs, rhs) = match inst.op {
        Op::MixedNumericBinop => {
            let op = match inst.immediate {
                Some(Immediate::MixedNumericOp(op @ (MixedNumericOp::Add | MixedNumericOp::Sub | MixedNumericOp::Mul))) => op,
                _ => return None,
            };
            (op, *inst.operands.first()?, *inst.operands.get(1)?)
        }
        Op::ICheckedAdd => (MixedNumericOp::Add, inst.operands[0], inst.operands[1]),
        Op::ICheckedSub => (MixedNumericOp::Sub, inst.operands[0], inst.operands[1]),
        Op::ICheckedMul => (MixedNumericOp::Mul, inst.operands[0], inst.operands[1]),
        Op::StrIncDec => {
            let step = match inst.immediate {
                Some(Immediate::I64(1)) => 1,
                Some(Immediate::I64(-1)) => -1,
                _ => return None,
            };
            let operand = classify(ctx, inst.operands.first().copied()?, index)?;
            if !matches!(operand, Operand::Carried(carried) if carried == slot) {
                return None;
            }
            let op = if step == 1 { MixedNumericOp::Add } else { MixedNumericOp::Sub };
            let kind = UpdateKind::Promote { op, lhs: operand, rhs: Operand::Const(1) };
            return Some(Update { slot, kind });
        }
        _ => return None,
    };
    if inst.operands.len() != 2 {
        return None;
    }
    let lhs = classify(ctx, lhs, index)?;
    let rhs = classify(ctx, rhs, index)?;
    let reads_itself = |operand: Operand| matches!(operand, Operand::Carried(read) if read == slot);
    if !reads_itself(lhs) && !reads_itself(rhs) {
        return None;
    }
    Some(Update { slot, kind: UpdateKind::Promote { op, lhs, rhs } })
}

/// Classifies a compare or arithmetic input as a raw integer source.
///
/// `at` is the index of the update that consumes the value, or `0` for the header compare.
/// The fast path reads a carried local right before that update runs.
fn classify(ctx: &ClassCtx<'_>, value: ValueId, at: usize) -> Option<Operand> {
    let value = unwrap_owners(ctx.function, value);
    if let Some(constant) = const_i64(ctx.function, value) {
        return Some(Operand::Const(constant));
    }
    if let Some(source) = invariant_param(ctx, value) {
        return classify(ctx, source, at);
    }
    if let Some(index) = ctx.header_params.iter().position(|param| *param == value) {
        return Some(Operand::Param(index));
    }
    let inst = defining_instruction(ctx.function, value);
    if let Some(inst) = inst {
        let in_loop = def_block(ctx.function, value).is_some_and(|block| ctx.lp.contains(block));
        if in_loop && recomputable(inst.op) && is_raw_int(ctx.function, value) {
            let operands = inst
                .operands
                .iter()
                .map(|operand| classify(ctx, *operand, at))
                .collect::<Option<Vec<_>>>()?;
            // A carried local changes inside the iteration and may stop being an integer.
            if !operands.iter().all(|operand| operand.is_plain()) {
                return None;
            }
            let mut exprs = ctx.exprs.borrow_mut();
            exprs.push(Expr { op: inst.op, immediate: inst.immediate.clone(), operands });
            return Some(Operand::Expr(exprs.len() - 1));
        }
        let checked_op = match inst.op {
            Op::ICheckedAdd => Some(MixedNumericOp::Add),
            Op::ICheckedSub => Some(MixedNumericOp::Sub),
            Op::ICheckedMul => Some(MixedNumericOp::Mul),
            _ => None,
        };
        if let (true, Some(op), [lhs, rhs]) = (in_loop, checked_op, inst.operands.as_slice()) {
            let lhs = classify(ctx, *lhs, at).filter(|operand| operand.is_plain())?;
            let rhs = classify(ctx, *rhs, at).filter(|operand| operand.is_plain())?;
            let mut checked = ctx.checked.borrow_mut();
            checked.push(Checked { op, lhs, rhs });
            return Some(Operand::Checked(checked.len() - 1));
        }
        if inst.op == Op::MixedBox || inst.op == Op::MixedUnbox {
            return classify(ctx, *inst.operands.first()?, at);
        }
        if inst.op == Op::LoadLocal {
            let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
                return None;
            };
            if ctx.carried.contains(&slot) {
                return reads_same_value(ctx, value, slot, at).then_some(Operand::Carried(slot));
            }
            let local = ctx.function.locals.get(slot.as_raw() as usize)?;
            if ctx.store_at.contains_key(&slot) {
                // An integer local the loop stores: the copy reloads the same slot.
                return reads_same_value(ctx, value, slot, at).then_some(Operand::Scalar(slot));
            }
            // Only a load inside the loop may be replayed as a reload of the slot. A load
            // before the loop is a snapshot: lowering unsets an expression temporary right
            // after reading it, so the copy must reuse that value, as the arm below does.
            if in_loop {
                if stored_in_loop(ctx, slot) {
                    return None;
                }
                if local.ir_type == IrType::I64 && local.php_type == PhpType::Int {
                    return Some(Operand::Scalar(slot));
                }
                if local.ir_type == IrType::Heap(IrHeapKind::Mixed)
                    && local.php_type == PhpType::Mixed
                    && !super::lazy_boxed_accumulator::is_lazy(ctx.function, slot)
                {
                    return Some(Operand::FrozenLocal(slot));
                }
                return None;
            }
        }
    }
    let block = def_block(ctx.function, value)?;
    if ctx.lp.contains(block) || !ctx.dominance.dominates(block, ctx.preheader) {
        return None;
    }
    let metadata = ctx.function.value(value)?;
    match (&metadata.ir_type, &metadata.php_type) {
        (IrType::I64, PhpType::Int) => Some(Operand::Value(value)),
        (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed) => Some(Operand::FrozenValue(value)),
        _ => None,
    }
}

/// Returns true when the mixed loop's load of `slot` sees the value the fast path reloads.
///
/// The fast path reads the raw slot right before update `at`, so it sees the store of
/// `slot` exactly when that store belongs to an earlier update. The original load must sit
/// on the same side of that store. `$sum + $i++` fails: it reads `$i` before the increment
/// but adds it after.
fn reads_same_value(ctx: &ClassCtx<'_>, load: ValueId, slot: LocalSlotId, at: usize) -> bool {
    let Some(ValueDef::Instruction { inst, .. }) = ctx.function.value(load).map(|value| value.def) else {
        return false;
    };
    let (Some(&load_at), Some(&(store_index, store_position))) =
        (ctx.positions.get(&inst), ctx.store_at.get(&slot))
    else {
        return false;
    };
    (store_position < load_at) == (store_index < at)
}

/// Returns the unchanged incoming value of an invariant header parameter.
fn invariant_param(ctx: &ClassCtx<'_>, value: ValueId) -> Option<ValueId> {
    let index = ctx.header_params.iter().position(|param| *param == value)?;
    let pre = *ctx.pre_args.get(index)?;
    let latch = *ctx.latch_args.get(index)?;
    (pre == latch).then_some(pre)
}

/// Returns true when any loop block stores `slot`.
fn stored_in_loop(ctx: &ClassCtx<'_>, slot: LocalSlotId) -> bool {
    ctx.lp.blocks.iter().any(|block| {
        ctx.function.blocks[block.as_raw() as usize]
            .instructions
            .iter()
            .any(|id| {
                let inst = &ctx.function.instructions[id.as_raw() as usize];
                inst.op == Op::StoreLocal && inst.immediate == Some(Immediate::LocalSlot(slot))
            })
    })
}

/// Follows transparent owner copies and unboxing to the value they forward.
fn unwrap_owners(function: &Function, mut value: ValueId) -> ValueId {
    for _ in 0..8 {
        let Some(inst) = defining_instruction(function, value) else {
            break;
        };
        let forward = matches!(
            inst.op,
            Op::Acquire | Op::Move | Op::Borrow | Op::MixedClone | Op::EnsureOwned | Op::MixedUnbox
        ) && inst.immediate.is_none()
            && inst.operands.len() == 1;
        if !forward {
            break;
        }
        value = inst.operands[0];
    }
    value
}

/// Returns a literal `I64` integer, when `value` is exactly that constant.
fn const_i64(function: &Function, value: ValueId) -> Option<i64> {
    let inst = defining_instruction(function, value)?;
    match (inst.op, &inst.immediate) {
        (Op::ConstI64, Some(Immediate::I64(value))) => Some(*value),
        _ => None,
    }
}

/// Returns the block that defines `value`.
fn def_block(function: &Function, value: ValueId) -> Option<BlockId> {
    match function.value(value)?.def {
        ValueDef::Instruction { block, .. } | ValueDef::BlockParam { block, .. } => Some(block),
    }
}

/// Parallel raw slots for one guarded value. Promoted locals also have an `F64` slot.
struct Storage {
    ints: HashMap<Guarded, LocalSlotId>,
    floats: HashMap<LocalSlotId, LocalSlotId>,
    /// One `I64` slot per header parameter, in parameter order.
    params: Vec<LocalSlotId>,
}

/// Builds the integer loop and every float continuation an overflow edge can reach.
///
/// A state is the sorted set of carried locals that are floats. Headers are shared per
/// state and update blocks per `(update index, state)`, so each reachable pair is built once.
struct LoopBuilder<'a> {
    builder: Builder<'a>,
    plan: &'a Plan,
    raw: &'a Storage,
    headers: HashMap<Vec<LocalSlotId>, BlockId>,
    latches: HashMap<Vec<LocalSlotId>, BlockId>,
    steps: HashMap<(usize, Vec<LocalSlotId>), BlockId>,
}

/// Inserts the int guards and the raw loops. A non-int seed still uses the mixed loop.
fn apply(function: &mut Function, plan: &Plan) {
    let guarded = guard_list(plan);
    let carried: HashSet<LocalSlotId> = plan.promotable().collect();
    let mut raw = Storage { ints: HashMap::new(), floats: HashMap::new(), params: Vec::new() };
    for index in 0..plan.header_args.len() {
        raw.params.push(function.add_local(
            Some(format!("rawphi{index}")),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        ));
    }
    for (index, item) in guarded.iter().enumerate() {
        // Mem2reg promotes only named scalar PHP locals. The names are not PHP variables.
        let int_slot = function.add_local(
            Some(format!("rawint{index}")),
            IrType::I64,
            PhpType::Int,
            LocalKind::PhpLocal,
        );
        raw.ints.insert(*item, int_slot);
        if let Guarded::Local(slot) = item {
            if carried.contains(slot) {
                let float_slot = function.add_local(
                    Some(format!("rawflt{index}")),
                    IrType::F64,
                    PhpType::Float,
                    LocalKind::PhpLocal,
                );
                raw.floats.insert(*slot, float_slot);
            }
        }
    }
    pass_header_params_to_exit(function, plan);
    function.blocks[plan.preheader.as_raw() as usize].terminator = None;
    let mut cfg = LoopBuilder {
        builder: Builder::new(function),
        plan,
        raw: &raw,
        headers: HashMap::new(),
        latches: HashMap::new(),
        steps: HashMap::new(),
    };
    let fast = cfg.header(&[]);
    let slow = plan.header;
    let slow_args = plan.header_args.clone();
    let mut guard_blocks = Vec::new();
    for _ in &guarded {
        guard_blocks.push(cfg.builder.create_named_block("rawint.guard", vec![]));
    }
    cfg.builder.position_at_end(plan.preheader);
    // The mixed loop still receives these values as block arguments when a guard fails.
    for (slot, value) in raw.params.iter().zip(&plan.header_args) {
        cfg.builder.emit_store_local(*slot, *value);
    }
    let entry = guard_blocks.first().copied().unwrap_or(fast);
    cfg.builder.terminate(Terminator::Br { target: entry, args: vec![] });
    for (index, item) in guarded.iter().enumerate() {
        cfg.builder.position_at_end(guard_blocks[index]);
        let mixed = match item {
            Guarded::Local(slot) => load_mixed(&mut cfg.builder, *slot),
            Guarded::Value(value) => *value,
        };
        let ok = emit(
            &mut cfg.builder,
            Op::TypePredicate,
            vec![mixed],
            Some(Immediate::TypePredicate(PhpTypePredicate::Int)),
            IrType::I64,
            PhpType::Bool,
        );
        let next = guard_blocks.get(index + 1).copied().unwrap_or(fast);
        // Unbox only after the predicate succeeds. A float payload must not be read as an integer.
        let unbox_at = cfg.builder.create_named_block("rawint.unbox", vec![]);
        cfg.builder.terminate(Terminator::CondBr {
            cond: ok,
            then_target: unbox_at,
            then_args: vec![],
            else_target: slow,
            else_args: slow_args.clone(),
        });
        cfg.builder.position_at_end(unbox_at);
        let payload = emit(&mut cfg.builder, Op::MixedUnbox, vec![mixed], None, IrType::I64, PhpType::Int);
        cfg.builder.emit_store_local(raw.ints[item], payload);
        cfg.builder.terminate(Terminator::Br { target: next, args: vec![] });
    }
    drop(cfg);
    for id in &plan.path {
        function.blocks[id.as_raw() as usize].name.push_str(".rawint.slow");
    }
}

/// Turns the header parameters used after the loop into parameters of the exit block.
///
/// The mixed header passes them on its exit edge and every use outside the loop reads the
/// new parameter. The fast exits pass the value of their own slot.
fn pass_header_params_to_exit(function: &mut Function, plan: &Plan) {
    if plan.exit_params.is_empty() {
        return;
    }
    let header_params = function.blocks[plan.header.as_raw() as usize].params.clone();
    let mut renamed = HashMap::new();
    let mut passed = Vec::new();
    for (position, index) in plan.exit_params.iter().enumerate() {
        let param = header_params[*index];
        let mut metadata = function.values[param.as_raw() as usize].clone();
        metadata.def = ValueDef::BlockParam { block: plan.exit, index: position as u16 };
        let received = ValueId::from_raw(function.values.len() as u32);
        function.values.push(metadata);
        function.blocks[plan.exit.as_raw() as usize].params.push(received);
        renamed.insert(param, received);
        passed.push(param);
    }
    let in_loop: HashSet<BlockId> = plan.path.iter().copied().collect();
    for index in 0..function.blocks.len() {
        if in_loop.contains(&function.blocks[index].id) {
            continue;
        }
        for id in function.blocks[index].instructions.clone() {
            for operand in &mut function.instructions[id.as_raw() as usize].operands {
                if let Some(received) = renamed.get(operand) {
                    *operand = *received;
                }
            }
        }
        if let Some(terminator) = function.blocks[index].terminator.as_mut() {
            super::rewrite::replace_in_terminator(terminator, &renamed);
        }
    }
    if let Some(Terminator::CondBr { then_target, then_args, else_args, .. }) =
        function.blocks[plan.header.as_raw() as usize].terminator.as_mut()
    {
        if *then_target == plan.exit {
            *then_args = passed;
        } else {
            *else_args = passed;
        }
    }
}

/// Lists each mixed seed once, carried locals first.
fn guard_list(plan: &Plan) -> Vec<Guarded> {
    let mut items = Vec::new();
    let push = |item: Guarded, items: &mut Vec<Guarded>| {
        if !items.contains(&item) {
            items.push(item);
        }
    };
    for slot in plan.promotable() {
        push(Guarded::Local(slot), &mut items);
    }
    for operand in plan.operands() {
        match operand {
            Operand::FrozenLocal(slot) => push(Guarded::Local(slot), &mut items),
            Operand::FrozenValue(value) => push(Guarded::Value(value), &mut items),
            Operand::Carried(slot) => push(Guarded::Local(slot), &mut items),
            Operand::Const(_) | Operand::Scalar(_) | Operand::Value(_) | Operand::Param(_)
            | Operand::Expr(_) | Operand::Checked(_) => {}
        }
    }
    items
}

impl<'a> LoopBuilder<'a> {
    /// Returns the loop header for one float set, building its compare and exit the first time.
    fn header(&mut self, floats: &[LocalSlotId]) -> BlockId {
        if let Some(header) = self.headers.get(floats) {
            return *header;
        }
        let promoted = !floats.is_empty();
        let header = self.builder.create_named_block(
            if promoted { "rawint.float.header" } else { "rawint.fast.header" },
            vec![],
        );
        let exit = self.builder.create_named_block(
            if promoted { "rawint.float.exit" } else { "rawint.fast.exit" },
            vec![],
        );
        self.headers.insert(floats.to_vec(), header);
        self.publish_state(floats, exit);
        let body = self.step(0, floats.to_vec());
        self.fill_compare(floats, header, body, exit);
        header
    }

    /// Returns the back edge of state `floats`: the next value of every header parameter.
    ///
    /// All values are computed before the first store, as block arguments are assigned in
    /// parallel. A loop without header parameters jumps straight to its header.
    fn latch(&mut self, floats: &[LocalSlotId]) -> BlockId {
        let header = self.header(floats);
        if self.plan.latch.is_empty() {
            return header;
        }
        if let Some(block) = self.latches.get(floats) {
            return *block;
        }
        let block = self.builder.create_named_block("rawint.latch", vec![]);
        self.latches.insert(floats.to_vec(), block);
        self.builder.position_at_end(block);
        let plan = self.plan;
        let values: Vec<ValueId> = plan.latch.iter().map(|operand| self.as_int(*operand)).collect();
        for (slot, value) in self.raw.params.iter().zip(values) {
            self.builder.emit_store_local(*slot, value);
        }
        self.builder.terminate(Terminator::Br { target: header, args: vec![] });
        block
    }

    /// Returns the block that runs update `index` and the ones after it in state `floats`.
    ///
    /// Past the last update this is the header of the state the iteration ended in. A local
    /// joins the float set only on the edge where its own update stored a float, so a loop
    /// that exits first still publishes it as an integer.
    fn step(&mut self, index: usize, floats: Vec<LocalSlotId>) -> BlockId {
        let Some(update) = self.plan.updates.get(index) else {
            return self.latch(&floats);
        };
        let key = (index, floats.clone());
        if let Some(block) = self.steps.get(&key) {
            return *block;
        }
        let slot = update.slot;
        let (op, mut lhs, mut rhs) = match update.kind {
            UpdateKind::Promote { op, lhs, rhs } => (op, lhs, rhs),
            UpdateKind::Assign { value } => {
                let block = self.builder.create_named_block("rawint.assign", vec![]);
                self.steps.insert(key, block);
                let next = self.step(index + 1, floats);
                self.builder.position_at_end(block);
                let value = self.as_int(value);
                self.builder.emit_store_local(slot, value);
                self.builder.terminate(Terminator::Br { target: next, args: vec![] });
                return block;
            }
        };
        let mut grown = floats.clone();
        insert_slot(&mut grown, slot);
        // A checked temporary is tested first. Its own blocks lead into the update's.
        let temporary = match (lhs, rhs) {
            (Operand::Checked(checked), _) => Some((self.plan.checked[checked], true)),
            (_, Operand::Checked(checked)) => Some((self.plan.checked[checked], false)),
            _ => None,
        };
        let tested = temporary.map(|_| {
            (
                self.builder.create_named_block("rawint.temp.check", vec![]),
                self.builder.create_named_block("rawint.temp.deopt", vec![]),
            )
        });
        let float_step = operand_is_float(lhs, &floats) || operand_is_float(rhs, &floats);
        let first = self.builder.create_named_block(
            if float_step {
                "rawint.float.step"
            } else if floats.is_empty() {
                "rawint.fast.check"
            } else {
                "rawint.float.check"
            },
            vec![],
        );
        let entry = tested.map_or(first, |(check, _)| check);
        self.steps.insert(key, entry);
        let overflowed = self.step(index + 1, grown);
        if let (Some((checked, on_left)), Some((temp_check, temp_deopt))) = (temporary, tested) {
            self.builder.position_at_end(temp_check);
            let left = self.as_int(checked.lhs);
            let right = self.as_int(checked.rhs);
            let value = emit(
                &mut self.builder,
                integer_op(checked.op),
                vec![left, right],
                None,
                IrType::I64,
                PhpType::Int,
            );
            let overflow = emit(
                &mut self.builder,
                Op::IOverflows,
                vec![left, right],
                Some(Immediate::MixedNumericOp(checked.op)),
                IrType::I64,
                PhpType::Bool,
            );
            self.builder.terminate(Terminator::CondBr {
                cond: overflow,
                then_target: temp_deopt,
                then_args: vec![],
                else_target: first,
                else_args: vec![],
            });
            // The temporary is a float, so PHP adds it to the local as floats.
            self.builder.position_at_end(temp_deopt);
            let left = self.to_float(left);
            let right = self.to_float(right);
            let promoted = emit(
                &mut self.builder,
                float_op(checked.op),
                vec![left, right],
                None,
                IrType::F64,
                PhpType::Float,
            );
            let other = self.as_float(Operand::Carried(slot), &floats);
            let pair = if on_left { vec![promoted, other] } else { vec![other, promoted] };
            let result = emit(&mut self.builder, float_op(op), pair, None, IrType::F64, PhpType::Float);
            let float_slot = self.raw.floats[&slot];
            self.builder.emit_store_local(float_slot, result);
            self.builder.terminate(Terminator::Br { target: overflowed, args: vec![] });
            // From here on the temporary is the integer that fit.
            if on_left {
                lhs = Operand::Value(value);
            } else {
                rhs = Operand::Value(value);
            }
        }
        if float_step {
            self.builder.position_at_end(first);
            self.store_float_result(slot, op, lhs, rhs, &floats);
            self.builder.terminate(Terminator::Br { target: overflowed, args: vec![] });
            return entry;
        }
        let check = first;
        let cont = self.builder.create_named_block("rawint.fast.cont", vec![]);
        let deopt = self.builder.create_named_block("rawint.deopt", vec![]);
        let fitting = self.step(index + 1, floats.clone());

        self.builder.position_at_end(check);
        let left = self.as_int(lhs);
        let right = self.as_int(rhs);
        let result = emit(
            &mut self.builder,
            integer_op(op),
            vec![left, right],
            None,
            IrType::I64,
            PhpType::Int,
        );
        let overflow = emit(
            &mut self.builder,
            Op::IOverflows,
            vec![left, right],
            Some(Immediate::MixedNumericOp(op)),
            IrType::I64,
            PhpType::Bool,
        );
        self.builder.terminate(Terminator::CondBr {
            cond: overflow,
            then_target: deopt,
            then_args: vec![],
            else_target: cont,
            else_args: vec![],
        });
        self.builder.position_at_end(cont);
        let int_slot = self.raw.ints[&Guarded::Local(slot)];
        self.builder.emit_store_local(int_slot, result);
        self.builder.terminate(Terminator::Br { target: fitting, args: vec![] });

        self.builder.position_at_end(deopt);
        self.store_float_result(slot, op, lhs, rhs, &floats);
        self.builder.terminate(Terminator::Br { target: overflowed, args: vec![] });
        entry
    }

    /// Computes one update on `f64` operands and stores it in the local's float slot.
    fn store_float_result(
        &mut self,
        slot: LocalSlotId,
        op: MixedNumericOp,
        lhs: Operand,
        rhs: Operand,
        floats: &[LocalSlotId],
    ) {
        let left = self.as_float(lhs, floats);
        let right = self.as_float(rhs, floats);
        let value = emit(
            &mut self.builder,
            float_op(op),
            vec![left, right],
            None,
            IrType::F64,
            PhpType::Float,
        );
        let float_slot = self.raw.floats[&slot];
        self.builder.emit_store_local(float_slot, value);
    }

    /// Emits the header comparison for the locals that are already floats.
    fn fill_compare(&mut self, floats: &[LocalSlotId], header: BlockId, body: BlockId, exit: BlockId) {
        let mut left_operand = self.plan.left;
        let mut right_operand = self.plan.right;
        let predicate = self.plan.predicate;
        self.builder.position_at_end(header);
        let temporary = match (left_operand, right_operand) {
            (Operand::Checked(checked), _) => Some((self.plan.checked[checked], true)),
            (_, Operand::Checked(checked)) => Some((self.plan.checked[checked], false)),
            _ => None,
        };
        if let Some((checked, on_left)) = temporary {
            // A bound computed in the header is tested on every iteration. A float bound
            // compares as floats and leaves every local in its current representation.
            let fit = self.builder.create_named_block("rawint.bound.fit", vec![]);
            let deopt = self.builder.create_named_block("rawint.bound.deopt", vec![]);
            self.builder.position_at_end(header);
            let left = self.as_int(checked.lhs);
            let right = self.as_int(checked.rhs);
            let value = emit(
                &mut self.builder,
                integer_op(checked.op),
                vec![left, right],
                None,
                IrType::I64,
                PhpType::Int,
            );
            let overflow = emit(
                &mut self.builder,
                Op::IOverflows,
                vec![left, right],
                Some(Immediate::MixedNumericOp(checked.op)),
                IrType::I64,
                PhpType::Bool,
            );
            self.builder.terminate(Terminator::CondBr {
                cond: overflow,
                then_target: deopt,
                then_args: vec![],
                else_target: fit,
                else_args: vec![],
            });
            self.builder.position_at_end(deopt);
            let left = self.to_float(left);
            let right = self.to_float(right);
            let bound = emit(
                &mut self.builder,
                float_op(checked.op),
                vec![left, right],
                None,
                IrType::F64,
                PhpType::Float,
            );
            let other = if on_left { right_operand } else { left_operand };
            let (other, other_float) = self.compare_operand(other, floats);
            let other = if other_float { other } else { self.to_float(other) };
            let pair = if on_left { vec![bound, other] } else { vec![other, bound] };
            let cond = emit(
                &mut self.builder,
                Op::FCmp,
                pair,
                Some(Immediate::CmpPredicate(ordered_predicate(predicate))),
                IrType::I64,
                PhpType::Bool,
            );
            self.branch_on_compare(cond, body, exit);
            self.builder.position_at_end(fit);
            if on_left {
                left_operand = Operand::Value(value);
            } else {
                right_operand = Operand::Value(value);
            }
        }
        let (left, left_float) = self.compare_operand(left_operand, floats);
        let (right, right_float) = self.compare_operand(right_operand, floats);
        // An integer header test keeps its own predicate. It never sees a promoted local.
        let cond = if left_float || right_float {
            debug_assert!(self.plan.php_compare);
            let left = if left_float { left } else { self.to_float(left) };
            let right = if right_float { right } else { self.to_float(right) };
            emit(
                &mut self.builder,
                Op::FCmp,
                vec![left, right],
                Some(Immediate::CmpPredicate(ordered_predicate(predicate))),
                IrType::I64,
                PhpType::Bool,
            )
        } else {
            emit(
                &mut self.builder,
                Op::ICmp,
                vec![left, right],
                Some(Immediate::CmpPredicate(predicate)),
                IrType::I64,
                PhpType::Bool,
            )
        };
        self.branch_on_compare(cond, body, exit);
    }

    /// Ends the current block with the loop test: into the body or out through the exit.
    fn branch_on_compare(&mut self, cond: ValueId, body: BlockId, exit: BlockId) {
        let (then_target, else_target) =
            if self.plan.enter_on_then { (body, exit) } else { (exit, body) };
        self.builder.terminate(Terminator::CondBr {
            cond,
            then_target,
            then_args: vec![],
            else_target,
            else_args: vec![],
        });
    }

    /// Boxes each carried local once, using the representation this loop state kept.
    fn publish_state(&mut self, floats: &[LocalSlotId], exit: BlockId) {
        let slots: Vec<LocalSlotId> = self.plan.promotable().collect();
        let original_exit = self.plan.exit;
        self.builder.position_at_end(exit);
        for slot in slots {
            if floats.binary_search(&slot).is_ok() {
                let float_slot = self.raw.floats[&slot];
                let value = self.builder.emit_load_local(float_slot, IrType::F64, PhpType::Float);
                publish_value(&mut self.builder, value, slot);
            } else {
                let int_slot = self.raw.ints[&Guarded::Local(slot)];
                publish_int(&mut self.builder, int_slot, slot);
            }
        }
        let plan = self.plan;
        let args = plan
            .exit_params
            .iter()
            .map(|index| self.as_int(Operand::Param(*index)))
            .collect();
        self.builder.terminate(Terminator::Br { target: original_exit, args });
    }

    /// Loads a compare operand and reports whether this state keeps it as a float.
    fn compare_operand(&mut self, operand: Operand, floats: &[LocalSlotId]) -> (ValueId, bool) {
        if operand_is_float(operand, floats) {
            (self.as_float(operand, floats), true)
        } else {
            (self.as_int(operand), false)
        }
    }

    /// Loads an operand that is still an integer in this state.
    fn as_int(&mut self, operand: Operand) -> ValueId {
        match operand {
            Operand::Carried(slot) | Operand::FrozenLocal(slot) => {
                let int_slot = self.raw.ints[&Guarded::Local(slot)];
                self.builder.emit_load_local(int_slot, IrType::I64, PhpType::Int)
            }
            Operand::FrozenValue(value) => {
                let int_slot = self.raw.ints[&Guarded::Value(value)];
                self.builder.emit_load_local(int_slot, IrType::I64, PhpType::Int)
            }
            Operand::Const(value) => self.builder.emit_const_i64(value),
            Operand::Scalar(slot) => self.builder.emit_load_local(slot, IrType::I64, PhpType::Int),
            Operand::Value(value) => value,
            Operand::Param(index) => {
                self.builder.emit_load_local(self.raw.params[index], IrType::I64, PhpType::Int)
            }
            Operand::Expr(index) => {
                let plan = self.plan;
                let expr = &plan.exprs[index];
                let operands = expr.operands.iter().map(|operand| self.as_int(*operand)).collect();
                emit(&mut self.builder, expr.op, operands, expr.immediate.clone(), IrType::I64, PhpType::Int)
            }
            Operand::Checked(index) => {
                // `step` replaces a checked operand with its tested value before any read.
                debug_assert!(false, "checked operand read without its overflow test");
                let checked = self.plan.checked[index];
                let left = self.as_int(checked.lhs);
                let right = self.as_int(checked.rhs);
                emit(&mut self.builder, integer_op(checked.op), vec![left, right], None, IrType::I64, PhpType::Int)
            }
        }
    }

    /// Loads a float operand, converting an integer operand with PHP's signed cast.
    fn as_float(&mut self, operand: Operand, promoted: &[LocalSlotId]) -> ValueId {
        if let Operand::Carried(slot) = operand {
            if promoted.binary_search(&slot).is_ok() {
                let float_slot = self.raw.floats[&slot];
                return self.builder.emit_load_local(float_slot, IrType::F64, PhpType::Float);
            }
        }
        let int_value = self.as_int(operand);
        self.to_float(int_value)
    }

    /// Converts one integer value to `f64`.
    fn to_float(&mut self, value: ValueId) -> ValueId {
        emit(&mut self.builder, Op::IToF, vec![value], None, IrType::F64, PhpType::Float)
    }
}

/// Returns true when a carried operand is already represented as a float.
fn operand_is_float(operand: Operand, floats: &[LocalSlotId]) -> bool {
    match operand {
        Operand::Carried(slot) => floats.binary_search(&slot).is_ok(),
        Operand::Const(_) | Operand::Scalar(_) | Operand::FrozenLocal(_) | Operand::Value(_)
        | Operand::FrozenValue(_) | Operand::Param(_) | Operand::Expr(_) | Operand::Checked(_) => false,
    }
}

/// Inserts a slot into a sorted float set.
fn insert_slot(floats: &mut Vec<LocalSlotId>, slot: LocalSlotId) {
    if let Err(position) = floats.binary_search(&slot) {
        floats.insert(position, slot);
    }
}

/// Returns the ordered float predicate that matches one PHP integer comparison.
fn ordered_predicate(predicate: CmpPredicate) -> CmpPredicate {
    match predicate {
        CmpPredicate::Slt => CmpPredicate::Olt,
        CmpPredicate::Sle => CmpPredicate::Ole,
        CmpPredicate::Sgt => CmpPredicate::Ogt,
        CmpPredicate::Sge => CmpPredicate::Oge,
        other => other,
    }
}

/// Replaces a carried slot with a boxed copy of its current raw integer.
fn publish_int(builder: &mut Builder<'_>, raw_slot: LocalSlotId, mixed_slot: LocalSlotId) {
    let raw = builder.emit_load_local(raw_slot, IrType::I64, PhpType::Int);
    publish_value(builder, raw, mixed_slot);
}

/// Releases the previous mixed occupant and stores a fresh box.
fn publish_value(builder: &mut Builder<'_>, value: ValueId, mixed_slot: LocalSlotId) {
    let boxed = emit(
        builder,
        Op::MixedBox,
        vec![value],
        None,
        IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed,
    );
    release_slot(builder, mixed_slot);
    builder.emit_store_local(mixed_slot, boxed);
}

/// Borrows the current mixed occupant of `slot`.
pub(super) fn load_mixed(builder: &mut Builder<'_>, slot: LocalSlotId) -> ValueId {
    builder
        .emit(
            Op::LoadLocal,
            vec![],
            Some(Immediate::LocalSlot(slot)),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            Ownership::Borrowed,
        )
        .expect("mixed load produces a value")
}

/// Drops the slot's current owner before a replacement store.
pub(super) fn release_slot(builder: &mut Builder<'_>, slot: LocalSlotId) {
    let _ = builder.emit(
        Op::ReleaseLocalSlot,
        vec![],
        Some(Immediate::LocalSlot(slot)),
        IrType::Void,
        PhpType::Void,
        Ownership::NonHeap,
    );
}

/// Emits a value instruction with the opcode's default effects and ownership.
pub(super) fn emit(
    builder: &mut Builder<'_>,
    op: Op,
    operands: Vec<ValueId>,
    immediate: Option<Immediate>,
    ir_type: IrType,
    php_type: PhpType,
) -> ValueId {
    builder
        .emit(
            op,
            operands,
            immediate,
            ir_type,
            php_type.clone(),
            Ownership::for_php_type(&php_type),
        )
        .expect("raw loop instruction produces a value")
}

/// Returns the wrapping integer opcode for one PHP arithmetic operation.
pub(super) fn integer_op(op: MixedNumericOp) -> Op {
    match op {
        MixedNumericOp::Add => Op::IAdd,
        MixedNumericOp::Sub => Op::ISub,
        MixedNumericOp::Mul => Op::IMul,
        MixedNumericOp::Pow => Op::IPow,
    }
}

/// Returns the float opcode used for the overflowing operation's exact PHP promotion.
fn float_op(op: MixedNumericOp) -> Op {
    match op {
        MixedNumericOp::Add => Op::FAdd,
        MixedNumericOp::Sub => Op::FSub,
        MixedNumericOp::Mul => Op::FMul,
        MixedNumericOp::Pow => Op::FPow,
    }
}
