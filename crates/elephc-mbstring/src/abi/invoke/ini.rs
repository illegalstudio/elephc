//! Purpose:
//! Routes coerced prelude INI operations through the shared Core and mbstring request APIs.
//!
//! Called from:
//! - The protected invocation coordinator for RuntimeBuiltinId::SharedIni.
//!
//! Key details:
//! - Native string identities remain leased by retained argument copies throughout mutation callbacks.
//! - The completed wire owner survives until final host cleanup succeeds, including identity leases.

use super::*;
use elephc_builtin_contract::mbstring_abi::ini::{core, INI_GET, INI_SET, INI_RESTORE, INI_GET_ALL};

/// Executes a validated operation with protected diagnostics and no request borrow across callbacks.
pub(super) unsafe fn invoke(values: &[Argument], session: &Session) -> Result<Completed, Status> {
    let [Argument::Int(operation), option, value, Argument::Bool(details)] = values else { return Err(Status::Fatal); };
    let name = bytes(option)?;
    let operation = u32::try_from(*operation).map_err(|_| Status::Fatal)?;
    let host = session.ini_host();
    let mut result = Completed(MbResultV1::default());
    let core = if operation == INI_GET_ALL {
        match name { b"core" => true, b"mbstring" => false, _ => return Ok(Outcome::boolean(false).into()) }
    } else { core::lookup(name).is_some() };
    let arguments = match operation {
        INI_GET | INI_RESTORE => vec![MbArgV1::string(name)],
        INI_SET => vec![MbArgV1::string(name), value.wire().ok_or(Status::Fatal)?],
        INI_GET_ALL => vec![MbArgV1::boolean(*details)],
        _ => return Err(Status::Fatal),
    };
    let status = unsafe {
        if core { super::super::ini::elephc_mbstring_core_ini_v1(operation, arguments.as_ptr(), arguments.len() as u64, &host, &mut result.0) }
        else { super::super::ini::elephc_mbstring_ini_v1(operation, arguments.as_ptr(), arguments.len() as u64, &host, &mut result.0) }
    };
    match status { 0 => Ok(result), 2 => Err(Status::Pending), _ => Err(Status::Fatal) }
}

/// Borrows option bytes without treating a name as an identity-bearing setter value.
fn bytes(value: &Argument) -> Result<&[u8], Status> {
    match value { Argument::String(bytes) => Ok(bytes), Argument::IniString(string) => Ok(string), _ => Err(Status::Fatal) }
}
