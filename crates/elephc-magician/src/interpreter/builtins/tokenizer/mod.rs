//! Purpose:
//! Implements tokenizer builtins in runtime eval using the shared PHP lexer/parser.
//!
//! Called from:
//! - Declarative direct and evaluated-argument hooks.
//!
//! Key details:
//! - Runtime arrays own their text and nested token tuples through EvalArrayBuilder.
//! - Parse errors leave through the ordinary catchable Throwable channel.

use elephc_chopper::syntax::php_tokenizer;

use super::super::*;
use super::collection_builder::EvalArrayBuilder;
use crate::runtime_hooks::owner_frames::{
    RuntimeOwnerSlot, StagedRuntimeOwners, with_runtime_owners,
};
mod arguments;
mod location;
mod token_get_all;
mod token_name;
pub(in crate::interpreter) use arguments::{normalize_tokenizer_native_arguments, tokenizer_native_binding_signature};
use arguments::{tokenizer_int, tokenizer_source};

/// Evaluates source arguments in order before dispatching one tokenizer builtin.
pub(in crate::interpreter) fn eval_builtin_tokenizer(
    name: &str, args: &[EvalExpr], context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let operands = args.iter().collect::<Vec<_>>();
    with_eval_operands(&operands, context, scope, values, |args, context, _scope, values| {
        eval_tokenizer_values(name, args, context, values)
    })
}

/// Emits each non-fatal tokenizer diagnostic through PHP's error-handler channel.
///
/// The tokenizer's severity is PHP's own (`E_WARNING` for `declare(encoding=...)`, otherwise
/// `E_DEPRECATED`), and the reported line is the line inside the scanned source. Routing
/// through the shared user-error dispatcher keeps a throwing handler catchable by the
/// `try`/`catch` around `eval()`.
fn report_tokenizer_warnings(
    warnings: &[php_tokenizer::Warning],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    for warning in warnings {
        eval_dispatch_php_error_at(
            &warning.message,
            warning.severity,
            Some(""),
            Some(warning.line),
            context,
            values,
        )?;
    }
    Ok(())
}

/// Materializes PHP's array-or-character stream from the shared engine's token offsets.
pub(in crate::interpreter) fn eval_tokenizer_values(
    name: &str, args: &[RuntimeCellHandle], context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if name == "token_name" {
        let [value] = args else { return Err(EvalStatus::RuntimeFatal); };
        let id = tokenizer_int(*value, name, 1, "id", context, values)?;
        return values.string(php_tokenizer::token_name(id).unwrap_or("UNKNOWN"));
    }
    let Some(code) = args.first().copied() else { return Err(EvalStatus::RuntimeFatal); };
    let code = tokenizer_source(code, name, context, values)?;
    let flags = match args.get(1) {
        Some(value) => tokenizer_int(*value, name, 2, "flags", context, values)?, None => 0,
    };
    let output = match php_tokenizer::tokenize_with_warnings(&code, flags) {
        Ok(output) => output,
        Err(error) => {
            // PHP emits the diagnostics that precede the failure, then throws.
            report_tokenizer_warnings(&error.warnings, context, values)?;
            return location::throw_parse_error(&error, context, values);
        }
    };
    report_tokenizer_warnings(&output.warnings, context, values)?;
    let tokens = output.tokens;
    let mut out = EvalArrayBuilder::indexed(values, tokens.len())?;
    for (index, token) in tokens.iter().enumerate() {
        let text = &code[token.pos..token.pos + token.len];
        out.index(index, |values| {
            if token.id < 256 { return values.string_bytes_value(text); }
            let mut row = EvalArrayBuilder::indexed(values, 3)?;
            row.index(0, |values| values.int(token.id))?;
            row.index(1, |values| values.string_bytes_value(text))?;
            row.index(2, |values| values.int(token.line as i64))?;
            Ok(row.finish())
        })?;
    }
    Ok(out.finish())
}

/// Materializes eval-declared PhpToken subclasses through the dynamic object registry.
pub(in crate::interpreter) fn eval_tokenizer_static_method(
    owner: &str, method: &str, called_class: &str, args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Option<Result<RuntimeCellHandle, EvalStatus>> {
    if !owner.trim_start_matches('\\').eq_ignore_ascii_case("PhpToken")
        || !method.eq_ignore_ascii_case("tokenize") { return None; }
    let class = context.class(called_class)?.clone();
    Some((|| {
        if class.is_abstract() {
            return eval_throw_error(&format!("Cannot instantiate abstract class {called_class}"), context, values);
        }
        let code = values.string_bytes(*args.first().ok_or(EvalStatus::RuntimeFatal)?)?;
        let flags = match args.get(1) {
            Some(value) => tokenizer_int(*value, "PhpToken::tokenize", 2, "flags", context, values)?,
            None => 0,
        };
        let output = match php_tokenizer::tokenize_with_warnings(&code, flags) {
            Ok(output) => output,
            Err(error) => {
                report_tokenizer_warnings(&error.warnings, context, values)?;
                return location::throw_parse_error(&error, context, values);
            }
        };
        report_tokenizer_warnings(&output.warnings, context, values)?;
        let tokens = output.tokens;
        let mut out = EvalArrayBuilder::indexed(values, tokens.len())?;
        for (index, token) in tokens.iter().enumerate() {
            out.index(index, |values| {
                use elephc_memoir::runtime_owners::{CleanupErrorPolicy, ReleaseOrder};
                let arguments = StagedRuntimeOwners::default();
                let output = RuntimeOwnerSlot::empty();
                let mut scope = ElephcEvalScope::new();
                with_runtime_owners(&(&arguments, &output), || {
                    let operation = (|| {
                        let id = arguments.stage(values.int(token.id)?);
                        let text = arguments.stage(values.string_bytes_value(
                            &code[token.pos..token.pos + token.len],
                        )?);
                        let line = arguments.stage(values.int(token.line as i64)?);
                        let position = arguments.stage(values.int(token.pos as i64)?);
                        let result = eval_dynamic_class_new_object(
                            &class,
                            positional_args(vec![
                                id.borrowed(),
                                text.borrowed(),
                                line.borrowed(),
                                position.borrowed(),
                            ]),
                            context,
                            &mut scope,
                            values,
                        )?;
                        output.replace(Some(result));
                        Ok(())
                    })();
                    let cleanup = arguments.take_all().release(
                        ReleaseOrder::Forward,
                        CleanupErrorPolicy::First,
                        |value| release_expr_result(value, context, values),
                    );
                    let result = output.finish(operation.and(cleanup), |value| {
                        release_expr_result(value, context, values)
                    });
                    finish_activation_scope(&mut scope, result, context, values)
                })
            })?;
        }
        Ok(out.finish())
    })())
}
