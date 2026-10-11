//! Purpose:
//! Formats mb_send_mail's Date header through the active eval timezone context.
//!
//! Called from:
//! - The generated mbstring V8 host when a runtime builtin call carries an eval context.
//!
//! Key details:
//! - Existing eval date helpers own timezone switching and PHP token formatting.
//! - Thread-local bytes remain borrowed until the native coordinator copies them immediately.

use std::cell::RefCell;
use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};

use elephc_builtin_contract::mbstring_abi::coercion::MbHostStringV1;

use crate::abi::ElephcEvalContext;

thread_local! {
    static MAIL_DATE: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Returns eval-context RFC 2822 bytes without unwinding across the generated runtime.
#[no_mangle]
pub unsafe extern "C" fn elephc_eval_mbstring_mail_date_v1(
    context: *mut c_void,
    output: *mut MbHostStringV1,
) -> i32 {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let context = unsafe { (context as *const ElephcEvalContext).as_ref() }.ok_or(())?;
        let output = unsafe { output.as_mut() }.ok_or(())?;
        let bytes = crate::interpreter::format_mail_date(context).map_err(|_| ())?;
        MAIL_DATE.with_borrow_mut(|storage| {
            *storage = bytes;
            output.bytes = storage.as_ptr();
            output.len = storage.len() as u64;
            output.owner = std::ptr::null_mut();
        });
        Ok::<(), ()>(())
    }));
    if matches!(result, Ok(Ok(()))) { 0 } else { 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn formatted(timezone: &str) -> String {
        let mut context = ElephcEvalContext::default();
        context.set_default_timezone(timezone);
        let mut output = MbHostStringV1 {
            bytes: std::ptr::null(), len: 0, owner: std::ptr::null_mut(),
        };
        assert_eq!(unsafe { elephc_eval_mbstring_mail_date_v1(
            (&mut context as *mut ElephcEvalContext).cast(), &mut output,
        ) }, 0);
        let bytes = unsafe { std::slice::from_raw_parts(output.bytes, output.len as usize) };
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    /// Eval mail dates follow context-local timezone changes, not the ambient OS default.
    #[test]
    fn eval_mail_date_uses_context_timezone() {
        let utc = formatted("UTC");
        let eastern = formatted("Etc/GMT+5");
        assert_eq!(utc.len(), 31, "{utc}");
        assert_eq!(eastern.len(), 31, "{eastern}");
        assert_eq!(&utc[3..5], ", ", "{utc}");
        assert_eq!(&eastern[3..5], ", ", "{eastern}");
        assert!(utc.ends_with("+0000"), "{utc}");
        assert!(eastern.ends_with("-0500"), "{eastern}");
    }
}
