//! Purpose:
//! Binds PHP's Windows-only console control-handler registration builtin.
//!
//! Called from:
//! - `crate::builtins::system::sapi_windows` during builtin inventory registration.
//!
//! Key details:
//! - Target gating remains part of the shared Windows-only semantic descriptor.

use crate::builtins::semantics::windows_only_runtime_fn_semantics;
use crate::ir::RuntimeFnId;

builtin! {
    contract: "sapi_windows_set_ctrl_handler",
    semantics: windows_only_runtime_fn_semantics(RuntimeFnId::SapiWindowsSetCtrlHandler),
}
