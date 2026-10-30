//! Purpose:
//! Adapts native process creation to the application callback's original signal mask.
//!
//! Called from:
//! - Web-mode AOT popen/shell_exec helpers and application system/passthru lowering.
//!
//! Key details:
//! - Only the libc call runs with the original mask; PHP resumes under its deferral guard.
//! - Request lifecycle callbacks have no deferral, so the permit leaves their mask alone.

use elephc_process_context::SpawnPermit;

/// Opens a shell pipe whose child inherits the pre-callback signal mask.
///
/// # Safety
/// `command` and `mode` must be valid null-terminated C strings for libc popen.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_application_popen(
    command: *const libc::c_char,
    mode: *const libc::c_char,
) -> *mut libc::FILE {
    let _signals = SpawnPermit::new();
    libc::popen(command, mode)
}

/// Executes a shell command with its normal signal inheritance and libc wait behavior.
///
/// # Safety
/// `command` must be a valid null-terminated C string for libc system.
#[no_mangle]
pub unsafe extern "C" fn elephc_web_application_system(command: *const libc::c_char) -> libc::c_int {
    let _signals = SpawnPermit::new();
    libc::system(command)
}
