//! Purpose:
//! Collects exception runtime emitters and re-exports the helper emission surface.
//! The module groups throw, rethrow, cleanup, dynamic instanceof, and catch matching helpers.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via the exception runtime section.
//!
//! Key details:
//! - Exception matching and unwinding must keep handler-stack, call-frame cleanup, and class metadata invariants aligned.

mod cleanup_frames;
mod class_implements;
mod dynamic_instanceof;
mod matches;
mod rethrow_current;
mod throw_current;
mod uncaught_date_traces;
mod uncaught_report;

pub use class_implements::emit_class_implements_interface;
pub use cleanup_frames::emit_exception_cleanup_frames;
pub use dynamic_instanceof::emit_dynamic_instanceof;
pub use matches::emit_exception_matches;
// The fixed-data emitter defines the string this helper prints; both must agree on its bytes.
pub(crate) use matches::{ABSENT_MESSAGE, ABSENT_MESSAGE_SYMBOL};
pub use rethrow_current::emit_rethrow_current;
pub use throw_current::emit_throw_current;
pub use uncaught_report::emit_report_uncaught_exception;
pub(crate) use uncaught_report::UNCAUGHT_EXIT_STATUS;
