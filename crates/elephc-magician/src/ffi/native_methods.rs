//! Purpose:
//! Exports registration of generated native PHP method signatures and related
//! metadata into an eval context so runtime fragments can bind AOT calls and
//! validate generated interface contracts.
//!
//! Called from:
//! - Generated EIR backend assembly before fragments can call AOT methods.
//!
//! Key details:
//! - Invalid names, handles, or indexes fail closed as `false`.
//! - The metadata records parameter names and supported defaults; generated user
//!   helpers still perform the actual method, static method, and constructor calls.

use super::util::abi_name_to_string;
use crate::abi::{ElephcEvalContext, ABI_VERSION};
use crate::context::{
    NativeCallableArrayDefaultElement, NativeCallableArrayDefaultKey, NativeCallableDefault,
    NativeCallableObjectDefaultArg, NativeCallableSignature,
};
use crate::eval_ir::{
    EvalAttribute, EvalAttributeArg, EvalInterfaceProperty, EvalParameterType,
    EvalParameterTypeVariant,
};

mod attribute_decoder;
mod callable_metadata;
mod constructor_registration;
mod method_registration;
mod property_registration;
mod public_abi;

use attribute_decoder::*;
use callable_metadata::*;
use constructor_registration::*;
use method_registration::*;
use property_registration::*;
pub use public_abi::*;

pub(in crate::ffi) use callable_metadata::{
    native_callable_array_default, native_callable_object_default, native_callable_scalar_default,
    native_callable_type_from_abi,
};

const NATIVE_DEFAULT_NULL: u64 = 0;
const NATIVE_DEFAULT_BOOL: u64 = 1;
const NATIVE_DEFAULT_INT: u64 = 2;
const NATIVE_DEFAULT_FLOAT: u64 = 3;
pub(crate) const NATIVE_DEFAULT_EMPTY_ARRAY: u64 = 4;
const NATIVE_MEMBER_ATTRIBUTE_METHOD: u8 = 0;
const NATIVE_MEMBER_ATTRIBUTE_PROPERTY: u8 = 1;
const NATIVE_MEMBER_ATTRIBUTE_CLASS_CONSTANT: u8 = 2;
const NATIVE_MEMBER_ATTRIBUTE_CLASS: u8 = 3;
const NATIVE_ATTRIBUTE_ARGS_UNSUPPORTED: u8 = 0;
const NATIVE_ATTRIBUTE_ARGS_SUPPORTED: u8 = 1;
const NATIVE_ATTRIBUTE_ARG_NULL: u8 = 0;
const NATIVE_ATTRIBUTE_ARG_BOOL: u8 = 1;
const NATIVE_ATTRIBUTE_ARG_INT: u8 = 2;
const NATIVE_ATTRIBUTE_ARG_STRING: u8 = 3;
const NATIVE_ATTRIBUTE_ARG_NAMED: u8 = 4;
const NATIVE_ATTRIBUTE_ARG_FLOAT: u8 = 5;
const NATIVE_ATTRIBUTE_ARG_ARRAY: u8 = 6;
const NATIVE_OBJECT_DEFAULT_ARG_SCALAR: u8 = 0;
const NATIVE_OBJECT_DEFAULT_ARG_STRING: u8 = 1;
const NATIVE_OBJECT_DEFAULT_ARG_OBJECT: u8 = 2;
const NATIVE_OBJECT_DEFAULT_ARG_NAMED: u8 = 3;
const NATIVE_OBJECT_DEFAULT_ARG_ARRAY: u8 = 4;
const NATIVE_ARRAY_DEFAULT_KEY_AUTO: u8 = 0;
const NATIVE_ARRAY_DEFAULT_KEY_INT: u8 = 1;
const NATIVE_ARRAY_DEFAULT_KEY_STRING: u8 = 2;
const NATIVE_PROPERTY_REQUIRES_GET: u64 = 1;
const NATIVE_PROPERTY_REQUIRES_SET: u64 = 2;
const MAX_NATIVE_OBJECT_DEFAULT_ARGS: usize = u8::MAX as usize;

#[derive(Clone, Copy)]
pub(super) enum NativeCallableTypePosition {
    Parameter,
    Return,
}
