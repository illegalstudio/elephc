//! Purpose:
//! Groups iterative loop lowering and loop-exit handling for break and continue.
//! Keeps loop label management separate from other control-flow statements.
//!
//! Called from:
//! - `crate::codegen::stmt::control_flow`
//!
//! Key details:
//! - Break and continue must respect nested loop depth and cleanup skipped switch/loop stack state.

mod exits;
mod iterative;

pub(super) use exits::{emit_break_stmt, emit_continue_stmt, emit_return_stmt};
pub(super) use iterative::{emit_do_while_stmt, emit_for_stmt, emit_while_stmt};
