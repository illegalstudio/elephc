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
mod chain;
mod cleanup_call;
mod class_implements;
mod dynamic_instanceof;
mod destructor_throw;
mod initialize;
mod matches;
mod rethrow_current;
mod previous;
mod throw_current;
mod uncaught_report;

pub use class_implements::emit_class_implements_interface;
pub use cleanup_frames::emit_exception_cleanup_frames;
pub use chain::emit_exception_chain;
pub use cleanup_call::emit_cleanup_invoke;
pub(crate) use cleanup_call::emit_guarded_cleanup_call;
pub use dynamic_instanceof::emit_dynamic_instanceof;
pub use destructor_throw::emit_destructor_throw;
pub use initialize::emit_throwable_initialize;
pub use matches::emit_exception_matches;
// The fixed-data emitter defines the string this helper prints; both must agree on its bytes.
pub(crate) use matches::{ABSENT_MESSAGE, ABSENT_MESSAGE_SYMBOL};
pub use rethrow_current::emit_rethrow_current;
pub use previous::emit_throwable_previous;
pub use throw_current::emit_throw_current;
pub use uncaught_report::emit_report_uncaught_exception;
pub(crate) use uncaught_report::UNCAUGHT_EXIT_STATUS;
