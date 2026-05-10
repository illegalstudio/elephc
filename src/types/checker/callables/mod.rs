//! Purpose:
//! Coordinates type checking for closures, first-class callables, captures, and extern callable calls.
//! Keeps callable inference shared between expression checking and function-call validation.
//!
//! Called from:
//! - `crate::types::checker::Checker`
//!
//! Key details:
//! - Callable targets must preserve parameter order, capture ownership expectations, and builtin alias resolution.

mod captures;
mod closures;
mod extern_calls;
mod first_class;
