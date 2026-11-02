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
//! - An include's file is loaded through `crate::script_cache`, which returns the whole
//!   file already parsed as one program (`segments::parse_script`), from the cache when
//!   the OPcache cache is enabled. Inline HTML is part of that program as `echo`
//!   statements, so the replay below only runs it.
//! - Whole included programs share strict-mode checks and template hoisting with direct eval.

use crate::context::EvalCallFrame;
use super::*;
use crate::parse_cache::{parse_fragment_cached, parse_source_file_at_path, parse_source_file_cached};
use crate::script_cache::{load_script, ScriptSegment};


#[cfg(all(test, unix))]
mod source_identity_tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt;

    #[test]
    fn include_keys_preserve_distinct_non_utf8_paths() {
        let a = std::path::PathBuf::from(std::ffi::OsString::from_vec(b"/nonexistent-source/\xff.php".to_vec()));
        let b = std::path::PathBuf::from(std::ffi::OsString::from_vec(b"/nonexistent-source/\xfe.php".to_vec()));
        assert_eq!(a.to_string_lossy(), b.to_string_lossy());
        let first = eval_include_key(&a);
        let second = eval_include_key(&b);
        assert_ne!(first, second, "physical identity must not use lossy display text");
        let mut context = ElephcEvalContext::new();
        context.mark_included_file(first.clone());
        assert!(context.has_included_file(&first));
        assert!(!context.has_included_file(&second));
    }
}

/// Copies and releases the source operand before parsing nested eval in the current scope.
pub(super) fn eval_nested_eval(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [code] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let source = eval_owned_expr(code, context, scope, values)?;
    let code = values.string_bytes(source);
    let released = eval_release_value(context, values, source);
    let code = code.and_then(|code| released.map(|()| code))?;
    let program = parse_fragment_cached(&code).map_err(|diagnostic| {
        let (file, _, line, _) = context.call_site();
        report_fatal_diagnostic(&diagnostic.eval_message(&file, line));
        diagnostic.status()
    })?;
    // php describes `eval()` as a frame of its own wherever it is written. The bridge entry point
    // pushes one for an eval called from compiled code; an eval written INSIDE interpreted code
    // comes through here instead and pushed none, so it was missing from `debug_backtrace()` and
    // from the declaring-file question a class declared inside it has to answer.
    let frame = EvalCallFrame::function("eval", None, context);
    context.push_call_frame(frame);
    context.push_eval_backtrace_boundary();
    context.push_include_execution(false);
    // Code inside `eval()` is not IN the file that called it, and php spells that:
    // `FILE(LINE) : eval()'d code`. `eval_file_magic()` builds exactly that string, but only
    // when no `__FILE__` override is set -- and an include sets one, to its own path. Left in
    // place it made an `eval()` inside an included file report the include's plain path, so a
    // class declared there had the wrong `getFileName()`. Cleared for the eval and put back
    // after, because the including file's own `__FILE__` is still its path.
    let previous_file_magic = context.call_site().3;
    context.set_file_magic_override(None);
    let previous_site = context.enter_eval_code_site();
    let result = execute_program_with_context(context, program.as_ref(), scope, values);
    context.leave_eval_code_site(previous_site);
    context.set_file_magic_override(previous_file_magic);
    context.pop_include_execution();
    context.pop_eval_backtrace_boundary();
    context.pop_call_frame();
    result
}

/// Evaluates an eval-fragment include or require expression whose VALUE the program uses.
pub(super) fn eval_include_expr(
    path: &EvalExpr,
    required: bool,
    once: bool,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_expr(path, context, scope, values)?;
    eval_include_value_with_use(path, required, once, true, context, scope, values)
}

/// Evaluates an include or require written as a STATEMENT, whose value is discarded.
///
/// The distinction is what makes skipping a compiler-included source safe -- see
/// [`eval_include_value_with_use`].
pub(super) fn eval_discarded_include_expr(
    path: &EvalExpr,
    required: bool,
    once: bool,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_expr(path, context, scope, values)?;
    eval_include_value_with_use(path, required, once, false, context, scope, values)
}

/// Shared include driver; `value_used` says whether the program reads what the include evaluates to.
///
/// A plain `include`/`require` of a source the COMPILER already included must not run the file
/// again. The autoload pass opened it at compile time and spliced its whole top-level into the
/// program -- eager `autoload.files` entries run before the entry body, in Composer's order -- so
/// executing the body a second time is DUPLICATE EXECUTION, not php's re-inclusion semantics:
/// every function it declares is redeclared, every `$GLOBALS` counter it bumps is bumped twice.
/// A Symfony `--web` worker hit this 12 times per request through Composer's own
/// `require $file;` loop, which is a plain `require` guarded only by a `$GLOBALS` flag the
/// compile-time inclusion never sets.
///
/// The skip is limited to a DISCARDED value, and that limit is not cosmetic. php hands back the
/// file's `return` value, and real code reads it: `vendor/autoload_runtime.php` writes
/// `if (true === (require_once __DIR__.'/autoload.php'))` and a routing dump is loaded as
/// `$routes = require $file;`. Answering `true` for those would silently change the program.
/// Where the value is thrown away there is nothing left to observe but the side effects, and
/// those already happened.
pub(super) fn eval_include_value_with_use(
    path: RuntimeCellHandle,
    required: bool,
    once: bool,
    value_used: bool,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let path = eval_path_string(path, values)?;
    if let Some(result) =
        eval_include_through_user_wrapper(&path, required, once, context, scope, values)?
    {
        return Ok(result);
    }
    let resolved_path = eval_resolve_include_path(&path, context);
    let outcome = eval_resolved_include(
        &path,
        &resolved_path,
        required,
        once,
        value_used,
        context,
        scope,
        values,
    );
    // `phase=include_ok` is emitted by the bridge entry point AFTER this returns, and all it has
    // is the caller's position -- which, for a statement the autoload pass spliced into the entry
    // program, names a file that does not contain the line. The resolved target is known only
    // here, so it is handed over through the trace's own slot. Recorded after the body has run, so
    // a nested include leaves the ENCLOSING target behind rather than its own.
    crate::eval_trace::note_include_target(&resolved_path);
    outcome
}

thread_local! {
    /// While true, an include does NOT honour the compiler's "I already included this" mark.
    ///
    /// REQUEST-SCOPED, not context-scoped, and that distinction is the whole reason this is a
    /// thread-local: an autoload callback is evaluated against a DIFFERENT `ElephcEvalContext`
    /// than the one that asked for the symbol, so a flag set on the asking context never reaches
    /// the include the callback performs. Measured — the retry ran and the include was skipped
    /// anyway.
    static REINCLUDE_COMPILER_INCLUDED_SOURCES: std::cell::Cell<bool> =
        const { std::cell::Cell::new(false) };
    /// How many includes this thread skipped on the compiler's mark, so a caller can tell whether
    /// a skip happened while it ran.
    static COMPILER_INCLUDE_SKIPS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Returns how many includes this thread has skipped on the compiler's inclusion mark.
pub(crate) fn compiler_include_skip_count() -> u64 {
    COMPILER_INCLUDE_SKIPS.with(std::cell::Cell::get)
}

/// Runs `body` with the compiler's inclusion marks ignored, then restores the previous mode.
///
/// The mark promises that the COMPILED program performed the inclusion, which is enough for a
/// class — the compiled class is callable. A TRAIT is different: its members are copied into the
/// using class when that class is declared, so an INTERPRETED class that `use`s a compiled trait
/// needs the trait's own body, which the compiled world does not expose. Scoped rather than a
/// plain setter so an early return cannot leave every later include re-reading files it should
/// skip — that skip is what keeps a request stat-ing its vendor tree instead of re-reading it.
pub(crate) fn with_compiler_included_sources_reincluded<T>(body: impl FnOnce() -> T) -> T {
    let previous = REINCLUDE_COMPILER_INCLUDED_SOURCES.with(|flag| flag.replace(true));
    let result = body();
    REINCLUDE_COMPILER_INCLUDED_SOURCES.with(|flag| flag.set(previous));
    result
}

/// Whether includes currently ignore the compiler's inclusion marks.
pub(crate) fn reincluding_compiler_included_sources() -> bool {
    REINCLUDE_COMPILER_INCLUDED_SOURCES.with(|flag| flag.get())
}

/// Answers a compiled `include`/`require` without a context, when the interpreter would answer it
/// without running anything: an `_once` form of a file this request already included, or a
/// statement include of a file the compiled program already included. `false` means "not
/// answered here", and the caller takes the full bridge path, which also owns every diagnostic.
///
/// WHY. The full path creates an eval context and flushes every local of the calling compiled
/// function into an eval scope, then reloads them, only to reach the same two early returns as
/// [`eval_resolved_include`]. Composer's `$requireFile` closure pays that for each of its `files`
/// on every request, although the compiled program included all of them at startup.
///
/// Only an absolute plain path is taken: a relative one depends on `include_path` and the calling
/// file, and a stream wrapper never names a compiled source.
pub(crate) fn fast_include_answer(path: &str, once: bool, value_discarded: bool) -> bool {
    if !path.starts_with('/') || path.contains("://") {
        return false;
    }
    let include_key = eval_include_key(std::path::Path::new(path));
    let (included, compiler_included) = crate::context::request_include_marks(&include_key);
    if once && included {
        return true;
    }
    if value_discarded && !reincluding_compiler_included_sources() && compiler_included {
        if crate::eval_trace::enabled() {
            eprintln!("[elephc-eval-trace] kind=include phase=include_fast_skip path={include_key:?}");
        }
        // Counted like the full path's skip: the autoloader retries a file whose class is still
        // missing after a skip (Carbon's `Translator.php` declares its class from top-level code),
        // and it can only tell that a skip happened from this count. Uncounted, Laravel's boot
        // died on `Carbon\Translator::get()` with "eval callback dispatch failed".
        COMPILER_INCLUDE_SKIPS.with(|skips| skips.set(skips.get().wrapping_add(1)));
        crate::runtime_hooks::activate_compiled_source(&include_key);
        return true;
    }
    false
}

/// Runs one include whose path is already a string and already resolved to a filesystem path.
#[allow(clippy::too_many_arguments)]
fn eval_resolved_include(
    path: &str,
    resolved_path: &std::path::Path,
    required: bool,
    once: bool,
    value_used: bool,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let include_key = eval_include_key(resolved_path);
    if once && context.has_included_file(&include_key) {
        return values.bool_value(true);
    }
    if !value_used
        && !reincluding_compiler_included_sources()
        && context.compiler_included_file(&include_key)
    {
        trace_compiler_included_skip(resolved_path, context);
        COMPILER_INCLUDE_SKIPS.with(|skips| skips.set(skips.get().wrapping_add(1)));
        // Skipping the body is right -- the program already ran it -- but the inclusion is also
        // the moment php DECLARES the file's symbols, and the compiled program only activates
        // them where it first names one. An autoloader reaching the file earlier than that must
        // leave them declared, or the interpreted `interface X extends Y` that asked for `Y`
        // still finds it missing.
        crate::runtime_hooks::activate_compiled_source(&include_key);
        return values.bool_value(true);
    }
    // A file already parsed in this process, unchanged on disk, needs no read: the path cache
    // answers from one `stat`. A web worker re-includes the same vendor tree on every request, so
    // this is the difference between reading 76 files per request and stat-ing them. A cached
    // PARSE ERROR deliberately falls through to the read below, so the diagnostic still carries
    // the offending bytes.
    // With the OPcache runtime cache ENABLED, `load_script` below owns caching and its
    // `validate_timestamps` / blacklist semantics, so this shortcut is for the cache-off build.
    // php describes an include as a frame of its own, named after the construct as written and
    // positioned at the caller: `require_once('/…/autoload_runtime.php')` at `index.php:5`. The
    // frame stays live for the WHOLE included program, and autoload_runtime.php runs the entire
    // request inside its include, so every dev error page lists it. The argument is the path the
    // construct evaluated to.
    let construct = match (required, once) {
        (true, true) => "require_once",
        (true, false) => "require",
        (false, true) => "include_once",
        (false, false) => "include",
    };
    let frame_argument = values.string(path)?;
    let frame = EvalCallFrame::function(construct, Some(vec![frame_argument]), context);
    context.push_call_frame(frame);
    let outcome = eval_resolved_include_body(
        path,
        resolved_path,
        include_key,
        required,
        context,
        scope,
        values,
    );
    context.pop_call_frame();
    let released = values.release(frame_argument);
    match (outcome, released) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(status), _) => Err(status),
        (Ok(_), Err(status)) => Err(status),
    }
}

/// Runs the included program once its construct frame is on the stack.
fn eval_resolved_include_body(
    path: &str,
    resolved_path: &std::path::Path,
    include_key: std::path::PathBuf,
    required: bool,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !crate::script_cache::config().enabled {
    if let Some(Ok(program)) = parse_source_file_at_path(resolved_path) {
        context.mark_included_file(include_key);
        trace_cached_include(resolved_path, context);
        return eval_include_control_value(
            eval_execute_include_program(program, resolved_path, context, scope, values)?,
            context,
            values,
        );
    }
    }
    let segments = match load_script(resolved_path) {
        Ok(segments) => segments,
        Err(_) => return eval_include_missing_file(path, required, values),
    };
    context.mark_included_file(include_key);
    eval_replay_include_segments(&segments, resolved_path, context, scope, values)
}

/// Emits the opt-in trace for an include the compiled program already performed.
///
/// Printed as its own phase so an include census can tell a skip from a re-execution: the
/// per-request include count is the whole point of the closed world, and a silent skip would
/// look exactly like a file that was never reached.
fn trace_compiler_included_skip(path: &std::path::Path, context: &ElephcEvalContext) {
    if !crate::eval_trace::enabled() {
        return;
    }
    let caller = context.call_site();
    eprintln!(
        "[elephc-eval-trace] kind=include phase=compiler_included_skip path={path:?} caller_file={:?} caller_line={}",
        caller.0, caller.2,
    );
}

/// Emits the opt-in include trace for a file served from the parse cache.
fn trace_cached_include(path: &std::path::Path, context: &ElephcEvalContext) {
    if !crate::eval_trace::enabled() {
        return;
    }
    let caller = context.call_site();
    eprintln!(
        "[elephc-eval-trace] kind=include phase=input_cached path={path:?} caller_file={:?} caller_line={}",
        caller.0, caller.2,
    );
}

/// Includes a URL whose scheme a userspace stream wrapper owns, or answers `None` for any other
/// path.
///
/// php opens the URL through the wrapper object -- `stream_open()`, then `stream_read()` until
/// `stream_eof()`, then `stream_close()` when the class has one -- and compiles what it read as a
/// file whose `__FILE__` is the URL. Serialized-closure libraries rely on exactly this: they
/// register a wrapper whose `stream_open()` wraps the path in `<?php return ...;` and `include` it
/// to get the closure back. The wrapper may have been registered by interpreted code (this
/// context's registry) or by compiled code (the generated runtime's table), and its class may be
/// either kind; the generic object and method paths below serve all four combinations.
fn eval_include_through_user_wrapper(
    path: &str,
    required: bool,
    once: bool,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let Some(scheme) = crate::stream_wrappers::stream_scheme(path) else {
        return Ok(None);
    };
    let Some(class_name) = context
        .stream_resources()
        .user_stream_wrapper_class_for_path(path)
        .or_else(|| crate::runtime_hooks::compiled_stream_wrapper_class(&scheme))
    else {
        return Ok(None);
    };
    let include_path = std::path::Path::new(path);
    let include_key = eval_include_key(include_path);
    if once && context.has_included_file(&include_key) {
        return values.bool_value(true).map(Some);
    }
    let Some(bytes) = eval_user_wrapper_include_bytes(path, &class_name, context, scope, values)?
    else {
        return eval_include_missing_file(path, required, values).map(Some);
    };
    context.mark_included_file(include_key);
    eval_execute_include_bytes(&bytes, include_path, context, scope, values).map(Some)
}

/// Reads a whole userspace-wrapper URL the way php's include does, or `None` when
/// `stream_open()` refuses it.
fn eval_user_wrapper_include_bytes(
    path: &str,
    class_name: &str,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<Vec<u8>>, EvalStatus> {
    let object = eval_new_object_result(class_name, Vec::new(), context, scope, values)?;
    let open_args = eval_user_wrapper_stream_open_args(path, "rb", values)?;
    let opened = eval_method_call_result_with_evaluated_args(
        object,
        "stream_open",
        open_args,
        context,
        values,
    )?;
    let accepted = values.truthy(opened)?;
    values.release(opened)?;
    if !accepted {
        values.release(object)?;
        return Ok(None);
    }
    let has_eof = eval_user_wrapper_include_has_method(object, "stream_eof", context, values)?;
    let mut bytes = Vec::new();
    loop {
        if has_eof {
            let eof = eval_method_call_result(object, "stream_eof", Vec::new(), context, values)?;
            let at_end = values.truthy(eof)?;
            values.release(eof)?;
            if at_end {
                break;
            }
        }
        let count = values.int(8192)?;
        let chunk = eval_method_call_result(object, "stream_read", vec![count], context, values)?;
        values.release(count)?;
        let chunk_bytes = if values.type_tag(chunk)? == EVAL_TAG_STRING {
            values.string_bytes(chunk)?
        } else {
            Vec::new()
        };
        values.release(chunk)?;
        if chunk_bytes.is_empty() {
            break;
        }
        bytes.extend_from_slice(&chunk_bytes);
    }
    if eval_user_wrapper_include_has_method(object, "stream_close", context, values)? {
        let closed = eval_method_call_result(object, "stream_close", Vec::new(), context, values)?;
        values.release(closed)?;
    }
    values.release(object)?;
    Ok(Some(bytes))
}

/// Whether a wrapper object has the optional method, asked the way `method_exists()` asks.
fn eval_user_wrapper_include_has_method(
    object: RuntimeCellHandle,
    method: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    let name = values.string(method)?;
    let exists = eval_member_exists_result("method_exists", &[object, name], context, values)?;
    values.release(name)?;
    let exists_bool = values.truthy(exists)?;
    values.release(exists)?;
    Ok(exists_bool)
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
///
/// The key has to be the CANONICAL path: `include_once` dedupes on it and `get_included_files()`
/// reports it, so two spellings that reach one file must produce one key — and on macOS even a
/// lexically normal absolute path can cross a symlinked component, `/tmp` and `/var` being
/// symlinks into `/private`. php-src canonicalizes here too, for every include and not only the
/// `_once` forms, because a plain `include` adds its file to the same set a later `include_once`
/// consults.
///
/// It is also the single most expensive thing an interpreted include does. `realpath()` issues one
/// `getattrlist` per path component on macOS, the generated Symfony container fragments sit ten
/// components deep, and a `--web` worker re-includes the same tree on every request: 31% of the
/// samples inside request handling were this one call. `realpath_cache` is php-src's answer to
/// exactly that, so the memo behind this call is matching PHP rather than diverging from it.
fn eval_include_key(path: &std::path::Path) -> std::path::PathBuf {
    crate::realpath_cache::canonicalize_cached(path)
}

/// Replays an included file's cached segments: the whole file, parsed as one program, whose
/// inline HTML is `echo` statements (`crate::script_cache::segments`).
///
/// A file that does not parse fails here, before any of it runs, as a script php-src could
/// not compile does. The uniform `int(1)` is the value of an include that did not `return`.
fn eval_replay_include_segments(
    segments: &[ScriptSegment],
    path: &std::path::Path,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    for segment in segments {
        let program = match segment {
            // php prints the parse error on standard output before the 255 exit; the read-and-
            // parse path (`eval_execute_include_code`) does, and this cached replay must too, or a
            // broken include ended the program with no diagnostic at all. The segment keeps only
            // the error kind (its on-disk format has no room for the line and token), so the file
            // is parsed once more for them; this runs only on the way to a fatal.
            ScriptSegment::ParseError(error) => {
                if let Ok(code) = std::fs::read(path) {
                    if let Err(diagnostic) = parse_source_file_cached(&code) {
                        report_fatal_diagnostic(&diagnostic.include_message(&path.to_string_lossy()));
                    }
                }
                return Err(error.clone().status());
            }
            ScriptSegment::Code(program) => program.clone(),
        };
        match eval_execute_include_program(program, path, context, scope, values)? {
            EvalControl::None => {}
            control => return eval_include_control_value(control, context, values),
        }
    }
    values.int(1)
}

/// Executes a local include file as one program covering its inline HTML and every PHP block.
///
/// PHP compiles an included file in one pass: the text outside the tags is `T_INLINE_HTML` and
/// becomes an echo, and a closing tag is an implicit semicolon. Parsing each `<?php … ?>` block on
/// its own could never accept a `{` that one block opens and a later block closes, which is how
/// every template in `error-handler/Resources/views` is written.
fn eval_execute_include_bytes(
    bytes: &[u8],
    path: &std::path::Path,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let control = eval_execute_include_code(bytes, path, context, scope, values)?;
    eval_include_control_value(control, context, values)
}

/// Converts the control flow an included file ended with into `include`'s PHP-visible value.
fn eval_include_control_value(
    control: EvalControl,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match control {
        EvalControl::None => values.int(1),
        EvalControl::ReturnVoid => values.null(),
        EvalControl::Return(value) => Ok(value),
        EvalControl::Throw(value) => {
            context.set_pending_throw(value);
            Err(EvalStatus::UncaughtThrowable)
        }
        EvalControl::Break(_) | EvalControl::Continue(_) | EvalControl::Goto(_) => {
            Err(EvalStatus::UnsupportedConstruct)
        }
    }
}

/// Parses and executes one whole included PHP source file.
fn eval_execute_include_code(
    code: &[u8],
    path: &std::path::Path,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    trace_include_fragment("input", code, path, context, None);
    let program = parse_source_file_cached(code).map_err(|diagnostic| {
        trace_include_fragment("parse_error", code, path, context, Some(&diagnostic));
        report_fatal_diagnostic(&diagnostic.include_message(&path.to_string_lossy()));
        diagnostic.status()
    })?;
    eval_execute_include_program(program, path, context, scope, values)
}

/// Executes one already-parsed included PHP source file.
fn eval_execute_include_program(
    program: std::sync::Arc<crate::eval_ir::EvalProgram>,
    path: &std::path::Path,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    let previous = context.call_site();
    // An included file's `declare(strict_types=1)` governs that file and stops at its edge. Left
    // unrestored it made one strict vendor file turn the whole rest of the program strict, which
    // is most of Symfony's vendor tree.
    let previous_strict_types = context.strict_types();
    // PHP reports the INCLUDED FILE's resolved path in `__FILE__`/`__DIR__`, not the spelling the
    // include expression used. A runtime include of `__DIR__.'/../b/two.php'` therefore answers
    // `<root>/b`, while this file used to answer `<root>/a/../b`; composer's autoloader and
    // Symfony's generated container include through paths containing `..`, and their service
    // factories compute `dirname(__DIR__, 4)` from it (the dev error page's frame paths lost the
    // app segment because of that). Resolve the path the way php does, with the uncanonicalized
    // path as the fallback for stream wrappers and other non-filesystem include targets.
    let canonical_path = std::fs::canonicalize(path).ok();
    let resolved = canonical_path.as_deref().unwrap_or(path);
    let file = resolved.to_string_lossy().into_owned();
    let dir = resolved
        .parent()
        .map(|parent| parent.to_string_lossy().into_owned())
        .unwrap_or_default();
    context.set_call_site(file.clone(), dir, 1);
    context.set_file_magic_override(Some(file));
    context.push_include_execution(true);
    let result = execute_program_statements(&program, context, scope, values);
    if let Err(ref status) = result {
        if crate::eval_trace::enabled() {
            eprintln!(
                "[elephc-eval-trace] kind=include phase=execute_error path={path:?} status={status:?}"
            );
        }
    }
    context.pop_include_execution();
    context.set_strict_types(previous_strict_types);
    context.set_call_site(previous.0, previous.1, previous.2);
    context.set_file_magic_override(previous.3);
    result
}

/// Emits an opt-in, panic-safe trace for the exact PHP block parsed from an included file.
fn trace_include_fragment(
    phase: &str,
    code: &[u8],
    path: &std::path::Path,
    context: &ElephcEvalContext,
    error: Option<&EvalParseDiagnostic>,
) {
    if !crate::eval_trace::enabled() {
        return;
    }
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let caller = context.call_site();
        if error.is_some() {
            let escaped = code.escape_ascii().map(char::from).collect::<String>();
            eprintln!(
                "[elephc-eval-trace] kind=include phase={phase} path={:?} len={} error={error:?} caller_file={:?} caller_dir={:?} caller_line={} caller_override={:?} bytes={escaped}",
                path,
                code.len(),
                caller.0,
                caller.1,
                caller.2,
                caller.3,
            );
        } else {
            eprintln!(
                "[elephc-eval-trace] kind=include phase={phase} path={:?} len={} caller_file={:?} caller_line={}",
                path,
                code.len(),
                caller.0,
                caller.2,
            );
        }
    }));
}
