//! Purpose:
//! Implements eval-side raw pointer and buffer extension builtins.
//! These helpers expose the AOT-visible names while preserving the raw-address
//! representation as integer runtime cells inside eval.
//!
//! Called from:
//! - `crate::interpreter::expressions::eval_positional_expr_call()`.
//! - `crate::interpreter::builtins::registry::dispatch::eval_builtin_with_values()`.
//!
//! Key details:
//! - `buffer_new()` returns the same header pointer shape used by AOT buffers:
//!   length word, stride word, then zeroed payload.
//! - `ptr($var)` still requires lvalue storage that the by-value eval call path
//!   does not expose, so it fails instead of inventing an unsafe fake address.

use std::mem;
use std::ptr;
use std::slice;

use super::super::*;

const BUFFER_HEADER_WORDS: usize = 2;
const BUFFER_DEFAULT_STRIDE: usize = 8;

/// Evaluates raw-memory builtins whose arguments are still source expressions.
pub(in crate::interpreter) fn eval_builtin_raw_memory(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if name == "ptr" {
        let [_value] = args else {
            return Err(EvalStatus::RuntimeFatal);
        };
        return Err(EvalStatus::UnsupportedConstruct);
    }
    if name == "buffer_free" {
        if let [EvalExpr::LoadVar(variable)] = args {
            return eval_buffer_free_direct_variable(variable, context, scope, values);
        }
    }
    let evaluated_args = args
        .iter()
        .map(|arg| eval_expr(arg, context, scope, values))
        .collect::<Result<Vec<_>, _>>()?;
    eval_raw_memory_builtin_result(name, &evaluated_args, context, values)
}

/// Dispatches already evaluated raw-memory builtin arguments for dynamic calls.
pub(in crate::interpreter) fn eval_raw_memory_builtin_with_values(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    match name {
        "buffer_free" | "buffer_len" | "buffer_new" | "ptr" | "ptr_get" | "ptr_is_null"
        | "ptr_null" | "ptr_offset" | "ptr_read8" | "ptr_read16" | "ptr_read32"
        | "ptr_read_string" | "ptr_set" | "ptr_sizeof" | "ptr_write8" | "ptr_write16"
        | "ptr_write32" | "ptr_write_string" => {
            eval_raw_memory_builtin_result(name, evaluated_args, context, values).map(Some)
        }
        _ => Ok(None),
    }
}

/// Applies one raw-memory builtin to already evaluated runtime cells.
fn eval_raw_memory_builtin_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "buffer_new" => {
            let [length] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_buffer_new_result(*length, values)
        }
        "buffer_len" => {
            let [buffer] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_buffer_len_result(*buffer, values)
        }
        "buffer_free" => {
            let [buffer] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_buffer_free_result(*buffer, values)
        }
        "ptr" => {
            let [_value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            Err(EvalStatus::UnsupportedConstruct)
        }
        "ptr_null" => {
            let [] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.int(0)
        }
        "ptr_is_null" => {
            let [pointer] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let address = eval_pointer_address(*pointer, values)?;
            values.bool_value(address == 0)
        }
        "ptr_offset" => {
            let [pointer, offset] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_ptr_offset_result(*pointer, *offset, values)
        }
        "ptr_get" => {
            let [pointer] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_pointer_read_result(*pointer, PointerReadWidth::Word64, values)
        }
        "ptr_set" => {
            let [pointer, value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_pointer_write_result(*pointer, *value, PointerWriteWidth::Word64, values)
        }
        "ptr_read8" => {
            let [pointer] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_pointer_read_result(*pointer, PointerReadWidth::Byte, values)
        }
        "ptr_read16" => {
            let [pointer] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_pointer_read_result(*pointer, PointerReadWidth::Half, values)
        }
        "ptr_read32" => {
            let [pointer] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_pointer_read_result(*pointer, PointerReadWidth::Word32, values)
        }
        "ptr_write8" => {
            let [pointer, value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_pointer_write_result(*pointer, *value, PointerWriteWidth::Byte, values)
        }
        "ptr_write16" => {
            let [pointer, value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_pointer_write_result(*pointer, *value, PointerWriteWidth::Half, values)
        }
        "ptr_write32" => {
            let [pointer, value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_pointer_write_result(*pointer, *value, PointerWriteWidth::Word32, values)
        }
        "ptr_read_string" => {
            let [pointer, length] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_ptr_read_string_result(*pointer, *length, values)
        }
        "ptr_write_string" => {
            let [pointer, string] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_ptr_write_string_result(*pointer, *string, values)
        }
        "ptr_sizeof" => {
            let [type_name] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_ptr_sizeof_result(*type_name, context, values)
        }
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Allocates a zero-filled AOT-shaped buffer and returns its header address.
fn eval_buffer_new_result(
    length: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let length = eval_int_value(length, values)?;
    if length < 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let length = usize::try_from(length).map_err(|_| EvalStatus::RuntimeFatal)?;
    let header_bytes = BUFFER_HEADER_WORDS
        .checked_mul(mem::size_of::<usize>())
        .ok_or(EvalStatus::RuntimeFatal)?;
    let payload_bytes = length
        .checked_mul(BUFFER_DEFAULT_STRIDE)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let total_bytes = header_bytes
        .checked_add(payload_bytes)
        .ok_or(EvalStatus::RuntimeFatal)?;
    let allocation = unsafe { libc::calloc(total_bytes.max(1), 1) };
    if allocation.is_null() {
        return Err(EvalStatus::RuntimeFatal);
    }
    unsafe {
        let header = allocation.cast::<usize>();
        ptr::write(header, length);
        ptr::write(header.add(1), BUFFER_DEFAULT_STRIDE);
    }
    eval_address_value(allocation as usize, values)
}

/// Reads the logical element count from an AOT-shaped buffer header.
fn eval_buffer_len_result(
    buffer: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let header = eval_non_null_pointer(buffer, values)?.cast::<usize>();
    let length = unsafe { ptr::read(header) };
    values.int(i64::try_from(length).map_err(|_| EvalStatus::RuntimeFatal)?)
}

/// Frees an AOT-shaped buffer header and returns PHP null.
fn eval_buffer_free_result(
    buffer: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_buffer_free_address(buffer, values)?;
    values.null()
}

/// Frees a local buffer variable and replaces the source variable with null.
fn eval_buffer_free_direct_variable(
    variable: &str,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let value = eval_expr(&EvalExpr::LoadVar(variable.to_string()), context, scope, values)?;
    eval_buffer_free_address(value, values)?;
    let null = values.null()?;
    for replaced in scope.set_respecting_references(
        variable.to_string(),
        null,
        ScopeCellOwnership::Owned,
    ) {
        values.release(replaced)?;
    }
    values.null()
}

/// Frees the raw allocation addressed by an AOT-shaped buffer header pointer.
fn eval_buffer_free_address(
    buffer: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let address = eval_non_null_pointer(buffer, values)?;
    unsafe {
        libc::free(address.cast::<libc::c_void>());
    }
    Ok(())
}

/// Computes a derived raw pointer address by adding a signed byte offset.
fn eval_ptr_offset_result(
    pointer: RuntimeCellHandle,
    offset: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let address = eval_pointer_address(pointer, values)?;
    let offset = eval_int_value(offset, values)?;
    let address = if offset >= 0 {
        let offset = usize::try_from(offset).map_err(|_| EvalStatus::RuntimeFatal)?;
        address.checked_add(offset).ok_or(EvalStatus::RuntimeFatal)?
    } else {
        let offset = usize::try_from(offset.unsigned_abs()).map_err(|_| EvalStatus::RuntimeFatal)?;
        address.checked_sub(offset).ok_or(EvalStatus::RuntimeFatal)?
    };
    eval_address_value(address, values)
}

/// Reads one unsigned or machine-word value from raw memory.
fn eval_pointer_read_result(
    pointer: RuntimeCellHandle,
    width: PointerReadWidth,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let address = eval_non_null_pointer(pointer, values)?;
    let value = unsafe {
        match width {
            PointerReadWidth::Byte => i64::from(ptr::read_unaligned(address.cast::<u8>())),
            PointerReadWidth::Half => i64::from(ptr::read_unaligned(address.cast::<u16>())),
            PointerReadWidth::Word32 => i64::from(ptr::read_unaligned(address.cast::<u32>())),
            PointerReadWidth::Word64 => {
                let word = ptr::read_unaligned(address.cast::<u64>());
                i64::from_ne_bytes(word.to_ne_bytes())
            }
        }
    };
    values.int(value)
}

/// Writes one integer payload to raw memory and returns PHP null.
fn eval_pointer_write_result(
    pointer: RuntimeCellHandle,
    value: RuntimeCellHandle,
    width: PointerWriteWidth,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let address = eval_non_null_pointer(pointer, values)?;
    let value = eval_int_value(value, values)?;
    unsafe {
        match width {
            PointerWriteWidth::Byte => ptr::write_unaligned(address.cast::<u8>(), value as u8),
            PointerWriteWidth::Half => ptr::write_unaligned(address.cast::<u16>(), value as u16),
            PointerWriteWidth::Word32 => ptr::write_unaligned(address.cast::<u32>(), value as u32),
            PointerWriteWidth::Word64 => {
                ptr::write_unaligned(address.cast::<u64>(), u64::from_ne_bytes(value.to_ne_bytes()))
            }
        }
    }
    values.null()
}

/// Copies raw memory bytes into a PHP byte string.
fn eval_ptr_read_string_result(
    pointer: RuntimeCellHandle,
    length: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let address = eval_non_null_pointer(pointer, values)?;
    let length = eval_int_value(length, values)?;
    if length < 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let length = usize::try_from(length).map_err(|_| EvalStatus::RuntimeFatal)?;
    let bytes = unsafe { slice::from_raw_parts(address.cast::<u8>(), length) };
    values.string_bytes_value(bytes)
}

/// Copies PHP string bytes into raw memory and returns the byte count written.
fn eval_ptr_write_string_result(
    pointer: RuntimeCellHandle,
    string: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let address = eval_non_null_pointer(pointer, values)?;
    let bytes = values.string_bytes(string)?;
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), address.cast::<u8>(), bytes.len());
    }
    values.int(i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?)
}

/// Computes the checked byte size for a low-level type name.
fn eval_ptr_sizeof_result(
    type_name: RuntimeCellHandle,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(type_name)?;
    let type_name = String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
    let size = eval_pointer_target_size(type_name.trim_start_matches('\\'), context)
        .ok_or(EvalStatus::RuntimeFatal)?;
    values.int(i64::try_from(size).map_err(|_| EvalStatus::RuntimeFatal)?)
}

/// Returns the eval-side byte size for one low-level pointer target name.
fn eval_pointer_target_size(type_name: &str, context: &ElephcEvalContext) -> Option<usize> {
    match type_name.to_ascii_lowercase().as_str() {
        "int" | "integer" => Some(8),
        "float" | "double" | "real" => Some(8),
        "bool" | "boolean" => Some(8),
        "string" => Some(16),
        "ptr" | "pointer" => Some(8),
        _ => context.class(type_name).map(eval_boxed_class_size),
    }
}

/// Returns the boxed object storage size used by AOT class metadata.
fn eval_boxed_class_size(class: &EvalClass) -> usize {
    let instance_properties = class
        .properties()
        .iter()
        .filter(|property| !property.is_static())
        .count();
    8 + instance_properties * 16
}

/// Converts a runtime cell to a raw pointer address encoded as a PHP integer.
fn eval_pointer_address(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<usize, EvalStatus> {
    let address = eval_int_value(value, values)?;
    usize::try_from(address).map_err(|_| EvalStatus::RuntimeFatal)
}

/// Converts a runtime cell to a non-null raw pointer.
fn eval_non_null_pointer(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<*mut u8, EvalStatus> {
    let address = eval_pointer_address(value, values)?;
    if address == 0 {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(address as *mut u8)
}

/// Boxes a raw pointer address as a PHP integer cell.
fn eval_address_value(
    address: usize,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    values.int(i64::try_from(address).map_err(|_| EvalStatus::RuntimeFatal)?)
}

/// Widths supported by pointer read helpers.
enum PointerReadWidth {
    Byte,
    Half,
    Word32,
    Word64,
}

/// Widths supported by pointer write helpers.
enum PointerWriteWidth {
    Byte,
    Half,
    Word32,
    Word64,
}
