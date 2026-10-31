//! Purpose:
//! Implements tokenizer builtins in runtime eval using the shared PHP lexer/parser.
//!
//! Called from:
//! - Declarative direct and evaluated-argument hooks.
//!
//! Key details:
//! - Runtime arrays own their text and nested token tuples through EvalArrayBuilder.
//! - Parse errors leave through the ordinary catchable Throwable channel.

use super::super::*;
use super::collection_builder::EvalArrayBuilder;
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

/// Materializes PHP's array-or-character stream from the shared engine's token offsets.
pub(in crate::interpreter) fn eval_tokenizer_values(
    name: &str, args: &[RuntimeCellHandle], context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if name == "token_name" {
        let [value] = args else { return Err(EvalStatus::RuntimeFatal); };
        let id = tokenizer_int(*value, name, 1, "id", context, values)?;
        return values.string(elephc_tokenizer_engine::token_name(id).unwrap_or("UNKNOWN"));
    }
    let Some(code) = args.first().copied() else { return Err(EvalStatus::RuntimeFatal); };
    let code = tokenizer_source(code, name, context, values)?;
    let flags = match args.get(1) {
        Some(value) => tokenizer_int(*value, name, 2, "flags", context, values)?, None => 0,
    };
    let output = match elephc_tokenizer_engine::tokenize_with_warnings(&code, flags) {
        Ok(output) => output,
        Err(error) => return location::throw_parse_error(&error, context, values),
    };
    for warning in &output.warnings {
        values.warning(&format!("Deprecated: {}\n", warning.message))?;
    }
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
        let output = match elephc_tokenizer_engine::tokenize_with_warnings(&code, flags) {
            Ok(output) => output,
            Err(error) => return location::throw_parse_error(&error, context, values),
        };
        for warning in &output.warnings {
            values.warning(&format!("Deprecated: {}\n", warning.message))?;
        }
        let tokens = output.tokens;
        let mut out = EvalArrayBuilder::indexed(values, tokens.len())?;
        for (index, token) in tokens.iter().enumerate() {
            out.index(index, |values| {
                let mut arguments = Vec::with_capacity(4);
                let mut scope = ElephcEvalScope::new();
                let result = (|| {
                    arguments.push(values.int(token.id)?);
                    arguments.push(values.string_bytes_value(&code[token.pos..token.pos + token.len])?);
                    arguments.push(values.int(token.line as i64)?);
                    arguments.push(values.int(token.pos as i64)?);
                    eval_dynamic_class_new_object(&class,
                        positional_args(arguments.iter().map(|value| value.borrowed()).collect()),
                        context, &mut scope, values)
                })();
                let mut released = Ok(());
                for argument in arguments {
                    let cleanup = release_expr_result(argument, context, values);
                    if released.is_ok() { released = cleanup; }
                }
                let result = match (result, released) {
                    (Ok(object), Err(status)) => {
                        let _ = release_expr_result(object, context, values);
                        Err(status)
                    }
                    (result, _) => result,
                };
                finish_activation_scope(&mut scope, result, context, values)
            })?;
        }
        Ok(out.finish())
    })())
}
