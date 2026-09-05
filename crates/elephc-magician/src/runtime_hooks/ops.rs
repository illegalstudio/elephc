//! Purpose:
//! Implements RuntimeValueOps by delegating each eval value operation to the
//! generated elephc runtime wrapper symbols.
//!
//! Called from:
//! - `crate::interpreter` when executing EvalIR in non-test builds.
//!
//! Key details:
//! - Every returned runtime pointer is checked before becoming a handle.
//! - Temporary argument arrays are released after object and method bridge calls.

use super::externs::*;
use super::tags::{bitwise_op_tag, compare_op_tag};
use super::ElephcRuntimeOps;
use elephc_builtin_contract::{RuntimeBuiltinId, RuntimeBuiltinStatus};
use crate::errors::EvalStatus;
use crate::eval_ir::EvalBinOp;
use crate::interpreter::RuntimeValueOps;
use crate::value::{RuntimeCell, RuntimeCellHandle};

mod collection_calls;
mod construction_raw;
mod lifecycle_scalars;
mod native_results;
mod numeric_string;
mod reflection;

use collection_calls::impl_collection_call_ops;
use construction_raw::impl_construction_raw_ops;
use lifecycle_scalars::impl_lifecycle_scalar_ops;
use numeric_string::impl_numeric_string_ops;
use reflection::impl_reflection_ops;

#[cfg(not(test))]
impl RuntimeValueOps for ElephcRuntimeOps {
    /// Dispatches a shareable builtin through the versioned generated-runtime C ABI.
    fn runtime_builtin_call(
        &mut self,
        id: RuntimeBuiltinId,
        args: &[RuntimeCellHandle],
    ) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        if !id.supports_arity(args.len()) {
            return Ok(None);
        }
        let raw_args = args.iter().map(|arg| arg.as_ptr()).collect::<Vec<_>>();
        let mut result = std::ptr::null_mut();
        let status = unsafe {
            __elephc_runtime_builtin_call_v1(
                id.as_u32(),
                raw_args.as_ptr(),
                raw_args.len() as u64,
                self.context.cast(),
                &mut result,
            )
        };
        match RuntimeBuiltinStatus::from_i32(status) {
            Some(RuntimeBuiltinStatus::Success) => Self::handle(result).map(Some),
            Some(RuntimeBuiltinStatus::Unsupported) => Ok(None),
            Some(RuntimeBuiltinStatus::PendingThrowable) => Err(EvalStatus::UncaughtThrowable),
            Some(RuntimeBuiltinStatus::RuntimeFatal) | None => Err(EvalStatus::RuntimeFatal),
        }
    }

    impl_collection_call_ops!();
    impl_reflection_ops!();
    impl_construction_raw_ops!();
    impl_lifecycle_scalar_ops!();
    impl_numeric_string_ops!();

    /// Retains the original boxed handler stored in the compiled runtime's PCNTL table.
    fn pcntl_aot_signal_handler(
        &mut self,
        signal: i64,
    ) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        let pointer = unsafe { __elephc_eval_pcntl_aot_signal_handler(signal) };
        if pointer.is_null() {
            Ok(None)
        } else {
            Self::handle(pointer).map(Some)
        }
    }
}
