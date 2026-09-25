//! Purpose:
//! Binds and type-checks PHP's Windows-only code-page conversion builtin.
//!
//! Called from:
//! - `crate::builtins::system::sapi_windows` during builtin inventory registration.
//!
//! Key details:
//! - Code-page selectors accept PHP integers or strings; the subject must be string-compatible.

use crate::builtins::semantics::windows_only_runtime_fn_semantics;
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::ir::RuntimeFnId;
use crate::types::PhpType;

builtin! {
    contract: "sapi_windows_cp_conv",
    check: check_cp_conv,
    semantics: windows_only_runtime_fn_semantics(RuntimeFnId::SapiWindowsCpConv),
}

/// Validates the two code-page selectors and the subject shape used by php-src.
fn check_cp_conv(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    for (index, label) in [(0, "in_codepage"), (1, "out_codepage")] {
        let ty = cx.checker.infer_type(&cx.args[index], cx.env)?.codegen_repr();
        if !matches!(ty, PhpType::Int | PhpType::Str | PhpType::Mixed | PhpType::Union(_)) {
            return Err(CompileError::new(
                cx.span,
                &format!("sapi_windows_cp_conv() ${label} must be an int or string"),
            ));
        }
    }
    let subject = cx.checker.infer_type(&cx.args[2], cx.env)?.codegen_repr();
    if subject != PhpType::Str && subject != PhpType::Mixed {
        return Err(CompileError::new(
            cx.span,
            "sapi_windows_cp_conv() $subject must be a string",
        ));
    }
    Ok(PhpType::Union(vec![PhpType::Str, PhpType::Void]))
}
