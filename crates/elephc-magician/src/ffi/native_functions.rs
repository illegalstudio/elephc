//! Purpose:
//! Exports registration of generated native PHP callbacks into an eval context.
//! Eval fragments use this metadata to call AOT functions through descriptor
//! invokers while preserving PHP-visible parameter names and defaults.
//!
//! Called from:
//! - Generated EIR backend assembly before fragments can call AOT functions.
//!
//! Key details:
//! - Invalid names, handles, descriptors, or indexes fail closed as `false`.
//! - Function names are stored under their PHP case-insensitive folded key.

use super::native_methods::{
    native_callable_array_default, native_callable_object_default, native_callable_scalar_default,
    native_callable_type_from_abi, NativeCallableTypePosition,
};
use super::util::abi_name_to_string;
use crate::abi::{ElephcEvalContext, ABI_VERSION};
use crate::context::{NativeCallableDefault, NativeFunction, NativeFunctionInvoker};
use std::ffi::c_void;

mod public_abi;
mod registration;

pub use public_abi::*;
use registration::*;
