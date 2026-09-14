//! Purpose:
//! Sends eval output operations through a versioned native exception boundary.
//!
//! Called from:
//! - The output methods in `RuntimeValueOps for ElephcRuntimeOps`.
//!
//! Key details:
//! - Borrowed arguments stay live for the call; native Throwables become ordinary eval failures.

use super::*;
use elephc_builtin_contract::output_abi::{OutputAction, OutputRequestV1};

#[cfg(not(test))]
impl ElephcRuntimeOps {
    /// Executes one output request and transfers any native pending Throwable into its eval context.
    pub(super) fn protected_output(
        &self, action: OutputAction, arguments: [u64; 6],
    ) -> Result<OutputRequestV1, EvalStatus> {
        let mut request = OutputRequestV1::new(action, arguments);
        let status = unsafe { __elephc_eval_output_v1(&mut request) };
        self.handle_native_cleanup_status(status)?;
        Ok(request)
    }
}
