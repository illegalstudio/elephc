//! Purpose:
//! Binds PHP's Windows-only UTF-8 console code-page predicate to its runtime identity.
//!
//! Called from:
//! - `crate::builtins::system::sapi_windows` during builtin inventory registration.
//!
//! Key details:
//! - Target gating remains part of the shared Windows-only semantic descriptor.

use crate::builtins::semantics::windows_only_runtime_fn_semantics;
use crate::ir::RuntimeFnId;

builtin! {
    contract: "sapi_windows_cp_is_utf8",
    semantics: windows_only_runtime_fn_semantics(RuntimeFnId::SapiWindowsCpIsUtf8),
}
