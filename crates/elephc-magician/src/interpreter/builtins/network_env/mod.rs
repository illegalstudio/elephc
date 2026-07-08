//! Purpose:
//! Groups network lookup, IP conversion, environment, and realpath-cache eval
//! builtins by focused runtime domain.
//!
//! Called from:
//! - `crate::interpreter::builtins` re-exports used by core call dispatch.
//!
//! Key details:
//! - libc lookup results are copied before subsequent lookups can overwrite
//!   process-global resolver storage.

mod cache;
mod declarations;
mod env;
mod hosts;
mod ip;
mod process;
mod protocols;

pub(in crate::interpreter) use cache::*;
pub(in crate::interpreter) use declarations::*;
pub(in crate::interpreter) use env::*;
pub(in crate::interpreter) use hosts::*;
pub(in crate::interpreter) use ip::*;
pub(in crate::interpreter) use process::*;
pub(in crate::interpreter) use protocols::*;
