//! Purpose:
//! Neutralizes the blocks that only code after a diverging operand reaches.
//!
//! Called from:
//! - `crate::ir_lower::program` on every body of a finished module, before it is validated.
//!
//! Key details:
//! - `Builder` opens a block named [`DEAD_BLOCK`] when lowering keeps emitting after a block
//!   ended (the cast, argument list or interpolation around `$null->method()`, which always
//!   throws). Those instructions can never run, and some of them are not even well typed (a
//!   `Void` operand where a string is wanted), so the validator and the backends must not see
//!   them.
//! - Only blocks reachable from a dead block and from nothing else are touched; the unreachable
//!   blocks lowering has always left behind (a join after two returning arms) keep their text.
//! - Block ids and the instruction arena stay as they are: a pruned block's instructions become
//!   `nop`s (their result values stay defined) and it ends in `unreachable`.

use super::block::Terminator;
use super::function::Function;
use super::instr::Op;
use super::validator::successors;
use crate::fast_hash::FastSet;

/// Name of the block `Builder` opens for code that follows a block's terminator.
pub(super) const DEAD_BLOCK: &str = "dead";

/// Neutralizes every block only the dead code after a diverging operand reaches.
///
/// A block the dead code reaches stays when something else reaches it too: the join after a
/// `try` is also the end of a `catch`, whose handler block no terminator edge leads to (the
/// unwinder enters it), so reachability from the entry alone would cut it off.
pub(crate) fn prune_dead_blocks(function: &mut Function) {
    if !function.blocks.iter().any(|block| block.name == DEAD_BLOCK) {
        return;
    }
    let dead_roots = function
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, block)| block.name == DEAD_BLOCK)
        .map(|(index, _)| index);
    let from_dead = reachable_from(function, dead_roots);
    let entered_from_elsewhere: Vec<usize> = function
        .blocks
        .iter()
        .enumerate()
        .filter(|(index, _)| !from_dead.contains(index))
        .filter_map(|(_, block)| block.terminator.as_ref())
        .flat_map(successors)
        .map(|target| target.as_raw() as usize)
        .filter(|target| from_dead.contains(target))
        .collect();
    let kept = reachable_from(function, entered_from_elsewhere.into_iter());
    for index in from_dead.difference(&kept) {
        let instructions = function.blocks[*index].instructions.clone();
        for id in instructions {
            // Like the later `branch_simplify` sweep: a `nop` keeps its result value, so the
            // value table stays consistent while nothing is left to type-check or emit.
            if let Some(inst) = function.instruction_mut(id) {
                inst.op = Op::Nop;
                inst.operands.clear();
                inst.immediate = None;
                inst.effects = Op::Nop.default_effects();
            }
        }
        function.blocks[*index].terminator = Some(Terminator::Unreachable);
    }
}

/// Returns the indexes of the blocks reachable from `roots` along terminator edges.
fn reachable_from(function: &Function, roots: impl Iterator<Item = usize>) -> FastSet<usize> {
    let mut seen: FastSet<usize> = FastSet::default();
    let mut work: Vec<usize> = roots.collect();
    while let Some(index) = work.pop() {
        if !seen.insert(index) {
            continue;
        }
        if let Some(term) = function.blocks.get(index).and_then(|block| block.terminator.as_ref()) {
            work.extend(successors(term).into_iter().map(|target| target.as_raw() as usize));
        }
    }
    seen
}
