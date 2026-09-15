//! Purpose:
//! Exposes read-only dynamic declaration entries to native Core inventories.
//!
//! Called from:
//! - Generated AOT get_defined_functions/get_defined_constants code.
//!
//! Key details:
//! - Names and cells remain borrowed from the live, unmodified eval context.
//! - Native consumers copy names and retain cells before storing their snapshot.

use crate::abi::{ElephcEvalContext, ABI_VERSION};
use crate::value::RuntimeCell;

/// Borrowed UTF-8 name and optional boxed constant payload for one dynamic declaration.
#[repr(C)]
#[derive(Default)]
pub struct ElephcEvalInventoryEntry {
    pub name_ptr: *const u8,
    pub name_len: u64,
    pub value_cell: *mut RuntimeCell,
}

/// Enumerates eval-only functions (kind 0) or dynamic constants (kind 1).
///
/// # Safety
/// `ctx` must be null or a valid context; `out` must be null or writable entry storage.
/// Returned pointers are borrowed until the context is mutated or freed.
#[no_mangle]
pub unsafe extern "C" fn __elephc_eval_inventory_entry(
    ctx: *const ElephcEvalContext,
    kind: u64,
    index: u64,
    out: *mut ElephcEvalInventoryEntry,
) -> i32 {
    std::panic::catch_unwind(|| {
        let Some(out) = (unsafe { out.as_mut() }) else { return 0 };
        *out = ElephcEvalInventoryEntry::default();
        let Some(ctx) = (unsafe { ctx.as_ref() }) else { return 0 };
        if ctx.abi_version() != ABI_VERSION { return 0; }
        let Ok(index) = usize::try_from(index) else { return 0 };
        let entry = match kind {
            0 => ctx.function_inventory_entry(index).map(|name| (name, std::ptr::null_mut())),
            1 => ctx.constant_inventory_entry(index).map(|(name, cell)| (name, cell.as_ptr())),
            _ => None,
        };
        let Some((name, cell)) = entry else { return 0 };
        out.name_ptr = name.as_ptr();
        out.name_len = name.len() as u64;
        out.value_cell = cell;
        1
    }).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::RuntimeCellHandle;

    /// Borrowed constant entries are sorted, keep cell identity, and terminate cleanly.
    #[test]
    fn dynamic_inventory_entries_borrow_sorted_constants() {
        let mut context = ElephcEvalContext::new();
        let mut payload = 42_u64;
        let cell = RuntimeCellHandle::from_raw((&mut payload as *mut u64).cast());
        assert!(context.define_constant("Z_LAST", cell));
        assert!(context.define_constant("A_FIRST", cell));
        let mut entry = ElephcEvalInventoryEntry::default();
        unsafe {
            assert_eq!(__elephc_eval_inventory_entry(&context, 1, 0, &mut entry), 1);
            assert_eq!(std::slice::from_raw_parts(entry.name_ptr, entry.name_len as usize), b"A_FIRST");
            assert_eq!(entry.value_cell, cell.as_ptr());
            assert_eq!(__elephc_eval_inventory_entry(&context, 1, 2, &mut entry), 0);
            assert!(entry.name_ptr.is_null());
            assert_eq!(__elephc_eval_inventory_entry(&context, 9, 0, &mut entry), 0);
            assert_eq!(__elephc_eval_inventory_entry(std::ptr::null(), 1, 0, &mut entry), 0);
        }
    }
}
