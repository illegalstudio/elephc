//! Purpose:
//! Reads the compiled caller's published call site (`_php_site`) when compiled code enters
//! the interpreter through the eval bridge.
//!
//! Called from:
//! - The eval-bridge method/static-method/object-construction FFI entries in `crate::ffi`.
//!
//! Key details:
//! - `_php_site` is the global record every compiled call site publishes before it can enter PHP
//!   code (`crate::codegen::lower_inst::core_builtins::backtrace::prepare_call_site`): a pointer
//!   to `{file descriptor, line}` with the descriptor a `{ptr, len}` pair, or `_php_site_none`.
//! - A compiled caller that dispatches a method through the eval bridge used to leave the
//!   interpreter's call site empty, so the synthetic frame it pushes for the native callee
//!   reported `file=""`, `line=0` (the dev error page's `matchRequest` frame lost its
//!   `Router.php:188` position). Propagating the published site before dispatch restores PHP's
//!   caller position for that frame and for any diagnostic raised while entering the callee.
//! - Only the FILE and LINE are taken; the directory is derived from the file, which is what
//!   `set_call_site` needs for include resolution and `__DIR__` while no eval file is active.

/// The compiled site record: `{ descriptor, line }`.
#[cfg(not(test))]
#[repr(C)]
struct CompiledSiteRecord {
    file: *const CompiledSiteDescriptor,
    line: u64,
}

/// The site's file descriptor: `{ptr, len}`.
#[cfg(not(test))]
#[repr(C)]
struct CompiledSiteDescriptor {
    ptr: *const u8,
    len: usize,
}

#[cfg(not(test))]
unsafe extern "C" {
    /// The most recent call site published by compiled code. The generated assembly spells this
    /// global `_php_site`; Rust's C ABI adds the Mach-O underscore, so the extern names it
    /// `php_site` to reach that exact symbol.
    static php_site: *const CompiledSiteRecord;
}

/// Returns the compiled caller's `(file, directory, line)`, when one is published.
///
/// The entry file has no source path (`_php_site_none` / a zero descriptor), in which case the
/// caller keeps whatever position the interpreter already had.
#[cfg(not(test))]
pub(crate) unsafe fn compiled_call_site() -> Option<(String, String, i64)> {
    let record = php_site;
    if record.is_null() {
        return None;
    }
    let file = (*record).file;
    if file.is_null() || (*file).ptr.is_null() || (*file).len == 0 {
        return None;
    }
    let bytes = std::slice::from_raw_parts((*file).ptr, (*file).len);
    let path = String::from_utf8_lossy(bytes).into_owned();
    let directory = std::path::Path::new(&path)
        .parent()
        .map(|parent| parent.to_string_lossy().into_owned())
        .unwrap_or_default();
    Some((path, directory, (*record).line as i64))
}

/// Test builds link no generated runtime, so there is no compiled site to read.
#[cfg(test)]
pub(crate) unsafe fn compiled_call_site() -> Option<(String, String, i64)> {
    None
}
