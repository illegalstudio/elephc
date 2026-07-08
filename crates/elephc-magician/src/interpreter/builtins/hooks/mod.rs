//! Purpose:
//! Groups declarative registry dispatch hooks for eval builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::spec` re-exports used by `eval_builtin!`.
//!
//! Key details:
//! - Direct expression dispatch, already-evaluated argument dispatch, and
//!   focused hook helpers stay split so ordinary files remain small.

mod direct;
mod hash;
mod string_split_join;
mod values;

pub(in crate::interpreter) use direct::EvalDirectHook;
pub(in crate::interpreter) use values::EvalValuesHook;
