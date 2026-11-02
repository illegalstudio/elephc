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
    /// The most recent call site published by compiled code, under its C name `php_site`.
    /// The compiler defines and references it through `abi::php_site_symbol`, which applies the
    /// target's C symbol mangling exactly as Rust does here: `_php_site` on Mach-O, `php_site` on
    /// ELF. Both sides spell the one logical name; neither hard-codes a platform prefix.
    static php_site: *const CompiledSiteRecord;
}

/// Returns the compiled caller's `(file, directory, line)`, when one is published.
///
/// A zero descriptor names the ENTRY file, not an unknown one: the compiler leaves it out of
/// sites of the main script and the runtime resolves it to `_script_source_file`. It used to be
/// read as "no site", so a call from the main script kept the interpreter's stale position:
/// `$evalObject->compiledMethod()` on line 11 reported the line of the `eval()` that declared the
/// class. The entry path comes from the runtime's source-file hook. Only `_php_site_none` (line
/// zero) and a module without a source path leave the interpreter's position alone.
#[cfg(not(test))]
pub(crate) unsafe fn compiled_call_site(
    values: &mut impl crate::interpreter::RuntimeValueOps,
) -> Option<(String, String, i64)> {
    let record = php_site;
    if record.is_null() || (*record).line == 0 {
        return None;
    }
    let file = (*record).file;
    let path = if file.is_null() || (*file).ptr.is_null() || (*file).len == 0 {
        values.reflection_source_file().ok().flatten().filter(|path| !path.is_empty())?
    } else {
        let bytes = std::slice::from_raw_parts((*file).ptr, (*file).len);
        String::from_utf8_lossy(bytes).into_owned()
    };
    let directory = std::path::Path::new(&path)
        .parent()
        .map(|parent| parent.to_string_lossy().into_owned())
        .unwrap_or_default();
    Some((path, directory, (*record).line as i64))
}

/// Test builds link no generated runtime, so there is no compiled site to read.
#[cfg(test)]
pub(crate) unsafe fn compiled_call_site(
    _values: &mut impl crate::interpreter::RuntimeValueOps,
) -> Option<(String, String, i64)> {
    None
}
