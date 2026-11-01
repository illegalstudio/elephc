//! Purpose:
//! Home of the PHP `get_class_methods` builtin: its registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The result is a boxed `Mixed` array: a class the compiled world does not know (declared by
//!   `eval` or a runtime include) is answered by the eval bridge, whose result is a Mixed cell.
//! - Visibility follows the calling scope at run time (see `RuntimeFnId::GetClassMethods`).

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "get_class_methods",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::GetClassMethods,
    ),
}

/// Requires an object or a class-name string and returns the method-name list as `Mixed`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if !matches!(
        ty.codegen_repr(),
        PhpType::Object(_) | PhpType::Str | PhpType::Mixed | PhpType::Union(_)
    ) {
        return Err(CompileError::new(
            cx.span,
            "get_class_methods(): Argument #1 ($object_or_class) must be an object or a valid class name",
        ));
    }
    Ok(PhpType::Mixed)
}
