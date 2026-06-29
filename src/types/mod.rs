//! Purpose:
//! Provides the public type-system entry point used by the compile pipeline.
//! Re-exports type models, signatures, schemas, call-argument planning, warnings, and checker results.
//!
//! Called from:
//! - `crate::pipeline::compile()`
//!
//! Key details:
//! - `check()` runs after name resolution and before optimization/codegen so later passes receive canonical type metadata.

/// Type checking module.
pub mod checker;
/// Trait flattening and resolution.
pub mod traits;
/// Array key type inference, normalization, and PHP integer/string coercion rules.
mod array_keys;
/// PHP array extension integer constants.
pub(crate) mod array_constants;
/// Call argument planning: named, positional, and spread semantics.
pub(crate) mod call_args;
/// Fiber/stack introspection for async and coroutine analysis.
pub(crate) mod fibers;
/// `ext/date` integer constants (e.g. `SUNFUNCS_RET_*`).
pub(crate) mod date_constants;
/// C FFI type mapping utilities.
mod ffi;
/// JSON literal constant type inference.
pub(crate) mod json_constants;
/// PHP type model and type environment for tracking variable types.
mod model;
/// Preg/PCRE flag constants shared by checker and codegen.
pub(crate) mod preg_constants;
/// Type checker result types and the `check` entry point.
mod result;
/// Class, interface, enum, and FFI schema definitions.
mod schema;
/// PHP `PHP_ROUND_HALF_*` mode constants for `round()`.
pub(crate) mod round_constants;
/// Function signature representation and builtin signature helpers.
mod signatures;
/// PHP `SORT_*` flag constants for the sort builtins.
pub(crate) mod sort_constants;
pub(crate) mod stream_constants;
/// Type checker diagnostics and warnings.
mod warnings;

pub(crate) use array_keys::{
    array_key_type_from_value_type, is_php_integer_array_key, merge_array_key_types,
    normalized_array_key_type, parse_php_string_offset_literal,
    static_array_key_forces_hash_storage,
};
pub use ffi::{ctype_stack_size, ctype_to_php_type, packed_type_size};
pub use model::{PhpType, TypeEnv};
pub use result::{check_with_target, CheckResult};
pub use schema::{
    AttrArgEntry, AttrArgValue, AttrKey, ClassInfo, EnumCaseInfo, EnumCaseValue, EnumInfo,
    ExternClassInfo, ExternFieldInfo, ExternFunctionSig, InterfaceInfo, PackedClassInfo,
    PackedFieldInfo, PropertyHookContract,
};
pub(crate) use signatures::{
    builtin_call_sig, callable_wrapper_sig, first_class_callable_builtin_sig,
};
pub use signatures::FunctionSig;

/// Returns true when an `array_slice()` call requests key preservation via a literal `true` fourth
/// argument. Shared by the type checker, codegen emitter, and codegen local-type inference so all
/// three agree on the result shape (an integer-keyed associative array). A drift between callers
/// would disagree on Array-vs-AssocArray storage, which is a heap-shape mismatch.
pub(crate) fn array_slice_literal_preserve_keys(args: &[crate::parser::ast::Expr]) -> bool {
    matches!(
        args.get(3).map(|arg| &arg.kind),
        Some(crate::parser::ast::ExprKind::BoolLiteral(true))
    )
}

/// Type checks the program after name resolution. Returns `CheckResult` with type
/// metadata, function/class/interface/enum info, warnings, required libraries, and the
/// internal `Mixed` type for heterogeneous assoc-array values. Runs before optimization/codegen.
#[allow(dead_code)]
pub fn check(
    program: &crate::parser::ast::Program,
) -> Result<CheckResult, crate::errors::CompileError> {
    result::check(program)
}
