//! Purpose:
//! Admission predicates for the encoding builtins: `base64_decode`, `parse_url` and `crc32`.
//!
//! Called from:
//! - `super::lower_direct_builtin` and `super::direct_builtin_shape_issue` dispatch to it.
//! - `crate::codegen_wasm::capability` reads its `*_shape_issue` predicates for the static audit.

use super::*;

/// Validates `base64_decode`: ONE string in, PHP's `string|false` Mixed cell out.
///
/// The two-argument STRICT form is what makes the declared type a union, and it is refused here
/// rather than approximated: php-src's strict decode rejects a stray byte, a misplaced `=` and a
/// truncated final group, and `$strict` is a runtime value, so a lowering that ignored it would
/// answer a string where PHP answers `false`. The lax arity has no such split and is exact.
pub(super) fn base64_decode_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    if call.operands.len() != 1 {
        return Some(format!(
            "only the one-argument (non-strict) form is lowered; the $strict flag decides between \
             a string and false at run time, got {} operands",
            call.operands.len()
        ));
    }
    let Some(value) = function.value(call.operands[0]) else {
        return Some("operand is missing from the value table".to_string());
    };
    if value.ir_type != IrType::Str || value.php_type.codegen_repr() != PhpType::Str {
        return Some(format!(
            "base64_decode operand is {:?}/{:?}, expected Str/Str",
            value.ir_type,
            value.php_type.codegen_repr()
        ));
    }
    if call.result.is_none()
        || call.result_type != IrType::Heap(IrHeapKind::Mixed)
        || call.result_php_type.codegen_repr() != PhpType::Mixed
    {
        return Some(format!(
            "base64_decode result {:?}/{:?} is not the Mixed cell PHP's string|false needs",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Validates `parse_url`: a string, a CONSTANT component selector, and a Mixed cell out.
///
/// The selector must be a compile-time constant, and this is the one place that matters: a
/// value ABOVE seven is php's catchable `ValueError`, not a result, and this target cannot
/// raise one from inside a runtime helper. Refusing the dynamic form keeps the helper total
/// over everything it does accept — any negative selects the whole array, `0..=7` selects one
/// component — instead of inventing an answer for a case php refuses to answer at all.
pub(super) fn parse_url_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    if !matches!(call.operands.len(), 1 | 2) {
        return Some(format!(
            "expected a url and an optional component selector, got {} operands",
            call.operands.len()
        ));
    }
    let Some(url) = function.value(call.operands[0]) else {
        return Some("url operand is missing from the value table".to_string());
    };
    if url.ir_type != IrType::Str || url.php_type.codegen_repr() != PhpType::Str {
        return Some(format!(
            "parse_url url is {:?}/{:?}, expected Str/Str",
            url.ir_type,
            url.php_type.codegen_repr()
        ));
    }
    // The one-argument form carries no selector at all; its default is the whole array, which
    // is the arity php's own signature declares and the one nothing can push out of range.
    if let Some(&selector) = call.operands.get(1) {
        let Some(component) = function.value(selector) else {
            return Some("component operand is missing from the value table".to_string());
        };
        if component.ir_type != IrType::I64 || component.php_type.codegen_repr() != PhpType::Int {
            return Some(format!(
                "parse_url component is {:?}/{:?}, expected I64/Int",
                component.ir_type,
                component.php_type.codegen_repr()
            ));
        }
        match constant_i64_operand(function, selector) {
            Some(value) if value <= 7 => {}
            Some(value) => {
                return Some(format!(
                    "component {value} is above PHP_URL_FRAGMENT, which php answers with a \
                     catchable ValueError this target cannot raise from a runtime helper"
                ))
            }
            None => {
                return Some(
                    "the component selector must be a compile-time constant, because a value \
                     above PHP_URL_FRAGMENT is a catchable ValueError rather than a result"
                        .to_string(),
                )
            }
        }
    }
    if call.result.is_none()
        || call.result_type != IrType::Heap(IrHeapKind::Mixed)
        || call.result_php_type.codegen_repr() != PhpType::Mixed
    {
        return Some(format!(
            "parse_url result {:?}/{:?} is not the Mixed cell its array|string|int|false needs",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Validates `crc32`: one string in, an integer out.
pub(super) fn crc32_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let [subject] = call.operands.as_slice() else {
        return Some(format!(
            "expected one string operand, got {}",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*subject) else {
        return Some("operand is missing from the value table".to_string());
    };
    if value.ir_type != IrType::Str || value.php_type.codegen_repr() != PhpType::Str {
        return Some(format!(
            "crc32 takes a string, got {:?}/{:?}",
            value.ir_type,
            value.php_type.codegen_repr()
        ));
    }
    if call.result.is_none()
        || call.result_type != IrType::I64
        || call.result_php_type.codegen_repr() != PhpType::Int
    {
        return Some(format!(
            "crc32 result {:?}/{:?} is not the expected I64/Int",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}
