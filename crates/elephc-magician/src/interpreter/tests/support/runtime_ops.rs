//! Purpose:
//! RuntimeValueOps implementation for interpreter test fake values.
//! This keeps the large trait surface separate from test fixture type
//! declarations and assertion-only conversion helpers.
//!
//! Called from:
//! - `crate::interpreter::tests::support::FakeOps` through trait dispatch.
//!
//! Key details:
//! - Methods intentionally model only the runtime behavior covered by eval tests.
//! - Handles are fake stable cells and must not be freed by this implementation.

use super::*;

mod collection_calls;
mod mbstring;
mod construction_raw;
mod lifecycle_scalars;
mod numeric_string;
mod reflection;

use collection_calls::impl_fake_collection_call_ops;
use construction_raw::impl_fake_construction_raw_ops;
use lifecycle_scalars::impl_fake_lifecycle_scalar_ops;
use numeric_string::impl_fake_numeric_string_ops;
use reflection::impl_fake_reflection_ops;

impl RuntimeValueOps for FakeOps {
    /// Uses the real mbstring bridge for encoded strings and primitive adapters for other IDs.
    fn runtime_builtin_call(&mut self, id: elephc_builtin_contract::RuntimeBuiltinId, args: &[RuntimeCellHandle]) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        if id.is_mbstring() {
            return self.mbstring_builtin_call(id, args);
        }
        crate::interpreter::runtime_ops::default_builtin_call(self, id, args)
    }

    /// Transfers an exception produced by the test bridge to the eval catch context.
    fn take_pending_runtime_throwable(&mut self) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        Ok(self.pending_runtime_throwable.take())
    }

    impl_fake_collection_call_ops!();
    impl_fake_reflection_ops!();
    impl_fake_construction_raw_ops!();
    impl_fake_lifecycle_scalar_ops!();
    impl_fake_numeric_string_ops!();
}
