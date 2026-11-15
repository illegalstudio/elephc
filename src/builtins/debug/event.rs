//! Purpose:
//! Home of the internal `__elephc_debug_event` builtin that emits dump and
//! trace-segment capture events from generated code.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - `kind` selects a dump (0), segment start (1) or segment end (2); the runtime
//!   formats the mixed value and returns nothing.
//! - The backend lowering requires `--debug` because the event recorder lives int
//!   the `elephc_debug` bridge.

builtin! {
    contract: "__elephc_debug_event",
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ElephcDebugEvent,
    ),
}
