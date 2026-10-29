//! Purpose:
//! Home of the internal `__elephc_set_memory_limit` builtin: installs a runtime
//! `memory_limit` ceiling below or above the reserved heap arena.
//!
//! Called from:
//! - The injected `ini_set('memory_limit', …)` wrapper after it has parsed the quantity
//!   and rejected a limit below the current live heap usage.
//!
//! Key details:
//! - `internal: true`: never PHP-visible, so `function_exists()` does not report it.
//! - The lowering writes `_heap_limit`, so it carries `WRITES_GLOBAL`; the optimizer must
//!   never fold two calls together or drop one whose result is discarded.
//! - `-1` means "unlimited", which the fixed arena cannot provide: it becomes the arena
//!   capacity, exactly like the compile-time `--ini memory_limit=-1` path.

builtin! {
    contract: "__elephc_set_memory_limit",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcSetMemoryLimit,
    ),
}
