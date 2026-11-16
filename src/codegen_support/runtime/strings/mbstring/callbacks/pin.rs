//! Purpose:
//! Pins original boxed identities used by protected mbstring graph readers.
//!
//! Called from:
//! - V3 argument preparation and the native graph-entry callback.
//!
//! Key details:
//! - Heap ownership is acquired without copying or interpreting a mutable PHP value.
//! - Stack-backed descriptors need no owner because their caller frame spans the invocation.
//! - No PHP callback or destructor runs while acquiring the pin.

pub(super) use elephc_memoir::identity_pin::emit;
