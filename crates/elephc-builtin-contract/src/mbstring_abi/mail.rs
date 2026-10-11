//! Purpose:
//! Defines the host callback used to format mb_send_mail's RFC 2822 Date header.
//!
//! Called from:
//! - Native and eval mbstring invocation hosts for the MbSendMail operation.
//! - The shared mbstring mail coordinator after warning callbacks complete.
//!
//! Key details:
//! - The host owns timezone state and current-time resolution.
//! - Returned bytes use the ordinary owned host-string and release protocol.

use std::ffi::c_void;

use super::{coercion::MbHostStringV1, invoke::MbInvokeHostV5};

pub type MbFormatMailDateV1 = unsafe extern "C" fn(
    *mut c_void,
    *mut MbHostStringV1,
) -> i32;

/// Mail-specific host extension, deliberately separate from native-variable V7.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct MbInvokeMailHostV8 {
    pub base: MbInvokeHostV5,
    pub format_mail_date: Option<MbFormatMailDateV1>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mail_host_layout_is_prefix_compatible() {
        assert_eq!(std::mem::size_of::<MbInvokeMailHostV8>(), 152);
        assert_eq!(std::mem::offset_of!(MbInvokeMailHostV8, base), 0);
        assert_eq!(std::mem::offset_of!(MbInvokeMailHostV8, format_mail_date), 144);
    }
}
