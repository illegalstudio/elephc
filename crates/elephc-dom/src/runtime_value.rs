//! Purpose:
//! Serializes borrowed Elephc runtime values into the versioned flat DOM request ABI.
//! Keeps PHP array/hash storage knowledge outside DOM dispatch and native engine code.
//!
//! Called from:
//! - Generated internal-extension lowering for array-valued DOM parameters.
//!
//! Key details:
//! - Measurement and writing traverse the same bounded value tree without retaining PHP storage.
//! - Root records stay in ABI argument order while descendants occupy validated trailing records.

use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::abi::{
    Value, VALUE_ARRAY, VALUE_BOOL, VALUE_BYTES, VALUE_CALLABLE, VALUE_FLOAT, VALUE_INT,
    VALUE_MAP, VALUE_NULL, VALUE_OBJECT, VALUE_RESOURCE,
};

const RUNTIME_INT: u64 = 0;
const RUNTIME_STRING: u64 = 1;
const RUNTIME_FLOAT: u64 = 2;
const RUNTIME_BOOL: u64 = 3;
const RUNTIME_ARRAY: u64 = 4;
const RUNTIME_HASH: u64 = 5;
const RUNTIME_OBJECT: u64 = 6;
const RUNTIME_MIXED: u64 = 7;
const RUNTIME_NULL: u64 = 8;
const RUNTIME_RESOURCE: u64 = 9;
const RUNTIME_CALLABLE: u64 = 10;
const INTEGER_HASH_KEY_LENGTH: u64 = u64::MAX;
const MAX_MIXED_INDIRECTIONS: usize = 64;
const MAX_CONTAINER_VALUES: usize = 1 << 24;

/// One compiler-emitted class-name table row.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RuntimeClassName {
    pub pointer: *const u8,
    pub length: u64,
}

/// Counts required for one serialized root value and all of its descendants.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct RuntimeValueMeasure {
    pub value_count: u64,
    pub byte_count: u64,
}

/// Caller-owned destinations and cursors used to append one serialized root tree.
#[repr(C)]
pub struct RuntimeValueWriteContext {
    pub values: *mut Value,
    pub value_capacity: u64,
    pub next_index: *mut u64,
    pub bytes: *mut u8,
    pub byte_capacity: u64,
    pub byte_cursor: *mut u64,
    pub class_names: *const RuntimeClassName,
    pub class_name_count: u64,
    pub max_depth: u32,
    pub reserved: u32,
}

/// The three-word boxed value representation shared by Elephc runtime containers.
#[repr(C)]
#[derive(Clone, Copy)]
struct RuntimeValue {
    tag: u64,
    payload0: u64,
    payload1: u64,
}

/// An owned tree ready to flatten into ABI records.
enum RuntimeNode {
    Null,
    Bool(u64),
    Int(u64),
    Float(u64),
    Bytes(Vec<u8>),
    Array {
        values: Vec<RuntimeNode>,
        runtime_pointer: u64,
    },
    Map {
        entries: Vec<(RuntimeNode, RuntimeNode)>,
        runtime_pointer: u64,
    },
    Object {
        class_name: Vec<u8>,
    },
    Resource,
    Callable(u64),
}

/// One locally indexed flat tree plus its compact byte section.
struct FlatRuntimeValue {
    values: Vec<Value>,
    bytes: Vec<u8>,
}

/// One pre-encoded XPath registration value plus descriptors owned until bridge return.
struct PreparedXPathCallbackValue {
    flat: FlatRuntimeValue,
    descriptors: Vec<u64>,
}

#[cfg(not(test))]
unsafe extern "C" {
    fn __rt_dom_xpath_resolve_callable_array(pointer: *const u8, kind: u64) -> u64;
}

/// Resolves one borrowed runtime callable-array pointer through generated program metadata.
unsafe fn resolve_xpath_callable_array(pointer: u64, kind: u64) -> u64 {
    #[cfg(not(test))]
    {
        __rt_dom_xpath_resolve_callable_array(pointer as *const u8, kind)
    }
    #[cfg(test)]
    {
        let _ = (pointer, kind);
        0xcafe
    }
}

/// Measures one borrowed runtime value for a later checked request allocation.
///
/// Returns zero on success and one when pointers, runtime tags, lengths, or storage
/// metadata do not satisfy Elephc's internal value contract.
#[no_mangle]
pub unsafe extern "C" fn elephc_dom_measure_runtime_value(
    value: *const u8,
    class_names: *const RuntimeClassName,
    class_name_count: u64,
    max_depth: u32,
    out_measure: *mut RuntimeValueMeasure,
) -> u32 {
    if value.is_null() || out_measure.is_null() {
        return 1;
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let classes = class_name_slice(class_names, class_name_count)?;
        let flat = encode_runtime_value(value.cast::<RuntimeValue>(), classes, max_depth)?;
        let value_count = u64::try_from(flat.values.len()).map_err(|_| ())?;
        let byte_count = u64::try_from(flat.bytes.len()).map_err(|_| ())?;
        std::ptr::write(
            out_measure,
            RuntimeValueMeasure {
                value_count,
                byte_count,
            },
        );
        Ok::<(), ()>(())
    }));
    match result {
        Ok(Ok(())) => 0,
        Ok(Err(())) | Err(_) => 1,
    }
}

/// Writes one borrowed runtime value into a root record and trailing flat sections.
///
/// Returns zero only after capacity checks and exact cursor advancement succeed.
#[no_mangle]
pub unsafe extern "C" fn elephc_dom_write_runtime_value(
    value: *const u8,
    root: *mut Value,
    context: *mut RuntimeValueWriteContext,
) -> u32 {
    if value.is_null() || root.is_null() || context.is_null() {
        return 1;
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let context = &mut *context;
        if context.next_index.is_null() || context.byte_cursor.is_null() {
            return Err(());
        }
        let classes = class_name_slice(context.class_names, context.class_name_count)?;
        let flat = encode_runtime_value(
            value.cast::<RuntimeValue>(),
            classes,
            context.max_depth,
        )?;
        append_flat_value(&flat, root, context)
    }));
    match result {
        Ok(Ok(())) => 0,
        Ok(Err(())) | Err(_) => 1,
    }
}

/// Prepares one XPath callback restriction value and resolves every nested callable array once.
#[no_mangle]
pub unsafe extern "C" fn elephc_dom_prepare_xpath_callback_value(
    value: *const u8,
    class_names: *const RuntimeClassName,
    class_name_count: u64,
    max_depth: u32,
    out_measure: *mut RuntimeValueMeasure,
    out_plan: *mut *mut u8,
) -> u32 {
    if value.is_null() || out_measure.is_null() || out_plan.is_null() {
        return 1;
    }
    std::ptr::write(out_plan, std::ptr::null_mut());
    let result = catch_unwind(AssertUnwindSafe(|| {
        let classes = class_name_slice(class_names, class_name_count)?;
        let mut node = decode_runtime_node(
            value.cast::<RuntimeValue>(),
            classes,
            0,
            max_depth,
        )?;
        let mut descriptors = Vec::new();
        normalize_xpath_callback_arrays(&mut node, true, &mut descriptors)?;
        let mut flat = FlatRuntimeValue {
            values: vec![null_value()],
            bytes: Vec::new(),
        };
        flatten_node(&node, 0, &mut flat)?;
        let value_count = u64::try_from(flat.values.len()).map_err(|_| ())?;
        let byte_count = u64::try_from(flat.bytes.len()).map_err(|_| ())?;
        let plan = Box::new(PreparedXPathCallbackValue { flat, descriptors });
        std::ptr::write(
            out_measure,
            RuntimeValueMeasure {
                value_count,
                byte_count,
            },
        );
        std::ptr::write(out_plan, Box::into_raw(plan).cast::<u8>());
        Ok::<(), ()>(())
    }));
    match result {
        Ok(Ok(())) => 0,
        Ok(Err(())) | Err(_) => 1,
    }
}

/// Appends one previously prepared XPath callback value without re-running resolution.
#[no_mangle]
pub unsafe extern "C" fn elephc_dom_write_prepared_xpath_callback_value(
    plan: *const u8,
    root: *mut Value,
    context: *mut RuntimeValueWriteContext,
) -> u32 {
    if plan.is_null() || root.is_null() || context.is_null() {
        return 1;
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let plan = &*plan.cast::<PreparedXPathCallbackValue>();
        append_flat_value(&plan.flat, root, &mut *context)
    }));
    match result {
        Ok(Ok(())) => 0,
        Ok(Err(())) | Err(_) => 1,
    }
}

/// Returns the number of temporary callable descriptors retained by one prepared plan.
#[no_mangle]
pub unsafe extern "C" fn elephc_dom_prepared_xpath_callback_descriptor_count(
    plan: *const u8,
) -> u64 {
    if plan.is_null() {
        return 0;
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        u64::try_from(
            (&*plan.cast::<PreparedXPathCallbackValue>())
                .descriptors
                .len(),
        )
        .unwrap_or(0)
    }));
    result.unwrap_or(0)
}

/// Returns one temporary descriptor from a prepared plan or zero for an invalid index.
#[no_mangle]
pub unsafe extern "C" fn elephc_dom_prepared_xpath_callback_descriptor_at(
    plan: *const u8,
    index: u64,
) -> u64 {
    if plan.is_null() {
        return 0;
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let index = usize::try_from(index).ok()?;
        (&*plan.cast::<PreparedXPathCallbackValue>())
            .descriptors
            .get(index)
            .copied()
    }));
    result.ok().flatten().unwrap_or(0)
}

/// Frees one prepared XPath callback plan after the caller releases its descriptors.
#[no_mangle]
pub unsafe extern "C" fn elephc_dom_prepared_xpath_callback_value_free(
    plan: *mut u8,
) {
    if plan.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        drop(Box::from_raw(
            plan.cast::<PreparedXPathCallbackValue>(),
        ));
    }));
}

/// Converts a raw class-name table into an optional borrowed slice.
unsafe fn class_name_slice<'a>(
    class_names: *const RuntimeClassName,
    class_name_count: u64,
) -> Result<&'a [RuntimeClassName], ()> {
    let count = usize::try_from(class_name_count).map_err(|_| ())?;
    if count == 0 {
        return Ok(&[]);
    }
    if class_names.is_null() {
        return Err(());
    }
    Ok(std::slice::from_raw_parts(class_names, count))
}

/// Builds a complete local flat tree from one borrowed runtime-value cell.
unsafe fn encode_runtime_value(
    value: *const RuntimeValue,
    class_names: &[RuntimeClassName],
    max_depth: u32,
) -> Result<FlatRuntimeValue, ()> {
    let node = decode_runtime_node(value, class_names, 0, max_depth)?;
    let mut flat = FlatRuntimeValue {
        values: vec![null_value()],
        bytes: Vec::new(),
    };
    flatten_node(&node, 0, &mut flat)?;
    Ok(flat)
}

/// Replaces nested callable-array shapes with descriptors while preserving the root list/map.
fn normalize_xpath_callback_arrays(
    node: &mut RuntimeNode,
    root: bool,
    descriptors: &mut Vec<u64>,
) -> Result<(), ()> {
    if !root {
        let candidate = match node {
            RuntimeNode::Array {
                values,
                runtime_pointer,
            } if indexed_node_is_callable_pair(values) => {
                Some((*runtime_pointer, RUNTIME_ARRAY))
            }
            RuntimeNode::Map {
                entries,
                runtime_pointer,
            } if associative_node_is_callable_pair(entries) => {
                Some((*runtime_pointer, RUNTIME_HASH))
            }
            _ => None,
        };
        if let Some((pointer, kind)) = candidate {
            let descriptor = unsafe {
                resolve_xpath_callable_array(pointer, kind)
            };
            if descriptor != 0 {
                descriptors.try_reserve(1).map_err(|_| ())?;
                descriptors.push(descriptor);
                *node = RuntimeNode::Callable(descriptor);
                return Ok(());
            }
        }
    }

    match node {
        RuntimeNode::Array { values, .. } => {
            for value in values {
                normalize_xpath_callback_arrays(value, false, descriptors)?;
            }
        }
        RuntimeNode::Map { entries, .. } => {
            for (key, value) in entries {
                normalize_xpath_callback_arrays(key, false, descriptors)?;
                normalize_xpath_callback_arrays(value, false, descriptors)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Reports whether one indexed node has PHP's `[object|string, string]` callable shape.
fn indexed_node_is_callable_pair(values: &[RuntimeNode]) -> bool {
    values.len() == 2
        && matches!(
            values.first(),
            Some(RuntimeNode::Object { .. } | RuntimeNode::Bytes(_))
        )
        && matches!(values.get(1), Some(RuntimeNode::Bytes(_)))
}

/// Reports whether one associative node exposes numeric keys zero and one as a callable pair.
fn associative_node_is_callable_pair(
    entries: &[(RuntimeNode, RuntimeNode)],
) -> bool {
    if entries.len() != 2 {
        return false;
    }
    let mut receiver = None;
    let mut method = None;
    for (key, value) in entries {
        match key {
            RuntimeNode::Int(0) => receiver = Some(value),
            RuntimeNode::Int(1) => method = Some(value),
            _ => return false,
        }
    }
    matches!(
        receiver,
        Some(RuntimeNode::Object { .. } | RuntimeNode::Bytes(_))
    ) && matches!(method, Some(RuntimeNode::Bytes(_)))
}

/// Decodes one runtime cell, following bounded nested-Mixed indirections.
unsafe fn decode_runtime_node(
    mut value: *const RuntimeValue,
    class_names: &[RuntimeClassName],
    depth: u32,
    max_depth: u32,
) -> Result<RuntimeNode, ()> {
    let mut current = std::ptr::read_unaligned(value);
    for _ in 0..MAX_MIXED_INDIRECTIONS {
        if current.tag != RUNTIME_MIXED {
            return decode_concrete_node(current, class_names, depth, max_depth);
        }
        value =
            usize::try_from(current.payload0).map_err(|_| ())? as *const RuntimeValue;
        if value.is_null() {
            return Ok(RuntimeNode::Null);
        }
        current = std::ptr::read_unaligned(value);
    }
    Err(())
}

/// Maps one concrete runtime tag and payload into an owned semantic node.
unsafe fn decode_concrete_node(
    value: RuntimeValue,
    class_names: &[RuntimeClassName],
    depth: u32,
    max_depth: u32,
) -> Result<RuntimeNode, ()> {
    match value.tag {
        RUNTIME_INT => Ok(RuntimeNode::Int(value.payload0)),
        RUNTIME_STRING => Ok(RuntimeNode::Bytes(copy_runtime_bytes(
            value.payload0,
            value.payload1,
        )?)),
        RUNTIME_FLOAT => Ok(RuntimeNode::Float(value.payload0)),
        RUNTIME_BOOL => (value.payload0 <= 1)
            .then_some(RuntimeNode::Bool(value.payload0))
            .ok_or(()),
        RUNTIME_ARRAY => decode_runtime_array(
            value.payload0,
            value.payload1,
            class_names,
            depth,
            max_depth,
        ),
        RUNTIME_HASH => decode_runtime_hash(value.payload0, class_names, depth, max_depth),
        RUNTIME_OBJECT => Ok(RuntimeNode::Object {
            class_name: runtime_object_name(value.payload0, class_names)?,
        }),
        RUNTIME_NULL => Ok(RuntimeNode::Null),
        RUNTIME_RESOURCE => Ok(RuntimeNode::Resource),
        RUNTIME_CALLABLE => (value.payload0 != 0)
            .then_some(RuntimeNode::Callable(value.payload0))
            .ok_or(()),
        _ => Err(()),
    }
}

/// Copies one runtime string after checked integer conversion and null validation.
unsafe fn copy_runtime_bytes(pointer: u64, length: u64) -> Result<Vec<u8>, ()> {
    let length = usize::try_from(length).map_err(|_| ())?;
    if length == 0 {
        return Ok(Vec::new());
    }
    let pointer = usize::try_from(pointer).map_err(|_| ())? as *const u8;
    if pointer.is_null() {
        return Err(());
    }
    Ok(std::slice::from_raw_parts(pointer, length).to_vec())
}

/// Decodes one indexed Elephc array using its packed value tag and element stride.
unsafe fn decode_runtime_array(
    pointer: u64,
    element_tag_override: u64,
    class_names: &[RuntimeClassName],
    depth: u32,
    max_depth: u32,
) -> Result<RuntimeNode, ()> {
    if depth >= max_depth {
        return Ok(RuntimeNode::Array {
            values: Vec::new(),
            runtime_pointer: pointer,
        });
    }
    let pointer = usize::try_from(pointer).map_err(|_| ())? as *const u8;
    if pointer.is_null() {
        return Ok(RuntimeNode::Null);
    }
    let length = usize::try_from(std::ptr::read_unaligned(pointer.cast::<u64>()))
        .map_err(|_| ())?;
    if length > MAX_CONTAINER_VALUES {
        return Err(());
    }
    let stride = usize::try_from(std::ptr::read_unaligned(pointer.add(16).cast::<u64>()))
        .map_err(|_| ())?;
    if stride == 0 || stride > std::mem::size_of::<RuntimeValue>() {
        return Err(());
    }
    let kind = std::ptr::read_unaligned(pointer.sub(8).cast::<u64>());
    let value_tag = if element_tag_override == 0 {
        (kind >> 8) & 0x7f
    } else if element_tag_override == RUNTIME_CALLABLE {
        RUNTIME_CALLABLE
    } else {
        return Err(());
    };
    let mut values = Vec::new();
    values.try_reserve_exact(length).map_err(|_| ())?;
    for index in 0..length {
        let offset = index.checked_mul(stride).ok_or(())?;
        let slot = pointer.add(24).add(offset);
        let child = if value_tag == RUNTIME_MIXED {
            let mixed = std::ptr::read_unaligned(slot.cast::<*const RuntimeValue>());
            if mixed.is_null() {
                RuntimeNode::Null
            } else {
                decode_runtime_node(mixed, class_names, depth + 1, max_depth)?
            }
        } else {
            let payload0 = std::ptr::read_unaligned(slot.cast::<u64>());
            let payload1 = if stride >= 16 {
                std::ptr::read_unaligned(slot.add(8).cast::<u64>())
            } else {
                0
            };
            decode_concrete_node(
                RuntimeValue {
                    tag: value_tag,
                    payload0,
                    payload1,
                },
                class_names,
                depth + 1,
                max_depth,
            )?
        };
        values.push(child);
    }
    Ok(RuntimeNode::Array {
        values,
        runtime_pointer: pointer as usize as u64,
    })
}

/// Decodes one insertion-ordered Elephc associative hash and its exact PHP keys.
unsafe fn decode_runtime_hash(
    pointer: u64,
    class_names: &[RuntimeClassName],
    depth: u32,
    max_depth: u32,
) -> Result<RuntimeNode, ()> {
    if depth >= max_depth {
        return Ok(RuntimeNode::Map {
            entries: Vec::new(),
            runtime_pointer: pointer,
        });
    }
    let pointer = usize::try_from(pointer).map_err(|_| ())? as *const u8;
    if pointer.is_null() {
        return Ok(RuntimeNode::Null);
    }
    let count = usize::try_from(std::ptr::read_unaligned(pointer.cast::<u64>()))
        .map_err(|_| ())?;
    let capacity =
        usize::try_from(std::ptr::read_unaligned(pointer.add(8).cast::<u64>()))
            .map_err(|_| ())?;
    if count > capacity || count > MAX_CONTAINER_VALUES {
        return Err(());
    }
    let mut slot = std::ptr::read_unaligned(pointer.add(24).cast::<i64>());
    let mut entries = Vec::new();
    entries.try_reserve_exact(count).map_err(|_| ())?;
    for position in 0..count {
        if slot < 0 || usize::try_from(slot).map_err(|_| ())? >= capacity {
            return Err(());
        }
        let entry = pointer
            .add(40)
            .add(usize::try_from(slot).map_err(|_| ())?.checked_mul(64).ok_or(())?);
        if std::ptr::read_unaligned(entry.cast::<u64>()) == 0 {
            return Err(());
        }
        let key_pointer = std::ptr::read_unaligned(entry.add(8).cast::<u64>());
        let key_length = std::ptr::read_unaligned(entry.add(16).cast::<u64>());
        let key = if key_length == INTEGER_HASH_KEY_LENGTH {
            RuntimeNode::Int(key_pointer)
        } else {
            RuntimeNode::Bytes(copy_runtime_bytes(key_pointer, key_length)?)
        };
        let child = RuntimeValue {
            tag: std::ptr::read_unaligned(entry.add(40).cast::<u64>()),
            payload0: std::ptr::read_unaligned(entry.add(24).cast::<u64>()),
            payload1: std::ptr::read_unaligned(entry.add(32).cast::<u64>()),
        };
        let value = if child.tag == RUNTIME_MIXED {
            let mixed =
                usize::try_from(child.payload0).map_err(|_| ())? as *const RuntimeValue;
            if mixed.is_null() {
                RuntimeNode::Null
            } else {
                decode_runtime_node(mixed, class_names, depth + 1, max_depth)?
            }
        } else {
            decode_concrete_node(child, class_names, depth + 1, max_depth)?
        };
        entries.push((key, value));
        slot = std::ptr::read_unaligned(entry.add(56).cast::<i64>());
        if position + 1 < count && slot < 0 {
            return Err(());
        }
    }
    Ok(RuntimeNode::Map {
        entries,
        runtime_pointer: pointer as usize as u64,
    })
}

/// Resolves one runtime object's exact concrete PHP class name.
unsafe fn runtime_object_name(
    pointer: u64,
    class_names: &[RuntimeClassName],
) -> Result<Vec<u8>, ()> {
    let pointer = usize::try_from(pointer).map_err(|_| ())? as *const u64;
    if pointer.is_null() {
        return Err(());
    }
    let class_id = usize::try_from(std::ptr::read_unaligned(pointer)).map_err(|_| ())?;
    let Some(class_name) = class_names.get(class_id) else {
        return Err(());
    };
    copy_runtime_bytes(class_name.pointer as usize as u64, class_name.length)
}

/// Flattens one owned node while reserving each container's direct children contiguously.
fn flatten_node(
    node: &RuntimeNode,
    index: usize,
    flat: &mut FlatRuntimeValue,
) -> Result<(), ()> {
    match node {
        RuntimeNode::Null => flat.values[index] = null_value(),
        RuntimeNode::Bool(value) => flat.values[index] = scalar_value(VALUE_BOOL, *value),
        RuntimeNode::Int(value) => flat.values[index] = scalar_value(VALUE_INT, *value),
        RuntimeNode::Float(value) => flat.values[index] = scalar_value(VALUE_FLOAT, *value),
        RuntimeNode::Bytes(bytes) => flat.values[index] = append_bytes(bytes, flat)?,
        RuntimeNode::Array { values, .. } => {
            let start = reserve_values(flat, values.len())?;
            flat.values[index] = range_value(VALUE_ARRAY, start, values.len())?;
            for (offset, child) in values.iter().enumerate() {
                flatten_node(child, start + offset, flat)?;
            }
        }
        RuntimeNode::Map { entries, .. } => {
            let child_count = entries.len().checked_mul(2).ok_or(())?;
            let start = reserve_values(flat, child_count)?;
            flat.values[index] = range_value(VALUE_MAP, start, entries.len())?;
            for (offset, (key, value)) in entries.iter().enumerate() {
                let pair = start + offset * 2;
                flatten_node(key, pair, flat)?;
                flatten_node(value, pair + 1, flat)?;
            }
        }
        RuntimeNode::Object { class_name, .. } => {
            let start = reserve_values(flat, 1)?;
            flat.values[index] = range_value(VALUE_OBJECT, start, 1)?;
            flat.values[start] = append_bytes(class_name, flat)?;
        }
        RuntimeNode::Resource => flat.values[index] = scalar_value(VALUE_RESOURCE, 0),
        RuntimeNode::Callable(descriptor) => {
            flat.values[index] = scalar_value(VALUE_CALLABLE, *descriptor)
        }
    }
    Ok(())
}

/// Appends one byte payload and returns its ABI range record.
fn append_bytes(bytes: &[u8], flat: &mut FlatRuntimeValue) -> Result<Value, ()> {
    let offset = u64::try_from(flat.bytes.len()).map_err(|_| ())?;
    let length = u64::try_from(bytes.len()).map_err(|_| ())?;
    flat.bytes.try_reserve_exact(bytes.len()).map_err(|_| ())?;
    flat.bytes.extend_from_slice(bytes);
    Ok(Value {
        tag: VALUE_BYTES,
        flags: 0,
        payload0: offset,
        payload1: length,
    })
}

/// Reserves null-initialized descendant records and returns their local start index.
fn reserve_values(flat: &mut FlatRuntimeValue, count: usize) -> Result<usize, ()> {
    let start = flat.values.len();
    flat.values.try_reserve_exact(count).map_err(|_| ())?;
    flat.values
        .resize_with(start.checked_add(count).ok_or(())?, null_value);
    Ok(start)
}

/// Constructs one scalar ABI record.
fn scalar_value(tag: u32, payload0: u64) -> Value {
    Value {
        tag,
        flags: 0,
        payload0,
        payload1: 0,
    }
}

/// Constructs one range-bearing ABI record after checked host-size conversion.
fn range_value(tag: u32, start: usize, count: usize) -> Result<Value, ()> {
    Ok(Value {
        tag,
        flags: 0,
        payload0: u64::try_from(start).map_err(|_| ())?,
        payload1: u64::try_from(count).map_err(|_| ())?,
    })
}

/// Constructs one canonical null ABI record.
fn null_value() -> Value {
    scalar_value(VALUE_NULL, 0)
}

/// Rebases and copies one local tree into the caller's root and trailing sections.
unsafe fn append_flat_value(
    flat: &FlatRuntimeValue,
    root: *mut Value,
    context: &mut RuntimeValueWriteContext,
) -> Result<(), ()> {
    let next = usize::try_from(std::ptr::read_unaligned(context.next_index)).map_err(|_| ())?;
    let value_capacity = usize::try_from(context.value_capacity).map_err(|_| ())?;
    let descendant_count = flat.values.len().checked_sub(1).ok_or(())?;
    let next_end = next.checked_add(descendant_count).ok_or(())?;
    if next_end > value_capacity || (descendant_count != 0 && context.values.is_null()) {
        return Err(());
    }
    let byte_cursor =
        usize::try_from(std::ptr::read_unaligned(context.byte_cursor)).map_err(|_| ())?;
    let byte_capacity = usize::try_from(context.byte_capacity).map_err(|_| ())?;
    let byte_end = byte_cursor.checked_add(flat.bytes.len()).ok_or(())?;
    if byte_end > byte_capacity || (!flat.bytes.is_empty() && context.bytes.is_null()) {
        return Err(());
    }

    let mut root_value = rebase_value(flat.values[0], next, byte_cursor)?;
    std::ptr::write(root, root_value);
    for (offset, value) in flat.values[1..].iter().copied().enumerate() {
        root_value = rebase_value(value, next, byte_cursor)?;
        std::ptr::write(context.values.add(next + offset), root_value);
    }
    if !flat.bytes.is_empty() {
        std::ptr::copy_nonoverlapping(
            flat.bytes.as_ptr(),
            context.bytes.add(byte_cursor),
            flat.bytes.len(),
        );
    }
    std::ptr::write(
        context.next_index,
        u64::try_from(next_end).map_err(|_| ())?,
    );
    std::ptr::write(
        context.byte_cursor,
        u64::try_from(byte_end).map_err(|_| ())?,
    );
    Ok(())
}

/// Rewrites local value and byte offsets into their caller-owned flat sections.
fn rebase_value(mut value: Value, descendant_start: usize, byte_start: usize) -> Result<Value, ()> {
    match value.tag {
        VALUE_BYTES => {
            value.payload0 = value
                .payload0
                .checked_add(u64::try_from(byte_start).map_err(|_| ())?)
                .ok_or(())?;
        }
        VALUE_ARRAY | VALUE_MAP | VALUE_OBJECT if value.payload1 != 0 => {
            let local = usize::try_from(value.payload0).map_err(|_| ())?;
            let descendant = local.checked_sub(1).ok_or(())?;
            value.payload0 = u64::try_from(
                descendant_start
                    .checked_add(descendant)
                    .ok_or(())?,
            )
            .map_err(|_| ())?;
        }
        _ => {}
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::{
        elephc_dom_measure_runtime_value, elephc_dom_write_runtime_value,
        flatten_node, normalize_xpath_callback_arrays, null_value, FlatRuntimeValue,
        RuntimeClassName, RuntimeNode, RuntimeValue, RuntimeValueMeasure,
        RuntimeValueWriteContext, RUNTIME_CALLABLE, RUNTIME_HASH, RUNTIME_MIXED,
        RUNTIME_STRING,
    };
    use crate::abi::{
        Value, VALUE_BYTES, VALUE_CALLABLE, VALUE_MAP, VALUE_NULL,
    };

    /// Verifies measurement and writing preserve an associative query map and root arity.
    #[test]
    fn runtime_hash_serialization_produces_one_flat_root_tree() {
        let key = b"query";
        let query = b"//item";
        let query_value = RuntimeValue {
            tag: RUNTIME_STRING,
            payload0: query.as_ptr() as u64,
            payload1: query.len() as u64,
        };
        let mut entry = [0_u64; 8];
        entry[0] = 1;
        entry[1] = key.as_ptr() as u64;
        entry[2] = key.len() as u64;
        entry[3] = (&query_value as *const RuntimeValue) as u64;
        entry[5] = RUNTIME_MIXED;
        entry[7] = u64::MAX;
        let mut hash = vec![0_u64; 13];
        hash[0] = 1;
        hash[1] = 1;
        hash[3] = 0;
        hash[4] = 0;
        hash[5..13].copy_from_slice(&entry);
        let root_value = RuntimeValue {
            tag: RUNTIME_HASH,
            payload0: hash.as_ptr() as u64,
            payload1: 0,
        };
        let mut measure = RuntimeValueMeasure::default();
        assert_eq!(
            unsafe {
                elephc_dom_measure_runtime_value(
                    (&root_value as *const RuntimeValue).cast::<u8>(),
                    std::ptr::null::<RuntimeClassName>(),
                    0,
                    2,
                    &mut measure,
                )
            },
            0
        );
        assert_eq!(measure.value_count, 3);
        assert_eq!(measure.byte_count, 11);

        let mut values = vec![
            Value {
                tag: VALUE_NULL,
                flags: 0,
                payload0: 0,
                payload1: 0,
            };
            4
        ];
        let mut bytes = vec![0_u8; 11];
        let mut next = 2_u64;
        let mut cursor = 0_u64;
        let mut context = RuntimeValueWriteContext {
            values: values.as_mut_ptr(),
            value_capacity: values.len() as u64,
            next_index: &mut next,
            bytes: bytes.as_mut_ptr(),
            byte_capacity: bytes.len() as u64,
            byte_cursor: &mut cursor,
            class_names: std::ptr::null(),
            class_name_count: 0,
            max_depth: 2,
            reserved: 0,
        };
        assert_eq!(
            unsafe {
                elephc_dom_write_runtime_value(
                    (&root_value as *const RuntimeValue).cast::<u8>(),
                    values.as_mut_ptr(),
                    &mut context,
                )
            },
            0
        );
        assert_eq!(values[0].tag, VALUE_MAP);
        assert_eq!(values[0].payload0, 2);
        assert_eq!(values[0].payload1, 1);
        assert_eq!(values[2].tag, VALUE_BYTES);
        assert_eq!(values[3].tag, VALUE_BYTES);
        assert_eq!(bytes, b"query//item");
        assert_eq!(next, 4);
        assert_eq!(cursor, 11);
    }

    /// Verifies flat callable serialization preserves its nonzero runtime descriptor.
    #[test]
    fn runtime_callable_serialization_preserves_descriptor() {
        let root_value = RuntimeValue {
            tag: RUNTIME_CALLABLE,
            payload0: 0xcafe,
            payload1: 0,
        };
        let mut measure = RuntimeValueMeasure::default();
        assert_eq!(
            unsafe {
                elephc_dom_measure_runtime_value(
                    (&root_value as *const RuntimeValue).cast::<u8>(),
                    std::ptr::null::<RuntimeClassName>(),
                    0,
                    2,
                    &mut measure,
                )
            },
            0,
        );
        assert_eq!(measure.value_count, 1);
        assert_eq!(measure.byte_count, 0);

        let mut value = Value {
            tag: VALUE_NULL,
            flags: 0,
            payload0: 0,
            payload1: 0,
        };
        let mut next = 1_u64;
        let mut cursor = 0_u64;
        let mut context = RuntimeValueWriteContext {
            values: &mut value,
            value_capacity: 1,
            next_index: &mut next,
            bytes: std::ptr::null_mut(),
            byte_capacity: 0,
            byte_cursor: &mut cursor,
            class_names: std::ptr::null(),
            class_name_count: 0,
            max_depth: 2,
            reserved: 0,
        };
        assert_eq!(
            unsafe {
                elephc_dom_write_runtime_value(
                    (&root_value as *const RuntimeValue).cast::<u8>(),
                    &mut value,
                    &mut context,
                )
            },
            0,
        );
        assert_eq!(value.tag, VALUE_CALLABLE);
        assert_eq!(value.payload0, 0xcafe);
        assert_eq!(value.payload1, 0);
    }

    /// Verifies nested callable-array preparation collapses one alias value to a descriptor.
    #[test]
    fn xpath_callback_preparation_resolves_nested_callable_array_once() {
        let mut node = RuntimeNode::Map {
            entries: vec![(
                RuntimeNode::Bytes(b"alias".to_vec()),
                RuntimeNode::Array {
                    values: vec![
                        RuntimeNode::Bytes(b"Handler".to_vec()),
                        RuntimeNode::Bytes(b"render".to_vec()),
                    ],
                    runtime_pointer: 1,
                },
            )],
            runtime_pointer: 2,
        };
        let mut descriptors = Vec::new();
        normalize_xpath_callback_arrays(&mut node, true, &mut descriptors)
            .expect("nested callable array should normalize");
        assert_eq!(descriptors, vec![0xcafe]);

        let mut flat = FlatRuntimeValue {
            values: vec![null_value()],
            bytes: Vec::new(),
        };
        flatten_node(&node, 0, &mut flat).expect("prepared callback tree should flatten");
        assert_eq!(flat.values.len(), 3);
        assert_eq!(flat.values[0].tag, VALUE_MAP);
        assert_eq!(flat.values[2].tag, VALUE_CALLABLE);
        assert_eq!(flat.values[2].payload0, 0xcafe);
    }
}
