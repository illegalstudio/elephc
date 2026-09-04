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
mod class_spec;
mod constant_spec;
mod catalog_classes;
mod catalog_constants;
mod catalog_constants_curl;
mod catalog_data;
#[cfg(feature = "curl")]
mod catalog_curl;
mod catalog_surfaces;
mod eval_profile;
mod id;
mod module;
mod php_version;
mod registry;
mod requirements;
mod runtime_id;
mod spec;
mod support;
mod symbol_registry;

pub use aot_profile::{
    aot_signature, aot_signature_profile, AotSignatureOverrideReason, AotSignatureProfile,
};
pub use eval_profile::{
    eval_signature, eval_signature_profile, EvalSignatureOverrideReason, EvalSignatureProfile,
};
pub use class_spec::{ClassContract, ClassKind, ClassRoute};
pub use constant_spec::{ConstType, ConstValue, ConstantContract, ConstantRoute};
pub use id::BuiltinId;
pub use module::PhpModule;
pub use php_version::PhpVersion;
pub use registry::{contracts, lookup, lookup_id};
pub use symbol_registry::{classes, constants, lookup_class, lookup_constant};
pub use runtime_id::{runtime_builtin_id, RuntimeBuiltinId, RuntimeBuiltinStatus};
pub use spec::{
    Area, BuiltinContract, BuiltinKind, BuiltinRequirement, BuiltinSignature, DefaultSpec,
    ParamSpec, PassingMode, TypeSpec,
};
pub use support::{
    aot_class_support, aot_constant_support, aot_support, backend_support, eval_class_support,
    eval_constant_support, eval_execution, eval_support, BackendImplementation,
    BackendSupport, BuiltinBackend, EvalAdapterReason, EvalExecution, UnsupportedReason,
};
