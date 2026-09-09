//! Purpose:
//! Home of PHP's `xml_set_end_namespace_decl_handler()`: installs the end namespace declaration handler, typing an unannotated
//! closure's parameters from the event it receives.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - Lowers to the xml prelude's `__elephc_xml_set_end_namespace_decl_handler()` twin; see `super::handler_setters`.

use super::handler_setters::{self, SetterSpec};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

/// The prelude twin and the handler shapes this setter installs.
pub(super) const SPEC: SetterSpec = SetterSpec {
    helper: "__elephc_xml_set_end_namespace_decl_handler",
    handlers: &[handler_setters::PARSER_AND_STRING],
};

/// PHP parameter names, for named-argument binding in the checker hook.
const PARAMETERS: &[&str] = &["parser", "handler"];

builtin! {
    contract: "xml_set_end_namespace_decl_handler",
    check: check,
    lazy_check: true,
    semantics: handler_setters::setter_semantics(lower),
}

/// Validates the parser and types the handler closures (see `handler_setters::check_setter`).
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    handler_setters::check_setter(cx, PARAMETERS, &SPEC)
}

/// Lowers to the prelude twin (see `handler_setters::lower_setter`).
fn lower(
    ctx: &mut dyn crate::builtins::semantics::BuiltinLoweringContext,
    call: &crate::builtins::semantics::NormalizedBuiltinCall<'_>,
) -> Result<crate::builtins::semantics::LoweredBuiltinValue, crate::builtins::semantics::BuiltinLoweringError> {
    handler_setters::lower_setter(ctx, call, &SPEC)
}
