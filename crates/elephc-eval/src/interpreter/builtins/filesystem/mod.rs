//! Purpose:
//! Groups filesystem, path, glob, stat, and fnmatch builtins for eval.
//!
//! Called from:
//! - `crate::interpreter::builtins` filesystem-related dispatch.
//!
//! Key details:
//! - Path arguments are converted through PHP string coercion before touching the
//!   host filesystem.

mod file_io;
mod fnmatch;
mod ops;
mod path;

pub(in crate::interpreter) use file_io::*;
pub(in crate::interpreter) use fnmatch::*;
pub(in crate::interpreter) use ops::*;
pub(in crate::interpreter) use path::*;
