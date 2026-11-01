//! Purpose:
//! Emits pure suspended-frame ownership visitors from the ordinary cleanup inventory.
//!
//! Called from:
//! - Per-function exceptional cleanup emission, alongside its activation callback.
//!
//! Key details:
//! - Borrowed, moved and uninitialized owners follow the same predicates as cleanup.
//! - Visitor calls read slots without retaining values or executing PHP.
//! - Raw reference cells expose their owned payload; managed cells remain independent nodes.
//! - The visitor is data, not code: an interned `[count, (offset, state_offset, tag)...]` table
//!   and a tail call into `__rt_gc_frame_owners_table`, which keeps the callback ABI the
//!   activation record publishes (frame pointer, traversal context). Unrolled per function, the
//!   visitors were 1.22 M instructions of the Symfony `--web` build.

use super::*;

/// Enumerates current graph owners while borrowing the original function's frame pointer.
pub(super) fn emit(ctx: &mut FunctionContext<'_>, entry: &str) {
    let table = owner_table(ctx);
    let label = ctx.data.add_words(table.into_iter().map(DataWord::U64).collect());
    ctx.emitter.label_global(&format!("{entry}__gc_frame_owners"));
    let table_reg = abi::int_arg_reg_name(ctx.emitter.target, 2);
    abi::emit_symbol_address(ctx.emitter, table_reg, &label);
    abi::emit_jump(ctx.emitter, "__rt_gc_frame_owners_table");
}

/// The visitor's slot table: the owner count, then one `(offset, state_offset, tag)` triple per
/// owner, in the order the unrolled visitor walked them. `state_offset` is 0 when the slot has no
/// ref-cell state word.
fn owner_table(ctx: &FunctionContext<'_>) -> Vec<u64> {
    let mut entries: Vec<[u64; 3]> = Vec::new();
    for (_, slot, ty, offset) in function_cleanup_locals(ctx, None) {
        let ty = if ctx.numeric_local_slots.contains(&slot) { PhpType::Mixed } else { ty };
        if !graph_type(&ty) {
            continue;
        }
        let state = ctx.ref_cell_state_offset(slot).unwrap_or(0);
        entries.push([offset as u64, state as u64, 0]);
    }
    for (_, _, ty, offset) in ref_cell_owner_locals(ctx) {
        let tag = 128 | crate::codegen_support::runtime::reference_cells::payload_tag(&ty);
        entries.push([offset as u64, 0, tag as u64]);
    }
    let mut table = Vec::with_capacity(1 + entries.len() * 3);
    table.push(entries.len() as u64);
    table.extend(entries.into_iter().flatten());
    table
}

/// Distinguishes graph storage from scalar, string and foreign-handle representations.
fn graph_type(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::Array(_) | PhpType::AssocArray { .. } |
        PhpType::Object(_) | PhpType::Mixed | PhpType::Union(_) | PhpType::Callable | PhpType::Iterable)
}
