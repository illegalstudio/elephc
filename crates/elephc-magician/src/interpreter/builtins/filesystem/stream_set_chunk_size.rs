//! Purpose:
//! Declarative eval registry entry for `stream_set_chunk_size`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the stream chunk-size metadata helper.

eval_builtin! {
    contract: "stream_set_chunk_size",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;

/// Dispatches direct eval calls for the `stream_set_chunk_size` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_set_chunk_size_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_stream_set_buffer_like("stream_set_chunk_size", args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `stream_set_chunk_size` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_stream_set_chunk_size_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [stream, size] => eval_stream_set_buffer_like_result("stream_set_chunk_size", *stream, *size, context, values),
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates chunk/read/write buffer setting builtins.
pub(in crate::interpreter) fn eval_builtin_stream_set_buffer_like(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [stream, size] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let stream = eval_expr(stream, context, scope, values)?;
    let size = eval_expr(size, context, scope, values)?;
    eval_stream_set_buffer_like_result(name, stream, size, context, values)
}

/// php-src's verbatim `ValueError` wording for a non-positive `stream_set_chunk_size()` `$size`.
const STREAM_SET_CHUNK_SIZE_NON_POSITIVE_MESSAGE: &str =
    "stream_set_chunk_size(): Argument #2 ($size) must be greater than 0";

/// php's answer for a `stream_set_write_buffer()` on a stream that is NOT a userspace wrapper.
///
/// MEASURED on `php -n` 8.5.6 — `-1` for a plain file, `php://memory` and `php://temp` alike,
/// because `_php_stream_set_option()` has no generic fallback for `PHP_STREAM_OPTION_WRITE_BUFFER`
/// and `stream_set_write_buffer()` maps the resulting `NOTIMPL` to `EOF`:
///
/// ```text
/// stream_set_write_buffer(fopen("php://memory","r+"), 0) int(-1)
/// stream_set_write_buffer(fopen($tmp,"w+"), 8192)        int(-1)
/// stream_set_read_buffer(fopen("php://memory","r+"), 0)  int(0)
/// ```
///
/// The READ buffer differs because `_php_stream_set_option()` DOES carry a generic fallback for
/// it, flipping `PHP_STREAM_FLAG_NO_BUFFER` and answering OK. Answering `0` for both — what this
/// used to do — reported a write-buffer change that never happened. The compiled backend has
/// carried the same split since the wrapper dispatch landed.
const EVAL_NATIVE_WRITE_BUFFER_RESULT: i64 = -1;

/// php's answer for a `stream_set_read_buffer()` on a stream that is NOT a userspace wrapper.
const EVAL_NATIVE_READ_BUFFER_RESULT: i64 = 0;

/// Applies a materialized chunk/read/write buffer setting.
pub(in crate::interpreter) fn eval_stream_set_buffer_like_result(
    name: &str,
    stream: RuntimeCellHandle,
    size: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let id = eval_stream_resource_id(stream, values)?;
    let size = eval_int_value(size, values)?;
    match name {
        "stream_set_chunk_size" => {
            // php-src rejects a non-positive chunk size before touching the stream; this used
            // to answer `false`, which is not even in the declared `int` return type.
            if size < 1 {
                return eval_stream_value_error(
                    STREAM_SET_CHUNK_SIZE_NON_POSITIVE_MESSAGE,
                    context,
                    values,
                );
            }
            match context.stream_resources_mut().set_chunk_size(id, size) {
                Some(previous) => values.int(previous),
                None => values.bool_value(false),
            }
        }
        "stream_set_read_buffer" => match context.stream_resources().set_buffer(id, size) {
            Some(()) => values.int(EVAL_NATIVE_READ_BUFFER_RESULT),
            None => values.bool_value(false),
        },
        "stream_set_write_buffer" => match context.stream_resources().set_buffer(id, size) {
            Some(()) => values.int(EVAL_NATIVE_WRITE_BUFFER_RESULT),
            None => values.bool_value(false),
        },
        _ => Err(EvalStatus::RuntimeFatal),
    }
}
