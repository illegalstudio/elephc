//! Purpose:
//! Decodes bounded binary member-attribute records emitted by generated native
//! code into eval attribute metadata.
//!
//! Called from:
//! - `super::property_registration` while registering class-member attributes.
//!
//! Key details:
//! - Every read is bounds checked and malformed or trailing data is rejected.

use super::*;

/// Decoded native member-attribute metadata record.
pub(super) struct NativeMemberAttributeRecord {
    pub(super) owner_kind: u8,
    pub(super) member_key: String,
    pub(super) attribute: EvalAttribute,
}

/// Decodes one generated native member-attribute metadata record.
pub(super) fn native_member_attribute_record_from_abi(
    record_ptr: *const u8,
    record_len: u64,
) -> Option<NativeMemberAttributeRecord> {
    if record_ptr.is_null() || record_len == 0 {
        return None;
    }
    let record_len = usize::try_from(record_len).ok()?;
    let bytes = unsafe { std::slice::from_raw_parts(record_ptr, record_len) };
    let mut offset = 0usize;
    let owner_kind = native_attribute_take_u8(bytes, &mut offset)?;
    let member_key = native_attribute_take_string(bytes, &mut offset)?;
    let attribute_name = native_attribute_take_string(bytes, &mut offset)?;
    let args = native_attribute_take_args(bytes, &mut offset)?;
    (offset == bytes.len()).then_some(NativeMemberAttributeRecord {
        owner_kind,
        member_key,
        attribute: EvalAttribute::new(attribute_name, args),
    })
}

/// Decodes the optional argument vector from a native attribute record.
fn native_attribute_take_args(
    bytes: &[u8],
    offset: &mut usize,
) -> Option<Option<Vec<EvalAttributeArg>>> {
    match native_attribute_take_u8(bytes, offset)? {
        NATIVE_ATTRIBUTE_ARGS_UNSUPPORTED => Some(None),
        NATIVE_ATTRIBUTE_ARGS_SUPPORTED => {
            let count = usize::try_from(native_attribute_take_u32(bytes, offset)?).ok()?;
            let mut args = Vec::with_capacity(count);
            for _ in 0..count {
                args.push(native_attribute_take_arg(bytes, offset)?);
            }
            Some(Some(args))
        }
        _ => None,
    }
}

/// Decodes one literal argument from a native attribute record.
fn native_attribute_take_arg(bytes: &[u8], offset: &mut usize) -> Option<EvalAttributeArg> {
    match native_attribute_take_u8(bytes, offset)? {
        NATIVE_ATTRIBUTE_ARG_NULL => Some(EvalAttributeArg::Null),
        NATIVE_ATTRIBUTE_ARG_BOOL => Some(EvalAttributeArg::Bool(
            native_attribute_take_u8(bytes, offset)? != 0,
        )),
        NATIVE_ATTRIBUTE_ARG_INT => Some(EvalAttributeArg::Int(native_attribute_take_i64(
            bytes, offset,
        )?)),
        NATIVE_ATTRIBUTE_ARG_FLOAT => Some(EvalAttributeArg::Float(
            native_attribute_take_u64(bytes, offset)?,
        )),
        NATIVE_ATTRIBUTE_ARG_STRING => {
            native_attribute_take_string(bytes, offset).map(EvalAttributeArg::String)
        }
        NATIVE_ATTRIBUTE_ARG_NAMED => {
            let name = native_attribute_take_string(bytes, offset)?;
            let value = native_attribute_take_arg(bytes, offset)?;
            Some(EvalAttributeArg::Named {
                name,
                value: Box::new(value),
            })
        }
        NATIVE_ATTRIBUTE_ARG_ARRAY => {
            let len = usize::try_from(native_attribute_take_u32(bytes, offset)?).ok()?;
            let mut elements = Vec::with_capacity(len);
            for _ in 0..len {
                elements.push(native_attribute_take_arg(bytes, offset)?);
            }
            Some(EvalAttributeArg::Array(elements))
        }
        _ => None,
    }
}

/// Reads one UTF-8 string with a little-endian u32 byte length prefix.
pub(super) fn native_attribute_take_string(bytes: &[u8], offset: &mut usize) -> Option<String> {
    let len = usize::try_from(native_attribute_take_u32(bytes, offset)?).ok()?;
    let chunk = native_attribute_take_bytes(bytes, offset, len)?;
    std::str::from_utf8(chunk).ok().map(str::to_string)
}

/// Reads one little-endian i64 from a native attribute record.
pub(super) fn native_attribute_take_i64(bytes: &[u8], offset: &mut usize) -> Option<i64> {
    let chunk = native_attribute_take_bytes(bytes, offset, std::mem::size_of::<i64>())?;
    Some(i64::from_le_bytes(chunk.try_into().ok()?))
}

/// Reads one little-endian u32 from a native attribute record.
pub(super) fn native_attribute_take_u32(bytes: &[u8], offset: &mut usize) -> Option<u32> {
    let chunk = native_attribute_take_bytes(bytes, offset, std::mem::size_of::<u32>())?;
    Some(u32::from_le_bytes(chunk.try_into().ok()?))
}

/// Reads one byte from a native attribute record.
pub(super) fn native_attribute_take_u8(bytes: &[u8], offset: &mut usize) -> Option<u8> {
    native_attribute_take_bytes(bytes, offset, 1).map(|chunk| chunk[0])
}

/// Reads one bounded byte slice and advances the decode offset.
pub(super) fn native_attribute_take_bytes<'a>(
    bytes: &'a [u8],
    offset: &mut usize,
    len: usize,
) -> Option<&'a [u8]> {
    let end = offset.checked_add(len)?;
    let chunk = bytes.get(*offset..end)?;
    *offset = end;
    Some(chunk)
}
