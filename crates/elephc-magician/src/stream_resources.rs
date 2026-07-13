//! Purpose:
//! Owns eval-local resource storage backed by host file handles, directory
//! snapshots, stream contexts, and hash contexts. Runtime Mixed cells only carry
//! a numeric resource id.
//!
//! Called from:
//! - `crate::context::ElephcEvalContext` stream-resource accessors.
//! - `crate::interpreter::builtins::filesystem` stream builtin helpers.
//!
//! Key details:
//! - Resource ids are zero-based runtime payloads; PHP display ids are payload + 1.
//! - Resource handles are process-local to eval and are not visible across the C ABI.

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::fs::{File, Metadata, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use crate::stream_wrappers;
use crate::value::RuntimeCellHandle;

mod file_process_opening;
mod operations;
mod resource_registration;
mod sockets;
mod storage;
mod types;

use types::*;

/// Eval-owned table of local file streams keyed by runtime resource payload.
#[derive(Default)]
pub(crate) struct EvalStreamResources {
    chunk_sizes: HashMap<i64, i64>,
    default_stream_context: Option<i64>,
    disabled_builtin_stream_wrappers: HashSet<String>,
    next_id: i64,
    directories: HashMap<i64, EvalDirectoryStream>,
    filter_resources: HashSet<i64>,
    hash_contexts: HashMap<i64, EvalHashContext>,
    process_children: HashMap<i64, Child>,
    socket_listeners: HashMap<i64, TcpListener>,
    socket_names: HashMap<i64, EvalSocketNames>,
    stream_contexts: HashMap<i64, EvalStreamContext>,
    streams: HashMap<i64, EvalFileStream>,
    user_stream_wrapper_classes: HashMap<String, String>,
    user_stream_wrappers: Vec<String>,
    user_wrapper_directories: HashMap<i64, EvalUserWrapperDirectory>,
    user_wrapper_streams: HashMap<i64, EvalUserWrapperStream>,
}
