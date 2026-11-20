//! Purpose:
//! Forwards eval calls of the `ext/filter` surface to the host's compiled filter prelude.
//!
//! Called from:
//! - Eval source-call dispatch, declarative builtin hooks, and dynamic-callable dispatch.
//!
//! Key details:
//! - No filter logic is implemented in Magician itself. Every home forwards to the host
//!   function the compiled prelude registered, so eval code runs the same `elephc_filter`
//!   bridge as native code.
//! - A host that did not inject the filter prelude has no registered function, so a call
//!   raises PHP's catchable `Error: Call to undefined function filter_*()`.

use super::super::*;

mod dispatch;
mod filter_var;
mod filter_var_array;
mod filter_input;
mod filter_input_array;
mod filter_has_var;
mod filter_id;
mod filter_list;

pub(in crate::interpreter) use dispatch::*;
