//! Purpose:
//! IR-level analyses and transformations over EIR functions. Phase 06 starts
//! here with liveness analysis, the foundation for the linear-scan register
//! allocator. Later phases (peephole, CSE, LICM) live alongside it.
//!
//! Called from:
//! - `crate::pipeline::compile()` after AST-to-EIR lowering, before codegen.
//!
//! Key details:
//! - Passes are read-only or produce sidecar tables (e.g. liveness, allocation).
//!   They do not mutate `Function` in place.

mod allocation;
mod cfg;
mod clobber;
mod intervals;
mod liveness;
mod regalloc;

#[cfg(test)]
mod tests;

pub use allocation::{Allocation, Location};
pub use intervals::{build_intervals, LiveInterval};
pub use liveness::{compute_liveness, LivenessInfo};
pub use regalloc::allocate_registers;
