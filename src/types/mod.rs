pub mod checker;
pub mod traits;
mod array_keys;
pub(crate) mod call_args;
mod ffi;
mod model;
mod result;
mod schema;
mod signatures;
mod warnings;

pub(crate) use array_keys::{
    array_key_type_from_value_type, is_php_integer_array_key, merge_array_key_types,
    normalized_array_key_type,
};
pub use ffi::{ctype_stack_size, ctype_to_php_type, packed_type_size};
pub use model::{PhpType, TypeEnv};
pub use result::{check_with_target, CheckResult};
pub use schema::{
    ClassInfo, EnumCaseInfo, EnumCaseValue, EnumInfo, ExternClassInfo, ExternFieldInfo,
    ExternFunctionSig, InterfaceInfo, PackedClassInfo, PackedFieldInfo,
};
pub(crate) use signatures::{builtin_call_sig, first_class_callable_builtin_sig};
pub use signatures::FunctionSig;

#[allow(dead_code)]
pub fn check(
    program: &crate::parser::ast::Program,
) -> Result<CheckResult, crate::errors::CompileError> {
    result::check(program)
}
