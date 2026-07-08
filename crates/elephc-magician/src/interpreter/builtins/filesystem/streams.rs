//! Purpose:
//! Groups shared helpers for eval-local filesystem stream builtins.
//!
//! Called from:
//! - Leaf filesystem stream builtin files that need stream-resource coercions,
//!   CSV formatting, or direct `flock()` argument binding.
//!
//! Key details:
//! - Builtin implementations live in their owning leaf files; this module only
//!   re-exports shared stream helper modules.

use super::super::super::*;
use super::*;

mod common;
mod csv_format;
mod flock;

pub(in crate::interpreter) use common::*;
pub(in crate::interpreter) use csv_format::*;
pub(in crate::interpreter) use flock::*;
