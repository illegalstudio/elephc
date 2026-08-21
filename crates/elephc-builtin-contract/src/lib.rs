//! Purpose:
//! Dependency-neutral PHP builtin identities and surface contracts shared by
//! the elephc compiler and the Magician runtime interpreter.
//!
//! Called from:
//! - `elephc::builtins` when joining compiler-specific checker/EIR semantics.
//! - `elephc_magician::interpreter::builtins` when joining eval dispatch hooks.
//!
//! Key details:
//! - This crate must remain independent of compiler AST, EIR, codegen, EvalIR,
//!   target ABI, and runtime-cell implementations.
//! - Builtin identities are derived from canonical lowercase PHP names and are
//!   validated for uniqueness by catalog consumers.

mod aot_profile;
mod callback_parameters;
mod catalog_data;
#[cfg(feature = "curl")]
mod catalog_curl;
mod catalog_surfaces;
mod eval_profile;
mod id;
mod registry;
mod requirements;
mod runtime_id;
mod spec;
mod support;

pub use aot_profile::{
    aot_signature, aot_signature_profile, AotSignatureOverrideReason, AotSignatureProfile,
};
pub use eval_profile::{
    eval_signature, eval_signature_profile, EvalSignatureOverrideReason, EvalSignatureProfile,
};
pub use id::BuiltinId;
pub use registry::{contracts, lookup, lookup_id};
pub use runtime_id::{runtime_builtin_id, RuntimeBuiltinId, RuntimeBuiltinStatus};
pub use spec::{
    Area, BuiltinContract, BuiltinKind, BuiltinRequirement, BuiltinSignature, DefaultSpec,
    ParamSpec, PassingMode, TypeSpec,
};
pub use support::{
    aot_support, backend_support, eval_execution, eval_support, BackendImplementation,
    BackendSupport, BuiltinBackend, EvalAdapterReason, EvalExecution, UnsupportedReason,
};
