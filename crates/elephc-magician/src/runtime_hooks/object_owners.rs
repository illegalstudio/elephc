//! Purpose:
//! Keeps eval closure receiver owners visible to native GC without adding PHP properties.
//!
//! Called from:
//! - Closure construction through RuntimeValueOps and native collector/free callbacks.
//!
//! Key details:
//! - Entries own retained Mixed cells, not external GC roots.
//! - The runtime visits these edges and removes them before recursive child release.
//! - No eval context pointer is needed, so object release may outlive eval teardown.

use super::{ElephcRuntimeOps, EvalStatus, RuntimeCellHandle};
use crate::interpreter::RuntimeValueOps;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

static OBJECT_OWNERS: OnceLock<Mutex<HashMap<u64, Vec<usize>>>> = OnceLock::new();

/// Returns the registry of actual runtime-owned closure edges.
fn object_owners() -> &'static Mutex<HashMap<u64, Vec<usize>>> {
    OBJECT_OWNERS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Acquires one owner for every child and transfers those owners into the native object's lifetime.
pub(super) fn retain_object_children(
    values: &mut ElephcRuntimeOps,
    object: RuntimeCellHandle,
    children: &[RuntimeCellHandle],
) -> Result<(), EvalStatus> {
    if children.is_empty() { return Ok(()); }
    let identity = values.object_identity(object)?;
    let mut retained = Vec::with_capacity(children.len());
    for child in children {
        match values.retain(*child) {
            Ok(child) => retained.push(child.as_ptr() as usize),
            Err(status) => {
                release_children(retained);
                return Err(status);
            }
        }
    }
    let previous = match object_owners().lock() {
        Ok(mut owners) => owners.insert(identity, retained),
        Err(_) => {
            release_children(retained);
            return Err(EvalStatus::RuntimeFatal);
        }
    };
    if let Some(previous) = previous { release_children(previous); }
    Ok(())
}

/// Returns one borrowed child cell address, or zero when the object's edge list is exhausted.
pub(super) extern "C" fn object_gc_child(identity: u64, index: u64) -> usize {
    std::panic::catch_unwind(|| {
        let index = usize::try_from(index).ok()?;
        let owners = object_owners().lock().ok()?;
        owners.get(&identity)?.get(index).copied()
    }).ok().flatten().unwrap_or(0)
}

/// Detaches a final object's edge list before releasing children that may recursively free other objects.
pub(super) extern "C" fn release_object_children(identity: u64) {
    let _ = std::panic::catch_unwind(|| {
        let children = object_owners().lock().ok()
            .and_then(|mut owners| owners.remove(&identity));
        if let Some(children) = children { release_children(children); }
        crate::ffi::dynamic_destructors::forget_released_closure(identity);
    });
}

/// Releases detached owners without holding the registry mutex across runtime/destructor callbacks.
fn release_children(children: Vec<usize>) {
    for child in children {
        unsafe { super::externs::__elephc_eval_value_release(child as *mut super::RuntimeCell); }
    }
}
