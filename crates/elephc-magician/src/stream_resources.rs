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
//! - Resource ids here are runtime PAYLOADS, drawn from a namespace disjoint from
//!   every payload the compiled host program can produce (see
//!   `EVAL_RESOURCE_PAYLOAD_BASE`). The PHP display id is a SEPARATE number minted
//!   by the runtime resource-id registry; it is not `payload + 1` and cannot be
//!   converted back into a payload.
//! - Resource handles are process-local to eval and are not visible across the C ABI.

/// First native payload handed out for an eval-owned resource.
///
/// WHY A BASE AT ALL. A tag-9 payload is the KEY of the runtime resource-id
/// registry (`elephc::codegen_support::runtime::resource_ids`), and that registry
/// is shared with the compiled host program — as it must be, because PHP keeps ONE
/// resource-id counter per request and `eval()` draws from it like any other code.
/// Measured against PHP 8.5.6: `$h = fopen(...); eval('$a = fopen(...); $b =
/// fopen(...);'); $i = fopen(...);` reports 5 / 6, 7 / 8. So the ID space is shared
/// on purpose; what must NOT be shared is the PAYLOAD space, because eval's keys
/// are a dense counter of its own while host payloads are file descriptors and
/// allocator handles. Without a base the two collide in two distinct ways:
///
/// - Eval's first three keys were 0, 1 and 2, which the registry answers directly
///   as `payload + 1` because those are the standard stream descriptors. Eval
///   resources therefore reported 1, 2, 3 and then jumped to the counter's 5 —
///   `get_resource_id()` inside `eval()` returned 1,2,3,5,6,7 where PHP returns
///   5,6,7,8,9,10 — while consuming nothing from the shared counter, so a later
///   host `fopen()` reported an id three too low.
/// - Beyond the third, eval key `n` and host descriptor `n` are the same registry
///   key, so an eval stream and an unrelated host stream aliased one id.
///
/// WHY THIS VALUE. It must exceed the standard-stream shortcut (payload 2), every
/// file descriptor, and every user-space address a `DIR*` or a hash-context handle
/// can take. `1 << 62` clears all three on both supported targets: canonical
/// user-space addresses stop at 2^47 on x86_64 (2^56 with 5-level paging) and at
/// 2^52 on AArch64, and it stays positive in `i64`, so payloads keep round-tripping
/// through `raw_value_word()` and `i64::try_from`.
pub(crate) const EVAL_RESOURCE_PAYLOAD_BASE: i64 = 1 << 62;

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

#[cfg(feature = "curl")]
mod curl;
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
    // Analogous to `hash_contexts`: PHP 8's `curl_init()` returns a `CurlHandle` OBJECT,
    // not a resource, so it is keyed from this SAME `next_id` counter (never its own),
    // consumes no PHP resource id, and is freed only here — see
    // `crate::interpreter::builtins::curl`'s module doc for the full argument and
    // `EvalCurlEasyHandle`'s own doc for the PHP-layer mirror fields it carries.
    #[cfg(feature = "curl")]
    curl_easy_handles: HashMap<i64, EvalCurlEasyHandle>,
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
