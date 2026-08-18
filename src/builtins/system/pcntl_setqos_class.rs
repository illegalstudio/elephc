//! Purpose:
//! Binds macOS `pcntl_setqos_class` to its typed PCNTL runtime operation.
//!
//! Called from:
//! - `crate::builtins::registry` while collecting AOT builtin homes.
//!
//! Key details:
//! - An omitted argument selects `Pcntl\QosClass::Default`.
//! - Explicit values must use the target-injected enum's exact object type.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::types::PhpType;

const PCNTL_QOS_CLASS: &str = "Pcntl\\QosClass";

/// Validates the optional enum object and returns PHP's `void` result type.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if let Some(value) = cx.args.first() {
        let ty = cx.checker.infer_type(value, cx.env)?;
        let valid = matches!(
            ty.codegen_repr(),
            PhpType::Object(ref name) if php_symbol_key(name) == php_symbol_key(PCNTL_QOS_CLASS)
        );
        if !valid {
            return Err(CompileError::new(
                value.span,
                &format!(
                    "pcntl_setqos_class() parameter $qos_class must be of type Pcntl\\QosClass, {ty:?} given"
                ),
            ));
        }
    }
    Ok(PhpType::Void)
}

builtin! {
    contract: "pcntl_setqos_class",
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::pcntl_semantics(
        crate::ir::PcntlRuntime::SetQosClass,
    ),
}
