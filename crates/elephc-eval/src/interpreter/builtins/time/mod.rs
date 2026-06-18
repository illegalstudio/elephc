//! Purpose:
//! Groups eval implementations for PHP time, date, sleep, version, and uname
//! related builtins.
//!
//! Called from:
//! - `crate::interpreter::builtins` re-exports used by core call dispatch.
//!
//! Key details:
//! - Time conversion helpers stay scoped to the eval interpreter and use libc for
//!   local calendar conversions where PHP behavior depends on host locale/timezone.

mod clock;
mod date;
mod mktime;
mod sleep;
mod strtotime;
mod system;

pub(in crate::interpreter) use clock::*;
pub(in crate::interpreter) use date::*;
pub(in crate::interpreter) use mktime::*;
pub(in crate::interpreter) use sleep::*;
pub(in crate::interpreter) use strtotime::*;
pub(in crate::interpreter) use system::*;
