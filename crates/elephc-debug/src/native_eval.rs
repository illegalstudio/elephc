//! Purpose:
//! Opt-in arbitrary PHP evaluation for a stopped debugger frame through the
//! Magician eval bridge. The compiler publishes a function table with
//! `--allow-eval`; this module materializes the frame locals into a fresh eval
//! scope, executes the expression, renders the result, and releases every
//! temporary.
//!
//! Called from:
//! - `crate::dap::Debugger::dap_evaluate` as the fallback after the read-only
//!   expression parser rejects an expression.
//! - The terminal `eval` command in `crate::Debugger::run_session`.
//!
//! Key details:
//! - The scope is a snapshot: assignments inside eval do not write back into the
//!   native frame. Objects and functions still execute real code and can have
//!   side effects; `--allow-eval` is an explicit opt-in.
//! - Site, exception and `elephc_break` hooks are suppressed while eval runs, so
//!   a user function called from the expression cannot re-enter the stopped
//!   debugger and deadlock on its own mutex.
//! - Locals the debug preset reports as `<optimized out>`, generated temporaries
//!   and `#[\SensitiveParameter]` values are not imported.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;

use super::{value, Debugger, Local, StackFrame};

/// ABI version of the generated hook table.
const EVAL_HOOK_VERSION: u64 = 1;
/// `SCOPE_FLAG_OWNED`: the scope releases the cell when replaced or freed.
const SCOPE_FLAG_OWNED: u32 = 1 << 4;
/// `SCOPE_FLAG_NATIVE_SNAPSHOT`: the imported value is a clean native snapshot.
const SCOPE_FLAG_NATIVE_SNAPSHOT: u32 = 1 << 5;
/// Process-lifetime catalog token used by the debug eval context.
static CATALOG_TOKEN: AtomicU64 = AtomicU64::new(0);
/// True while arbitrary eval executes, so generated hooks stay dormant.
static EVAL_EXECUTING: AtomicBool = AtomicBool::new(false);

/// C layout of the generated debugger eval hook table.
#[repr(C)]
struct RawHooks {
    version: u64,
    scope_new: usize,
    scope_free: usize,
    scope_set: usize,
    context_new: usize,
    context_free: usize,
    execute: usize,
    render: usize,
    push_class: usize,
    pop_class: usize,
    register_inventory: usize,
    box_mixed: usize,
    release_any: usize,
}

/// Typed, validated function table copied from the generated data section.
struct EvalHooks {
    scope_new: unsafe extern "C" fn() -> *mut c_void,
    scope_free: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    scope_set: unsafe extern "C" fn(
        *mut c_void,
        *const u8,
        u64,
        *mut c_void,
        u32,
        *mut *mut c_void,
    ) -> i32,
    context_new: unsafe extern "C" fn(*mut u64) -> *mut c_void,
    context_free: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    execute: unsafe extern "C" fn(
        *mut c_void,
        *mut c_void,
        *const u8,
        u64,
        *mut EvalResult,
    ) -> i32,
    render: unsafe extern "C" fn(*mut c_void, *mut EvalResult, *mut u8, u64) -> u64,
    push_class: unsafe extern "C" fn(*mut c_void, *const u8, u64, *const u8, u64) -> i32,
    pop_class: unsafe extern "C" fn(*mut c_void) -> i32,
    register_inventory: unsafe extern "C" fn(*mut c_void),
    box_mixed: unsafe extern "C" fn(u64, u64, u64) -> *mut c_void,
    release_any: unsafe extern "C" fn(*mut c_void),
}

/// `ElephcEvalResult` layout from the Magician bridge.
#[repr(C)]
struct EvalResult {
    kind: u32,
    value_cell: *mut c_void,
    error: *mut c_void,
}

/// Installed hook table; absent when the binary was not built with `--allow-eval`.
static HOOKS: OnceLock<EvalHooks> = OnceLock::new();

/// Installs the generated eval bridge table at program boot.
#[no_mangle]
pub extern "C" fn elephc_debug_set_eval_hooks(raw: *const c_void) {
    let Some(raw) = (unsafe { raw.cast::<RawHooks>().as_ref() }) else {
        return;
    };
    if raw.version != EVAL_HOOK_VERSION {
        return;
    }
    if [
        raw.scope_new,
        raw.scope_free,
        raw.scope_set,
        raw.context_new,
        raw.context_free,
        raw.execute,
        raw.render,
        raw.push_class,
        raw.pop_class,
        raw.register_inventory,
        raw.box_mixed,
        raw.release_any,
    ]
    .iter()
    .any(|pointer| *pointer == 0)
    {
        return;
    }
    let hooks = EvalHooks {
        scope_new: unsafe { std::mem::transmute(raw.scope_new) },
        scope_free: unsafe { std::mem::transmute(raw.scope_free) },
        scope_set: unsafe { std::mem::transmute(raw.scope_set) },
        context_new: unsafe { std::mem::transmute(raw.context_new) },
        context_free: unsafe { std::mem::transmute(raw.context_free) },
        execute: unsafe { std::mem::transmute(raw.execute) },
        render: unsafe { std::mem::transmute(raw.render) },
        push_class: unsafe { std::mem::transmute(raw.push_class) },
        pop_class: unsafe { std::mem::transmute(raw.pop_class) },
        register_inventory: unsafe { std::mem::transmute(raw.register_inventory) },
        box_mixed: unsafe { std::mem::transmute(raw.box_mixed) },
        release_any: unsafe { std::mem::transmute(raw.release_any) },
    };
    let _ = HOOKS.set(hooks);
}

/// Runs `expression` through Magician against a snapshot of `activation`'s locals.
pub(crate) fn evaluate(
    debugger: &Debugger,
    activation: StackFrame,
    expression: &str,
) -> Result<String, String> {
    if !debugger.allow_eval {
        return Err(
            "arbitrary eval is not enabled: rebuild with --debug --allow-eval".to_string(),
        );
    }
    let Some(hooks) = HOOKS.get() else {
        return Err("the eval bridge is not linked into this debug build".to_string());
    };
    let scope = unsafe { (hooks.scope_new)() };
    if scope.is_null() {
        return Err("cannot allocate the eval scope".to_string());
    }
    if let Err(error) = import_locals(debugger, activation, hooks, scope) {
        unsafe { release_scope(hooks, scope) };
        return Err(error);
    }
    let context = unsafe { (hooks.context_new)(CATALOG_TOKEN.as_ptr()) };
    if context.is_null() {
        unsafe { release_scope(hooks, scope) };
        return Err("cannot allocate the eval context".to_string());
    }
    unsafe { (hooks.register_inventory)(context) };
    let class = debugger
        .functions
        .get(&(activation.function_id as u64))
        .and_then(|function| function.name.split_once("::").map(|(class, _)| class.to_string()));
    let pushed_class = class.as_deref().is_some_and(|class| {
        unsafe {
            (hooks.push_class)(
                context,
                class.as_ptr(),
                class.len() as u64,
                class.as_ptr(),
                class.len() as u64,
            ) == 0
        }
    });
    let mut result = EvalResult {
        kind: 0,
        value_cell: std::ptr::null_mut(),
        error: std::ptr::null_mut(),
    };
    // The debugger evaluates an expression, not a statement list. `return`
    // makes PHP's implied result value observable and also accepts assignment
    // expressions (`$x = 1`), which the statement parser would otherwise reject
    // at end of input.
    let source = format!("return {expression};");
    let _executing = ExecutingGuard::new();
    let status = unsafe {
        (hooks.execute)(
            context,
            scope,
            source.as_ptr(),
            source.len() as u64,
            &mut result,
        )
    };
    if pushed_class {
        unsafe { (hooks.pop_class)(context) };
    }
    let mut buffer = vec![0u8; 16 * 1024];
    let written = unsafe {
        (hooks.render)(
            context,
            &mut result,
            buffer.as_mut_ptr(),
            buffer.len() as u64,
        )
    };
    let text = String::from_utf8_lossy(&buffer[..written.min(buffer.len() as u64) as usize])
        .trim_end()
        .to_string();
    unsafe {
        release_context(hooks, context);
        release_scope(hooks, scope);
    }
    if status == 0 {
        Ok(if text.is_empty() {
            "(no result)".to_string()
        } else {
            text
        })
    } else if text.is_empty() {
        Err(format!("eval failed with status {status}"))
    } else {
        Err(text)
    }
}

/// Materializes the visible frame locals into `scope` as owned Mixed snapshots.
fn import_locals(
    debugger: &Debugger,
    activation: StackFrame,
    hooks: &EvalHooks,
    scope: *mut c_void,
) -> Result<(), String> {
    let Some(locals) = debugger.locals.get(&(activation.function_id as u64)) else {
        return Ok(());
    };
    for local in locals {
        if !importable(local) || debugger.local_optimized_out(local, activation.last_site) {
            continue;
        }
        let value = value::local_value(local, activation.frame);
        let cell = unsafe { (hooks.box_mixed)(value.tag as u64, value.lo, value.hi) };
        if cell.is_null() {
            continue;
        }
        let mut throwable = std::ptr::null_mut();
        let flags = SCOPE_FLAG_OWNED | SCOPE_FLAG_NATIVE_SNAPSHOT;
        let status = unsafe {
            (hooks.scope_set)(
                scope,
                local.name.as_ptr(),
                local.name.len() as u64,
                cell,
                flags,
                &mut throwable,
            )
        };
        if !throwable.is_null() {
            unsafe { (hooks.release_any)(throwable) };
        }
        if status != 0 {
            unsafe { (hooks.release_any)(cell) };
            return Err(format!(
                "cannot import ${} into the eval scope (status {status})",
                local.name
            ));
        }
    }
    Ok(())
}

/// Returns whether a metadata local is safe and meaningful to expose to eval.
fn importable(local: &Local) -> bool {
    (local.kind.is_empty() || local.kind == "php_local")
        && !local.name.contains('#')
        && !local.name.starts_with("__elephc_")
        && !local.name.starts_with("__eir_")
        && !local.sensitive
}

/// Frees one eval scope and releases any escaping throwable it returns.
unsafe fn release_scope(hooks: &EvalHooks, scope: *mut c_void) {
    if scope.is_null() {
        return;
    }
    let throwable = unsafe { (hooks.scope_free)(scope) };
    if !throwable.is_null() {
        unsafe { (hooks.release_any)(throwable) };
    }
}

/// Frees one eval context and releases any escaping throwable it returns.
unsafe fn release_context(hooks: &EvalHooks, context: *mut c_void) {
    if context.is_null() {
        return;
    }
    let throwable = unsafe { (hooks.context_free)(context) };
    if !throwable.is_null() {
        unsafe { (hooks.release_any)(throwable) };
    }
}

/// Clears the eval-executing flag on every exit path, including an unwind.
struct ExecutingGuard;

impl ExecutingGuard {
    /// Marks the current thread as executing arbitrary eval.
    fn new() -> Self {
        EVAL_EXECUTING.store(true, Ordering::Release);
        Self
    }
}

impl Drop for ExecutingGuard {
    /// Restores generated debug hooks after evaluation.
    fn drop(&mut self) {
        EVAL_EXECUTING.store(false, Ordering::Release);
    }
}

/// Returns whether generated site/exception/debug-break hooks must stay dormant.
pub(crate) fn executing() -> bool {
    EVAL_EXECUTING.load(Ordering::Acquire)
}
