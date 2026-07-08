//! Purpose:
//! Re-exports eval OOP introspection helpers for class/object variable builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins::class_metadata` re-exports.
//!
//! Key details:
//! - `get_class_vars()` materializes declarative defaults, not current runtime
//!   static property state.
//! - `get_object_vars()` filters declared storage slots so inaccessible
//!   protected/private eval properties do not leak as dynamic properties.

use super::super::super::*;
use super::{eval_class_metadata_name, eval_class_relation_name_exists};

mod class_vars;
mod common;
mod methods;
mod object_vars;

pub(in crate::interpreter) use class_vars::*;
pub(in crate::interpreter) use common::*;
pub(in crate::interpreter) use methods::*;
pub(in crate::interpreter) use object_vars::*;
