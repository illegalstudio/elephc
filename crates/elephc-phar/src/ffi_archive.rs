//! Purpose:
//! C ABI wrappers for PHAR archive, metadata, compression, and signature operations.
//!
//! Called from:
//! - Generated native programs through exported `elephc_phar_*` symbols.
//!
//! Key details:
//! - All pointer inputs are contained behind panic and length validation boundaries.

use super::*;

/// C ABI wrapper around [`extract_url_bytes`].
///
/// Returns a pointer to a stable process-global buffer and writes the byte
/// length into `out_len`. Returns null and writes zero on any failure.
///
/// # Safety
/// `url_ptr` must be valid for `url_len` bytes unless `url_len` is zero.
/// `out_len` may be null; when non-null it must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_extract_url(
    url_ptr: *const u8,
    url_len: usize,
    out_len: *mut usize,
) -> *const u8 {
    match std::panic::catch_unwind(|| extract_url_bytes(slice(url_ptr, url_len))) {
        Ok(Some(bytes)) => publish_result(bytes, out_len),
        _ => {
            write_len(out_len, 0);
            std::ptr::null()
        }
    }
}

/// C ABI wrapper around [`zip_stat_entries_bytes`].
///
/// Returns a pointer to the serialized per-entry stat buffer and writes its byte
/// length into `out_len`. Returns null and writes zero when the file cannot be
/// read or is no ZIP archive.
///
/// # Safety
/// `path_ptr` must be valid for `path_len` bytes unless `path_len` is zero.
/// `out_len` may be null; when non-null it must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_zip_stat_entries(
    path_ptr: *const u8,
    path_len: usize,
    out_len: *mut usize,
) -> *const u8 {
    match std::panic::catch_unwind(|| zip_stat_entries_bytes(slice(path_ptr, path_len))) {
        Ok(Some(bytes)) => publish_result(bytes, out_len),
        _ => {
            write_len(out_len, 0);
            std::ptr::null()
        }
    }
}

/// C ABI wrapper around [`entry_names_bytes`].
///
/// Returns a pointer to the serialized entry-name buffer and writes its byte
/// length into `out_len`. Returns null and writes zero when the archive cannot
/// be read or parsed.
///
/// # Safety
/// `path_ptr` must be valid for `path_len` bytes unless `path_len` is zero.
/// `out_len` may be null; when non-null it must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_list_entries(
    path_ptr: *const u8,
    path_len: usize,
    out_len: *mut usize,
) -> *const u8 {
    match std::panic::catch_unwind(|| entry_names_bytes(slice(path_ptr, path_len))) {
        Ok(Some(bytes)) => publish_result(bytes, out_len),
        _ => {
            write_len(out_len, 0);
            std::ptr::null()
        }
    }
}

/// C ABI wrapper around [`put_entry_bytes`].
///
/// Returns the written payload length on success and `usize::MAX` on failure.
/// The archive is always a native PHAR after a successful write.
///
/// # Safety
/// Each pointer must be valid for its paired byte length unless that length is
/// zero. `entry_ptr` must not describe an empty entry name.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_put_entry(
    archive_ptr: *const u8,
    archive_len: usize,
    entry_ptr: *const u8,
    entry_len: usize,
    data_ptr: *const u8,
    data_len: usize,
) -> usize {
    let result = std::panic::catch_unwind(|| {
        put_entry_bytes(
            slice(archive_ptr, archive_len),
            slice(entry_ptr, entry_len),
            slice(data_ptr, data_len),
        )
    });
    match result {
        Ok(Some(len)) => len,
        _ => usize::MAX,
    }
}

/// C ABI wrapper around [`put_url_bytes`].
///
/// Returns the written payload length on success and `usize::MAX` on failure.
///
/// # Safety
/// Each pointer must be valid for its paired byte length unless that length is
/// zero. `url_ptr` must point to a complete `phar://archive/entry` URL.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_put_url(
    url_ptr: *const u8,
    url_len: usize,
    data_ptr: *const u8,
    data_len: usize,
) -> usize {
    let result = std::panic::catch_unwind(|| {
        put_url_bytes(slice(url_ptr, url_len), slice(data_ptr, data_len))
    });
    match result {
        Ok(Some(len)) => len,
        _ => usize::MAX,
    }
}

/// C ABI wrapper around [`delete_url_bytes`].
///
/// Returns `1` when the entry was removed and the archive was rewritten, or `0`
/// when the URL is invalid, the archive cannot be parsed, or the entry is absent.
///
/// # Safety
/// `url_ptr` must be valid for `url_len` bytes unless `url_len` is zero.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_delete_url(
    url_ptr: *const u8,
    url_len: usize,
) -> usize {
    let result = std::panic::catch_unwind(|| delete_url_bytes(slice(url_ptr, url_len)));
    match result {
        Ok(Some(())) => 1,
        _ => 0,
    }
}

/// C ABI wrapper around [`set_zip_password`].
///
/// Sets the password used to read and write traditional-PKWARE (ZipCrypto)
/// encrypted ZIP entries; an empty password clears it. Always returns `1`.
///
/// # Safety
/// `password_ptr` must be valid for `password_len` bytes unless `password_len` is zero.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_set_zip_password(
    password_ptr: *const u8,
    password_len: usize,
) -> usize {
    let _ = std::panic::catch_unwind(|| set_zip_password(slice(password_ptr, password_len)));
    1
}

/// C ABI wrapper around [`set_archive_compression`].
///
/// Returns `1` when the native PHAR archive was rewritten, or `0` for invalid
/// paths, unsupported archive families, or unsupported compression constants.
///
/// # Safety
/// `path_ptr` must be valid for `path_len` bytes unless `path_len` is zero.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_set_compression(
    path_ptr: *const u8,
    path_len: usize,
    compression_code: usize,
) -> usize {
    let result = std::panic::catch_unwind(|| {
        set_archive_compression(slice(path_ptr, path_len), compression_code)
    });
    match result {
        Ok(Some(())) => 1,
        _ => 0,
    }
}

/// C ABI wrapper around [`get_metadata_bytes`].
///
/// Returns a pointer to the serialized global metadata buffer and writes its byte
/// length into `out_len`. Returns null and writes zero when there is no metadata or
/// the archive cannot be read.
///
/// # Safety
/// `path_ptr` must be valid for `path_len` bytes unless `path_len` is zero.
/// `out_len` may be null; when non-null it must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_get_metadata(
    path_ptr: *const u8,
    path_len: usize,
    out_len: *mut usize,
) -> *const u8 {
    match std::panic::catch_unwind(|| get_metadata_bytes(slice(path_ptr, path_len))) {
        Ok(Some(bytes)) if !bytes.is_empty() => publish_result(bytes, out_len),
        _ => {
            write_len(out_len, 0);
            std::ptr::null()
        }
    }
}

/// C ABI wrapper around [`get_stub_bytes`].
///
/// Returns a pointer to the stub buffer and writes its byte length into `out_len`.
/// Returns null and writes zero when there is no stub or the archive cannot be read.
///
/// # Safety
/// `path_ptr` must be valid for `path_len` bytes unless `path_len` is zero.
/// `out_len` may be null; when non-null it must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_get_stub(
    path_ptr: *const u8,
    path_len: usize,
    out_len: *mut usize,
) -> *const u8 {
    match std::panic::catch_unwind(|| get_stub_bytes(slice(path_ptr, path_len))) {
        Ok(Some(bytes)) if !bytes.is_empty() => publish_result(bytes, out_len),
        _ => {
            write_len(out_len, 0);
            std::ptr::null()
        }
    }
}

/// C ABI wrapper around [`set_metadata_bytes`].
///
/// Returns `1` when the archive was rewritten with the new global metadata, or `0`
/// on any failure.
///
/// # Safety
/// Each pointer must be valid for its paired byte length unless that length is zero.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_set_metadata(
    path_ptr: *const u8,
    path_len: usize,
    data_ptr: *const u8,
    data_len: usize,
) -> usize {
    let result = std::panic::catch_unwind(|| {
        set_metadata_bytes(slice(path_ptr, path_len), slice(data_ptr, data_len))
    });
    match result {
        Ok(Some(())) => 1,
        _ => 0,
    }
}

/// C ABI wrapper around [`set_stub_bytes`].
///
/// Returns `1` when the archive was rewritten with the new stub, or `0` on any
/// failure (including a stub missing the `__HALT_COMPILER();` marker).
///
/// # Safety
/// Each pointer must be valid for its paired byte length unless that length is zero.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_set_stub(
    path_ptr: *const u8,
    path_len: usize,
    data_ptr: *const u8,
    data_len: usize,
) -> usize {
    let result = std::panic::catch_unwind(|| {
        set_stub_bytes(slice(path_ptr, path_len), slice(data_ptr, data_len))
    });
    match result {
        Ok(Some(())) => 1,
        _ => 0,
    }
}

/// C ABI wrapper around [`get_file_metadata_url`].
///
/// Takes a `phar://archive/entry` URL and returns a pointer to that entry's
/// serialized metadata, writing its byte length into `out_len`. Returns null and
/// writes zero when the entry has no metadata, the entry is absent, or the archive
/// cannot be read.
///
/// # Safety
/// Each pointer must be valid for its paired byte length unless that length is zero.
/// `out_len` may be null; when non-null it must be writable.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_get_file_metadata(
    url_ptr: *const u8,
    url_len: usize,
    out_len: *mut usize,
) -> *const u8 {
    let result = std::panic::catch_unwind(|| get_file_metadata_url(slice(url_ptr, url_len)));
    match result {
        Ok(Some(bytes)) if !bytes.is_empty() => publish_result(bytes, out_len),
        _ => {
            write_len(out_len, 0);
            std::ptr::null()
        }
    }
}

/// C ABI wrapper around [`set_file_metadata_url`].
///
/// Takes a `phar://archive/entry` URL and serialized metadata, rewriting the archive
/// so the entry carries it (an empty `data` clears it). Returns `1` on success, or
/// `0` on any failure including a missing entry.
///
/// # Safety
/// Each pointer must be valid for its paired byte length unless that length is zero.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_set_file_metadata(
    url_ptr: *const u8,
    url_len: usize,
    data_ptr: *const u8,
    data_len: usize,
) -> usize {
    let result = std::panic::catch_unwind(|| {
        set_file_metadata_url(slice(url_ptr, url_len), slice(data_ptr, data_len))
    });
    match result {
        Ok(Some(())) => 1,
        _ => 0,
    }
}

/// C ABI wrapper around [`gzip_archive`] — whole-archive gzip compression.
///
/// Returns a pointer to the written destination path and writes its length into
/// `out_len`; returns null and writes zero on failure.
///
/// # Safety
/// `src` must be valid for `src_len` unless zero; `out_len` must be writable when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_gzip_archive(
    src_ptr: *const u8,
    src_len: usize,
    out_len: *mut usize,
) -> *const u8 {
    publish_archive_path_result(
        std::panic::catch_unwind(|| gzip_archive(slice(src_ptr, src_len))),
        out_len,
    )
}

/// C ABI wrapper around [`bzip2_archive`] — whole-archive bzip2 compression.
///
/// Returns a pointer to the written destination path and writes its length into
/// `out_len`; returns null and writes zero on failure.
///
/// # Safety
/// `src` must be valid for `src_len` unless zero; `out_len` must be writable when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_bzip2_archive(
    src_ptr: *const u8,
    src_len: usize,
    out_len: *mut usize,
) -> *const u8 {
    publish_archive_path_result(
        std::panic::catch_unwind(|| bzip2_archive(slice(src_ptr, src_len))),
        out_len,
    )
}

/// C ABI wrapper around [`decompress_archive`] — whole-archive decompression.
///
/// Returns a pointer to the written destination path and writes its length into
/// `out_len`; returns null and writes zero on failure (including an uncompressed src).
///
/// # Safety
/// `src` must be valid for `src_len` unless zero; `out_len` must be writable when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_decompress_archive(
    src_ptr: *const u8,
    src_len: usize,
    out_len: *mut usize,
) -> *const u8 {
    publish_archive_path_result(
        std::panic::catch_unwind(|| decompress_archive(slice(src_ptr, src_len))),
        out_len,
    )
}

/// Shared result handling for the archive (de)compression bridges: publishes a
/// non-empty destination path, or returns null + zero length on failure.
fn publish_archive_path_result(
    result: std::thread::Result<Option<Vec<u8>>>,
    out_len: *mut usize,
) -> *const u8 {
    match result {
        Ok(Some(path)) if !path.is_empty() => publish_result(path, out_len),
        _ => {
            write_len(out_len, 0);
            std::ptr::null()
        }
    }
}

/// C ABI wrapper around [`sign_archive_openssl`] — RSA-SHA1 (OpenSSL) PHAR signing.
///
/// Returns `1` when the archive was re-signed, `0` on any failure (bad key, unreadable
/// archive).
///
/// # Safety
/// Each pointer must be valid for its paired byte length unless that length is zero.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_sign_openssl(
    path_ptr: *const u8,
    path_len: usize,
    key_ptr: *const u8,
    key_len: usize,
) -> usize {
    let result = std::panic::catch_unwind(|| {
        sign_archive_openssl(slice(path_ptr, path_len), slice(key_ptr, key_len))
    });
    usize::from(matches!(result, Ok(Some(_))))
}

/// C ABI wrapper around [`sign_archive_hash`] — MD5/SHA1/SHA256/SHA512 PHAR signing.
///
/// Returns `1` when the archive was re-signed, `0` on any failure or unknown `algo`.
///
/// # Safety
/// `path` must be valid for `path_len` unless that length is zero.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_sign_hash(
    path_ptr: *const u8,
    path_len: usize,
    algo: usize,
) -> usize {
    let result = std::panic::catch_unwind(|| {
        sign_archive_hash(slice(path_ptr, path_len), algo as u32)
    });
    usize::from(matches!(result, Ok(Some(()))))
}

/// C ABI wrapper around [`signature_hash_hex`] — `Phar::getSignature()['hash']`.
///
/// Returns the uppercase-hex signature/digest pointer and writes its length into
/// `out_len`; returns null + zero on failure.
///
/// # Safety
/// `path` must be valid for `path_len`; `out_len` must be writable when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_get_signature_hash(
    path_ptr: *const u8,
    path_len: usize,
    out_len: *mut usize,
) -> *const u8 {
    publish_archive_path_result(
        std::panic::catch_unwind(|| signature_hash_hex(slice(path_ptr, path_len))),
        out_len,
    )
}

/// C ABI wrapper around [`signature_type_name`] — `Phar::getSignature()['hash_type']`.
///
/// Returns the type-name pointer and writes its length into `out_len`; returns null +
/// zero on failure.
///
/// # Safety
/// `path` must be valid for `path_len`; `out_len` must be writable when non-null.
#[no_mangle]
pub unsafe extern "C" fn elephc_phar_get_signature_type(
    path_ptr: *const u8,
    path_len: usize,
    out_len: *mut usize,
) -> *const u8 {
    publish_archive_path_result(
        std::panic::catch_unwind(|| signature_type_name(slice(path_ptr, path_len))),
        out_len,
    )
}
