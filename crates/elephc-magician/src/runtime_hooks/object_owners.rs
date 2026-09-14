//! Purpose:
//! Keeps eval closure receiver owners visible to native GC without PHP properties.
//!
//! Called from:
//! - Closure construction and native object GC/final-release callbacks.
//!
//! Key details:
//! - Each entry owns one retained boxed Mixed receiver cell.
//! - Final release detaches the entry before recursive receiver destruction.
//! - The ordinary object-free callback drops closure metadata after all child cleanup.

use super::{ElephcRuntimeOps, EvalStatus, RuntimeCellHandle};
use crate::interpreter::RuntimeValueOps;
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

static OBJECT_OWNERS: OnceLock<Mutex<HashMap<u64, usize>>> = OnceLock::new();

/// Returns the registry of native object-owned closure receiver cells.
fn object_owners() -> &'static Mutex<HashMap<u64, usize>> {
    OBJECT_OWNERS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Recovers the registry after an unrelated callback panic so final release can still drain it.
fn locked_object_owners() -> MutexGuard<'static, HashMap<u64, usize>> {
    object_owners()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Acquires the single receiver edge transferred into one closure object's lifetime.
pub(super) fn retain_object_children(
    values: &mut ElephcRuntimeOps,
    object: RuntimeCellHandle,
    children: &[RuntimeCellHandle],
) -> Result<(), EvalStatus> {
    let [child] = children else {
        return if children.is_empty() {
            Ok(())
        } else {
            Err(EvalStatus::RuntimeFatal)
        };
    };
    let identity = values.object_identity(object)?;
    let child_handle = *child;
    let child = child_handle.as_ptr() as usize;
    let mut owners = locked_object_owners();
    if owners.get(&identity).copied() == Some(child) {
        return Ok(());
    }
    if owners.contains_key(&identity) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let retained = values.retain(child_handle)?;
    let retained = retained.as_ptr() as usize;
    let inserted = owners.insert(identity, retained);
    debug_assert!(inserted.is_none());
    Ok(())
}

/// Returns the one borrowed child cell, or zero when enumeration is complete.
pub(super) extern "C" fn object_gc_child(identity: u64, index: u64) -> usize {
    std::panic::catch_unwind(|| {
        if index != 0 {
            return 0;
        }
        locked_object_owners()
            .get(&identity)
            .copied()
            .unwrap_or(0)
    })
    .unwrap_or(0)
}

/// Detaches and releases one receiver without holding the registry lock.
pub(super) extern "C" fn release_object_children(identity: u64) -> u64 {
    std::panic::catch_unwind(|| {
        let child = locked_object_owners().remove(&identity);
        child.map_or(0, |child| unsafe {
            super::externs::__elephc_eval_value_release_protected(
                child as *mut super::RuntimeCell,
            )
        })
    })
    .unwrap_or(0)
}
