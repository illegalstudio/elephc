//! Purpose:
//! Opt-in arbitrary PHP evaluation for a stopped debugger frame through the
//! Magician eval bridge. The compiler publishes a function table with
//! `--allow-eval`; this module materializes the frame locals into a fresh eval
//! scope, executes the expression or statement list, renders the result, and
//! releases every temporary.
//!
//! Called from:
//! - `crate::dap::Debugger::dap_evaluate` as the fallback after the read-only
//!   expression parser rejects an expression.
//! - The terminal `eval` command in `crate::Debugger::run_session`.
//!
//! Key details:
//! - The scope starts as a snapshot. Type-compatible scalar assignments write
//!   back into the native frame slot; containers, references and type-changing
//!   assignments stay in the snapshot. Objects and functions still execute real
//!   code and can have side effects; `--allow-eval` is an explicit opt-in.
//! - Site, exception, function enter/exit and `elephc_break` hooks are
//!   suppressed while eval runs, so a user function called from the expression
//!   cannot re-enter the stopped debugger, deadlock on its own mutex, or pollute
//!   call capture with evaluation frames.
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
/// `SCOPE_FLAG_PRESENT`: the scope still has a value for the name.
const SCOPE_FLAG_PRESENT: u32 = 1 << 0;
/// `SCOPE_FLAG_DIRTY`: eval mutated the value since the native import.
const SCOPE_FLAG_DIRTY: u32 = 1 << 2;
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
    scope_get: usize,
    context_new: usize,
    context_free: usize,
    execute: usize,
    render: usize,
    value_bits: usize,
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
    scope_get: unsafe extern "C" fn(
        *mut c_void,
        *const u8,
        u64,
        *mut *mut c_void,
        *mut u32,
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
    value_bits: unsafe extern "C" fn(
        *mut c_void,
        *mut c_void,
        *mut u32,
        *mut u64,
        *mut u64,
    ) -> u32,
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
        raw.scope_get,
        raw.context_new,
        raw.context_free,
        raw.execute,
        raw.render,
        raw.value_bits,
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
        scope_get: unsafe { std::mem::transmute(raw.scope_get) },
        context_new: unsafe { std::mem::transmute(raw.context_new) },
        context_free: unsafe { std::mem::transmute(raw.context_free) },
        execute: unsafe { std::mem::transmute(raw.execute) },
        render: unsafe { std::mem::transmute(raw.render) },
        value_bits: unsafe { std::mem::transmute(raw.value_bits) },
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
        .and_then(|function| {
            function
                .class
                .clone()
                .or_else(|| function.name.split_once("::").map(|(class, _)| class.to_string()))
        });
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
    // `return` makes PHP's implied result value observable and also accepts
    // assignment expressions (`$x = 1`), which the statement parser would
    // otherwise reject at end of input. A top-level `;` means the input is a
    // statement list: wrapping it would parse as `return $x = 1; $x = 2;` and
    // silently skip everything after the first statement.
    let statement_list = is_statement_list(expression);
    let expression_source = format!("return {expression};");
    let _executing = ExecutingGuard::new();
    let mut status = if statement_list {
        1
    } else {
        unsafe {
            (hooks.execute)(
                context,
                scope,
                expression_source.as_ptr(),
                expression_source.len() as u64,
                &mut result,
            )
        }
    };
    // A statement list (`echo ...;`, `if (...) { ... }`) is not a valid
    // expression. Parse errors are safe to retry because no user code ran;
    // execute the fragment as written and let its side effects happen.
    if status == 1 {
        let statement_source = if expression.trim_end().ends_with(';') {
            expression.to_string()
        } else {
            format!("{expression};")
        };
        result.kind = 0;
        result.value_cell = std::ptr::null_mut();
        result.error = std::ptr::null_mut();
        status = unsafe {
            (hooks.execute)(
                context,
                scope,
                statement_source.as_ptr(),
                statement_source.len() as u64,
                &mut result,
            )
        };
    }
    if pushed_class {
        unsafe { (hooks.pop_class)(context) };
    }
    write_back_scalars(debugger, activation, hooks, scope, context);
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

/// Returns whether `source` is a statement list rather than one expression.
///
/// A single trailing semicolon is tolerated (`$x = 1;` stays an expression), but
/// any other top-level `;` means the input must run as written. Quotes, escapes
/// and nested brackets are tracked so `for (...; ...; ...)` inside a statement
/// list is not mistaken for a top-level separator.
fn is_statement_list(source: &str) -> bool {
    let trimmed = source.trim_end();
    let body = trimmed.strip_suffix(';').unwrap_or(trimmed);
    let mut quote = None;
    let mut escaped = false;
    let mut depth = 0u32;
    for ch in body.chars() {
        if let Some(active) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == active {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ';' if depth == 0 => return true,
            _ => {}
        }
    }
    false
}

/// Writes type-compatible scalar assignments back into native frame slots.
///
/// Containers, references and type-changing assignments stay in the eval
/// snapshot: replacing their storage while the native frame is stopped would
/// require the full relocation/COW path, not a word store.
fn write_back_scalars(
    debugger: &Debugger,
    activation: StackFrame,
    hooks: &EvalHooks,
    scope: *mut c_void,
    context: *mut c_void,
) {
    let Some(locals) = debugger.locals.get(&(activation.function_id as u64)) else {
        return;
    };
    for local in locals {
        if !importable(local)
            || local.storage != "raw"
            || debugger.local_optimized_out(local, activation.last_site)
            || !matches!(local.php_type.as_str(), "int" | "float" | "bool")
        {
            continue;
        }
        let mut cell = std::ptr::null_mut();
        let mut flags = 0u32;
        let status = unsafe {
            (hooks.scope_get)(
                scope,
                local.name.as_ptr(),
                local.name.len() as u64,
                &mut cell,
                &mut flags,
            )
        };
        if status != 0
            || cell.is_null()
            || flags & SCOPE_FLAG_PRESENT == 0
            || flags & SCOPE_FLAG_DIRTY == 0
        {
            continue;
        }
        let mut tag = 0u32;
        let mut low = 0u64;
        let mut high = 0u64;
        if unsafe { (hooks.value_bits)(context, cell, &mut tag, &mut low, &mut high) } == 0 {
            continue;
        }
        let word = match (local.php_type.as_str(), tag) {
            ("int", 0) => low,
            ("float", 2) => low,
            ("bool", 3) => u64::from(low != 0),
            _ => continue,
        };
        let address = activation.frame.wrapping_sub(local.frame_offset);
        unsafe {
            std::ptr::write_unaligned(address as *mut u64, word);
        }
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

#[cfg(test)]
mod tests {
    use super::is_statement_list;

    /// Verifies only a real top-level separator selects statement mode, so an
    /// assignment list runs every statement while a lone expression keeps its
    /// return value.
    #[test]
    fn detects_statement_lists_without_misreading_nested_separators() {
        assert!(is_statement_list("$x = 1; $x = 2;"));
        assert!(is_statement_list("$x = 1; $x = 2"));
        assert!(is_statement_list("echo 'a;b'; echo 'c';"));
        assert!(!is_statement_list("$x = 1;"));
        assert!(!is_statement_list("strlen('a;b')"));
        assert!(!is_statement_list("if (true) { $x = 1; }"));
        assert!(!is_statement_list("$items[0] ?? 'x;y'"));
    }
}
