//! Purpose:
//! Recovers reported submission failures and formats successful interactive eval results.
//!
//! Called from:
//! - `crate::ffi::execute` for the outer REPL submission only.
//!
//! Key details:
//! - Uses eval's existing var_dump semantics, including objects and recursive values.
//! - Preserves result ownership on success and releases it if formatting fails.
//! - Recovery runs after interpreter cleanup; ABI guards and panic recovery stay fatal.

use super::{eval_throw_builtin_exception, eval_var_dump_result, ElephcEvalContext, EvalOutcome, EvalStatus, RuntimeValueOps};

/// Converts reported eval failures to the host's Throwable path after execution has unwound.
pub(crate) fn finish_submission(
    outcome: Result<EvalOutcome, EvalStatus>,
    display: bool,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    let outcome = outcome.and_then(|outcome| {
        if display { display_result(outcome, context, values) } else { Ok(outcome) }
    });
    let status = match outcome {
        Ok(outcome) => return Ok(outcome),
        Err(status) => status,
    };
    let message = match status {
        EvalStatus::ParseError => "eval() fragment is invalid",
        EvalStatus::RuntimeFatal => "eval() runtime failed",
        EvalStatus::UnsupportedConstruct => "eval() fragment uses an unsupported construct",
        EvalStatus::UserFatal => {
            // trigger_error has already reported the user's message. Do not print it twice.
            crate::repl::__elephc_repl_failed();
            return values.null().map(EvalOutcome::Value);
        }
        // In particular, PCNTL escape rejection cannot safely write the scope back to AOT.
        EvalStatus::Ok | EvalStatus::UncaughtThrowable | EvalStatus::AbiMismatch
        | EvalStatus::EscapingPcntlCallable => return Err(status),
    };
    if let Some(error) = context.take_pending_throw() {
        return Ok(EvalOutcome::Throwable(error));
    }
    let status = eval_throw_builtin_exception::<()>("Error", message, context, values)
        .expect_err("creating a Throwable always returns an error status");
    if status == EvalStatus::UncaughtThrowable {
        context.take_pending_throw().map(EvalOutcome::Throwable).ok_or(status)
    } else { Err(status) }
}

/// Displays a successful value and preserves catchable exceptions raised during formatting.
pub(crate) fn display_result(
    outcome: EvalOutcome,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalOutcome, EvalStatus> {
    let EvalOutcome::Value(result) = outcome else { return Ok(outcome); };
    match eval_var_dump_result(&[result], context, values) {
        Ok(null) => {
            values.release(null)?;
            Ok(EvalOutcome::Value(result))
        }
        Err(status) => {
            values.release(result)?;
            if status == EvalStatus::UncaughtThrowable {
                context.take_pending_throw().map(EvalOutcome::Throwable).ok_or(status)
            } else { Err(status) }
        }
    }
}
