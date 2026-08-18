//! Purpose:
//! Binds macOS `pcntl_getqos_class` to its typed PCNTL runtime operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - The checker result is the platform-injected `Pcntl\QosClass` enum object.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

/// Returns the exact builtin enum object type exposed by php-src on macOS.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Object("Pcntl\\QosClass".to_string()))
}

builtin! {
    contract: "pcntl_getqos_class",
    check: check,
    semantics: crate::builtins::semantics::pcntl_semantics(
        crate::ir::PcntlRuntime::GetQosClass,
    ),
}
