//! Purpose:
//! Yield detection and yield-context validation for the type checker.
//! Detects whether a function/method body is a generator (so the return type
//! can be coerced to `Object("Generator")`), and enforces that `yield` never
//! appears in invalid contexts (global scope, inside a closure's enclosing
//! function, or inside `try`/`catch`/`finally`).
//!
//! Called from:
//!  - `crate::types::checker::driver::mod` — runs `validate_yield_contexts`
//!    as part of the checker driver pipeline before inference.
//!  - `crate::types::checker::functions::resolution::signature` — calls
//!    `body_contains_yield` to override the function's return type.
//!  - `crate::codegen::functions::mod` — calls `body_contains_yield` to
//!    route the function through the generator codegen pipeline.
//!
//! Key details:
//!  - The module splits its responsibilities across two leaf files:
//!    - `detect` provides `body_contains_yield` and the helpers it relies on
//!      for top-level yield discovery (skipping closures).
//!    - `validate` walks the full program, tracking function/try depth, and
//!      emits a `CompileError` for each yield in an illegal context so
//!      multiple errors can surface in one pass.

mod detect;
mod validate;

pub(crate) use detect::body_contains_yield;
pub(crate) use validate::validate_yield_contexts;
