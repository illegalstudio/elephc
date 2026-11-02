//! Purpose:
//! Executes one materialized web request inside a handler child and writes bounded failures.
//!
//! Called from:
//! - `super::request` for disposable request children.
//! - `super::pool` for each dispatch handled by a persistent child.
//!
//! Key details:
//! - Request-local session and response state are reset before invoking generated PHP.
//! - The configured execution alarm is armed only around handler execution and cancelled after it.
//! - Request channels arrive close-on-exec from the broker control receiver.

use std::fs::File;
use std::os::fd::{AsRawFd, FromRawFd, RawFd};

use crate::handler_ipc;
use crate::probe_route;
use crate::request_state::{self, RequestMeta};

use super::super::MAX_EXEC_SECS;
use super::{restore_default_signal};

/// Reads and executes one request in a disposable child, then exits immediately.
pub(super) unsafe fn run_request_child(handler: extern "C" fn(), response_fd: RawFd, id: u64) -> ! {
    restore_default_signal(libc::SIGCHLD);
    restore_default_signal(libc::SIGTERM);
    let stream = File::from_raw_fd(response_fd);
    let complete = execute_handler_request(handler, stream, id);
    libc::_exit(if complete { 0 } else { 1 });
}

/// Materializes one request snapshot, resets request-local state, and invokes PHP.
pub(super) unsafe fn execute_handler_request(handler: extern "C" fn(), mut stream: File, id: u64) -> bool {
    let request = match handler_ipc::read_request(&mut stream) {
        Ok(request) => request,
        Err(error) => {
            eprintln!("elephc-web: handler could not read request {id}: {error}");
            write_failure_response(stream.as_raw_fd());
            return false;
        }
    };
    crate::session::elephc_web_session_reset();
    // Read out of the request before `set_request` moves it, exactly as the
    // default worker does before its own move. PHP runs HERE, in the handler
    // child, not in the worker that accepted the connection — so this is the
    // process that has to tag its samples, adopt an ask, and open an exact
    // slice. None of it happened in the isolated modes, which is why
    // `--web-isolation=pool|request` reported nothing at all.
    let probe_route_label = probe_route::route_label(&request.method, &request.path);
    let req_traceparent = request
        .headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("traceparent"))
        .map(|(_, v)| v.clone());
    // Signed, not merely present — the same authenticity check the default
    // worker applies, so a header captured from a log stops working within
    // minutes and an invented one never does.
    let profile_this = request
        .headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("x-elephc-query"))
        .is_some_and(|(_, value)| probe_route::query_is_authentic(value));
    request_state::set_request(
        request.method,
        request.uri,
        request.path,
        request.query,
        request.headers,
        request.body,
        RequestMeta {
            remote_addr: request.remote_addr,
            remote_port: request.remote_port,
            server_addr: request.server_addr,
            server_port: request.server_port,
            protocol: request.protocol,
        },
    );
    request_state::begin_response_stream(stream.as_raw_fd());
    request_state::set_capture(true);
    // Route first: `probe_route::set` is also where a child adopts an ask that
    // reached the shared mapping after it was forked, which for a disposable
    // request child is every ask there has ever been.
    probe_route::set(&probe_route_label);
    // A signed header authorizes this request outright; without one, offer it
    // anyway — the instrumentation opens a slice only if something is waiting
    // for one, which is how `monitor <address> --exact` gets an answer without a
    // second way into the request path. Before the trace, because this call is
    // what decides whether the request is profiled at all.
    probe_route::profile_request_kind(if profile_this { 1 } else { 2 });
    probe_route::trace_begin(req_traceparent.as_deref(), &probe_route_label);
    let seconds = MAX_EXEC_SECS.load(std::sync::atomic::Ordering::Relaxed);
    if seconds > 0 {
        libc::alarm(seconds);
    }
    crate::request_env::capture_baseline();
    handler();
    if seconds > 0 {
        libc::alarm(0);
    }
    // A pool child serves many requests: undo this one's putenv()s, as php does.
    crate::request_env::restore_baseline();
    // Unconditional and idempotent, as in the default worker: a request that
    // started no slice ends none, and consecutive profiled requests stay
    // separate captures. A pool child serves many requests, so leaving a slice
    // open here would merge the next one into it.
    probe_route::profile_request_kind(0);
    probe_route::clear();
    let complete = request_state::finish_response_stream();
    if !complete {
        eprintln!("elephc-web: handler could not finish response stream");
    }
    complete
}

/// Emits the bounded 500 response used when a handler cannot be started.
pub(super) unsafe fn write_failure_response(fd: RawFd) {
    let headers = Vec::new();
    let body = b"Internal Server Error";
    let _ = handler_ipc::write_response_start(fd, 500, &headers)
        && handler_ipc::write_response_chunks(fd, body)
        && handler_ipc::write_response_end(fd);
}
