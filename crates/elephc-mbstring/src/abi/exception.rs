//! Purpose:
//! Exposes validated borrowed exception records to native mbstring result adapters.
//!
//! Called from:
//! - The native error materializer and protected invocation coordinator.
//!
//! Key details:
//! - Record reads never consume the result or allocate native PHP storage.
//! - Validation covers the complete chain before returning its first record.

use super::*;
use elephc_builtin_contract::mbstring_abi::exception::{self, Exception, MbExceptionV1};

/// Returns one oldest-first record (1), the exact end (0), or invalid framing (-1).
///
/// # Safety
/// `out` is writable aligned record storage. `result` is a readable aligned wire result
/// whose nonempty payload remains readable through the call. Returned bytes borrow that
/// payload and must not be retained after it is released or mutated.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_exception_at_v1(
    result: *const MbResultV1, index: u64, out: *mut MbExceptionV1,
) -> i32 {
    if out.is_null() { return -1; }
    unsafe { out.write(MbExceptionV1 { kind: 0, bytes: std::ptr::null(), len: 0 }); }
    catch_unwind(AssertUnwindSafe(|| {
        let Some(result) = (unsafe { result.as_ref() }) else { return -1; };
        if result.len > isize::MAX as u64 || (result.len != 0 && result.bytes.is_null()) { return -1; }
        let bytes = if result.len == 0 { &[] } else {
            unsafe { std::slice::from_raw_parts(result.bytes, result.len as usize) }
        };
        let Ok(index) = usize::try_from(index) else { return -1; };
        let errors = if exception::is_error(result.kind) {
            vec![Exception { kind: result.kind, message: bytes }]
        } else if result.kind == RESULT_EXCEPTION_CHAIN {
            let Ok(count) = usize::try_from(result.value) else { return -1; };
            let Some(errors) = exception::decode(bytes, count) else { return -1; };
            errors
        } else { return -1; };
        if index == errors.len() { return 0; }
        let Some(error) = errors.get(index) else { return -1; };
        unsafe { out.write(MbExceptionV1 { kind: error.kind, bytes: error.message.as_ptr(), len: error.message.len() as u64 }); }
        1
    })).unwrap_or(-1)
}

impl Outcome {
    /// Retains every engine exception, preserving the original single-error wire representation.
    pub(super) fn error_chain(errors: Vec<MbError>) -> Option<Self> {
        let errors: Vec<_> = errors.into_iter().map(Self::error).collect();
        if errors.len() == 1 { return errors.into_iter().next(); }
        let records: Vec<_> = errors.iter().map(|error| Exception { kind: error.kind, message: &error.bytes }).collect();
        Some(Self { value: errors.len() as i64, bytes: exception::encode(&records)?, ..Self::empty(RESULT_EXCEPTION_CHAIN) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Preserves every engine error and exposes borrowed records until one idempotent wire release.
    #[test]
    fn mbstring_exception_chain_wire_ownership() {
        assert!(Outcome::error_chain(Vec::new()).is_none());
        let mut single = Outcome::error_chain(vec![MbError::Value("single".into())]).unwrap().into_wire();
        assert_eq!(single.kind, RESULT_VALUE_ERROR);
        unsafe { elephc_mbstring_release_v1(&mut single); }
        let mut wire = Outcome::error_chain(vec![MbError::ValueBytes(b"option\0\xff".to_vec()),
            MbError::Runtime("No pattern was provided".into())]).unwrap().into_wire();
        assert_eq!((wire.kind, wire.value), (RESULT_EXCEPTION_CHAIN, 2));
        let mut record = MbExceptionV1 { kind: 0, bytes: std::ptr::null(), len: 0 };
        for (index, kind, message) in [(0, RESULT_VALUE_ERROR, b"option\0\xff".as_slice()),
            (1, RESULT_ERROR, b"No pattern was provided".as_slice())] {
            assert_eq!(unsafe { elephc_mbstring_exception_at_v1(&wire, index, &mut record) }, 1);
            assert_eq!(record.kind, kind);
            assert_eq!(unsafe { std::slice::from_raw_parts(record.bytes, record.len as usize) }, message);
        }
        assert_eq!(unsafe { elephc_mbstring_exception_at_v1(&wire, 2, &mut record) }, 0);
        assert!(record.bytes.is_null() && record.kind == 0 && record.len == 0);
        assert_eq!(unsafe { elephc_mbstring_exception_at_v1(&wire, 3, &mut record) }, -1);
        unsafe { elephc_mbstring_release_v1(&mut wire); elephc_mbstring_release_v1(&mut wire); }
        assert!(wire.bytes.is_null());
    }

    /// Rejects malformed later records before exposing the first message or retaining any ownership.
    #[test]
    fn mbstring_exception_chain_validates_complete_input() {
        let mut wire = Outcome::error_chain(vec![MbError::Value("first".into()), MbError::Runtime("last".into())])
            .unwrap().into_wire();
        let length = wire.len;
        let mut record = MbExceptionV1 { kind: 9, bytes: std::ptr::dangling(), len: 1 };
        wire.len -= 1;
        assert_eq!(unsafe { elephc_mbstring_exception_at_v1(&wire, 0, &mut record) }, -1);
        assert!(record.bytes.is_null() && record.kind == 0 && record.len == 0);
        wire.len = length;
        wire.value = -1;
        assert_eq!(unsafe { elephc_mbstring_exception_at_v1(&wire, 0, &mut record) }, -1);
        assert_eq!(unsafe { elephc_mbstring_exception_at_v1(std::ptr::null(), 0, &mut record) }, -1);
        assert_eq!(unsafe { elephc_mbstring_exception_at_v1(&wire, 0, std::ptr::null_mut()) }, -1);
        unsafe { elephc_mbstring_release_v1(&mut wire); }
    }
}
