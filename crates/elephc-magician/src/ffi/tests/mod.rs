//! Purpose:
//! Unit tests for the eval C ABI layer.
//! They validate handle allocation, stable status codes, scope flags, and
//! dynamic symbol registration without requiring generated runtime assembly.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Execute remains a controlled unsupported stub in crate unit tests.
//! - Fake runtime-cell pointers are never dereferenced by these tests.

use super::context::*;
use super::declared_symbols::*;
use super::execute::*;
use super::native_functions::*;
use super::native_methods::*;
use super::scope::*;
use super::symbols::*;
use crate::abi::{
    ElephcEvalContext, ElephcEvalResult, ElephcEvalScope, ABI_VERSION, SCOPE_FLAG_DIRTY,
    SCOPE_FLAG_OWNED, SCOPE_FLAG_PRESENT, SCOPE_FLAG_UNSET,
};
use crate::context::{
    NativeCallableArrayDefaultElement, NativeCallableArrayDefaultKey, NativeCallableDefault,
    NativeCallableObjectDefaultArg, push_native_frame_called_class_override,
};
use crate::errors::EvalStatus;
use crate::eval_ir::{EvalAttributeArg, EvalParameterTypeVariant};
use crate::value::{RuntimeCell, RuntimeCellHandle};
use std::ffi::c_void;

mod callable_defaults;
mod class_metadata;
mod context_symbols;
mod native_functions;
mod native_methods;
mod scope_execution;

const TEST_NATIVE_DEFAULT_NULL: u64 = 0;
const TEST_NATIVE_DEFAULT_BOOL: u64 = 1;
const TEST_NATIVE_DEFAULT_INT: u64 = 2;
const TEST_NATIVE_DEFAULT_FLOAT: u64 = 3;
const TEST_NATIVE_DEFAULT_EMPTY_ARRAY: u64 = 4;
const TEST_NATIVE_PROPERTY_REQUIRES_GET: u64 = 1;
const TEST_NATIVE_PROPERTY_REQUIRES_SET: u64 = 2;
const TEST_NATIVE_OBJECT_DEFAULT_ARG_SCALAR: u8 = 0;
const TEST_NATIVE_OBJECT_DEFAULT_ARG_STRING: u8 = 1;
const TEST_NATIVE_OBJECT_DEFAULT_ARG_OBJECT: u8 = 2;
const TEST_NATIVE_OBJECT_DEFAULT_ARG_NAMED: u8 = 3;
const TEST_NATIVE_OBJECT_DEFAULT_ARG_ARRAY: u8 = 4;
const TEST_NATIVE_ARRAY_DEFAULT_KEY_AUTO: u8 = 0;
const TEST_NATIVE_ARRAY_DEFAULT_KEY_INT: u8 = 1;
const TEST_NATIVE_ARRAY_DEFAULT_KEY_STRING: u8 = 2;
const TEST_MAX_NATIVE_OBJECT_DEFAULT_ARGS: usize = u8::MAX as usize;

/// Test native invoker placeholder used only to validate ABI registration.
unsafe extern "C" fn fake_native_invoker(
    _descriptor: *mut c_void,
    _args: *mut RuntimeCell,
) -> *mut RuntimeCell {
    std::ptr::null_mut()
}

/// Builds one native member-attribute ABI record for registration tests.
fn native_member_attribute_record(
    owner_kind: u8,
    member_key: &str,
    attribute_name: &str,
    args: Option<&[EvalAttributeArg]>,
) -> Vec<u8> {
    let mut record = Vec::new();
    record.push(owner_kind);
    native_member_attribute_push_string(&mut record, member_key);
    native_member_attribute_push_string(&mut record, attribute_name);
    match args {
        Some(args) => {
            record.push(1);
            record.extend_from_slice(&(args.len() as u32).to_le_bytes());
            for arg in args {
                native_member_attribute_push_arg(&mut record, arg);
            }
        }
        None => record.push(0),
    }
    record
}

/// Appends one test attribute argument to a native member-attribute ABI record.
fn native_member_attribute_push_arg(record: &mut Vec<u8>, arg: &EvalAttributeArg) {
    match arg {
        EvalAttributeArg::Null => record.push(0),
        EvalAttributeArg::Bool(value) => {
            record.push(1);
            record.push(u8::from(*value));
        }
        EvalAttributeArg::Int(value) => {
            record.push(2);
            record.extend_from_slice(&value.to_le_bytes());
        }
        EvalAttributeArg::Float(bits) => {
            record.push(5);
            record.extend_from_slice(&bits.to_le_bytes());
        }
        EvalAttributeArg::String(value) => {
            record.push(3);
            native_member_attribute_push_string(record, value);
        }
        EvalAttributeArg::Named { name, value } => {
            record.push(4);
            native_member_attribute_push_string(record, name);
            native_member_attribute_push_arg(record, value);
        }
        EvalAttributeArg::IntKeyed { .. } => {
            panic!("native attribute test ABI does not encode int-keyed array arguments")
        }
        EvalAttributeArg::Array(elements) => {
            record.push(6);
            record.extend_from_slice(&(elements.len() as u32).to_le_bytes());
            for element in elements {
                native_member_attribute_push_arg(record, element);
            }
        }
    }
}

/// Appends one length-prefixed string to a native member-attribute ABI record.
fn native_member_attribute_push_string(record: &mut Vec<u8>, value: &str) {
    record.extend_from_slice(&(value.len() as u32).to_le_bytes());
    record.extend_from_slice(value.as_bytes());
}

/// Builds one object-valued native parameter default ABI record for registration tests.
fn native_object_default_record(
    class_name: &str,
    args: &[NativeCallableObjectDefaultArg],
) -> Vec<u8> {
    let mut record = Vec::new();
    native_member_attribute_push_string(&mut record, class_name);
    record.push(args.len() as u8);
    for arg in args {
        native_object_default_push_arg(&mut record, arg);
    }
    record
}

/// Builds one array-valued native parameter default ABI record for registration tests.
fn native_array_default_record(elements: &[NativeCallableArrayDefaultElement]) -> Vec<u8> {
    let mut record = Vec::new();
    record.extend_from_slice(&(elements.len() as u32).to_le_bytes());
    for element in elements {
        native_array_default_push_element(&mut record, element);
    }
    record
}

/// Appends one array-default element and optional static key to a default record.
fn native_array_default_push_element(
    record: &mut Vec<u8>,
    element: &NativeCallableArrayDefaultElement,
) {
    match &element.key {
        Some(NativeCallableArrayDefaultKey::Int(value)) => {
            record.push(TEST_NATIVE_ARRAY_DEFAULT_KEY_INT);
            record.extend_from_slice(&value.to_le_bytes());
        }
        Some(NativeCallableArrayDefaultKey::String(value)) => {
            record.push(TEST_NATIVE_ARRAY_DEFAULT_KEY_STRING);
            native_member_attribute_push_string(record, value);
        }
        None => record.push(TEST_NATIVE_ARRAY_DEFAULT_KEY_AUTO),
    }
    native_object_default_push_arg_value(record, &element.value);
}

/// Appends one object-default constructor argument to a native parameter default record.
fn native_object_default_push_arg(record: &mut Vec<u8>, arg: &NativeCallableObjectDefaultArg) {
    if let Some(name) = &arg.name {
        record.push(TEST_NATIVE_OBJECT_DEFAULT_ARG_NAMED);
        native_member_attribute_push_string(record, name);
    }
    native_object_default_push_arg_value(record, &arg.value);
}

/// Appends one object-default constructor argument value to a native parameter default record.
fn native_object_default_push_arg_value(record: &mut Vec<u8>, value: &NativeCallableDefault) {
    match value {
        NativeCallableDefault::Null => {
            native_object_default_push_scalar(record, TEST_NATIVE_DEFAULT_NULL, 0)
        }
        NativeCallableDefault::Bool(value) => {
            native_object_default_push_scalar(record, TEST_NATIVE_DEFAULT_BOOL, u64::from(*value))
        }
        NativeCallableDefault::Int(value) => {
            native_object_default_push_scalar(record, TEST_NATIVE_DEFAULT_INT, *value as u64)
        }
        NativeCallableDefault::Float(value) => {
            native_object_default_push_scalar(record, TEST_NATIVE_DEFAULT_FLOAT, value.to_bits())
        }
        NativeCallableDefault::String(value) => {
            record.push(TEST_NATIVE_OBJECT_DEFAULT_ARG_STRING);
            native_member_attribute_push_string(record, value);
        }
        NativeCallableDefault::EmptyArray => {
            native_object_default_push_scalar(record, TEST_NATIVE_DEFAULT_EMPTY_ARRAY, 0)
        }
        NativeCallableDefault::Object { class_name, args } => {
            record.push(TEST_NATIVE_OBJECT_DEFAULT_ARG_OBJECT);
            record.extend_from_slice(&native_object_default_record(class_name, args));
        }
        NativeCallableDefault::Array(elements) => {
            record.push(TEST_NATIVE_OBJECT_DEFAULT_ARG_ARRAY);
            record.extend_from_slice(&native_array_default_record(elements));
        }
    }
}

/// Appends one scalar object-default constructor argument to a native parameter default record.
fn native_object_default_push_scalar(record: &mut Vec<u8>, kind: u64, payload: u64) {
    record.push(TEST_NATIVE_OBJECT_DEFAULT_ARG_SCALAR);
    record.extend_from_slice(&kind.to_le_bytes());
    record.extend_from_slice(&payload.to_le_bytes());
}
