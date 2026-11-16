//! Purpose:
//! Constructs tokenizer ParseErrors with their scanned source coordinates.
//!
//! Called from:
//! - Both tokenizer eval entry points after the shared parser rejects input.
//!
//! Key details:
//! - The versioned runtime operation stamps metadata before publishing the exception owner.

use super::*;

eval_builtin! {
    contract: "__elephc_tokenizer_error_location",
    area: String,
    direct: none,
    values: none,
}

/// Constructs a catchable ParseError and preserves the parser's source-buffer location.
pub(super) fn throw_parse_error<T>(
    error: &elephc_tokenizer_engine::ParseError,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    let exception = values.new_object("ParseError")?;
    let constructed = (|| {
        let message = values.string(&error.message)?;
        with_eval_value_lease(message, context, values, |message, context, values| {
            let code = values.int(0)?;
            with_eval_value_lease(code, context, values, |code, _, values| {
                values.construct_object(exception, vec![message, code])
            })
        })?;
        let line = values.int(error.line as i64)?;
        with_eval_value_lease(line, context, values, |line, _, values| {
            let result = values.runtime_builtin_call(
                elephc_builtin_contract::RuntimeBuiltinId::TokenizerErrorLocation,
                &[exception.borrowed(), line],
            )?;
            // Embedders without native Throwable metadata retain their catchable error channel.
            match result { Some(result) => values.release(result), None => Ok(()) }
        })
    })();
    if let Err(status) = constructed {
        let _ = eval_release_value(context, values, exception);
        return Err(status);
    }
    context.set_pending_throw(exception);
    Err(EvalStatus::UncaughtThrowable)
}
