//! Purpose:
//! Groups eval implementations for PHP-visible builtins and related helpers.
//! Submodules are organized by builtin domain while this module re-exports the
//! callable surface expected by the core interpreter.
//!
//! Called from:
//! - `crate::interpreter::eval_call()` and positional builtin dispatch paths.
//!
//! Key details:
//! - Builtin modules are children of `interpreter`, so they can reuse core EvalIR
//!   execution helpers without widening crate-level visibility.
//! - Runtime value creation and PHP coercions still flow through `RuntimeValueOps`.

#[macro_use]
mod macros;

mod array;
mod arrays;
mod class_metadata;
mod filesystem;
mod formatting;
mod math;
mod network_env;
mod process_control;
mod raw_memory;
mod ref_targets;
mod regex;
mod registry;
mod scalars;
mod spl_autoload;
mod spec;
mod string;
mod strings;
mod symbols;
mod time;
mod types;

pub(super) use arrays::*;
pub(super) use class_metadata::*;
pub(super) use filesystem::*;
pub(super) use formatting::*;
pub(super) use network_env::*;
pub(super) use process_control::*;
pub(super) use raw_memory::*;
pub(super) use ref_targets::*;
pub(super) use regex::*;
pub(super) use registry::*;
pub(super) use scalars::*;
pub(super) use spl_autoload::*;
pub(super) use strings::*;
pub(super) use symbols::*;
pub(super) use time::*;
