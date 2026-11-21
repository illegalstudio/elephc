//! Purpose:
//! Home of the PHP `proc_get_status` builtin: shared-contract binding and AOT semantics.
//!
//! Called from:
//! - `crate::builtins::registry` through the inventory submission below.
//!
//! Key details:
//! - The prelude's `__elephc_proc_get_status` builds php-src 8.5's nine-key array,
//!   including the `cached` exit-status flag.

use crate::builtins::semantics::{
    callable_accepts_any_source, BuiltinArgumentLowering, BuiltinCallablePolicy,
    BuiltinResultOwnership,
};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "proc_get_status",
    check: check,
    semantics: super::semantics(
        super::status_result,
        BuiltinResultOwnership::Fresh,
        BuiltinArgumentLowering::PreserveUncoerced,
        BuiltinCallablePolicy::Dynamic(callable_accepts_any_source),
    ),
}

/// Refuses sandboxed targets and answers the string-keyed status array.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    super::check_spawning(cx, PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    })
}
