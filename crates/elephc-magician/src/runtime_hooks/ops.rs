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

    /// Gets or replaces the process-wide PHP error-reporting mask.
    fn runtime_error_reporting(
        &mut self,
        replacement: Option<i64>,
    ) -> Result<i64, EvalStatus> {
        Ok(unsafe {
            __elephc_eval_error_reporting(
                replacement.unwrap_or_default(),
                u64::from(replacement.is_some()),
            )
        })
    }

    /// Installs an eval-owned callback in the native user-error dispatcher.
    fn runtime_error_handler_set(
        &mut self,
        callback: Option<RuntimeCellHandle>,
        levels: i64,
    ) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        let mut previous = std::ptr::null_mut();
        let status = unsafe {
            __elephc_eval_error_handler_set(
                self.context.cast(),
                callback.map_or(std::ptr::null_mut(), RuntimeCellHandle::as_ptr),
                levels,
                &mut previous,
            )
        };
        if status != EvalStatus::Ok.code() {
            return Err(EvalStatus::RuntimeFatal);
        }
        Ok((!previous.is_null()).then(|| RuntimeCellHandle::from_raw(previous)))
    }

    /// Restores the prior native user error handler and releases eval-owned state.
    fn runtime_error_handler_restore(&mut self) -> Result<(), EvalStatus> {
        let status = unsafe { __elephc_eval_error_handler_restore() };
        if status == EvalStatus::Ok.code() {
            Ok(())
        } else {
            Err(EvalStatus::RuntimeFatal)
        }
    }

    /// Invokes the native user error handler through its uniform descriptor ABI.
    fn runtime_error_handler_dispatch(
        &mut self,
        level: i64,
        args: &[RuntimeCellHandle],
    ) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        let mut arg_array = self.array_new(args.len())?;
        for (index, value) in args.iter().copied().enumerate() {
            let key = self.int(i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
            let retained = self.retain(value)?;
            arg_array = self.array_set(arg_array, key, retained)?;
        }
        let mut result = std::ptr::null_mut();
        let mut invoked = 0u64;
        let status = unsafe {
            __elephc_eval_error_handler_dispatch(
                level,
                arg_array.as_ptr(),
                &mut result,
                &mut invoked,
            )
        };
        self.release(arg_array)?;
        if status != EvalStatus::Ok.code() {
            return Err(EvalStatus::RuntimeFatal);
        }
        if invoked == 0 {
            return Ok(None);
        }
        Self::handle(result).map(Some)
    }

    /// Installs an eval-owned callback in the native terminal exception dispatcher.
    fn runtime_exception_handler_set(
        &mut self,
        callback: Option<RuntimeCellHandle>,
    ) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        let mut previous = std::ptr::null_mut();
        let status = unsafe {
            __elephc_eval_exception_handler_set(
                self.context.cast(),
                callback.map_or(std::ptr::null_mut(), RuntimeCellHandle::as_ptr),
                &mut previous,
            )
        };
        if status != EvalStatus::Ok.code() {
            return Err(EvalStatus::RuntimeFatal);
        }
        Ok((!previous.is_null()).then(|| RuntimeCellHandle::from_raw(previous)))
    }

    /// Restores the prior native exception handler and releases eval-owned state.
    fn runtime_exception_handler_restore(&mut self) -> Result<(), EvalStatus> {
        let status = unsafe { __elephc_eval_exception_handler_restore() };
        if status == EvalStatus::Ok.code() {
            Ok(())
        } else {
            Err(EvalStatus::RuntimeFatal)
        }
    }

    impl_collection_call_ops!();
    impl_reflection_ops!();
    impl_construction_raw_ops!();
    impl_lifecycle_scalar_ops!();
    impl_numeric_string_ops!();
}
