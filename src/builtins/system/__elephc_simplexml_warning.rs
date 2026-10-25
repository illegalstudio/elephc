//! Purpose:
//! Gives the compiled SimpleXML prelude access to ordinary PHP warnings.
//!
//! Called from:
//! - SimpleXMLElement::xpath after rejecting a scalar XPath result.
//!
//! Key details:
//! - The existing diagnostic EIR target preserves E_WARNING, handlers, and suppression.
//! - Eval invokes the same compiled prelude methods, as with XML object allocation.

use crate::builtins::semantics::{
    internal_eir_semantics, BuiltinLoweringContext, BuiltinLoweringError,
    BuiltinResultOwnership, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::ir::{Effects, Immediate, Op, RuntimeCallTarget};
use crate::types::PhpType;

builtin! {
    contract: "__elephc_simplexml_warning",
    semantics: internal_eir_semantics(lower, Effects::all(), BuiltinResultOwnership::NonHeap),
}

/// Dispatches the complete warning before returning the internal success sentinel.
fn lower(ctx: &mut dyn BuiltinLoweringContext, call: &NormalizedBuiltinCall<'_>)
    -> Result<LoweredBuiltinValue, BuiltinLoweringError>
{
    ctx.emit_void(Op::RuntimeCall, vec![call.operand(0)?],
        Some(Immediate::RuntimeCall(RuntimeCallTarget::WriteDiagnostic)),
        Effects::all(), Some(call.span));
    Ok(ctx.emit_value(Op::ConstBool, vec![], Some(Immediate::Bool(true)),
        PhpType::Bool, Effects::PURE, Some(call.span)))
}
