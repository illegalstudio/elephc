//! Purpose:
//! Implements standard array sorting, key extraction, and natural ordering helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins::arrays::sort` re-exports.
//!
//! Key details:
//! - Homogeneous sort keys model the eval-supported PHP ordering subset and
//!   shuffled output remains deterministic through runtime value operations.

use super::super::super::super::*;
use super::super::super::*;
use super::super::*;

/// Returns the dynamic callable result for by-value array ordering calls.
pub(in crate::interpreter) fn eval_array_sort_value_result(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !matches!(values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
        return Err(EvalStatus::RuntimeFatal);
    }
    values.bool_value(true)
}

/// Sort key shape supported by eval's homogeneous array ordering implementation.
#[derive(Clone)]
pub(in crate::interpreter) enum EvalArraySortKey {
    Numeric(f64),
    Natural(Vec<u8>),
    String(Vec<u8>),
}

/// One source array entry plus its precomputed ordering key.
pub(in crate::interpreter) struct EvalArraySortEntry {
    sort_key: EvalArraySortKey,
    source_key: RuntimeCellHandle,
    value: RuntimeCellHandle,
}

/// Builds the sorted replacement array for eval array ordering builtins.
pub(in crate::interpreter) fn eval_array_sort_replacement(
    name: &str,
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut entries = match name {
        "krsort" | "ksort" => eval_array_key_sort_entries(array, values)?,
        "natcasesort" => eval_array_natural_sort_entries(array, true, values)?,
        "natsort" => eval_array_natural_sort_entries(array, false, values)?,
        "arsort" | "asort" | "rsort" | "sort" => eval_array_value_sort_entries(array, values)?,
        "shuffle" => return eval_array_shuffle_replacement(array, values),
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    entries.sort_by(|left, right| {
        let order = eval_array_sort_key_cmp(&left.sort_key, &right.sort_key);
        if matches!(name, "arsort" | "krsort" | "rsort") {
            order.reverse()
        } else {
            order
        }
    });

    if matches!(
        name,
        "arsort" | "asort" | "krsort" | "ksort" | "natcasesort" | "natsort"
    ) {
        return eval_array_preserve_key_sort_result(entries, values);
    }
    eval_array_reindex_sort_result(entries, values)
}

/// Builds a shuffled, reindexed replacement array for `shuffle()`.
pub(in crate::interpreter) fn eval_array_shuffle_replacement(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut entries = Vec::with_capacity(len);
    for position in 0..len {
        let source_key = values.array_iter_key(array, position)?;
        entries.push(values.array_get(array, source_key)?);
    }

    for index in (1..entries.len()).rev() {
        let swap_with = (eval_random_u128() % ((index + 1) as u128)) as usize;
        entries.swap(index, swap_with);
    }

    let mut result = values.array_new(entries.len())?;
    for (index, value) in entries.into_iter().enumerate() {
        let key = values.int(i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        result = values.array_set(result, key, value)?;
    }
    Ok(result)
}

/// Builds an indexed result for `sort()` / `rsort()` after value ordering.
pub(in crate::interpreter) fn eval_array_reindex_sort_result(
    entries: Vec<EvalArraySortEntry>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.array_new(entries.len())?;
    for (index, entry) in entries.into_iter().enumerate() {
        let key = values.int(i64::try_from(index).map_err(|_| EvalStatus::RuntimeFatal)?)?;
        result = values.array_set(result, key, entry.value)?;
    }
    Ok(result)
}

/// Builds a key-preserving associative result after value or key ordering.
pub(in crate::interpreter) fn eval_array_preserve_key_sort_result(
    entries: Vec<EvalArraySortEntry>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut result = values.assoc_new(entries.len())?;
    for entry in entries {
        result = values.array_set(result, entry.source_key, entry.value)?;
    }
    Ok(result)
}

/// Collects values and comparable value-sort keys from one eval array.
pub(in crate::interpreter) fn eval_array_value_sort_entries(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvalArraySortEntry>, EvalStatus> {
    let len = values.array_len(array)?;
    let mut entries = Vec::with_capacity(len);
    let mut expects_numeric = None;

    for position in 0..len {
        let source_key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, source_key)?;
        let sort_key = eval_array_sort_key(value, values)?;
        let is_numeric = matches!(sort_key, EvalArraySortKey::Numeric(_));
        match expects_numeric {
            Some(expected) if expected != is_numeric => return Err(EvalStatus::RuntimeFatal),
            Some(_) => {}
            None => expects_numeric = Some(is_numeric),
        }
        entries.push(EvalArraySortEntry {
            sort_key,
            source_key,
            value,
        });
    }

    Ok(entries)
}

/// Collects values and natural-sort keys from one eval array.
pub(in crate::interpreter) fn eval_array_natural_sort_entries(
    array: RuntimeCellHandle,
    case_insensitive: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvalArraySortEntry>, EvalStatus> {
    let len = values.array_len(array)?;
    let mut entries = Vec::with_capacity(len);
    let mut expects_numeric = None;

    for position in 0..len {
        let source_key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, source_key)?;
        let sort_key = eval_array_natural_sort_key(value, case_insensitive, values)?;
        let is_numeric = matches!(sort_key, EvalArraySortKey::Numeric(_));
        match expects_numeric {
            Some(expected) if expected != is_numeric => return Err(EvalStatus::RuntimeFatal),
            Some(_) => {}
            None => expects_numeric = Some(is_numeric),
        }
        entries.push(EvalArraySortEntry {
            sort_key,
            source_key,
            value,
        });
    }

    Ok(entries)
}

/// Collects values and comparable key-sort keys from one eval array.
pub(in crate::interpreter) fn eval_array_key_sort_entries(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvalArraySortEntry>, EvalStatus> {
    let len = values.array_len(array)?;
    let mut entries = Vec::with_capacity(len);

    for position in 0..len {
        let source_key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, source_key)?;
        let sort_key = eval_array_sort_key(source_key, values)?;
        entries.push(EvalArraySortEntry {
            sort_key,
            source_key,
            value,
        });
    }

    Ok(entries)
}

/// Converts one scalar eval value into a natural-sort key.
pub(in crate::interpreter) fn eval_array_natural_sort_key(
    value: RuntimeCellHandle,
    case_insensitive: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalArraySortKey, EvalStatus> {
    match values.type_tag(value)? {
        EVAL_TAG_INT | EVAL_TAG_FLOAT => {
            Ok(EvalArraySortKey::Numeric(eval_float_value(value, values)?))
        }
        EVAL_TAG_STRING => {
            let mut bytes = values.string_bytes(value)?;
            if case_insensitive {
                bytes.make_ascii_lowercase();
            }
            Ok(EvalArraySortKey::Natural(bytes))
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Converts one scalar eval value into a homogeneous sort key.
pub(in crate::interpreter) fn eval_array_sort_key(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalArraySortKey, EvalStatus> {
    match values.type_tag(value)? {
        EVAL_TAG_INT | EVAL_TAG_FLOAT => {
            Ok(EvalArraySortKey::Numeric(eval_float_value(value, values)?))
        }
        EVAL_TAG_STRING => {
            let bytes = values.string_bytes(value)?;
            match eval_array_numeric_string_sort_key(&bytes) {
                Some(value) => Ok(EvalArraySortKey::Numeric(value)),
                None => Ok(EvalArraySortKey::String(bytes)),
            }
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Parses one PHP numeric string into the numeric sort domain when possible.
pub(in crate::interpreter) fn eval_array_numeric_string_sort_key(bytes: &[u8]) -> Option<f64> {
    if !eval_is_numeric_string(bytes) {
        return None;
    }
    std::str::from_utf8(bytes).ok()?.parse::<f64>().ok()
}

/// Compares two precomputed eval sort keys.
pub(in crate::interpreter) fn eval_array_sort_key_cmp(
    left: &EvalArraySortKey,
    right: &EvalArraySortKey,
) -> std::cmp::Ordering {
    match (left, right) {
        (EvalArraySortKey::Numeric(left), EvalArraySortKey::Numeric(right)) => {
            left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal)
        }
        (EvalArraySortKey::Natural(left), EvalArraySortKey::Natural(right)) => {
            eval_natural_bytes_cmp(left, right)
        }
        (EvalArraySortKey::String(left), EvalArraySortKey::String(right)) => left.cmp(right),
        _ => eval_array_sort_key_rank(left).cmp(&eval_array_sort_key_rank(right)),
    }
}

/// Returns a deterministic rank for mixed key-sort domains.
pub(in crate::interpreter) fn eval_array_sort_key_rank(key: &EvalArraySortKey) -> u8 {
    match key {
        EvalArraySortKey::Numeric(_) => 0,
        EvalArraySortKey::Natural(_) => 1,
        EvalArraySortKey::String(_) => 2,
    }
}

/// Compares byte strings with a small PHP-style natural ordering.
pub(in crate::interpreter) fn eval_natural_bytes_cmp(
    left: &[u8],
    right: &[u8],
) -> std::cmp::Ordering {
    let mut left_index = 0;
    let mut right_index = 0;
    while left_index < left.len() && right_index < right.len() {
        if left[left_index].is_ascii_digit() && right[right_index].is_ascii_digit() {
            let order = eval_natural_digit_run_cmp(left, &mut left_index, right, &mut right_index);
            if order != std::cmp::Ordering::Equal {
                return order;
            }
            continue;
        }

        let order = left[left_index].cmp(&right[right_index]);
        if order != std::cmp::Ordering::Equal {
            return order;
        }
        left_index += 1;
        right_index += 1;
    }
    left.len().cmp(&right.len())
}

/// Compares two natural-sort digit runs and advances both byte indexes past them.
pub(in crate::interpreter) fn eval_natural_digit_run_cmp(
    left: &[u8],
    left_index: &mut usize,
    right: &[u8],
    right_index: &mut usize,
) -> std::cmp::Ordering {
    let left_start = *left_index;
    let right_start = *right_index;
    while *left_index < left.len() && left[*left_index].is_ascii_digit() {
        *left_index += 1;
    }
    while *right_index < right.len() && right[*right_index].is_ascii_digit() {
        *right_index += 1;
    }

    let left_digits = &left[left_start..*left_index];
    let right_digits = &right[right_start..*right_index];
    let left_trimmed = eval_trim_leading_zeroes(left_digits);
    let right_trimmed = eval_trim_leading_zeroes(right_digits);
    left_trimmed
        .len()
        .cmp(&right_trimmed.len())
        .then_with(|| left_trimmed.cmp(right_trimmed))
        .then_with(|| left_digits.len().cmp(&right_digits.len()))
}

/// Drops leading zero bytes while keeping one zero for an all-zero digit run.
pub(in crate::interpreter) fn eval_trim_leading_zeroes(digits: &[u8]) -> &[u8] {
    let trimmed = digits
        .iter()
        .position(|digit| *digit != b'0')
        .map_or(&digits[digits.len().saturating_sub(1)..], |index| {
            &digits[index..]
        });
    if trimmed.is_empty() {
        digits
    } else {
        trimmed
    }
}
