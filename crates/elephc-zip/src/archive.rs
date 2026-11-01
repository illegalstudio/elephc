//! Purpose:
//! Safe ownership layer around libzip's `zip_t`: opening, metadata, payload reads, mutations,
//! comments, flags, passwords, and close/discard lifecycle.
//!
//! Called from:
//! - `crate::abi` for every PHP-visible `ZipArchive` operation.
//!
//! Key details:
//! - `zip_close` only frees the archive on success; a failed close keeps the handle alive so
//!   the prelude can read `status`/`getStatusString()` before `discard`.
//! - `zip_source_buffer` never copies caller memory, so added payloads are retained in
//!   `buffers` until the archive is closed or discarded.
//! - PHP's `FL_OPEN_FILE_NOW` is an Elephc-only flag; when set the recipe opens the file
//!   immediately with `fopen` and hands the `FILE *` to `zip_source_filep`.
//! - Names are C strings at the libzip boundary; payloads, comments, and returned names cross
//!   the ABI as explicit byte slices (hex-encoded by `crate::abi`).

use std::ffi::{c_char, c_void, CStr, CString};
use std::fs;
use std::path::{Path, PathBuf};
use std::ptr;

use crate::consts::*;
use crate::error::ZipError;
use crate::ffi;

/// PHP's `ZipArchive::FL_OPEN_FILE_NOW` is not a libzip flag.
const FL_OPEN_FILE_NOW: i64 = 1 << 30;

/// One open (or successfully closed) archive and everything libzip must keep alive.
pub(crate) struct Archive {
    /// Live libzip handle; null after a successful close or discard.
    za: *mut ffi::zip_t,
    /// Absolute path PHP exposes through `ZipArchive::$filename`.
    pub(crate) path: String,
    /// Whether `open()` promoted a pre-existing zero-byte file to `ZIP_TRUNCATE`.
    pub(crate) empty_file: bool,
    /// PHP's `lastId` property.
    pub(crate) last_id: i64,
    /// Caller-owned buffers referenced by `zip_source_buffer`.
    buffers: Vec<Vec<u8>>,
    /// Status recorded for archives after a failed close or a bridge-side rejection.
    manual_status: Option<(i64, i64)>,
    /// Status after a successful close.
    closed_status: (i64, i64),
}

// The PHP runtime is single-threaded, but the bridge registry is process-wide and the
// handle table requires `Send`. Every access is serialized by that table's mutex.
unsafe impl Send for Archive {}

impl Archive {
    /// Opens or creates one archive, returning PHP's resolved filename on success.
    pub(crate) fn open(path: String, flags: i64) -> Result<Self, i64> {
        let absolute = absolute_path(&path);
        let Some(path_c) = cstring(&absolute) else {
            return Err(ZipError::Invalid.code());
        };
        let mut flags = flags;
        let mut empty_file = false;
        if flags & (OVERWRITE | RDONLY) == 0 {
            if fs::metadata(&absolute).is_ok_and(|metadata| metadata.len() == 0) {
                empty_file = true;
                flags |= OVERWRITE;
            }
        }
        let mut error = 0i32;
        let za = unsafe { ffi::zip_open(path_c.as_ptr(), flags as i32, &mut error) };
        if za.is_null() || error != 0 {
            if !za.is_null() {
                unsafe { ffi::zip_discard(za) };
            }
            return Err(if error == 0 { ZipError::NotZip.code() } else { i64::from(error) });
        }
        Ok(Self {
            za,
            path: absolute,
            empty_file,
            last_id: -1,
            buffers: Vec::new(),
            manual_status: None,
            closed_status: (0, 0),
        })
    }

    /// Returns the live libzip handle, or `None` once closed.
    pub(crate) fn live(&self) -> Option<*mut ffi::zip_t> {
        (!self.za.is_null()).then_some(self.za)
    }

    /// Returns the last error as `(zip code, system code)`.
    pub(crate) fn status(&self) -> (i64, i64) {
        if let Some(status) = self.manual_status {
            return status;
        }
        match self.live() {
            Some(za) => unsafe {
                let error = ffi::zip_get_error(za);
                (i64::from(ffi::zip_error_code_zip(error)), i64::from(ffi::zip_error_code_system(error)))
            },
            None => self.closed_status,
        }
    }

    /// Returns libzip's human-readable status string.
    pub(crate) fn status_string(&self) -> String {
        if let Some((code, _)) = self.manual_status {
            return ZipError::from_code(code).message().to_string();
        }
        let Some(za) = self.live() else {
            return ZipError::from_code(self.closed_status.0).message().to_string();
        };
        unsafe {
            let error = ffi::zip_get_error(za);
            let pointer = ffi::zip_error_strerror(error);
            let message = if pointer.is_null() {
                ZipError::from_code(i64::from(ffi::zip_error_code_zip(error))).message().to_string()
            } else {
                CStr::from_ptr(pointer).to_string_lossy().into_owned()
            };
            // php-src frees the cached message after each getStatusString() call.
            ffi::zip_error_fini(error);
            message
        }
    }

    /// Marks the last operation as successful and clears libzip's stored error.
    pub(crate) fn succeed(&mut self) {
        self.manual_status = None;
        if let Some(za) = self.live() {
            unsafe { ffi::zip_error_clear(za) };
        }
    }

    /// Records a bridge-side rejection that libzip did not see.
    pub(crate) fn fail(&mut self, error: ZipError) {
        self.manual_status = Some((error.code(), 0));
    }

    /// Clears any stored error, mirroring `ZipArchive::clearError()`.
    pub(crate) fn clear_error(&mut self) {
        self.manual_status = None;
        self.closed_status = (0, 0);
        if let Some(za) = self.live() {
            unsafe { ffi::zip_error_clear(za) };
        }
    }

    /// Overwrites the visible status for a prelude-side argument rejection.
    pub(crate) fn set_status(&mut self, code: i64) {
        self.manual_status = Some((code, 0));
    }

    /// Writes pending changes and releases the handle on success.
    pub(crate) fn close(&mut self) -> Result<(), ZipError> {
        let Some(za) = self.live() else {
            return Err(ZipError::ZipClosed);
        };
        if unsafe { ffi::zip_close(za) } == 0 {
            self.za = ptr::null_mut();
            self.closed_status = (0, 0);
            self.manual_status = None;
            return Ok(());
        }
        let (code, system) = unsafe {
            let error = ffi::zip_get_error(za);
            (i64::from(ffi::zip_error_code_zip(error)), i64::from(ffi::zip_error_code_system(error)))
        };
        self.manual_status = Some((code, system));
        Err(ZipError::from_code(code))
    }

    /// Discards pending changes and frees the handle.
    pub(crate) fn discard(&mut self) {
        if let Some(za) = self.live() {
            unsafe { ffi::zip_discard(za) };
            self.za = ptr::null_mut();
        }
    }

    /// Returns the entry count, including deleted placeholders.
    pub(crate) fn num_entries(&self) -> i64 {
        match self.live() {
            Some(za) => unsafe { ffi::zip_get_num_entries(za, 0) },
            None => 0,
        }
    }

    /// Returns one entry's name as raw bytes.
    pub(crate) fn name(&self, index: i64, flags: i64) -> Option<Vec<u8>> {
        let za = self.live()?;
        unsafe {
            let pointer = ffi::zip_get_name(za, index as u64, flags as u32);
            (!pointer.is_null()).then(|| CStr::from_ptr(pointer).to_bytes().to_vec())
        }
    }

    /// Locates an entry by name.
    pub(crate) fn locate(&mut self, name: &[u8], flags: i64) -> Option<i64> {
        let za = self.live()?;
        let name = CString::new(name).ok()?;
        let index = unsafe { ffi::zip_name_locate(za, name.as_ptr(), flags as u32) };
        (index >= 0).then_some(index)
    }

    /// Reads one entry's stat record by index.
    pub(crate) fn stat(&self, index: i64, flags: i64) -> Result<ffi::zip_stat, ZipError> {
        let za = self.live().ok_or(ZipError::ZipClosed)?;
        let mut stat = ffi::zip_stat::zeroed();
        if unsafe { ffi::zip_stat_index(za, index as u64, flags as u32, &mut stat) } != 0 {
            return Err(self.error());
        }
        Ok(stat)
    }

    /// Reads one entry's stat record by name.
    pub(crate) fn stat_name(&self, name: &[u8], flags: i64) -> Result<ffi::zip_stat, ZipError> {
        let za = self.live().ok_or(ZipError::ZipClosed)?;
        let name = CString::new(name).map_err(|_| ZipError::Invalid)?;
        let mut stat = ffi::zip_stat::zeroed();
        if unsafe { ffi::zip_stat(za, name.as_ptr(), flags as u32, &mut stat) } != 0 {
            return Err(self.error());
        }
        Ok(stat)
    }

    /// Returns the archive comment bytes, or `None` when it has no comment.
    pub(crate) fn archive_comment(&self, flags: i64) -> Option<Vec<u8>> {
        let za = self.live()?;
        let mut length = 0i32;
        unsafe {
            let pointer = ffi::zip_get_archive_comment(za, &mut length, flags as u32);
            (!pointer.is_null()).then(|| {
                std::slice::from_raw_parts(pointer.cast::<u8>(), length.max(0) as usize).to_vec()
            })
        }
    }

    /// Returns one entry comment, or `None` when it has none.
    pub(crate) fn comment(&self, index: i64, flags: i64) -> Option<Vec<u8>> {
        let za = self.live()?;
        let mut length = 0u32;
        unsafe {
            let pointer = ffi::zip_file_get_comment(za, index as u64, &mut length, flags as u32);
            (!pointer.is_null()).then(|| {
                std::slice::from_raw_parts(pointer.cast::<u8>(), length as usize).to_vec()
            })
        }
    }

    /// Returns one entry's external attributes as `(opsys, attr)`.
    pub(crate) fn external_attributes(&self, index: i64, flags: i64) -> Result<(i64, u32), ZipError> {
        let za = self.live().ok_or(ZipError::ZipClosed)?;
        let mut opsys = 0u8;
        let mut attributes = 0u32;
        if unsafe { ffi::zip_file_get_external_attributes(za, index as u64, flags as u32, &mut opsys, &mut attributes) } != 0 {
            return Err(self.error());
        }
        Ok((i64::from(opsys), attributes))
    }

    /// Reads one entry's payload as `(bytes, crc_mismatch)`.
    pub(crate) fn payload_stream(&self, index: i64, flags: i64) -> Result<(Vec<u8>, bool), ZipError> {
        let za = self.live().ok_or(ZipError::ZipClosed)?;
        let file = unsafe { ffi::zip_fopen_index(za, index as u64, flags as u32) };
        if file.is_null() {
            return Err(self.error());
        }
        let mut data = Vec::new();
        let mut crc_error = false;
        loop {
            let mut chunk = [0u8; 65_536];
            let read = unsafe { ffi::zip_fread(file, chunk.as_mut_ptr().cast::<c_void>(), chunk.len() as u64) };
            if read > 0 {
                data.extend_from_slice(&chunk[..read as usize]);
            } else if read == 0 {
                break;
            } else {
                let code = unsafe { ffi::zip_error_code_zip(ffi::zip_file_get_error(file)) };
                if i64::from(code) == ZipError::Crc.code() {
                    crc_error = true;
                    break;
                }
                let _ = unsafe { ffi::zip_fclose(file) };
                return Err(ZipError::from_code(i64::from(code)));
            }
        }
        let _ = unsafe { ffi::zip_fclose(file) };
        Ok((data, crc_error))
    }

    /// Reads one entry's payload for `getFrom*()` and extraction.
    ///
    /// PHP's one-shot `zip_fread` returns the bytes it decoded before the CRC check at EOF,
    /// so a CRC mismatch must not discard them here either.
    pub(crate) fn payload(&self, index: i64, flags: i64) -> Result<Vec<u8>, ZipError> {
        self.payload_stream(index, flags).map(|(data, _crc)| data)
    }

    /// Adds a memory-backed entry and returns its index.
    pub(crate) fn add_from_string(&mut self, name: &[u8], data: Vec<u8>, flags: i64) -> Result<i64, ZipError> {
        let za = self.live().ok_or(ZipError::ZipClosed)?;
        let name = CString::new(name).map_err(|_| ZipError::Invalid)?;
        // libzip reads this buffer at close time; the outer Vec may grow without moving it.
        self.buffers.push(data);
        let buffer = self.buffers.last().expect("buffer was just pushed");
        let source = unsafe {
            ffi::zip_source_buffer(za, buffer.as_ptr().cast::<c_void>(), buffer.len() as u64, 0)
        };
        if source.is_null() {
            return Err(self.error());
        }
        let index = unsafe { ffi::zip_file_add(za, name.as_ptr(), source, flags as u32) };
        if index < 0 {
            unsafe { ffi::zip_source_free(source) };
            self.last_id = -1;
            return Err(self.error());
        }
        self.last_id = index;
        self.succeed();
        Ok(index)
    }

    /// Adds an empty directory entry, appending PHP's trailing slash.
    pub(crate) fn add_directory(&mut self, name: &[u8], flags: i64) -> Result<i64, ZipError> {
        let za = self.live().ok_or(ZipError::ZipClosed)?;
        let mut full = name.to_vec();
        if !full.ends_with(b"/") {
            full.push(b'/');
        }
        let name = CString::new(full).map_err(|_| ZipError::Invalid)?;
        let index = unsafe { ffi::zip_dir_add(za, name.as_ptr(), flags as u32) };
        self.last_id = index;
        if index < 0 {
            return Err(self.error());
        }
        self.succeed();
        Ok(index)
    }

    /// Adds or replaces one file-backed entry from a validated range.
    pub(crate) fn add_or_replace_file(
        &mut self,
        path: &str,
        entry: Option<&[u8]>,
        start: i64,
        length: i64,
        flags: i64,
        replace: Option<i64>,
    ) -> Result<i64, ZipError> {
        let za = self.live().ok_or(ZipError::ZipClosed)?;
        let metadata = fs::metadata(path).map_err(|_| ZipError::Open)?;
        validate_range(metadata.len(), start, length)?;
        let resolved = CString::new(path).map_err(|_| ZipError::Invalid)?;
        let source = if flags & FL_OPEN_FILE_NOW != 0 {
            let mode = CString::new("rb").expect("static mode string");
            let file = unsafe { ffi::fopen(resolved.as_ptr(), mode.as_ptr()) };
            if file.is_null() {
                return Err(ZipError::Open);
            }
            let source = unsafe { ffi::zip_source_filep(za, file, start as u64, length) };
            if source.is_null() {
                unsafe { ffi::fclose(file) };
                return Err(self.error());
            }
            source
        } else {
            unsafe { ffi::zip_source_file(za, resolved.as_ptr(), start as u64, length) }
        };
        if source.is_null() {
            return Err(self.error());
        }
        let flags = (flags & !FL_OPEN_FILE_NOW) as u32;
        match replace {
            Some(index) => {
                if unsafe { ffi::zip_file_replace(za, index as u64, source, flags) } != 0 {
                    unsafe { ffi::zip_source_free(source) };
                    return Err(self.error());
                }
                self.succeed();
                Ok(index)
            }
            None => {
                let name = entry.filter(|name| !name.is_empty()).unwrap_or_else(|| path.as_bytes());
                let name = CString::new(name).map_err(|_| ZipError::Invalid)?;
                let index = unsafe { ffi::zip_file_add(za, name.as_ptr(), source, flags) };
                self.last_id = index;
                if index < 0 {
                    unsafe { ffi::zip_source_free(source) };
                    return Err(self.error());
                }
                self.succeed();
                Ok(index)
            }
        }
    }

    /// Deletes one entry.
    pub(crate) fn delete(&mut self, index: i64) -> Result<(), ZipError> {
        let za = self.live().ok_or(ZipError::ZipClosed)?;
        if unsafe { ffi::zip_delete(za, index as u64) } != 0 {
            return Err(self.error());
        }
        self.succeed();
        Ok(())
    }

    /// Renames one entry.
    pub(crate) fn rename(&mut self, index: i64, name: &[u8]) -> Result<(), ZipError> {
        let za = self.live().ok_or(ZipError::ZipClosed)?;
        let name = CString::new(name).map_err(|_| ZipError::Invalid)?;
        if unsafe { ffi::zip_file_rename(za, index as u64, name.as_ptr(), 0) } != 0 {
            return Err(self.error());
        }
        self.succeed();
        Ok(())
    }

    /// Sets one entry's comment, reproducing php-src's libzip use-after-free workaround.
    ///
    /// `php_zip_set_file_comment()` first compares the raw current comment with the new
    /// bytes and returns success without calling `zip_file_set_comment()` when they match.
    /// That matters after an overwrite: libzip 1.11.4 frees the inherited comment while
    /// resolving it, and the next set-comment call on the changed entry crashes. The
    /// gh22649 fixture repeats the inherited comment verbatim, which is exactly this path.
    pub(crate) fn set_comment(&mut self, index: i64, comment: &[u8]) -> Result<(), ZipError> {
        let za = self.live().ok_or(ZipError::ZipClosed)?;
        if self.comment(index, FL_ENC_RAW).is_some_and(|current| current == comment) {
            return Ok(());
        }
        let result = if comment.is_empty() {
            unsafe { ffi::zip_file_set_comment(za, index as u64, ptr::null(), 0, 0) }
        } else {
            unsafe {
                ffi::zip_file_set_comment(
                    za,
                    index as u64,
                    comment.as_ptr().cast::<c_char>(),
                    comment.len() as u16,
                    0,
                )
            }
        };
        if result != 0 {
            return Err(self.error());
        }
        self.succeed();
        Ok(())
    }

    /// Sets one entry's mtime.
    pub(crate) fn set_mtime(&mut self, index: i64, timestamp: i64, flags: i64) -> Result<(), ZipError> {
        let za = self.live().ok_or(ZipError::ZipClosed)?;
        if unsafe { ffi::zip_file_set_mtime(za, index as u64, timestamp, flags as u32) } != 0 {
            return Err(self.error());
        }
        self.succeed();
        Ok(())
    }

    /// Sets one entry's compression method.
    pub(crate) fn set_compression(&mut self, index: i64, method: i64, compflags: i64) -> Result<(), ZipError> {
        let za = self.live().ok_or(ZipError::ZipClosed)?;
        if unsafe { ffi::zip_set_file_compression(za, index as u64, method as i32, compflags as u32) } != 0 {
            return Err(self.error());
        }
        self.succeed();
        Ok(())
    }

    /// Sets one entry's encryption method and password.
    pub(crate) fn set_encryption(&mut self, index: i64, method: i64, password: Option<&[u8]>) -> Result<(), ZipError> {
        let za = self.live().ok_or(ZipError::ZipClosed)?;
        let password = CString::new(password.unwrap_or_default()).map_err(|_| ZipError::Invalid)?;
        if unsafe { ffi::zip_file_set_encryption(za, index as u64, method as u16, password.as_ptr()) } != 0 {
            return Err(self.error());
        }
        self.succeed();
        Ok(())
    }

    /// Sets one entry's external attributes.
    pub(crate) fn set_external_attributes(&mut self, index: i64, opsys: i64, attr: u32, flags: i64) -> Result<(), ZipError> {
        let za = self.live().ok_or(ZipError::ZipClosed)?;
        if unsafe { ffi::zip_file_set_external_attributes(za, index as u64, flags as u32, opsys as u8, attr) } != 0 {
            return Err(self.error());
        }
        self.succeed();
        Ok(())
    }

    /// Sets or removes the archive comment.
    pub(crate) fn set_archive_comment(&mut self, comment: &[u8]) -> Result<(), ZipError> {
        let za = self.live().ok_or(ZipError::ZipClosed)?;
        if unsafe { ffi::zip_set_archive_comment(za, comment.as_ptr().cast::<c_char>(), comment.len() as u16) } != 0 {
            return Err(self.error());
        }
        self.succeed();
        Ok(())
    }

    /// Sets the archive-wide default password.
    pub(crate) fn set_password(&mut self, password: &[u8]) -> Result<(), ZipError> {
        let za = self.live().ok_or(ZipError::ZipClosed)?;
        let password = CString::new(password).map_err(|_| ZipError::Invalid)?;
        if unsafe { ffi::zip_set_default_password(za, password.as_ptr()) } != 0 {
            return Err(self.error());
        }
        self.succeed();
        Ok(())
    }

    /// Reads one archive flag.
    pub(crate) fn archive_flag(&self, flag: i64, flags: i64) -> i64 {
        match self.live() {
            Some(za) => i64::from(unsafe { ffi::zip_get_archive_flag(za, flag as u32, flags as u32) }),
            None => 0,
        }
    }

    /// Sets one archive flag.
    pub(crate) fn set_archive_flag(&mut self, flag: i64, value: i64) -> Result<(), ZipError> {
        let za = self.live().ok_or(ZipError::ZipClosed)?;
        if unsafe { ffi::zip_set_archive_flag(za, flag as u32, value as i32) } != 0 {
            return Err(self.error());
        }
        self.succeed();
        Ok(())
    }

    /// Reverts all changes.
    pub(crate) fn unchange_all(&mut self) -> Result<(), ZipError> {
        let za = self.live().ok_or(ZipError::ZipClosed)?;
        if unsafe { ffi::zip_unchange_all(za) } != 0 {
            return Err(self.error());
        }
        self.succeed();
        Ok(())
    }

    /// Reverts one entry.
    pub(crate) fn unchange_index(&mut self, index: i64) -> Result<(), ZipError> {
        let za = self.live().ok_or(ZipError::ZipClosed)?;
        if unsafe { ffi::zip_unchange(za, index as u64) } != 0 {
            return Err(self.error());
        }
        self.succeed();
        Ok(())
    }

    /// Reverts archive-level changes.
    pub(crate) fn unchange_archive(&mut self) -> Result<(), ZipError> {
        let za = self.live().ok_or(ZipError::ZipClosed)?;
        if unsafe { ffi::zip_unchange_archive(za) } != 0 {
            return Err(self.error());
        }
        self.succeed();
        Ok(())
    }

    /// Reads the current libzip error without consuming it.
    fn error(&self) -> ZipError {
        let Some(za) = self.live() else {
            return ZipError::ZipClosed;
        };
        let code = unsafe {
            let error = ffi::zip_get_error(za);
            ffi::zip_error_code_zip(error)
        };
        ZipError::from_code(i64::from(code))
    }
}

impl Drop for Archive {
    /// Frees any live libzip handle if the registry drops without an explicit discard.
    fn drop(&mut self) {
        self.discard();
    }
}

/// Converts a Rust string into a C string, rejecting interior NUL bytes.
fn cstring(value: &str) -> Option<CString> {
    CString::new(value.as_bytes()).ok()
}

/// Makes a PHP path absolute without requiring the target file to exist.
fn absolute_path(path: &str) -> String {
    let path = Path::new(path);
    if path.is_absolute() {
        return path.to_string_lossy().into_owned();
    }
    std::path::absolute(path)
        .unwrap_or_else(|_| PathBuf::from(path))
        .to_string_lossy()
        .into_owned()
}

/// Validates PHP's `addFile`/`replaceFile` start/length pair against the file size.
fn validate_range(file_len: u64, start: i64, length: i64) -> Result<(), ZipError> {
    if start < 0 {
        return Err(ZipError::Invalid);
    }
    let start = start as u64;
    if start > file_len {
        return Err(ZipError::Invalid);
    }
    match length {
        LENGTH_TO_END | LENGTH_UNCHECKED => Ok(()),
        value if value < 0 => Err(ZipError::Invalid),
        value => {
            let end = start.checked_add(value as u64).ok_or(ZipError::Invalid)?;
            if end > file_len {
                return Err(ZipError::Invalid);
            }
            Ok(())
        }
    }
}
