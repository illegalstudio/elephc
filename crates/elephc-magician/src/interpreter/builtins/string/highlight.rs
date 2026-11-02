//! Purpose:
//! Shared implementation of PHP's `highlight_string()` and `highlight_file()`: render through the
//! PHP-exact highlighter, then echo the result or return it.
//!
//! Called from:
//! - `crate::interpreter::builtins::string::highlight_string` and `highlight_file`.
//!
//! Key details:
//! - Both builtins return `string|bool`: `$return = true` answers the rendered HTML, the default
//!   echoes it and answers `true`, and a file that cannot be read warns and answers `false`.
//! - The renderer is `crate::interpreter::highlight`, verified byte for byte against reference PHP.

use super::super::super::*;

/// Evaluates one `highlight_string(...)`/`highlight_file(...)` call from source expressions.
pub(in crate::interpreter) fn eval_highlight_named(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_highlight_values(name, &evaluated, values)
}

/// Applies `highlight_string()`/`highlight_file()` to already-evaluated arguments.
pub(in crate::interpreter) fn eval_highlight_values(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (subject, return_html) = match evaluated_args {
        [subject] => (*subject, false),
        [subject, flag] => (*subject, values.truthy(*flag)?),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    eval_highlight_render(name, subject, return_html, values)
}

/// Renders one subject with PHP's highlighter, then echoes or returns it.
///
/// Shared by the eval builtins and the generated-code bridge, so both backends answer the same
/// union (`string` when `$return` is true, the echoed `true` otherwise, `false` on a read error).
pub(crate) fn eval_highlight_render(
    name: &str,
    subject: RuntimeCellHandle,
    return_html: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let subject = values.string_bytes(subject)?;
    let source = match name {
        "highlight_string" => subject,
        "highlight_file" => {
            let path = String::from_utf8_lossy(&subject).into_owned();
            match std::fs::read(&path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    // php warns twice for an unreadable file: the stream failure, then the
                    // highlighter's own failure, and answers false. The reason is strerror's
                    // wording, so Rust's "(os error N)" suffix is dropped from Display.
                    let reason = error.to_string();
                    let reason = reason.split(" (os error").next().unwrap_or(&reason);
                    values.warning(&format!(
                        "highlight_file({path}): Failed to open stream: {reason}"
                    ))?;
                    values.warning(&format!(
                        "highlight_file(): Failed opening '{path}' for highlighting"
                    ))?;
                    return values.bool_value(false);
                }
            }
        }
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let rendered = crate::interpreter::highlight::highlight_document(&source);
    if return_html {
        return values.string_bytes_value(&rendered);
    }
    let output = values.string_bytes_value(&rendered)?;
    values.echo(output)?;
    values.bool_value(true)
}
