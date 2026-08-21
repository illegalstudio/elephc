//! Purpose:
//! Validates and decodes flat DOM ABI requests without borrowing unchecked foreign memory.
//! Rejects overflow, truncated values/bytes, invalid tags, and out-of-range nested slices.
//!
//! Called from:
//! - `crate::exports::elephc_dom_call()` before any DOM mutation.
//!
//! Key details:
//! - Validation completes before opcode dispatch, so malformed messages have no native side effects.
//! - Embedded NUL is preserved because byte strings use offsets and lengths, never C strings.

use crate::abi::{
    RequestHeader, Value, REQUEST_FLAG_ARGUMENT_COUNT, VALUE_ARRAY, VALUE_BOOL,
    VALUE_BRIDGE_HANDLE, VALUE_BYTES, VALUE_CALLABLE, VALUE_FLOAT, VALUE_HOST_HANDLE,
    VALUE_INT, VALUE_MAP, VALUE_NULL, VALUE_OBJECT, VALUE_RESOURCE, VALUE_SIMPLEXML_APPEND,
};

/// A fully bounds-validated request copied from foreign memory.
pub(crate) struct Request {
    pub header: RequestHeader,
    pub values: Vec<Value>,
    pub bytes: Vec<u8>,
    flat_values: Vec<Value>,
}

/// Distinguishes unreadable request structure from an otherwise readable incompatible ABI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DecodeError {
    /// The request layout, ranges, tags, or nested value tree cannot be decoded safely.
    MalformedRequest,
    /// The readable header names a bridge ABI version this binary does not support.
    IncompatibleAbiVersion,
}

impl DecodeError {
    /// Returns the stable C ABI status associated with this decode failure.
    pub(crate) const fn status(self) -> u32 {
        match self {
            Self::MalformedRequest => crate::abi::STATUS_MALFORMED_REQUEST,
            Self::IncompatibleAbiVersion => crate::abi::STATUS_ABI_ERROR,
        }
    }
}

impl From<()> for DecodeError {
    /// Normalizes internal validation failures to the externally visible malformed status.
    fn from(_: ()) -> Self {
        Self::MalformedRequest
    }
}

impl Request {
    /// Builds one trusted receiver-only request for reusing a zero-argument bridge projection.
    pub(crate) fn receiver_only(&self) -> Self {
        let mut header = self.header;
        header.flags = REQUEST_FLAG_ARGUMENT_COUNT;
        header.value_count = 0;
        header.byte_count = 0;
        Self {
            header,
            values: Vec::new(),
            bytes: Vec::new(),
            flat_values: Vec::new(),
        }
    }

    /// Returns one positional value after whole-message validation.
    pub(crate) fn value(&self, index: usize) -> Result<&Value, ()> {
        self.values.get(index).ok_or(())
    }

    /// Returns one length-delimited byte-string argument.
    pub(crate) fn byte_string(&self, index: usize) -> Result<&[u8], ()> {
        let value = self.value(index)?;
        if value.tag != VALUE_BYTES {
            return Err(());
        }
        let start = usize::try_from(value.payload0).map_err(|_| ())?;
        let length = usize::try_from(value.payload1).map_err(|_| ())?;
        let end = start.checked_add(length).ok_or(())?;
        self.bytes.get(start..end).ok_or(())
    }

    /// Returns one signed integer argument from its ABI bit pattern.
    pub(crate) fn integer(&self, index: usize) -> Result<i64, ()> {
        let value = self.value(index)?;
        (value.tag == VALUE_INT)
            .then_some(value.payload0 as i64)
            .ok_or(())
    }

    /// Returns one boolean argument encoded as exactly zero or one.
    pub(crate) fn boolean(&self, index: usize) -> Result<bool, ()> {
        let value = self.value(index)?;
        if value.tag != VALUE_BOOL || value.payload0 > 1 {
            return Err(());
        }
        Ok(value.payload0 == 1)
    }

    /// Returns one nullable boolean argument while preserving omitted/null semantics.
    pub(crate) fn optional_boolean(&self, index: usize) -> Result<Option<bool>, ()> {
        let value = self.value(index)?;
        if value.tag == VALUE_NULL {
            Ok(None)
        } else {
            self.boolean(index).map(Some)
        }
    }

    /// Returns one generation-checked native bridge handle argument.
    pub(crate) fn bridge_handle(&self, index: usize) -> Result<u64, ()> {
        let value = self.value(index)?;
        (value.tag == VALUE_BRIDGE_HANDLE && value.payload0 != 0)
            .then_some(value.payload0)
            .ok_or(())
    }

    /// Returns one nullable generation-checked native bridge handle argument.
    pub(crate) fn optional_bridge_handle(&self, index: usize) -> Result<Option<u64>, ()> {
        let value = self.value(index)?;
        if value.tag == VALUE_NULL {
            Ok(None)
        } else {
            self.bridge_handle(index).map(Some)
        }
    }

    /// Returns one non-null borrowed PHP callable descriptor argument.
    pub(crate) fn callable_descriptor(&self, index: usize) -> Result<u64, ()> {
        let value = self.value(index)?;
        (value.tag == VALUE_CALLABLE
            && value.flags == 0
            && value.payload0 != 0
            && value.payload1 == 0)
            .then_some(value.payload0)
            .ok_or(())
    }

    /// Returns one nullable byte-string argument.
    pub(crate) fn optional_byte_string(&self, index: usize) -> Result<Option<&[u8]>, ()> {
        let value = self.value(index)?;
        if value.tag == VALUE_NULL {
            Ok(None)
        } else {
            self.byte_string(index).map(Some)
        }
    }

    /// Returns the descendants of one indexed-array argument from the validated flat section.
    pub(crate) fn array_values(&self, index: usize) -> Result<&[Value], ()> {
        let value = self.value(index)?;
        if value.tag != VALUE_ARRAY {
            return Err(());
        }
        self.flat_range(value.payload0, value.payload1)
    }

    /// Returns alternating key/value descendants for one validated map argument.
    pub(crate) fn map_values(&self, index: usize) -> Result<&[Value], ()> {
        let value = self.value(index)?;
        if value.tag != VALUE_MAP {
            return Err(());
        }
        let count = value.payload1.checked_mul(2).ok_or(())?;
        self.flat_range(value.payload0, count)
    }

    /// Returns one range carried by an arbitrary validated array, map, or object record.
    pub(crate) fn nested_values(&self, value: &Value) -> Result<&[Value], ()> {
        let count = match value.tag {
            VALUE_ARRAY | VALUE_OBJECT => value.payload1,
            VALUE_MAP => value.payload1.checked_mul(2).ok_or(())?,
            _ => return Err(()),
        };
        self.flat_range(value.payload0, count)
    }

    /// Returns one byte string referenced by a root or nested value record.
    pub(crate) fn bytes_for_value(&self, value: &Value) -> Result<&[u8], ()> {
        if value.tag != VALUE_BYTES {
            return Err(());
        }
        let start = usize::try_from(value.payload0).map_err(|_| ())?;
        let length = usize::try_from(value.payload1).map_err(|_| ())?;
        let end = start.checked_add(length).ok_or(())?;
        self.bytes.get(start..end).ok_or(())
    }

    /// Returns one already validated range from the complete flat value section.
    fn flat_range(&self, offset: u64, count: u64) -> Result<&[Value], ()> {
        let start = usize::try_from(offset).map_err(|_| ())?;
        let count = usize::try_from(count).map_err(|_| ())?;
        let end = start.checked_add(count).ok_or(())?;
        self.flat_values.get(start..end).ok_or(())
    }
}

/// Copies and validates one request message.
pub(crate) fn decode(
    pointer: *const u8,
    length: u64,
) -> Result<Request, DecodeError> {
    let length = usize::try_from(length).map_err(|_| ())?;
    if pointer.is_null() || length < std::mem::size_of::<RequestHeader>() {
        return Err(());
    }
    let input = unsafe { std::slice::from_raw_parts(pointer, length) };
    let header = read_header(input)?;
    if header.abi_version != crate::abi::ABI_VERSION {
        return Err(DecodeError::IncompatibleAbiVersion);
    }
    let header_size = usize::try_from(header.header_size).map_err(|_| ())?;
    if header_size < std::mem::size_of::<RequestHeader>() || header_size > input.len() {
        return Err(());
    }
    let value_count = usize::try_from(header.value_count).map_err(|_| ())?;
    let values_size = value_count
        .checked_mul(std::mem::size_of::<Value>())
        .ok_or(())?;
    let values_end = header_size.checked_add(values_size).ok_or(())?;
    let byte_count = usize::try_from(header.byte_count).map_err(|_| ())?;
    let bytes_end = values_end.checked_add(byte_count).ok_or(())?;
    if bytes_end != input.len() {
        return Err(());
    }

    let flat_values = read_values(&input[header_size..values_end], value_count)?;
    for value in &flat_values {
        validate_value(value, value_count, byte_count)?;
    }
    let argument_count = request_argument_count(header.flags, value_count)?;
    validate_value_tree(&flat_values, argument_count)?;
    Ok(Request {
        header,
        values: flat_values[..argument_count].to_vec(),
        bytes: input[values_end..bytes_end].to_vec(),
        flat_values,
    })
}

/// Decodes the optional root-argument count while preserving legacy flat requests.
fn request_argument_count(flags: u32, value_count: usize) -> Result<usize, ()> {
    if flags & REQUEST_FLAG_ARGUMENT_COUNT == 0 {
        return Ok(value_count);
    }
    let argument_count =
        usize::try_from(flags & !REQUEST_FLAG_ARGUMENT_COUNT).map_err(|_| ())?;
    (argument_count <= value_count)
        .then_some(argument_count)
        .ok_or(())
}

/// Reads a potentially unaligned request header from the input prefix.
fn read_header(input: &[u8]) -> Result<RequestHeader, ()> {
    if input.len() < std::mem::size_of::<RequestHeader>() {
        return Err(());
    }
    Ok(unsafe { std::ptr::read_unaligned(input.as_ptr().cast::<RequestHeader>()) })
}

/// Copies potentially unaligned flat values from the request.
fn read_values(input: &[u8], count: usize) -> Result<Vec<Value>, ()> {
    let expected = count
        .checked_mul(std::mem::size_of::<Value>())
        .ok_or(())?;
    if input.len() != expected {
        return Err(());
    }
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(|_| ())?;
    for index in 0..count {
        let offset = index.checked_mul(std::mem::size_of::<Value>()).ok_or(())?;
        values.push(unsafe {
            std::ptr::read_unaligned(input.as_ptr().add(offset).cast::<Value>())
        });
    }
    Ok(values)
}

/// Validates one value tag and every offset/count range it carries.
fn validate_value(value: &Value, value_count: usize, byte_count: usize) -> Result<(), ()> {
    match value.tag {
        VALUE_NULL
        | VALUE_BOOL
        | VALUE_INT
        | VALUE_FLOAT
        | VALUE_HOST_HANDLE
        | VALUE_BRIDGE_HANDLE
        | VALUE_CALLABLE
        | VALUE_RESOURCE
        | VALUE_SIMPLEXML_APPEND => Ok(()),
        VALUE_BYTES => validate_range(value.payload0, value.payload1, byte_count),
        VALUE_ARRAY => validate_range(value.payload0, value.payload1, value_count),
        VALUE_MAP => {
            let entries = value.payload1.checked_mul(2).ok_or(())?;
            validate_range(value.payload0, entries, value_count)
        }
        VALUE_OBJECT => validate_range(value.payload0, value.payload1, value_count),
        _ => Err(()),
    }
}

/// Proves that every nested value belongs to exactly one acyclic root argument tree.
fn validate_value_tree(values: &[Value], argument_count: usize) -> Result<(), ()> {
    let mut state = vec![0_u8; values.len()];
    for index in 0..argument_count {
        visit_value(values, argument_count, index, true, &mut state)?;
    }
    state.iter().all(|entry| *entry == 2).then_some(()).ok_or(())
}

/// Visits one root or descendant while rejecting cycles, aliases, and root overlap.
fn visit_value(
    values: &[Value],
    argument_count: usize,
    index: usize,
    root: bool,
    state: &mut [u8],
) -> Result<(), ()> {
    if index >= values.len() || (!root && index < argument_count) || state[index] != 0 {
        return Err(());
    }
    state[index] = 1;
    let value = values[index];
    let (start, count) = match value.tag {
        VALUE_ARRAY | VALUE_OBJECT => (value.payload0, value.payload1),
        VALUE_MAP => (value.payload0, value.payload1.checked_mul(2).ok_or(())?),
        _ => {
            state[index] = 2;
            return Ok(());
        }
    };
    let start = usize::try_from(start).map_err(|_| ())?;
    let count = usize::try_from(count).map_err(|_| ())?;
    let end = start.checked_add(count).ok_or(())?;
    if end > values.len() {
        return Err(());
    }
    for child in start..end {
        visit_value(values, argument_count, child, false, state)?;
    }
    state[index] = 2;
    Ok(())
}

/// Checks an offset/count pair against one flat message section.
fn validate_range(offset: u64, count: u64, section_len: usize) -> Result<(), ()> {
    let offset = usize::try_from(offset).map_err(|_| ())?;
    let count = usize::try_from(count).map_err(|_| ())?;
    let end = offset.checked_add(count).ok_or(())?;
    (end <= section_len).then_some(()).ok_or(())
}
