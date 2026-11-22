//! Purpose:
//! Copies a native byte span with its borrowed or transferred ownership contract.
//!
//! Called from:
//! - Magician's output-buffer queries and native string bridge.
//!
//! Key details:
//! - An empty span may use a null pointer.
//! - A transferred pair is released by its own producer after the copy.

/// Copies a borrowed native span after checking the ABI length and null sentinel.
///
/// # Safety
/// A non-null pointer with a valid nonzero length must be readable for that many bytes.
#[allow(unsafe_code)]
pub unsafe fn copy_borrowed(pointer: *const u8, length: impl TryInto<usize>) -> Option<Vec<u8>> {
    let length = length.try_into().ok()?;
    if length == 0 {
        return Some(Vec::new());
    }
    if pointer.is_null() || length > isize::MAX as usize {
        return None;
    }
    Some(unsafe { std::slice::from_raw_parts(pointer, length) }.to_vec())
}

/// Copies a transferred native span and returns its ownership to the producing allocator.
///
/// # Safety
/// The pointer and length must satisfy the borrowed contract when valid. The release callback
/// must accept this pointer even for empty spans and rejected lengths.
#[allow(unsafe_code)]
pub unsafe fn copy_owned(
    pointer: *const u8,
    length: impl TryInto<usize>,
    release: impl FnOnce(*const u8),
) -> Option<Vec<u8>> {
    let bytes = unsafe { copy_borrowed(pointer, length) };
    release(pointer);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The original allocator receives a transferred pointer even when its length is rejected.
    #[test]
    #[allow(unsafe_code)]
    fn owned_bytes_release_survives_rejected_lengths() {
        let bytes = [1u8, 2, 3];
        let pointer = bytes.as_ptr();
        let mut released = None;
        let copied = unsafe { copy_owned(pointer, -1i64, |pointer| released = Some(pointer)) };
        assert!(copied.is_none());
        assert_eq!(released, Some(pointer));
    }
}
