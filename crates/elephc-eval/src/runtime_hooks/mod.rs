//! Purpose:
//! Bridges EvalIR value operations to elephc runtime values.
//! The module wires the stateless adapter type while focused submodules own
//! generated C-ABI symbols, trait operations, and opcode mappings.
//!
//! Called from:
//! - `crate::ffi::execute::__elephc_eval_execute()` in non-test builds.
//!
//! Key details:
//! - The wrapper symbols adapt to elephc's target-specific internal helper ABI.
//! - Unit tests do not link the generated runtime object, so real hooks compile
//!   only outside `cfg(test)`.

#[cfg(not(test))]
mod externs;
#[cfg(not(test))]
mod ops;
#[cfg(not(test))]
mod tags;

#[cfg(not(test))]
use crate::errors::EvalStatus;
#[cfg(not(test))]
use crate::value::{RuntimeCell, RuntimeCellHandle};
#[cfg(not(test))]
use externs::{
    __elephc_eval_value_array_new, __elephc_eval_value_array_set, __elephc_eval_value_int,
};

/// Runtime hook adapter that produces and consumes boxed elephc Mixed cells.
#[cfg(not(test))]
pub struct ElephcRuntimeOps;

#[cfg(not(test))]
impl ElephcRuntimeOps {
    /// Creates a new stateless runtime hook adapter.
    pub const fn new() -> Self {
        Self
    }

    /// Converts a runtime wrapper result into an interpreter handle.
    fn handle(ptr: *mut RuntimeCell) -> Result<RuntimeCellHandle, EvalStatus> {
        if ptr.is_null() {
            Err(EvalStatus::RuntimeFatal)
        } else {
            Ok(RuntimeCellHandle::from_raw(ptr))
        }
    }

    /// Packs source-order argument cells into the boxed eval array ABI.
    fn arg_array(args: Vec<RuntimeCellHandle>) -> Result<RuntimeCellHandle, EvalStatus> {
        let arg_array = unsafe { __elephc_eval_value_array_new(args.len() as u64) };
        let arg_array = Self::handle(arg_array)?;
        for (index, value) in args.into_iter().enumerate() {
            let index = Self::handle(unsafe { __elephc_eval_value_int(index as i64) })?;
            Self::handle(unsafe {
                __elephc_eval_value_array_set(arg_array.as_ptr(), index.as_ptr(), value.as_ptr())
            })?;
        }
        Ok(arg_array)
    }
}
