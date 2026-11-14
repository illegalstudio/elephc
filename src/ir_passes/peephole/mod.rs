//! Purpose:
//! Local peephole rewrites over EIR: box/unbox cancellation, redundant
//! `Move`/`Borrow` cleanup, scalar load/store forwarding, paired
//! acquire/release cancellation, and string-literal concat folding.
//!
//! Called from:
//! - The fixed-point pass driver in `crate::ir_passes::driver`.
//!
//! Key details:
//! - Every rewrite is dominance-safe, validator-clean, and PHP-equivalent
//!   (refcount-balanced, no observable change). Patterns collect into a shared
//!   `Rewrites` accumulator, then a single apply phase neutralizes folded
//!   instructions, converts folded concats to `ConstStr`, and redirects uses via
//!   `replace_all_uses`. Nested cases converge across driver sweeps.

use crate::fast_hash::FastMap as HashMap;

use crate::ir::{DataPool, Function, Immediate, InstId, Op, Ownership, ValueId};

use super::driver::IrPass;
use super::rewrite::{neutralize_to_nop, replace_all_uses, resolve_chains};

mod acquire_release;
#[cfg(test)]
pub(crate) use acquire_release::OWNER_TRANSFERS;
mod box_unbox;
mod forward_ops;
mod load_store;
mod str_concat;

/// Local peephole rewrite pass. See the module docs for the pattern set.
pub struct Peephole;

impl IrPass for Peephole {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "peephole"
    }

    /// Runs every peephole pattern over one function, returning true on change.
    /// Patterns only collect rewrite intents; the single apply phase commits
    /// them so the patterns never observe each other's partial mutation.
    fn run(&self, function: &mut Function, data: &mut DataPool) -> bool {
        let mut rewrites = Rewrites::default();
        box_unbox::collect(function, &mut rewrites);
        forward_ops::collect(function, &mut rewrites);
        acquire_release::collect(function, &mut rewrites);
        load_store::collect(function, &mut rewrites);
        str_concat::collect(function, data, &mut rewrites);
        if rewrites.is_empty() {
            return false;
        }
        apply(function, data, &rewrites);
        true
    }
}

/// Owner-transfer finalization: `b = acquire a` with the owned temporary `a` released later in
/// the same block and used by nothing else becomes `b = a` (see
/// `acquire_release::collect_transfers`). It runs once after the fixed point: the buffer passes
/// inside it match the acquire shape, and rewriting it earlier kept them from resolving.
pub struct OwnerTransfer;

impl IrPass for OwnerTransfer {
    /// Returns the stable pass name used in driver diagnostics.
    fn name(&self) -> &'static str {
        "owner-transfer"
    }

    /// Keeps the rewrite out of the fixed point the buffer passes run in.
    fn run_after_fixed_point(&self) -> bool {
        true
    }

    /// Commits every owner transfer of the function at once.
    fn run(&self, function: &mut Function, data: &mut DataPool) -> bool {
        let mut rewrites = Rewrites::default();
        acquire_release::collect_transfers(function, &mut rewrites);
        if rewrites.is_empty() {
            return false;
        }
        apply(function, data, &rewrites);
        true
    }
}

/// Accumulated rewrites collected by the peephole patterns before the apply
/// phase commits them.
#[derive(Default)]
struct Rewrites {
    /// Fold-to-operand redirections: each result value maps to the surviving
    /// value its uses are redirected to (`replace_all_uses`).
    rauw: HashMap<ValueId, ValueId>,
    /// Instructions neutralized to `Nop` (folded ops, cancelled releases, dead
    /// stores).
    nops: Vec<InstId>,
    /// `StrConcat` instructions to convert in place to `ConstStr` with the given
    /// concatenated literal.
    str_folds: Vec<(InstId, String)>,
}

impl Rewrites {
    /// Returns true when no pattern recorded any rewrite this sweep.
    fn is_empty(&self) -> bool {
        self.rauw.is_empty() && self.nops.is_empty() && self.str_folds.is_empty()
    }
}

/// Commits the collected rewrites: neutralize folded instructions, convert
/// folded concats to interned `ConstStr` literals, then redirect every use of a
/// folded result to its terminal replacement.
fn apply(function: &mut Function, data: &mut DataPool, rewrites: &Rewrites) {
    let resolved = resolve_chains(&rewrites.rauw);
    for &inst_id in &rewrites.nops {
        if let Some(inst) = function.instruction_mut(inst_id) {
            neutralize_to_nop(inst);
        }
    }
    for (inst_id, text) in &rewrites.str_folds {
        convert_to_const_str(function, data, *inst_id, text);
    }
    replace_all_uses(function, &resolved);
}

/// Converts a folded `StrConcat` instruction in place into a `ConstStr` literal,
/// interning `text` into the module pool and marking both the instruction and
/// its result value `Persistent` so cleanup paths never free the data-section
/// literal.
fn convert_to_const_str(function: &mut Function, data: &mut DataPool, inst_id: InstId, text: &str) {
    let data_id = data.intern_string(text);
    let result = match function.instruction_mut(inst_id) {
        Some(inst) => {
            inst.op = Op::ConstStr;
            inst.operands.clear();
            inst.immediate = Some(Immediate::Data(data_id));
            inst.result_ownership = Ownership::Persistent;
            inst.effects = Op::ConstStr.default_effects();
            inst.result
        }
        None => return,
    };
    if let Some(result) = result {
        if let Some(value) = function.values.get_mut(result.as_raw() as usize) {
            value.ownership = Ownership::Persistent;
        }
    }
}
