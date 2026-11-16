//! Purpose:
//! Reexports stack memory helpers through the compiler runtime namespace.
//!
//! Called from:
//! - Fiber runtime orchestration and guard sizing.
//!
//! Key details:
//! - Target mapping policy stays in the compiler host adapter.

pub use elephc_memoir::stacks::{emit_fiber_alloc_stack, emit_fiber_free_stack};
pub(super) use elephc_memoir::stacks::FIBER_GUARD_PAGE_SIZE;
