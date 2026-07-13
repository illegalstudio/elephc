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
    impl_fake_collection_call_ops!();
    impl_fake_reflection_ops!();
    impl_fake_construction_raw_ops!();
    impl_fake_lifecycle_scalar_ops!();
    impl_fake_numeric_string_ops!();
}
