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
    impl_collection_call_ops!();
    impl_reflection_ops!();
    impl_construction_raw_ops!();
    impl_lifecycle_scalar_ops!();
    impl_numeric_string_ops!();
}
