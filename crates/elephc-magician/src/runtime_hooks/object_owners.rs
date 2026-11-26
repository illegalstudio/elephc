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

use super::{ElephcRuntimeOps, EvalStatus, RuntimeCell, RuntimeCellHandle};
use crate::interpreter::RuntimeValueOps;
use elephc_memoir::object_owners;
use elephc_memoir::runtime_owners::{RuntimeOwnerBatch, StagedRuntimeOwners};
use super::owner_frames::with_runtime_owners;

/// Acquires one owner for every child and transfers those owners into the native object's lifetime.
pub(super) fn retain_object_children(
    values: &mut ElephcRuntimeOps,
    object: RuntimeCellHandle,
    children: &[RuntimeCellHandle],
) -> Result<(), EvalStatus> {
    if children.is_empty() { return Ok(()); }
    let identity = values.object_identity(object)?;
    let retained = StagedRuntimeOwners::default();
    with_runtime_owners(&retained, || {
        for child in children {
            match values.retain(*child) {
                Ok(child) => { retained.stage(child); }
                Err(status) => {
                    values.release_owner_batch(retained.take_all())?;
                    return Err(status);
                }
            }
        }
        let retained = retained.take_all().transfer().into_iter()
            .map(|cell| cell.as_ptr() as usize).collect();
        let previous = match object_owners::replace(identity, retained) {
            Ok(previous) => previous,
            Err(retained) => {
                values.release_owner_batch(RuntimeOwnerBatch::new(owned_children(retained)))?;
                return Err(EvalStatus::RuntimeFatal);
            }
        };
        if let Some(previous) = previous {
            values.release_owner_batch(RuntimeOwnerBatch::new(owned_children(previous)))?;
        }
        Ok(())
    })
}

/// Returns one borrowed child cell address, or zero when the object's edge list is exhausted.
pub(super) extern "C" fn object_gc_child(identity: u64, index: u64) -> usize {
    std::panic::catch_unwind(|| {
        let index = usize::try_from(index).ok()?;
        object_owners::child(identity, index)
    }).ok().flatten().unwrap_or(0)
}

/// Releases every detached child, returning an owned Throwable only after Rust cleanup has finished.
pub(super) extern "C" fn release_object_children(identity: u64) -> *mut RuntimeCell {
    std::panic::catch_unwind(|| {
        let children = object_owners::take(identity);
        let thrown = super::release::release_native_owner_batch(
            RuntimeOwnerBatch::new(owned_children(children.unwrap_or_default())),
            None,
        );
        crate::ffi::dynamic_destructors::forget_released_object(identity);
        thrown.map_or(std::ptr::null_mut(), RuntimeCellHandle::as_ptr)
    }).unwrap_or(std::ptr::null_mut())
}

/// Converts a detached registry vector into owned handles without keeping the registry locked.
fn owned_children(children: Vec<usize>) -> impl Iterator<Item = RuntimeCellHandle> {
    children.into_iter().map(|child| RuntimeCellHandle::from_raw(child as *mut RuntimeCell))
}
