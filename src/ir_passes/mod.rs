//! Purpose:
//! IR-level analyses and transformations over EIR functions. Phase 06 starts
//! here with liveness analysis, the foundation for the linear-scan register
//! allocator. Later phases (peephole, CSE, LICM) live alongside it.
//!
//! Called from:
//! - `crate::pipeline::compile()` after AST-to-EIR lowering, before codegen.
//!
//! Key details:
//! - Passes are either read-only analyses that produce sidecar tables (e.g.
//!   liveness, allocation) or in-place transformations driven by the fixed-point
//!   `driver`, which re-validates each function after every pass in debug/test
//!   builds.

mod allocation;
mod branch_simplify;
mod cfg;
mod clobber;
mod const_fold;
mod cse;
mod dead_inst;
mod dead_store;
mod dominance;
mod driver;
mod identity_arith;
mod intervals;
mod licm;
mod liveness;
mod loops;
mod peephole;
mod regalloc;
mod rewrite;

#[cfg(test)]
mod tests;

pub use allocation::{Allocation, Location};
pub use dominance::{compute_dominance, DominanceInfo};
pub use driver::optimize_module;
pub use intervals::{build_intervals, LiveInterval};
pub use liveness::{compute_liveness, LivenessInfo};
pub use loops::{compute_loops, LoopInfo, NaturalLoop};
pub use regalloc::allocate_registers;
