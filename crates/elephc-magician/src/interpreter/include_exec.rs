//! Purpose:
//! Executes nested `eval(...)`, include, include_once, require, and require_once expressions.
//! This keeps source-file loading and PHP open/close-tag handling outside the core interpreter loop.
//!
//! Called from:
//! - `crate::interpreter::eval_positional_expr_call()` for `eval(...)`.
//! - `crate::interpreter::eval_expr()` for include/require expression nodes.
//!
//! Key details:
//! - Included code runs against the current eval context and materialized scope.
//! - Missing include emits a warning and returns false; missing require is fatal.
//! - An include's file is loaded through `crate::script_cache`, which returns the
//!   already-segmented, already-parsed form when the OPcache cache is enabled. The
//!   replay below must therefore stay an exact mirror of `segments::segment_script`:
//!   same alternation, same "a file ending inside PHP emits no trailing output".

use super::*;
use crate::parse_cache::parse_fragment_cached;
use crate::script_cache::{load_script, ScriptSegment};

/// Evaluates nested `eval(...)` calls against the current materialized scope.
pub(super) fn eval_nested_eval(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [code] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let code = eval_expr(code, context, scope, values)?;
    let code = values.string_bytes(code)?;
    let program = parse_fragment_cached(&code).map_err(EvalParseError::status)?;
    execute_program_with_context(context, program.as_ref(), scope, values)
}

/// Evaluates an eval-fragment include or require expression.
pub(super) fn eval_include_expr(
    path: &EvalExpr,
    required: bool,
    once: bool,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_expr(path, context, scope, values)?;
    let path = eval_path_string(path, values)?;
    let resolved_path = eval_resolve_include_path(&path, context);
    let include_key = eval_include_key(&resolved_path);
    if once && context.has_included_file(&include_key) {
        return values.bool_value(true);
    }
    let segments = match load_script(&resolved_path) {
        Ok(segments) => segments,
        Err(_) => return eval_include_missing_file(&path, required, values),
    };
    context.mark_included_file(include_key);
    eval_replay_include_segments(&segments, &resolved_path, context, scope, values)
}

/// Returns the include/require result for a file that cannot be opened.
fn eval_include_missing_file(
    path: &str,
    required: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let construct = if required { "require" } else { "include" };
    values.warning(&format!(
        "Warning: {construct}({path}): Failed to open stream: No such file or directory\n"
    ))?;
    values.warning(&format!(
        "Warning: {construct}(): Failed opening '{path}' for inclusion\n"
    ))?;
    if required {
        Err(EvalStatus::RuntimeFatal)
    } else {
        values.bool_value(false)
    }
}

/// Resolves eval include paths using PHP's cwd-first and caller-directory fallback.
fn eval_resolve_include_path(path: &str, context: &ElephcEvalContext) -> std::path::PathBuf {
    let raw_path = std::path::Path::new(path);
    if raw_path.is_absolute() || raw_path.exists() {
        return raw_path.to_path_buf();
    }
    if context.call_dir().is_empty() {
        return raw_path.to_path_buf();
    }
    let caller_path = std::path::Path::new(context.call_dir()).join(raw_path);
    if caller_path.exists() {
        caller_path
    } else {
        raw_path.to_path_buf()
    }
}

/// Builds the stable include_once key for a resolved path.
fn eval_include_key(path: &std::path::Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

/// Replays an included file's cached segments, alternating raw output and code blocks.
///
/// The segment list already encodes what the old inline scan computed per include:
/// a file that ended inside a code block simply has no trailing output segment, so
/// the uniform `int(1)` here reproduces that path's early return.
fn eval_replay_include_segments(
    segments: &[ScriptSegment],
    path: &std::path::Path,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    for segment in segments {
        let program = match segment {
            ScriptSegment::Output(bytes) => {
                eval_echo_include_bytes(bytes, values)?;
                continue;
            }
            // Parse failures are stored rather than raised when the file is segmented,
            // so that a block only fails once replay actually reaches it — after the
            // earlier blocks have run and produced their output.
            ScriptSegment::ParseError(error) => return Err(error.clone().status()),
            ScriptSegment::Code(program) => program,
        };
        match eval_execute_include_program(program, path, context, scope, values)? {
            EvalControl::None => {}
            EvalControl::ReturnVoid => return values.null(),
            EvalControl::Return(value) => return Ok(value),
            EvalControl::Throw(value) => {
                context.set_pending_throw(value);
                return Err(EvalStatus::UncaughtThrowable);
            }
            EvalControl::Break | EvalControl::Continue => {
                return Err(EvalStatus::UnsupportedConstruct);
            }
        }
    }
    values.int(1)
}

/// Executes one already-parsed PHP code block from an included file.
///
/// Parsing moved to `crate::script_cache::segments`, so a warm include reaches this
/// with no lexing, no parsing and no source hashing left to do.
fn eval_execute_include_program(
    program: &EvalProgram,
    path: &std::path::Path,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    let previous = context.call_site();
    let file = path.to_string_lossy().into_owned();
    let dir = path
        .parent()
        .map(|parent| parent.to_string_lossy().into_owned())
        .unwrap_or_default();
    context.set_call_site(file.clone(), dir, 1);
    context.set_file_magic_override(Some(file));
    let result = execute_statements(program.statements(), context, scope, values);
    context.set_call_site(previous.0, previous.1, previous.2);
    context.set_file_magic_override(previous.3);
    result
}

/// Echoes raw non-PHP include bytes through the eval value hooks.
fn eval_echo_include_bytes(
    bytes: &[u8],
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if bytes.is_empty() {
        return Ok(());
    }
    let output = values.string_bytes_value(bytes)?;
    values.echo(output)
}
