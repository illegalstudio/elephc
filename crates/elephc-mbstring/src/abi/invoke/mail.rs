//! Purpose:
//! Delivers mb_send_mail warnings through the protected PHP callback before transport.
//!
//! Called from:
//! - The shared invocation coordinator after ordered argument coercion and header snapshots.
//!
//! Key details:
//! - The request-state borrow ends before a warning handler can reenter mbstring.
//! - A throwing warning handler prevents the sendmail process from starting.

use super::*;

/// Prepares mail, delivers ordered warnings, then opens the configured transport.
pub(super) unsafe fn invoke(args: &[MbArgV1], session: &mut Session) -> Result<Outcome, Status> {
    let arguments = unsafe { Arguments::new(RuntimeBuiltinId::MbSendMail, args) }.ok_or(Status::Fatal)?;
    let prepared = REQUEST.with(|state| crate::abi::mail::prepare(&arguments, &state.borrow()));
    let prepared = match prepared {
        Ok(prepared) => prepared,
        Err(error) => return Ok(Outcome::error(error)),
    };
    for warning in &prepared.warnings {
        unsafe { session.diagnostic(2, warning)?; }
    }
    Ok(REQUEST.with(|state| crate::abi::mail::send(prepared.bytes, &state.borrow(), arguments.nullable_string(4))))
}
