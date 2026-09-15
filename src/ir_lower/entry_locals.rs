//! Purpose:
//! Identifies process locals whose initial storage is populated by the native entry point.
//!
//! Called from:
//! - Local assignment and increment/decrement lowering before the first write.
//!
//! Key details:
//! - Only main receives implicit argc/argv values; names in user functions remain ordinary locals.
//! - Declaring these slots before writes preserves initial reads and releases displaced owners.

use super::context::LoweringContext;
use crate::types::PhpType;

/// Records the entry-point owner before an assignment can treat a process local as uninitialized.
pub(super) fn prepare_process_local_for_write(ctx: &mut LoweringContext<'_, '_>, name: &str) {
    if !ctx.in_main { return; }
    let ty = match name {
        "argc" => PhpType::Int,
        "argv" => PhpType::Array(Box::new(PhpType::Str)),
        _ => return,
    };
    ctx.declare_local(name, ty);
    ctx.mark_local_initialized(name);
}
