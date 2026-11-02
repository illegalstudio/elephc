//! Purpose:
//! Records the interpreter's live call frames so `debug_backtrace()` can describe them.
//!
//! Called from:
//! - `crate::interpreter::dynamic_functions` and `crate::interpreter::statements` around each
//!   eval-declared function, closure, method, and static-method activation.
//! - `crate::interpreter::builtins::registry` around the class-probe builtins.
//! - `crate::interpreter::builtins::core::debug_backtrace` when a program asks for the trace.
//!
//! Key details:
//! - PHP describes a frame by the CALLEE's name and the CALLER's position: `function` is the
//!   function being entered, while `file` and `line` are where the call was written. Both halves
//!   are captured at the same moment, before the callee overrides the source metadata.
//! - A frame carries argument values by handle and does NOT retain them. A frame lives strictly
//!   inside the call it describes, so the caller's own references outlive it; retaining here would
//!   keep every argument of every live call alive for no reader.
//! - `args` is an `Option` because one caller genuinely cannot supply it: the direct builtin
//!   dispatch path hands the builtin unevaluated expressions, so no handle exists yet. PHP omits
//!   the `args` key under `DEBUG_BACKTRACE_IGNORE_ARGS`, so an omitted key is a shape PHP itself
//!   produces — an empty `args` array would instead be a wrong value.
//! - Builtin probes that can run an autoloader push frames too. That is not decoration: Symfony's
//!   `ClassExistenceResource::throwOnRequiredClass` reads `$trace[1]` and returns silently when
//!   that frame's `function` is one of the class probes and it has NO `class` key. Without those
//!   frames the same code throws `ReflectionException` instead.

use super::*;
use std::cell::RefCell;

/// How one call was written, which PHP reports as the frame's `type`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EvalCallFrameKind {
    /// A plain function or closure call, which has neither `class` nor `type`.
    Function,
    /// An instance method call, reported as `->` with the receiver in `object`.
    Method,
    /// A static method call, reported as `::` with no `object`.
    StaticMethod,
}

/// One live interpreter call.
pub struct EvalCallFrame {
    /// PHP's `function` key: the unqualified name of the callee.
    pub function: String,
    /// PHP's `class` key, absent for a plain function.
    pub class: Option<String>,
    /// Selects PHP's `type` key and whether `object` can be present.
    pub kind: EvalCallFrameKind,
    /// PHP's `object` key for an instance call.
    pub object: Option<RuntimeCellHandle>,
    /// PHP's `args` key, or `None` when the call site could not supply the values.
    pub args: Option<Vec<RuntimeCellHandle>>,
    /// PHP's `file` key: the source file the call was written in.
    pub file: String,
    /// PHP's `line` key: the caller's source line.
    pub line: i64,
}

impl EvalCallFrame {
    /// Builds a plain-function frame positioned at the context's current call site.
    pub fn function(
        name: impl Into<String>,
        args: Option<Vec<RuntimeCellHandle>>,
        context: &ElephcEvalContext,
    ) -> Self {
        let (file, line) = eval_call_frame_position(context);
        Self {
            function: name.into(),
            class: None,
            kind: EvalCallFrameKind::Function,
            object: None,
            args,
            file,
            line,
        }
    }

    /// Builds a method frame positioned at the context's current call site.
    pub fn method(
        class_name: &str,
        method_name: impl Into<String>,
        object: Option<RuntimeCellHandle>,
        args: Option<Vec<RuntimeCellHandle>>,
        context: &ElephcEvalContext,
    ) -> Self {
        let (file, line) = eval_call_frame_position(context);
        Self {
            function: method_name.into(),
            class: Some(class_name.trim_start_matches('\\').to_string()),
            kind: match object {
                Some(_) => EvalCallFrameKind::Method,
                None => EvalCallFrameKind::StaticMethod,
            },
            object,
            args,
            file,
            line,
        }
    }
}

/// Reads the caller's source file and line for a frame about to be pushed.
///
/// `file` is exact: the interpreter overrides it per included file and per class method. `line` is
/// the context's current call-site line, which is the closest thing the eval IR carries — no
/// `EvalStmt` or `EvalExpr` records the line it was written on, so a per-statement line is not
/// available to this crate.
///
/// The file is the `__FILE__` override when one is installed — an included file, a class method's
/// declaring file, or eval'd code, which `enter_eval_code_site` names `FILE(LINE) : eval()'d code`
/// — and the plain call-site file otherwise. It used to fall back to `eval_file_magic()`, which
/// synthesizes the eval'd-code name from the call site even for a frame pushed by COMPILED code:
/// the `eval` frame itself then claimed to be inside the code it was about to run.
fn eval_call_frame_position(context: &ElephcEvalContext) -> (String, i64) {
    let (file, _, line, file_magic) = context.call_site();
    (file_magic.unwrap_or(file), line)
}

/// Returns whether PHP shows this builtin as its own `debug_backtrace()` frame here.
///
/// PHP shows every internal function it calls, but only this family is load-bearing: these are the
/// probes that can run an autoloader, so they are the frames a userland autoloader observes above
/// itself. The list is exactly the one `ClassExistenceResource::throwOnRequiredClass` switches on,
/// plus the two SPL entry points that reach the same callbacks.
pub fn eval_builtin_is_backtrace_visible(name: &str) -> bool {
    matches!(
        name,
        "class_exists"
            | "class_implements"
            | "class_parents"
            | "class_uses"
            | "defined"
            | "enum_exists"
            | "get_class_methods"
            | "get_class_vars"
            | "get_parent_class"
            | "interface_exists"
            | "is_a"
            | "is_callable"
            | "is_subclass_of"
            | "method_exists"
            | "property_exists"
            | "spl_autoload"
            | "spl_autoload_call"
            | "trait_exists"
    )
}

thread_local! {
    /// PHP has ONE call stack per request; elephc had one per eval context.
    ///
    /// That difference is not academic. The interpreter runs an autoload callback in the context
    /// that REGISTERED it, not in the one that asked, so a loader calling `debug_backtrace()` saw
    /// only its own frame: the `class_exists()` that started the chain had been recorded on a
    /// different context and was invisible. Symfony's
    /// `ClassExistenceResource::throwOnRequiredClass` reads exactly that frame -- `$trace[1]`,
    /// whose `function` is one of the class probes and which has no `class` key -- and when it is
    /// missing the same code throws a `ReflectionException` nobody catches.
    ///
    /// A request runs on one thread and a forked web worker gets its own copy, so thread-local is
    /// the request's own stack. Pushes and pops are paired by the callers that own them.
    static EVAL_CALL_FRAMES: RefCell<Vec<EvalCallFrame>> = const { RefCell::new(Vec::new()) };

    /// Quiet-fetch depths suspended by the calls currently on the stack, innermost last.
    ///
    /// Parallel to `EVAL_CALL_FRAMES` and pushed and popped with it, so the caller's mode is
    /// restored exactly when its callee returns.
    static EVAL_QUIET_FETCH_BARRIERS: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };

    /// Compile-time-constant depths suspended by the calls currently on the stack.
    ///
    /// A function or method BODY is never a compile-time constant position, even when the call
    /// happens while a constant or property default is being evaluated (an autoloader that runs
    /// to resolve a class named in the default). Without this the loader's `static::` was refused
    /// as "not allowed in compile-time constants".
    static EVAL_CONSTANT_CONTEXT_BARRIERS: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

/// Returns whether calls from the interpreter into compiled code record a frame.
///
/// TEMPORARY A/B SWITCH for measuring the cost of those frames: `ELEPHC_NO_NATIVE_FRAMES` turns
/// them off for one process so the same binary can be measured both ways.
pub fn native_call_frames_enabled() -> bool {
    static DISABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    !*DISABLED.get_or_init(|| std::env::var_os("ELEPHC_NO_NATIVE_FRAMES").is_some())
}

/// Returns whether no interpreter call is live on this thread.
///
/// Readable without a context, which is what lets a compiled `new` of a Throwable ask the
/// question before paying for one.
pub fn eval_call_frames_are_empty() -> bool {
    EVAL_CALL_FRAMES.with(|frames| frames.borrow().is_empty())
}

impl ElephcEvalContext {
    /// Records one call as entered, and closes the quiet-fetch mode across the call.
    ///
    /// Entering a call is exactly where PHP's quiet property fetch stops. `$o->t ?? 'D'` answers
    /// `'D'` for an uninitialized `$o->t`, but `f($o) ?? 'D'` and `$o->getT() ?? 'D'` both THROW
    /// when the callee reads one -- measured against `php -n` 8.5.6. Saving and zeroing the
    /// depth here means no call site has to remember the rule, and the frame stack that already
    /// marks call boundaries is what carries it.
    pub fn push_call_frame(&mut self, frame: EvalCallFrame) {
        let saved = self.enter_call_barrier();
        EVAL_CALL_FRAMES.with(|frames| frames.borrow_mut().push(frame));
        EVAL_QUIET_FETCH_BARRIERS.with(|saved_depths| saved_depths.borrow_mut().push(saved));
        let constant_depth = std::mem::take(&mut self.compile_time_constant_depth);
        EVAL_CONSTANT_CONTEXT_BARRIERS.with(|depths| depths.borrow_mut().push(constant_depth));
    }

    /// Records one call as left, restoring the caller's quiet-fetch mode.
    pub fn pop_call_frame(&mut self) {
        EVAL_CALL_FRAMES.with(|frames| {
            frames.borrow_mut().pop();
        });
        let saved = EVAL_QUIET_FETCH_BARRIERS
            .with(|saved_depths| saved_depths.borrow_mut().pop())
            .unwrap_or(0);
        self.leave_call_barrier(saved);
        let constant_depth = EVAL_CONSTANT_CONTEXT_BARRIERS
            .with(|depths| depths.borrow_mut().pop())
            .unwrap_or(0);
        self.compile_time_constant_depth = constant_depth;
    }

    /// Reads the live frames, innermost last, for the backtrace builder to reverse.
    pub fn with_call_frames<R>(&self, read: impl FnOnce(&[EvalCallFrame]) -> R) -> R {
        EVAL_CALL_FRAMES.with(|frames| read(&frames.borrow()))
    }
}
