//! Purpose:
//! Validates recursively flattened native result trees before compiler materialization.
//! Rejects malformed ownership graphs without allocating PHP runtime values.
//!
//! Called from:
//! - Generated heterogeneous internal-extension map result lowering.
//!
//! Key details:
//! - Monotone value and byte cursors reproduce the bridge flattener's reserve-before-descent order.
//! - Exact final cursors reject gaps, trailing records/bytes, aliasing, overlap, back-edges, and cycles.

use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::abi::{
    Value, VALUE_ARRAY, VALUE_BOOL, VALUE_BRIDGE_HANDLE, VALUE_BYTES, VALUE_CALLABLE,
    VALUE_FLOAT, VALUE_INT, VALUE_MAP, VALUE_NULL, VALUE_OBJECT,
};
use crate::objects::{VALUE_OBJECT_LIBXML_ERROR, VALUE_OBJECT_NAMESPACE_INFO};

const MAX_RESULT_TREE_DEPTH: u32 = 64;

/// Monotone reservation cursors shared by one complete validation traversal.
struct ValidationCursor {
    next_value: u64,
    next_byte: u64,
}

/// Validates one top-level heterogeneous map and every recursively referenced record.
///
/// Returns zero only for the exact canonical flat encoding produced by the bridge.
/// The function performs no heap allocation and contains every invalid pointer,
/// range, payload, graph edge, or recursion depth as status one.
#[no_mangle]
pub unsafe extern "C" fn elephc_dom_validate_result_map_tree(
    values: *const Value,
    value_count: u64,
    bytes: *const u8,
    byte_count: u64,
    entry_count: u64,
    mixed_keys: u32,
) -> u32 {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let direct_count = entry_count.checked_mul(2).ok_or(())?;
        if direct_count > value_count
            || (value_count != 0 && values.is_null())
            || (byte_count != 0 && bytes.is_null())
            || mixed_keys > 1
        {
            return Err(());
        }
        let mut cursor = ValidationCursor {
            next_value: direct_count,
            next_byte: 0,
        };
        for entry in 0..entry_count {
            let key = entry.checked_mul(2).ok_or(())?;
            validate_record(
                values,
                value_count,
                byte_count,
                key,
                true,
                mixed_keys != 0,
                0,
                &mut cursor,
            )?;
            validate_record(
                values,
                value_count,
                byte_count,
                key.checked_add(1).ok_or(())?,
                false,
                true,
                0,
                &mut cursor,
            )?;
        }
        if cursor.next_value != value_count || cursor.next_byte != byte_count {
            return Err(());
        }
        Ok::<(), ()>(())
    }));
    match result {
        Ok(Ok(())) => 0,
        Ok(Err(())) | Err(_) => 1,
    }
}

/// Validates one record and advances the canonical reservation cursors recursively.
#[allow(clippy::too_many_arguments)]
unsafe fn validate_record(
    values: *const Value,
    value_count: u64,
    byte_count: u64,
    index: u64,
    key: bool,
    integer_keys: bool,
    depth: u32,
    cursor: &mut ValidationCursor,
) -> Result<(), ()> {
    if depth >= MAX_RESULT_TREE_DEPTH || index >= value_count {
        return Err(());
    }
    let index = usize::try_from(index).map_err(|_| ())?;
    let record = std::ptr::read_unaligned(values.add(index));
    if key {
        if record.flags != 0 {
            return Err(());
        }
        return match record.tag {
            VALUE_INT if integer_keys && record.payload1 == 0 => Ok(()),
            VALUE_BYTES => validate_bytes(record, byte_count, cursor),
            _ => Err(()),
        };
    }
    match record.tag {
        VALUE_NULL
            if record.flags == 0 && record.payload0 == 0 && record.payload1 == 0 =>
        {
            Ok(())
        }
        VALUE_BOOL if record.flags == 0 && record.payload0 <= 1 && record.payload1 == 0 => {
            Ok(())
        }
        VALUE_INT | VALUE_FLOAT if record.flags == 0 && record.payload1 == 0 => Ok(()),
        VALUE_BYTES if record.flags == 0 => validate_bytes(record, byte_count, cursor),
        VALUE_ARRAY if record.flags == 0 => validate_array(
            values,
            value_count,
            byte_count,
            record,
            depth,
            cursor,
        ),
        VALUE_MAP if record.flags == 0 => validate_map(
            values,
            value_count,
            byte_count,
            record,
            depth,
            cursor,
        ),
        VALUE_BRIDGE_HANDLE if record.flags == 0 && record.payload0 != 0 => Ok(()),
        VALUE_CALLABLE
            if record.flags == 0 && record.payload0 != 0 && record.payload1 == 0 =>
        {
            Ok(())
        }
        VALUE_OBJECT => validate_value_object(
            values,
            value_count,
            byte_count,
            record,
            depth,
            cursor,
        ),
        _ => Err(()),
    }
}

/// Validates one canonical append-only byte range and advances the byte cursor.
fn validate_bytes(
    record: Value,
    byte_count: u64,
    cursor: &mut ValidationCursor,
) -> Result<(), ()> {
    if record.payload0 != cursor.next_byte {
        return Err(());
    }
    let end = record.payload0.checked_add(record.payload1).ok_or(())?;
    if end > byte_count {
        return Err(());
    }
    cursor.next_byte = end;
    Ok(())
}

/// Validates one reserve-before-descent indexed range and all of its children.
unsafe fn validate_array(
    values: *const Value,
    value_count: u64,
    byte_count: u64,
    record: Value,
    depth: u32,
    cursor: &mut ValidationCursor,
) -> Result<(), ()> {
    if record.payload0 != cursor.next_value {
        return Err(());
    }
    let end = record.payload0.checked_add(record.payload1).ok_or(())?;
    if end > value_count {
        return Err(());
    }
    cursor.next_value = end;
    for offset in 0..record.payload1 {
        validate_record(
            values,
            value_count,
            byte_count,
            record.payload0.checked_add(offset).ok_or(())?,
            false,
            true,
            depth.checked_add(1).ok_or(())?,
            cursor,
        )?;
    }
    Ok(())
}

/// Validates one reserve-before-descent alternating map range and all of its pairs.
unsafe fn validate_map(
    values: *const Value,
    value_count: u64,
    byte_count: u64,
    record: Value,
    depth: u32,
    cursor: &mut ValidationCursor,
) -> Result<(), ()> {
    if record.payload0 != cursor.next_value {
        return Err(());
    }
    let child_count = record.payload1.checked_mul(2).ok_or(())?;
    let end = record.payload0.checked_add(child_count).ok_or(())?;
    if end > value_count {
        return Err(());
    }
    cursor.next_value = end;
    for offset in 0..record.payload1 {
        let key = record
            .payload0
            .checked_add(offset.checked_mul(2).ok_or(())?)
            .ok_or(())?;
        validate_record(
            values,
            value_count,
            byte_count,
            key,
            true,
            true,
            depth.checked_add(1).ok_or(())?,
            cursor,
        )?;
        validate_record(
            values,
            value_count,
            byte_count,
            key.checked_add(1).ok_or(())?,
            false,
            true,
            depth.checked_add(1).ok_or(())?,
            cursor,
        )?;
    }
    Ok(())
}

/// Validates one copied native object descriptor and its exact PHP field schema.
unsafe fn validate_value_object(
    values: *const Value,
    value_count: u64,
    byte_count: u64,
    record: Value,
    depth: u32,
    cursor: &mut ValidationCursor,
) -> Result<(), ()> {
    let expected_count = match record.flags {
        VALUE_OBJECT_LIBXML_ERROR => 6,
        VALUE_OBJECT_NAMESPACE_INFO => 3,
        _ => return Err(()),
    };
    if record.payload1 != expected_count || record.payload0 != cursor.next_value {
        return Err(());
    }
    let end = record.payload0.checked_add(expected_count).ok_or(())?;
    if end > value_count || depth.checked_add(1).ok_or(())? >= MAX_RESULT_TREE_DEPTH {
        return Err(());
    }
    cursor.next_value = end;
    match record.flags {
        VALUE_OBJECT_LIBXML_ERROR => {
            validate_libxml_error_fields(values, byte_count, record.payload0, cursor)
        }
        VALUE_OBJECT_NAMESPACE_INFO => {
            validate_namespace_info_fields(values, byte_count, record.payload0, cursor)
        }
        _ => Err(()),
    }
}

/// Validates the six declaration-ordered fields of one copied `LibXMLError`.
unsafe fn validate_libxml_error_fields(
    values: *const Value,
    byte_count: u64,
    start: u64,
    cursor: &mut ValidationCursor,
) -> Result<(), ()> {
    for offset in [0, 1, 2, 5] {
        let field = read_record(values, start.checked_add(offset).ok_or(())?)?;
        if field.tag != VALUE_INT || field.flags != 0 || field.payload1 != 0 {
            return Err(());
        }
    }
    for offset in [3, 4] {
        let field = read_record(values, start.checked_add(offset).ok_or(())?)?;
        if field.tag != VALUE_BYTES || field.flags != 0 {
            return Err(());
        }
        validate_bytes(field, byte_count, cursor)?;
    }
    Ok(())
}

/// Validates nullable namespace strings and the canonical modern element owner.
unsafe fn validate_namespace_info_fields(
    values: *const Value,
    byte_count: u64,
    start: u64,
    cursor: &mut ValidationCursor,
) -> Result<(), ()> {
    for offset in [0, 1] {
        let field = read_record(values, start.checked_add(offset).ok_or(())?)?;
        match field.tag {
            VALUE_NULL
                if field.flags == 0 && field.payload0 == 0 && field.payload1 == 0 => {}
            VALUE_BYTES if field.flags == 0 => validate_bytes(field, byte_count, cursor)?,
            _ => return Err(()),
        }
    }
    let element = read_record(values, start.checked_add(2).ok_or(())?)?;
    if element.tag != VALUE_BRIDGE_HANDLE
        || element.flags != 0
        || element.payload0 == 0
        || !matches!(element.payload1, 201 | 301)
    {
        return Err(());
    }
    Ok(())
}

/// Reads one already range-checked flat record without imposing alignment.
unsafe fn read_record(values: *const Value, index: u64) -> Result<Value, ()> {
    let index = usize::try_from(index).map_err(|_| ())?;
    Ok(std::ptr::read_unaligned(values.add(index)))
}

#[cfg(test)]
mod tests {
    use super::elephc_dom_validate_result_map_tree;
    use crate::abi::{
        Value, VALUE_ARRAY, VALUE_BRIDGE_HANDLE, VALUE_BYTES, VALUE_INT, VALUE_MAP,
        VALUE_NULL, VALUE_OBJECT,
    };
    use crate::objects::{VALUE_OBJECT_LIBXML_ERROR, VALUE_OBJECT_NAMESPACE_INFO};

    /// Builds one canonical recursive map matching the SimpleXML debug-info flat shape.
    fn valid_tree() -> (Vec<Value>, Vec<u8>) {
        let bytes = b"@attributesid7aA2".to_vec();
        let values = vec![
            Value { tag: VALUE_BYTES, flags: 0, payload0: 0, payload1: 11 },
            Value { tag: VALUE_MAP, flags: 0, payload0: 4, payload1: 1 },
            Value { tag: VALUE_BYTES, flags: 0, payload0: 14, payload1: 1 },
            Value { tag: VALUE_ARRAY, flags: 0, payload0: 6, payload1: 2 },
            Value { tag: VALUE_BYTES, flags: 0, payload0: 11, payload1: 2 },
            Value { tag: VALUE_BYTES, flags: 0, payload0: 13, payload1: 1 },
            Value { tag: VALUE_BRIDGE_HANDLE, flags: 0, payload0: 9, payload1: 0 },
            Value { tag: VALUE_BYTES, flags: 0, payload0: 15, payload1: 2 },
        ];
        (values, bytes)
    }

    /// Calls the exported validator with one owned fixture's stable raw pointers.
    fn validate(values: &[Value], bytes: &[u8], entries: u64) -> u32 {
        unsafe {
            elephc_dom_validate_result_map_tree(
                values.as_ptr(),
                values.len() as u64,
                bytes.as_ptr(),
                bytes.len() as u64,
                entries,
                1,
            )
        }
    }

    /// Accepts exact reserve-before-descent value ranges and DFS byte append order.
    #[test]
    fn accepts_canonical_recursive_tree() {
        let (values, bytes) = valid_tree();
        assert_eq!(validate(&values, &bytes, 2), 0);
    }

    /// Rejects orphan records, aliased child ranges, back-edges, and trailing bytes.
    #[test]
    fn rejects_noncanonical_graph_and_byte_coverage() {
        let (values, bytes) = valid_tree();
        let mut orphan = values.clone();
        orphan.push(Value { tag: VALUE_INT, flags: 0, payload0: 1, payload1: 0 });
        assert_eq!(validate(&orphan, &bytes, 2), 1);

        let mut alias = values.clone();
        alias[1].payload0 = 2;
        assert_eq!(validate(&alias, &bytes, 2), 1);

        let mut trailing = bytes.clone();
        trailing.push(b'x');
        assert_eq!(validate(&values, &trailing, 2), 1);
    }

    /// Rejects non-zero flags, noncanonical scalar payloads, bad keys, and null handles.
    #[test]
    fn rejects_noncanonical_record_contracts() {
        let (values, bytes) = valid_tree();
        let mut flagged = values.clone();
        flagged[0].flags = 1;
        assert_eq!(validate(&flagged, &bytes, 2), 1);

        let mut bad_key = values.clone();
        bad_key[0].tag = VALUE_INT;
        bad_key[0].payload1 = 1;
        assert_eq!(validate(&bad_key, &bytes, 2), 1);

        let mut null_handle = values.clone();
        null_handle[6].payload0 = 0;
        assert_eq!(validate(&null_handle, &bytes, 2), 1);

        let null_values = unsafe {
            elephc_dom_validate_result_map_tree(
                std::ptr::null(),
                values.len() as u64,
                bytes.as_ptr(),
                bytes.len() as u64,
                2,
                1,
            )
        };
        assert_eq!(null_values, 1);

        let null_bytes = unsafe {
            elephc_dom_validate_result_map_tree(
                values.as_ptr(),
                values.len() as u64,
                std::ptr::null(),
                bytes.len() as u64,
                2,
                1,
            )
        };
        assert_eq!(null_bytes, 1);
    }

    /// Accepts the exact copied `LibXMLError` field schema inside a recursive map.
    #[test]
    fn accepts_canonical_libxml_error_value_object() {
        let bytes = b"kmessagefile".to_vec();
        let values = vec![
            Value { tag: VALUE_BYTES, flags: 0, payload0: 0, payload1: 1 },
            Value {
                tag: VALUE_OBJECT,
                flags: VALUE_OBJECT_LIBXML_ERROR,
                payload0: 2,
                payload1: 6,
            },
            Value { tag: VALUE_INT, flags: 0, payload0: 1, payload1: 0 },
            Value { tag: VALUE_INT, flags: 0, payload0: 2, payload1: 0 },
            Value { tag: VALUE_INT, flags: 0, payload0: 3, payload1: 0 },
            Value { tag: VALUE_BYTES, flags: 0, payload0: 1, payload1: 7 },
            Value { tag: VALUE_BYTES, flags: 0, payload0: 8, payload1: 4 },
            Value { tag: VALUE_INT, flags: 0, payload0: 4, payload1: 0 },
        ];
        assert_eq!(validate(&values, &bytes, 1), 0);
    }

    /// Accepts the exact copied namespace-info schema and rejects schema drift.
    #[test]
    fn validates_namespace_info_value_object_schema() {
        let bytes = b"kns".to_vec();
        let values = vec![
            Value { tag: VALUE_BYTES, flags: 0, payload0: 0, payload1: 1 },
            Value {
                tag: VALUE_OBJECT,
                flags: VALUE_OBJECT_NAMESPACE_INFO,
                payload0: 2,
                payload1: 3,
            },
            Value { tag: VALUE_NULL, flags: 0, payload0: 0, payload1: 0 },
            Value { tag: VALUE_BYTES, flags: 0, payload0: 1, payload1: 2 },
            Value {
                tag: VALUE_BRIDGE_HANDLE,
                flags: 0,
                payload0: 9,
                payload1: 201,
            },
        ];
        assert_eq!(validate(&values, &bytes, 1), 0);

        let mut wrong_field = values.clone();
        wrong_field[3].tag = VALUE_INT;
        assert_eq!(validate(&wrong_field, &bytes, 1), 1);

        let mut wrong_kind = values.clone();
        wrong_kind[4].payload1 = 200;
        assert_eq!(validate(&wrong_kind, &bytes, 1), 1);
    }
}
