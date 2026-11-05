//! Purpose:
//! Implements PHP's internal `FFI\CData` operator handlers: pointer arithmetic, pointer
//! difference, ordering, and truthiness, shared by the prelude's static adapters.
//!
//! Called from:
//! - `crate::abi` operator entry points invoked by the AST-built `FFI\CData` adapters.
//!
//! Key details:
//! - Only pointer/array CData participates in `+`/`-`; a scalar CData is rejected by the
//!   prelude with php-src's `Unsupported operand types` TypeError.
//! - `pointer + int` and `int + pointer` produce a pointer CData; `pointer - pointer`
//!   produces the element difference PHP reports; `pointer - int` produces a pointer.
//! - Ordering is defined only for two CData pointers, matching php-src; every other
//!   combination raises `Comparison of incompatible C types`.

use crate::context::{cdata, set_error, type_clone, ERROR_EXCEPTION};
use crate::types::{TypeId, TypeKind};

/// Operator codes shared with the PHP adapters.
pub const OP_ADD: u32 = 1;
pub const OP_SUB: u32 = 2;

/// One decoded operand.
enum Operand {
    /// PHP integer.
    Int(i64),
    /// CData handle.
    CData(usize),
}

/// Decodes the prelude's `(value, kind)` pair.
fn operand(value: i64, kind: u32) -> Option<Operand> {
    if kind == 1 {
        let handle = value as usize;
        cdata(handle)?;
        Some(Operand::CData(handle))
    } else {
        Some(Operand::Int(value))
    }
}

/// Pointer/array parts of one CData value.
struct PointerParts {
    base: usize,
    element: TypeId,
    stride: usize,
}

/// Returns pointer/array parts when the CData is a pointer-like object.
fn pointer_parts(handle: usize) -> Option<PointerParts> {
    let data = cdata(handle)?;
    let ty = type_clone(data.ty)?;
    match ty.kind {
        TypeKind::Pointer(element) => {
            let element_ty = type_clone(element)?;
            Some(PointerParts {
                base: data.pointer_value(),
                element,
                stride: element_ty.size.max(1),
            })
        }
        TypeKind::Array { element, .. } => {
            let element_ty = type_clone(element)?;
            Some(PointerParts {
                base: data.addr,
                element,
                stride: element_ty.size.max(1),
            })
        }
        _ => None,
    }
}

/// Creates a pointer CData whose storage holds `address`.
fn pointer_result(address: usize, element: TypeId) -> usize {
    crate::context::pointer_to(address, element).unwrap_or(0)
}

/// Returns whether two pointer elements are the same C type.
///
/// php-src compares the pointee types structurally before it computes a pointer difference;
/// TypeIds are not interned across separate `FFI::addr()` calls, so the visible spelling and
/// size stand in for the structural comparison.
fn same_element_type(left: &PointerParts, right: &PointerParts) -> bool {
    match (type_clone(left.element), type_clone(right.element)) {
        (Some(left), Some(right)) => left.display == right.display && left.size == right.size,
        _ => false,
    }
}

/// Returns the result kind for one binary operation.
///
/// `0` means PHP's engine would reject the operation; the prelude turns that into the exact
/// `TypeError` php-src throws. `1` is a PHP integer result and `2` a pointer CData result.
pub fn binary_kind(left: i64, left_kind: u32, right: i64, right_kind: u32, op: u32) -> u32 {
    let (Some(left), Some(right)) = (operand(left, left_kind), operand(right, right_kind)) else {
        set_error(ERROR_EXCEPTION, "Invalid CData operand");
        return 0;
    };
    match op {
        OP_ADD => {
            let left_pointer = matches!(&left, Operand::CData(handle) if pointer_parts(*handle).is_some());
            if left_pointer {
                return 2;
            }
            // `int + pointer` reaches the right-hand pointer branch; a scalar CData on the
            // left never does, matching `zend_ffi_cdata_do_operation`.
            let right_pointer = matches!(&right, Operand::CData(handle) if pointer_parts(*handle).is_some());
            if !matches!(left, Operand::CData(_)) && right_pointer {
                2
            } else {
                0
            }
        }
        OP_SUB => {
            let Operand::CData(left_handle) = left else { return 0 };
            let Some(left_parts) = pointer_parts(left_handle) else { return 0 };
            if let Operand::CData(right_handle) = right {
                if let Some(right_parts) = pointer_parts(right_handle) {
                    if same_element_type(&left_parts, &right_parts) {
                        return 1;
                    }
                }
            }
            2
        }
        _ => 0,
    }
}

/// Computes an integer-result binary operation.
pub fn binary_int(left: i64, left_kind: u32, right: i64, right_kind: u32, op: u32) -> i64 {
    let (Some(left), Some(right)) = (operand(left, left_kind), operand(right, right_kind)) else {
        set_error(ERROR_EXCEPTION, "Invalid CData operand");
        return 0;
    };
    match op {
        OP_SUB => match (left, right) {
            (Operand::CData(left), Operand::CData(right)) => {
                if let (Some(left), Some(right)) = (pointer_parts(left), pointer_parts(right)) {
                    let delta = left.base as isize - right.base as isize;
                    return (delta / left.stride.max(1) as isize) as i64;
                }
                0
            }
            _ => 0,
        },
        _ => 0,
    }
}

/// Applies one element offset to a pointer-like CData.
fn offset_pointer(handle: usize, offset: i64) -> usize {
    let Some(parts) = pointer_parts(handle) else { return 0 };
    let address = (parts.base as isize)
        .wrapping_add((offset as isize).wrapping_mul(parts.stride as isize))
        as usize;
    pointer_result(address, parts.element)
}

/// Computes a CData-result binary operation.
///
/// Only pointer/array bases reach this entry point; scalar CData operands are rejected by the
/// prelude with PHP's `Unsupported operand types` `TypeError`.
pub fn binary_cdata(left: i64, left_kind: u32, right: i64, right_kind: u32, op: u32) -> usize {
    let (Some(left), Some(right)) = (operand(left, left_kind), operand(right, right_kind)) else {
        set_error(ERROR_EXCEPTION, "Invalid CData operand");
        return 0;
    };
    match op {
        OP_ADD => match (left, right) {
            (Operand::CData(handle), Operand::Int(offset)) => offset_pointer(handle, offset),
            // `pointer + <CData>` converts the object to int with a PHP warning and adds 1.
            (Operand::CData(handle), Operand::CData(_)) => offset_pointer(handle, 1),
            (Operand::Int(offset), Operand::CData(handle)) => offset_pointer(handle, offset),
            _ => 0,
        },
        OP_SUB => match (left, right) {
            (Operand::CData(handle), Operand::Int(offset)) => {
                offset_pointer(handle, offset.wrapping_neg())
            }
            // A pointer minus a different-type pointer (or a scalar CData) converts the right
            // operand to int with a PHP warning and subtracts 1.
            (Operand::CData(handle), Operand::CData(_)) => offset_pointer(handle, -1),
            _ => 0,
        },
        _ => 0,
    }
}

/// Compares two operands and returns PHP's normalized ordering.
///
/// php-src only installs a comparison handler for two CData pointers: pointer values are
/// ordered, a null pointer raises `NULL pointer dereference`, and every other combination
/// (scalar CData, arrays, mixed CData/non-CData) raises `Comparison of incompatible C types`.
/// The engine short-circuits identical objects before the handler, so equal handles compare
/// equal here as well.
pub fn compare(left: i64, left_kind: u32, right: i64, right_kind: u32) -> i64 {
    let (Some(left), Some(right)) = (operand(left, left_kind), operand(right, right_kind)) else {
        set_error(ERROR_EXCEPTION, "Invalid CData operand");
        return 0;
    };
    match (&left, &right) {
        (Operand::CData(left), Operand::CData(right)) => {
            if left == right {
                return 0;
            }
            if !is_pointer(*left) || !is_pointer(*right) {
                set_error(ERROR_EXCEPTION, "Comparison of incompatible C types");
                return 0;
            }
            let left_value = cdata(*left).map(|data| data.pointer_value()).unwrap_or(0);
            let right_value = cdata(*right).map(|data| data.pointer_value()).unwrap_or(0);
            if left_value == 0 || right_value == 0 {
                set_error(ERROR_EXCEPTION, "NULL pointer dereference");
                return 0;
            }
            match left_value.cmp(&right_value) {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            }
        }
        (Operand::CData(_), _) | (_, Operand::CData(_)) => {
            set_error(ERROR_EXCEPTION, "Comparison of incompatible C types");
            0
        }
        (Operand::Int(left), Operand::Int(right)) => match left.cmp(right) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        },
    }
}

/// Returns whether this CData's declared type is a pointer (not an array).
fn is_pointer(handle: usize) -> bool {
    cdata(handle)
        .and_then(|data| type_clone(data.ty))
        .is_some_and(|ty| matches!(ty.kind, TypeKind::Pointer(_)))
}
