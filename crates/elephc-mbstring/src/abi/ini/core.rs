//! Purpose:
//! Exposes shared Core INI state and the live mb_parse_str configuration callback.
//!
//! Called from:
//! - Core INI adapters and native/eval V5 query invocation hosts.
//!
//! Key details:
//! - Reads and display_errors mutations use the same thread-local request as mbstring.
//! - The provider never calls PHP; borrowed separator bytes last through the next host callback.
//! - All results and identity leases use the ordinary INI result-release contract.

use std::ffi::c_void;
use elephc_builtin_contract::mbstring_abi::{coercion::MbHostStringV1, invoke::{
    MbQueryConfigV1, QUERY_CONFIG_ENTRY, QUERY_CONFIG_FIELD, QUERY_CONFIG_DIAGNOSTIC,
}};
use super::*;

/// Reads, changes, restores, or enumerates the supported Core directives on this request.
///
/// # Safety
/// Arguments, complete host metadata, and writable result storage obey elephc_mbstring_ini_v1.
/// String identity arguments borrow an existing live INI lease; published results own new leases.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_core_ini_v1(op: u32, args: *const MbArgV1, count: u64,
    host: *const MbIniHostV1, out: *mut MbResultV1) -> i32 {
    unsafe { boundary(out, |_| {
        read_host(host)?;
        let args = read_args(args, count)?;
        let result = match (op, args) {
            (INI_GET, [name]) => REQUEST.with(|state| state.borrow().core_ini.get(string_arg(name)?)
                .map_or_else(|| Ok(Outcome::boolean(false).into()), |value| Ok(Reply::string(value)))),
            (INI_SET, [name, value]) => {
                let name = string_arg(name)?;
                let value = if value.kind == ARG_INI_STRING {
                    if value.value <= 0 || value.len != 0 || !value.bytes.is_null() { return Err(1); }
                    strings::lookup(value.value as u64)?
                } else { IniString::new(string_arg(value)?) };
                REQUEST.with(|state| Ok(state.borrow_mut().core_ini.set(name, value)
                    .map_or_else(|| Outcome::boolean(false).into(), Reply::string)))
            },
            (INI_RESTORE, [name]) => {
                let name = string_arg(name)?;
                REQUEST.with(|state| state.borrow_mut().core_ini.restore(name));
                Ok(Outcome::empty(RESULT_NULL).into())
            },
            (INI_GET_ALL, [details]) if details.kind == ARG_BOOL && matches!(details.value, 0 | 1) => {
                let (graph, strings) = REQUEST.with(|state| state.borrow().core_ini.all(details.value != 0));
                Ok(Reply::array(graph, strings))
            },
            _ => Err(1),
        };
        result
    }) }
}

/// Reads the current query policy, including display changes made by preceding host callbacks.
///
/// # Safety
/// `out` points to writable, aligned MbQueryConfigV1 storage. `context` is unused and may be null.
/// Returned separator bytes are borrowed only until the next host callback or request mutation;
/// owner is null, so callers must copy them before reentry and must not release them as native storage.
#[no_mangle]
pub unsafe extern "C" fn elephc_mbstring_query_configuration_v1(_context: *mut c_void,
    phase: u32, out: *mut MbQueryConfigV1) -> i32 {
    if out.is_null() { return 1; }
    unsafe { out.write(MbQueryConfigV1 { separators: MbHostStringV1 {
        bytes: std::ptr::null(), len: 0, owner: std::ptr::null_mut(),
    }, max_variables: 0, max_nesting: 0, display_errors: 0 }); }
    if !matches!(phase, QUERY_CONFIG_ENTRY | QUERY_CONFIG_FIELD | QUERY_CONFIG_DIAGNOSTIC) { return 1; }
    catch_unwind(AssertUnwindSafe(|| REQUEST.with(|state| {
        let state = state.borrow();
        let core = &state.core_ini;
        let separators = core.separators();
        unsafe { out.write(MbQueryConfigV1 {
            separators: MbHostStringV1 { bytes: separators.as_ptr(), len: separators.len() as u64, owner: std::ptr::null_mut() },
            max_variables: core.max_variables(), max_nesting: core.max_nesting(),
            display_errors: u64::from(core.display_errors() != 0),
        }); }
        0
    }))).unwrap_or(1)
}
