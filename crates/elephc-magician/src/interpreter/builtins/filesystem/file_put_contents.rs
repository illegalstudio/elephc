//! Purpose:
//! Declarative eval registry entry for `file_put_contents`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem`.
//!
//! Key details:
//! - Runtime dispatch is declared here and delegated through the one-shot file write helper.
//! - `$context` is accepted and ignored; array `$data` is written as its values concatenated, as
//!   php does.
//! - `$flags` honours `FILE_APPEND` (8) and `LOCK_EX` (2), matching the AOT runtime (Symfony's
//!   `SodiumVault` writes its keys with `LOCK_EX`). The lock
//!   is advisory and released when the handle drops, which is the same lifetime `close()`
//!   gives the compiled path.
//! - `LOCK_EX` WITHOUT `FILE_APPEND` opens without truncating and truncates once the lock is
//!   held, which is php-src's `'c'` mode. Truncating at open would destroy a previous
//!   writer's contents while this writer is still waiting for the lock.

eval_builtin! {
    contract: "file_put_contents",
    area: Filesystem,
    direct: Filesystem,
    values: Filesystem,
}

use super::super::super::*;
use super::*;
use crate::stream_wrappers;

/// Dispatches direct eval calls for the `file_put_contents` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_file_put_contents_declared_call(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    eval_builtin_file_put_contents(args, context, scope, values)
}

/// Dispatches evaluated-argument calls for the `file_put_contents` filesystem builtin through the area dispatcher.
pub(in crate::interpreter) fn eval_file_put_contents_declared_values_result(
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match evaluated_args {
        [filename, data] => eval_file_put_contents_result(*filename, *data, 0, context, values),
        [filename, data, flags] | [filename, data, flags, _] => {
            let flags = eval_int_value(*flags, values)?;
            eval_file_put_contents_result(*filename, *data, flags, context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP `file_put_contents($filename, $data, $flags = 0, $context = null)`.
pub(in crate::interpreter) fn eval_builtin_file_put_contents(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !(2..=4).contains(&args.len()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated = Vec::with_capacity(args.len());
    for arg in args {
        evaluated.push(eval_expr(arg, context, scope, values)?);
    }
    eval_file_put_contents_declared_values_result(&evaluated, context, values)
}

/// Writes a PHP string to a local file or supported wrapper and returns a byte count.
pub(in crate::interpreter) fn eval_file_put_contents_result(
    filename: RuntimeCellHandle,
    data: RuntimeCellHandle,
    flags: i64,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(filename, values)?;
    let data = if values.is_array_like(data)? {
        let separator = values.string("")?;
        let joined = eval_implode_result(separator, data, context, values)?;
        values.string_bytes(joined)?
    } else {
        values.string_bytes(data)?
    };
    if stream_wrappers::is_phar_stream(&path) {
        // A phar entry is rewritten whole, so FILE_APPEND has nothing to append to and
        // LOCK_EX has no descriptor to lock. Report failure rather than truncating what the
        // caller asked to extend, matching the compiled path.
        if flags != 0 {
            return values.bool_value(false);
        }
        return match elephc_phar::put_url_bytes(path.as_bytes(), &data) {
            Some(len) => values.int(i64::try_from(len).map_err(|_| EvalStatus::RuntimeFatal)?),
            None => values.bool_value(false),
        };
    }
    if let Some(result) =
        eval_user_wrapper_file_put_contents_result(&path, &data, context, values)?
    {
        return Ok(result);
    }
    let Some(path) = stream_wrappers::local_filesystem_path(&path) else {
        return values.bool_value(false);
    };
    let written = if flags & (EVAL_FILE_APPEND | EVAL_LOCK_EX) == 0 {
        std::fs::write(path, &data)
    } else {
        eval_write_with_flags(&path, &data, flags)
    };
    match written {
        Ok(()) => values.int(i64::try_from(data.len()).map_err(|_| EvalStatus::RuntimeFatal)?),
        Err(_) => values.bool_value(false),
    }
}

/// PHP's `FILE_APPEND`: extend the file instead of truncating it.
const EVAL_FILE_APPEND: i64 = 8;

/// PHP's `LOCK_EX`: hold an exclusive advisory lock for the duration of the write.
const EVAL_LOCK_EX: i64 = 2;

/// Writes one file honouring `FILE_APPEND` and `LOCK_EX`, in php-src's order.
///
/// `LOCK_EX` without `FILE_APPEND` opens with neither truncation nor append and truncates
/// only once the lock is held — php-src's `'c'` mode. A REFUSED lock is a failed write, not
/// an unlocked one.
fn eval_write_with_flags(path: &str, data: &[u8], flags: i64) -> std::io::Result<()> {
    use std::io::Write;

    let appending = flags & EVAL_FILE_APPEND != 0;
    let locking = flags & EVAL_LOCK_EX != 0;
    let mut options = std::fs::OpenOptions::new();
    options.create(true).write(true);
    if appending {
        options.append(true);
    } else if !locking {
        options.truncate(true);
    }
    let mut file = options.open(path)?;
    if locking {
        lock_exclusive(&file)?;
        if !appending {
            // A refused truncation is a failed write, not a silent one: writing over a file
            // that could not be emptied reports a byte count and leaves a stale tail. The
            // compiled runtime reports it the same way.
            file.set_len(0)?;
        }
    }
    file.write_all(data)
}

/// Takes an exclusive advisory lock, blocking until it is held.
#[cfg(unix)]
fn lock_exclusive(file: &std::fs::File) -> std::io::Result<()> {
    use std::os::unix::io::AsRawFd;
    // SAFETY: the descriptor is owned by `file` and outlives the call.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } == 0 {
        return Ok(());
    }
    Err(std::io::Error::last_os_error())
}

/// Non-Unix targets have no `flock`; the write proceeds unlocked, as the AOT runtime does.
#[cfg(not(unix))]
fn lock_exclusive(_file: &std::fs::File) -> std::io::Result<()> {
    Ok(())
}
