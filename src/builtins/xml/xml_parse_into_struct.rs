//! Purpose:
//! Home of PHP's `xml_parse_into_struct()`: the registry builtin that parses a whole
//! document into the `$values` / `$index` output arrays by composing the xml prelude's
//! helpers.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - WHY A REGISTRY BUILTIN. PHP code never predeclares `$values` and `$index`; only a
//!   builtin's checker hook can accept an undefined variable as a write-only output (the
//!   `preg_match()` / `pcntl_wait()` model), so this is the one xml function that is not a
//!   prelude declaration. The work itself is PHP: `__elephc_xml_parse_into_struct()` runs
//!   the parse with the struct accumulator installed, and the two `__elephc_xml_struct_*`
//!   helpers hand the arrays over.
//! - The outputs are typed `mixed` (boxed arrays) after the call; see
//!   `types::checker::inference::expr::effects::xml_struct_output_type`.
//! - `lazy_check: true` so the hook infers only the parser and data arguments; the
//!   output arguments must be variables. Any variable storage works — a frame local, a
//!   `static`, a `global`, a by-reference parameter or `use (&$v)` capture — because the
//!   lowering hands the arrays to `store_operand_local`, which routes by storage kind. A
//!   property or array element is rejected at check time: PHP accepts them, but the
//!   lowering has no way to write back into one, and rejecting there keeps the hook the
//!   single authority on what the lowering can store.

use crate::builtins::semantics::{
    BuiltinArgumentLowering, BuiltinCallablePolicy, BuiltinEffects, BuiltinLowering,
    BuiltinLoweringContext, BuiltinLoweringError, BuiltinRequirements, BuiltinResultOwnership,
    BuiltinResultType, BuiltinRuntimeFunctions, BuiltinSemantics, BuiltinTargetStrategy,
    BuiltinTargetSupport, BuiltinValidation, LoweredBuiltinValue, NormalizedBuiltinCall,
};
use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::ir::{Effects, Immediate, Op, ValueId};
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

builtin! {
    contract: "xml_parse_into_struct",
    check: check,
    lazy_check: true,
    semantics: BuiltinSemantics {
        validation: BuiltinValidation::SignatureOnly,
        result_type: BuiltinResultType::Declared,
        // The parse runs user handlers, which may do anything a PHP call can.
        effects: BuiltinEffects::Static(Effects::all()),
        result_ownership: BuiltinResultOwnership::NonHeap,
        requirements: BuiltinRequirements::Static(&[]),
        target_strategy: BuiltinTargetStrategy::EirGraph,
        target_support: BuiltinTargetSupport::All,
        runtime_functions: BuiltinRuntimeFunctions::None,
        argument_lowering: BuiltinArgumentLowering::Standard,
        callable: BuiltinCallablePolicy::StaticOnly(
            "xml_parse_into_struct() writes by-reference outputs into caller variables",
        ),
        lowering: BuiltinLowering::Eir(lower),
    },
}

/// Returns the expression behind a possibly named argument.
fn argument_value(arg: &Expr) -> &Expr {
    match &arg.kind {
        ExprKind::NamedArg { value, .. } => value,
        _ => arg,
    }
}

/// Returns the parameter an argument binds: its name when named, else by position.
fn parameter_name(arg: &Expr, index: usize) -> Option<String> {
    match &arg.kind {
        ExprKind::NamedArg { name, .. } => Some(php_symbol_key(name)),
        _ => ["parser", "data", "values", "index"]
            .get(index)
            .map(|name| (*name).to_string()),
    }
}

/// Validates the parser and data operands and requires plain variables for the two
/// write-only outputs, which are left uninferred (they may not exist before the call).
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    for (index, arg) in cx.args.iter().enumerate() {
        let parameter = parameter_name(arg, index);
        let value = argument_value(arg);
        match parameter.as_deref() {
            Some("parser") => {
                let ty = cx.checker.infer_type(value, cx.env)?;
                let is_parser = matches!(
                    ty.codegen_repr(),
                    PhpType::Object(ref name) if php_symbol_key(name) == "xmlparser"
                );
                if !is_parser {
                    return Err(CompileError::new(
                        value.span,
                        &format!(
                            "xml_parse_into_struct(): Argument #1 ($parser) must be of type XMLParser, {ty:?} given"
                        ),
                    ));
                }
            }
            Some("data") => {
                let ty = cx.checker.infer_type(value, cx.env)?;
                if ty.codegen_repr() != PhpType::Str {
                    return Err(CompileError::new(
                        value.span,
                        &format!(
                            "xml_parse_into_struct(): Argument #2 ($data) must be of type string, {ty:?} given"
                        ),
                    ));
                }
            }
            Some("values") | Some("index") => {
                if !matches!(value.kind, ExprKind::Variable(_)) {
                    return Err(CompileError::new(
                        value.span,
                        &format!(
                            "xml_parse_into_struct() parameter ${} must be passed a variable",
                            parameter.expect("output parameter name must be present")
                        ),
                    ));
                }
            }
            _ => {
                cx.checker.infer_type(value, cx.env)?;
            }
        }
    }
    Ok(PhpType::Int)
}

/// Runs the prelude's parse helper, then assigns each accumulated array to the caller's
/// output variable. The checker hook guarantees both outputs are variables, so a store
/// that fails is a lowering bug and is reported rather than dropping the array. An
/// omitted `$index` arrives in one of two shapes: positional planning passes no fourth
/// operand at all, and named-argument planning materializes the `null` default; neither
/// has a home, and only the second reaches the store. The helper is told whether `$index`
/// was asked for, so the per-tag position lists are only built when the caller will
/// receive them (php-src's `parser->info` is unset otherwise).
fn lower(
    ctx: &mut dyn BuiltinLoweringContext,
    call: &NormalizedBuiltinCall<'_>,
) -> Result<LoweredBuiltinValue, BuiltinLoweringError> {
    let parser = call.operand(0)?;
    let data = call.operand(1)?;
    let index_requested = call
        .operands
        .get(3)
        .copied()
        .is_some_and(|operand| !is_omitted_output_default(ctx, operand));
    let with_index = ctx.emit_value(
        Op::ConstBool,
        Vec::new(),
        Some(Immediate::Bool(index_requested)),
        PhpType::Bool,
        Op::ConstBool.default_effects(),
        Some(call.span),
    );
    let status = ctx.emit_user_call(
        super::PARSE_INTO_STRUCT_HELPERS[0],
        vec![parser, data, with_index.value],
        PhpType::Int,
        Some(call.span),
    );
    let values_target = call.operand(2)?;
    let values = ctx.emit_user_call(
        super::PARSE_INTO_STRUCT_HELPERS[1],
        vec![parser],
        PhpType::Mixed,
        Some(call.span),
    );
    if !ctx.store_operand_local(values_target, values.value, PhpType::Mixed, Some(call.span)) {
        return Err(output_not_storable("values"));
    }
    if let Some(index_target) = call.operands.get(3).copied() {
        let index = ctx.emit_user_call(
            super::PARSE_INTO_STRUCT_HELPERS[2],
            vec![parser],
            PhpType::Mixed,
            Some(call.span),
        );
        // The helper runs before the operand's home is resolved so the parser drops its
        // copy of the array even for a materialized default.
        if !ctx.store_operand_local(index_target, index.value, PhpType::Mixed, Some(call.span))
            && !is_omitted_output_default(ctx, index_target)
        {
            return Err(output_not_storable("index"));
        }
    }
    Ok(status)
}

/// Returns whether an output operand is the planned default of an omitted argument: the
/// `null` literal, which lowers to a constant typed `Void`. A variable load always resolves
/// to its name in `store_operand_local`, so this is consulted only after that failed.
fn is_omitted_output_default(ctx: &dyn BuiltinLoweringContext, operand: ValueId) -> bool {
    matches!(ctx.value_php_type(operand), PhpType::Void)
}

/// The lowering error for an output the checker admitted but no storage kind can take.
fn output_not_storable(parameter: &str) -> BuiltinLoweringError {
    BuiltinLoweringError::new(format!(
        "xml_parse_into_struct() parameter ${parameter} must be a variable the lowering can store into"
    ))
}
