//! Purpose:
//! Defines binary exception-chain transport shared by mbstring and its native hosts.
//!
//! Called from:
//! - Shared operation outcomes and the native exception materialization adapter.
//!
//! Key details:
//! - Records are oldest first, so each new Throwable consumes the preceding chain.
//! - Messages preserve arbitrary bytes; decoding rejects an invalid entire chain.

use super::{coercion::PREPARED_ARGUMENT_COUNT_ERROR, RESULT_ERROR, RESULT_TYPE_ERROR,
    RESULT_VALUE_ERROR, RESULT_EXCEPTION_CHAIN};

/// A borrowed PHP exception class and its complete binary message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Exception<'a> { pub kind: u64, pub message: &'a [u8] }

/// Borrowed fields consumed by native emitters before the enclosing result is released.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MbExceptionV1 { pub kind: u64, pub bytes: *const u8, pub len: u64 }

/// Recognizes individual PHP error classes accepted by the shared result protocol.
pub const fn is_error(kind: u64) -> bool {
    matches!(kind, RESULT_VALUE_ERROR | RESULT_TYPE_ERROR | RESULT_ERROR | PREPARED_ARGUMENT_COUNT_ERROR)
}

/// Recognizes individual errors and ordered chains without confusing successful scalar values.
pub const fn is_exception(kind: u64) -> bool { is_error(kind) || kind == RESULT_EXCEPTION_CHAIN }

/// Encodes valid, nonempty chains as class/length/message records without nested ownership.
pub fn encode(errors: &[Exception<'_>]) -> Option<Vec<u8>> {
    if errors.is_empty() || errors.iter().any(|error| !is_error(error.kind)) { return None; }
    let mut bytes = Vec::new();
    for error in errors {
        bytes.extend_from_slice(&error.kind.to_le_bytes());
        bytes.extend_from_slice(&(error.message.len() as u64).to_le_bytes());
        bytes.extend_from_slice(error.message);
    }
    Some(bytes)
}

/// Validates every record and exact buffer consumption before exposing any message to a host.
pub fn decode(mut bytes: &[u8], count: usize) -> Option<Vec<Exception<'_>>> {
    if count == 0 || count > bytes.len() / 16 { return None; }
    let mut errors = Vec::with_capacity(count);
    for _ in 0..count {
        let kind = u64::from_le_bytes(bytes.get(..8)?.try_into().ok()?);
        if !is_error(kind) { return None; }
        let len = usize::try_from(u64::from_le_bytes(bytes.get(8..16)?.try_into().ok()?)).ok()?;
        bytes = bytes.get(16..)?;
        errors.push(Exception { kind, message: bytes.get(..len)? });
        bytes = bytes.get(len..)?;
    }
    bytes.is_empty().then_some(errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Preserves oldest-first order, embedded NULs, empty messages, and all native error classes.
    #[test]
    fn mbstring_exception_chain_roundtrip() {
        let errors = [Exception { kind: RESULT_VALUE_ERROR, message: b"option\0\xff" },
            Exception { kind: RESULT_ERROR, message: b"" },
            Exception { kind: RESULT_TYPE_ERROR, message: b"type" },
            Exception { kind: PREPARED_ARGUMENT_COUNT_ERROR, message: b"arity" }];
        let bytes = encode(&errors).unwrap();
        assert_eq!(decode(&bytes, errors.len()), Some(errors.to_vec()));
        assert_eq!(std::mem::size_of::<MbExceptionV1>(), 24);
        assert_eq!(std::mem::offset_of!(MbExceptionV1, kind), 0);
        assert_eq!(std::mem::offset_of!(MbExceptionV1, bytes), 8);
        assert_eq!(std::mem::offset_of!(MbExceptionV1, len), 16);
    }

    /// Rejects truncated, overlong, unsupported, empty, and inconsistent chains before publication.
    #[test]
    fn mbstring_exception_chain_rejects_malformed_records() {
        let bytes = encode(&[Exception { kind: RESULT_ERROR, message: b"a" },
            Exception { kind: RESULT_VALUE_ERROR, message: b"b" }]).unwrap();
        for end in 0..bytes.len() { assert!(decode(&bytes[..end], 2).is_none()); }
        for count in [0, 1, 3, usize::MAX] { assert!(decode(&bytes, count).is_none()); }
        let mut invalid = bytes.clone();
        invalid[17..25].copy_from_slice(&0_u64.to_le_bytes());
        assert!(decode(&invalid, 2).is_none());
        invalid = bytes.clone();
        invalid[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(decode(&invalid, 2).is_none());
        invalid = bytes;
        invalid.push(0);
        assert!(decode(&invalid, 2).is_none());
        assert!(encode(&[]).is_none());
        assert!(encode(&[Exception { kind: RESULT_EXCEPTION_CHAIN, message: b"" }]).is_none());
    }
}
