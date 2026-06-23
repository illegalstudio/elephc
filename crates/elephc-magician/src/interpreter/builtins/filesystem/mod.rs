//! Purpose:
//! Groups filesystem, path, glob, stat, and fnmatch builtins for eval.
//!
//! Called from:
//! - `crate::interpreter::builtins` filesystem-related dispatch.
//!
//! Key details:
//! - Path arguments are converted through PHP string coercion before touching the
//!   host filesystem.

mod directories;
mod file_io;
mod fnmatch;
mod ops;
mod path;
mod process_pipes;
mod readline;
mod stream_extensions;
mod stream_context;
mod stream_settings;
mod stream_sockets;
mod streams;

pub(in crate::interpreter) use directories::*;
pub(in crate::interpreter) use file_io::*;
pub(in crate::interpreter) use fnmatch::*;
pub(in crate::interpreter) use ops::*;
pub(in crate::interpreter) use path::*;
pub(in crate::interpreter) use process_pipes::*;
pub(in crate::interpreter) use readline::*;
pub(in crate::interpreter) use stream_extensions::*;
pub(in crate::interpreter) use stream_context::*;
pub(in crate::interpreter) use stream_settings::*;
pub(in crate::interpreter) use stream_sockets::*;
pub(in crate::interpreter) use streams::*;
