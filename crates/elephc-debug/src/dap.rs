//! Purpose:
//! Minimal Debug Adapter Protocol (DAP) server for `elephc debug --dap`.
//! The runtime speaks DAP over an inherited Unix socket; the CLI forwards the
//! IDE's stdio frames to this connection and keeps the program's own stdout out
//! of the protocol stream.
//!
//! Called from:
//! - `Debugger::dap_stopped` while the program is stopped at a site;
//! - `elephc_debug_dump` at process exit through `Debugger::dap_terminated`.
//!
//! Key details:
//! - DAP `launch` is the only launch request accepted. Transport attach uses
//!   `ELEPHC_DEBUG_DAP_LISTEN` plus the CLI `--attach <socket>` bridge; the
//!   DAP request itself still looks like a launch. Breakpoints, stackTrace,
//!   scopes, variables and evaluate operate on the existing activation stack
//!   and metadata.
//! - The entry stop is held until `configurationDone`; the `stopped` event is
//!   emitted after that request, matching the DAP launch sequence.
//! - Requests that arrive while the program is running stay buffered in the
//!   socket until the next stop; pause-while-running is deliberately unsupported.

use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Error as IoError, ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::unix::fs::{FileTypeExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::io::FromRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value as JsonValue};
use sha2::{Digest, Sha256};

use super::{condition, eval, value, Debugger, StepMode, StepState};

/// Maximum DAP body accepted from a client.
const MAX_DAP_BODY: usize = 16 * 1024 * 1024;

/// One active DAP write watch.
pub(crate) struct DataBreakpoint {
    /// What the watch resolves at each site transition.
    target: DataWatchTarget,
    /// Last rendered value; `None` until the target is observed.
    last: Option<String>,
}

/// The storage a data breakpoint watches.
enum DataWatchTarget {
    /// A named local slot in one function.
    Local {
        /// Function owning the local.
        function_id: u64,
        /// PHP local name without the `$` sigil.
        local_name: String,
    },
    /// One declared property slot on a live object.
    Property {
        /// Object pointer observed when the watch was armed.
        object: u64,
        /// Class id the object must still carry.
        class_id: u64,
        /// Property layout entry, including its offset and PHP type.
        property: value::Property,
    },
    /// One string-keyed entry in a live object's dynamic property hash.
    Dynamic {
        /// Object pointer observed when the watch was armed.
        object: u64,
        /// Class id the object must still carry.
        class_id: u64,
        /// Byte offset of the per-object dynamic hash.
        dynamic_offset: usize,
        /// Raw property-name bytes; DAP can carry non-UTF-8 keys through hex.
        key: Vec<u8>,
    },
}

/// One frame-addressed or value-addressed DAP variables container.
#[derive(Clone, Copy)]
pub(crate) enum VariableContainer {
    /// The locals of one activation, selected by thread-like id and stack index.
    Frame { thread_id: i64, index: usize },
    /// A nested array/hash/ref-cell value returned by `value::children`.
    Value(value::Value),
}

/// Outcome of one DAP request while stopped.
enum DapAction {
    /// Keep processing requests at the current stop.
    Stay,
    /// Resume the program.
    Resume,
    /// Close the adapter and terminate the program.
    Exit,
}

/// Framed DAP transport over the inherited control socket.
pub(crate) struct DapConnection {
    reader: Box<dyn BufRead + Send>,
    writer: Arc<Mutex<DapWriter>>,
}

/// Serialized writer shared by request responses and asynchronous output pumps.
struct DapWriter {
    writer: Box<dyn Write + Send>,
    next_seq: i64,
}

/// Cloneable handle used by the stdout/stderr pumps.
#[derive(Clone)]
pub(crate) struct DapOutputHandle {
    writer: Arc<Mutex<DapWriter>>,
}

impl DapWriter {
    /// Writes one framed DAP message.
    fn send(&mut self, message: &JsonValue) -> io::Result<()> {
        let body = serde_json::to_vec(message)
            .map_err(|error| IoError::new(ErrorKind::InvalidData, error))?;
        write!(self.writer, "Content-Length: {}\r\n\r\n", body.len())?;
        self.writer.write_all(&body)?;
        self.writer.flush()
    }

    /// Allocates the next adapter-wide sequence number.
    fn next_seq(&mut self) -> i64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        seq
    }

    /// Sends one DAP event and allocates its sequence number.
    fn event(&mut self, event: &str, body: JsonValue) -> io::Result<()> {
        let seq = self.next_seq();
        self.send(&json!({
            "seq": seq,
            "type": "event",
            "event": event,
            "body": body,
        }))
    }
}

impl DapConnection {
    /// Wraps an inherited file descriptor into a framed DAP connection.
    ///
    /// The caller must own `fd` for the lifetime of the connection. The CLI
    /// passes fd 3 through `ELEPHC_DEBUG_FD`; the runtime takes ownership here.
    pub(crate) fn from_fd(fd: i32) -> std::io::Result<Self> {
        let stream = unsafe { UnixStream::from_raw_fd(fd) };
        Self::from_stream(stream)
    }

    /// Wraps an accepted or connected Unix stream into a framed DAP connection.
    pub(crate) fn from_stream(stream: UnixStream) -> std::io::Result<Self> {
        let reader = Box::new(BufReader::new(stream.try_clone()?));
        Ok(Self {
            reader,
            writer: Arc::new(Mutex::new(DapWriter {
                writer: Box::new(stream),
                next_seq: 1,
            })),
        })
    }

    /// Wraps an accepted loopback TCP stream into a framed DAP connection.
    pub(crate) fn from_tcp_stream(stream: TcpStream) -> std::io::Result<Self> {
        let reader = Box::new(BufReader::new(stream.try_clone()?));
        Ok(Self {
            reader,
            writer: Arc::new(Mutex::new(DapWriter {
                writer: Box::new(stream),
                next_seq: 1,
            })),
        })
    }

    /// Reads one `Content-Length` framed DAP message, or `None` on EOF.
    fn read_message(&mut self) -> std::io::Result<Option<JsonValue>> {
        let mut content_length = None;
        loop {
            let mut line = String::new();
            if self.reader.read_line(&mut line)? == 0 {
                return Ok(None);
            }
            let line = line.trim_end_matches(|ch| ch == '\r' || ch == '\n');
            if line.is_empty() {
                break;
            }
            if let Some(value) = line.strip_prefix("Content-Length:") {
                content_length = value.trim().parse::<usize>().ok();
            }
        }
        let length = content_length.ok_or_else(|| {
            IoError::new(ErrorKind::InvalidData, "DAP message has no Content-Length")
        })?;
        if length > MAX_DAP_BODY {
            return Err(IoError::new(
                ErrorKind::InvalidData,
                "DAP message exceeds the size limit",
            ));
        }
        let mut body = vec![0u8; length];
        self.reader.read_exact(&mut body)?;
        let message = serde_json::from_slice(&body)
            .map_err(|error| IoError::new(ErrorKind::InvalidData, error))?;
        Ok(Some(message))
    }

    /// Returns a cloneable output handle for the stdout/stderr pumps.
    pub(crate) fn output_handle(&self) -> DapOutputHandle {
        DapOutputHandle {
            writer: Arc::clone(&self.writer),
        }
    }

    /// Runs one closure with the serialized writer.
    fn with_writer<T>(
        &self,
        write: impl FnOnce(&mut DapWriter) -> io::Result<T>,
    ) -> io::Result<T> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| IoError::new(ErrorKind::Other, "DAP writer is poisoned"))?;
        write(&mut writer)
    }

    /// Sends a successful response to `request`.
    fn respond(&mut self, request: &JsonValue, body: Option<JsonValue>) -> io::Result<()> {
        self.with_writer(|writer| {
            let seq = writer.next_seq();
            let mut message = json!({
                "seq": seq,
                "type": "response",
                "request_seq": request.get("seq").and_then(JsonValue::as_i64).unwrap_or(0),
                "success": true,
                "command": request.get("command").and_then(JsonValue::as_str).unwrap_or(""),
            });
            if let Some(body) = body {
                message["body"] = body;
            }
            writer.send(&message)
        })
    }

    /// Sends a failed response to `request`.
    fn respond_error(&mut self, request: &JsonValue, message: &str) -> io::Result<()> {
        self.with_writer(|writer| {
            let seq = writer.next_seq();
            let response = json!({
                "seq": seq,
                "type": "response",
                "request_seq": request.get("seq").and_then(JsonValue::as_i64).unwrap_or(0),
                "success": false,
                "command": request.get("command").and_then(JsonValue::as_str).unwrap_or(""),
                "message": message,
            });
            writer.send(&response)
        })
    }

    /// Sends one DAP event.
    fn event(&mut self, event: &str, body: JsonValue) -> io::Result<()> {
        self.with_writer(|writer| writer.event(event, body))
    }
}

impl DapOutputHandle {
    /// Sends one stdout/stderr chunk to the DAP client.
    pub(crate) fn output(&self, category: &str, text: &str) -> io::Result<()> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| IoError::new(ErrorKind::Other, "DAP writer is poisoned"))?;
        writer.event("output", json!({"category": category, "output": text}))
    }
}

impl Debugger {
    /// Sends the DAP exit/terminated events and closes the remote channel.
    pub(crate) fn dap_terminated(&mut self) {
        // Drain and restore stdout/stderr before the terminal events so the
        // client receives every captured chunk ahead of `terminated`.
        self.flush_dap_output();
        if let Some(dap) = self.dap.as_mut() {
            let _ = dap.event("exited", json!({"exitCode": 0}));
            let _ = dap.event("terminated", json!({}));
        }
        self.dap = None;
    }

    /// Runs the DAP request loop while the program is stopped.
    ///
    /// `reason` is `entry`, `breakpoint` or `step`. The entry stop suppresses the
    /// `stopped` event until `configurationDone`, so the client can finish setting
    /// breakpoints before the first user interaction.
    pub(crate) fn dap_stopped(&mut self, site_id: u32, reason: &str) {
        self.variable_refs.clear();
        let entry = reason == "entry";
        if !entry {
            let thread_id = self.current_thread_id();
            let body = self.dap_stopped_body(site_id, reason, thread_id);
            self.dap_event("stopped", body);
        }
        self.dap_wait_for_commands(entry, entry.then_some(site_id));
    }

    /// Replaces the selected DAP exception filters.
    fn dap_set_exception_breakpoints(&mut self, request: &JsonValue) {
        let filters: Vec<String> = request["arguments"]
            .get("filters")
            .and_then(JsonValue::as_array)
            .into_iter()
            .flatten()
            .filter_map(JsonValue::as_str)
            .map(str::to_string)
            .collect();
        self.exception_filters = super::ExceptionFilters::from_names(&filters);
        super::EXCEPTION_BREAKS_ACTIVE.store(
            !self.exception_filters.is_empty(),
            std::sync::atomic::Ordering::Relaxed,
        );
        self.dap_respond(request, Some(json!({"breakpoints": []})));
    }

    /// Stops when the throw helper sees an exception matching the active filters.
    pub(crate) fn dap_exception_stopped(&mut self, throwable: usize, caught: bool) {
        self.variable_refs.clear();
        let value = value::Value {
            tag: value::TAG_OBJECT,
            lo: throwable as u64,
            hi: 0,
            by_ref: false,
        };
        let description = value::describe_with(value, &self.classes);
        let class = description.type_name;
        let label = if caught { "caught" } else { "uncaught" };
        let location = super::ACTIVATIONS.with(|stack| {
            stack
                .borrow()
                .last()
                .and_then(|frame| frame.last_site)
                .and_then(|site_id| self.sites.get(&site_id))
                .map(|site| {
                    let path = self
                        .sources
                        .get(&site.source_id)
                        .map(String::as_str)
                        .unwrap_or("<unknown>");
                    format!(" at {path}:{}", site.line)
                })
                .unwrap_or_default()
        });
        let thread_id = self.current_thread_id();
        self.dap_event(
            "stopped",
            json!({
                "reason": "exception",
                "threadId": thread_id,
                "allThreadsStopped": true,
                "description": format!("Paused on {label} exception: {class}{location}"),
                "text": class,
            }),
        );
        self.dap_wait_for_commands(false, None);
    }

    /// Stops on entry to a DAP function breakpoint.
    pub(crate) fn dap_function_stopped(&mut self, function_id: u32, _frame: usize) {
        self.variable_refs.clear();
        let thread_id = self.current_thread_id();
        let name = self
            .functions
            .get(&(function_id as u64))
            .map(|function| super::display_function_name(&function.name))
            .unwrap_or("<unknown>");
        self.dap_event(
            "stopped",
            json!({
                "reason": "function breakpoint",
                "threadId": thread_id,
                "allThreadsStopped": true,
                "description": format!("Stopped at function {name}"),
                "hitBreakpointIds": [function_id],
            }),
        );
        self.dap_wait_for_commands(false, None);
    }

    /// Shared DAP request loop for source and function stops.
    ///
    /// `entry_site` is `Some(site)` only for the entry stop; the entry
    /// `stopped` event is emitted after `configurationDone` unless
    /// `stopOnEntry` is false, in which case the loop resumes.
    fn dap_wait_for_commands(&mut self, entry: bool, entry_site: Option<u32>) {
        let mut stopped_sent = !entry;
        loop {
            let message = {
                let Some(dap) = self.dap.as_mut() else {
                    return;
                };
                match dap.read_message() {
                    Ok(Some(message)) => message,
                    Ok(None) => {
                        // A web request outlives its debug client: resume the
                        // handler instead of killing the worker that also serves
                        // every subsequent request.
                        if self.web_session {
                            self.close_dap();
                            return;
                        }
                        std::process::exit(0)
                    }
                    Err(error) => {
                        if self.web_session {
                            self.close_dap();
                            return;
                        }
                        eprintln!("elephc-debug: DAP read failed: {error}");
                        std::process::exit(2);
                    }
                }
            };
            let command = message
                .get("command")
                .and_then(JsonValue::as_str)
                .unwrap_or("");
            if command == "configurationDone" {
                self.dap_respond(&message, None);
                if entry {
                    if self.dap_stop_on_entry {
                        if !stopped_sent {
                            let site = entry_site.unwrap_or(0);
                            let thread_id = self.current_thread_id();
                            let body = self.dap_stopped_body(site, "entry", thread_id);
                            self.dap_event("stopped", body);
                            stopped_sent = true;
                        }
                    } else {
                        // Configuration is complete and the client did not ask to
                        // stop on entry: run to the first breakpoint.
                        return;
                    }
                }
                continue;
            }
            match self.dap_handle_request(&message) {
                DapAction::Stay => {}
                DapAction::Resume => return,
                DapAction::Exit => {
                    self.close_dap();
                    if self.web_session {
                        return;
                    }
                    std::process::exit(0);
                }
            }
        }
    }

    /// Builds the `stopped` event body for one site.
    fn dap_stopped_body(&self, site_id: u32, reason: &str, thread_id: i64) -> JsonValue {
        let mut body = json!({
            "reason": reason,
            "threadId": thread_id,
            "allThreadsStopped": true,
        });
        if let Some(site) = self.sites.get(&site_id) {
            let path = self
                .sources
                .get(&site.source_id)
                .map(String::as_str)
                .unwrap_or("<unknown>");
            body["description"] = json!(format!(
                "Stopped at {}:{}:{} ({})",
                path,
                site.line,
                site.column,
                self.site_function(site)
            ));
            if reason == "breakpoint" {
                body["hitBreakpointIds"] = json!([site_id]);
            }
        }
        body
    }

    /// Dispatches one DAP request.
    fn dap_handle_request(&mut self, request: &JsonValue) -> DapAction {
        let command = request
            .get("command")
            .and_then(JsonValue::as_str)
            .unwrap_or("");
        match command {
            "initialize" => {
                self.dap_respond(
                    request,
                    Some(json!({
                        "supportsConfigurationDoneRequest": true,
                        "supportsTerminateRequest": true,
                        "supportsEvaluateForHovers": true,
                        "supportsSetVariable": false,
                        "supportsConditionalBreakpoints": true,
                        "supportsFunctionBreakpoints": true,
                        "supportsDataBreakpoints": true,
                        "supportsExceptionFilterOptions": false,
                        "exceptionBreakpointFilters": [
                            {"filter": "uncaught", "label": "Uncaught Exceptions", "default": true},
                            {"filter": "caught", "label": "Caught Exceptions"},
                            {"filter": "throw", "label": "All Thrown Exceptions"}
                        ],
                        "supportsLogPoints": false,
                        "supportsStepBack": false,
                        "supportsRestartRequest": false,
                    })),
                );
                self.dap_event("initialized", json!({}));
                DapAction::Stay
            }
            "launch" => {
                self.dap_stop_on_entry = request["arguments"]
                    .get("stopOnEntry")
                    .and_then(JsonValue::as_bool)
                    .unwrap_or(false);
                self.dap_respond(request, Some(json!({})));
                DapAction::Stay
            }
            "attach" => {
                self.dap_respond_error(request, "attach is not supported yet");
                DapAction::Stay
            }
            "setBreakpoints" => {
                self.dap_set_breakpoints(request);
                DapAction::Stay
            }
            "setFunctionBreakpoints" => {
                self.dap_set_function_breakpoints(request);
                DapAction::Stay
            }
            "dataBreakpointInfo" => {
                self.dap_data_breakpoint_info(request);
                DapAction::Stay
            }
            "setDataBreakpoints" => {
                self.dap_set_data_breakpoints(request);
                DapAction::Stay
            }
            "setExceptionBreakpoints" => {
                self.dap_set_exception_breakpoints(request);
                DapAction::Stay
            }
            "threads" => {
                self.dap_threads(request);
                DapAction::Stay
            }
            "stackTrace" => {
                self.dap_stack_trace(request);
                DapAction::Stay
            }
            "scopes" => {
                self.dap_scopes(request);
                DapAction::Stay
            }
            "variables" => {
                self.dap_variables(request);
                DapAction::Stay
            }
            "evaluate" => {
                self.dap_evaluate(request);
                DapAction::Stay
            }
            "pause" => {
                // A pause signal that arrived while already stopped must not
                // make the next continue stop again immediately.
                super::PAUSE_REQUESTED.store(false, std::sync::atomic::Ordering::Relaxed);
                self.dap_respond(request, Some(json!({})));
                DapAction::Stay
            }
            "continue" => {
                self.step = None;
                self.dap_respond(request, Some(json!({"allThreadsContinued": true})));
                DapAction::Resume
            }
            "next" => {
                self.dap_start_step(StepMode::Next);
                self.dap_respond(request, Some(json!({"allThreadsContinued": true})));
                DapAction::Resume
            }
            "stepIn" => {
                self.dap_start_step(StepMode::Step);
                self.dap_respond(request, Some(json!({"allThreadsContinued": true})));
                DapAction::Resume
            }
            "stepOut" => {
                self.dap_start_step(StepMode::Finish);
                self.dap_respond(request, Some(json!({"allThreadsContinued": true})));
                DapAction::Resume
            }
            "disconnect" | "terminate" => {
                self.dap_respond(request, Some(json!({})));
                DapAction::Exit
            }
            _ => {
                self.dap_respond_error(
                    request,
                    &format!("unsupported DAP command '{command}'"),
                );
                DapAction::Stay
            }
        }
    }

    /// Responds to one request, exiting if the transport has gone away.
    fn dap_respond(&mut self, request: &JsonValue, body: Option<JsonValue>) {
        let Some(dap) = self.dap.as_mut() else {
            return;
        };
        if let Err(error) = dap.respond(request, body) {
            eprintln!("elephc-debug: DAP write failed: {error}");
            std::process::exit(2);
        }
    }

    /// Sends one failed response, exiting if the transport has gone away.
    fn dap_respond_error(&mut self, request: &JsonValue, message: &str) {
        let Some(dap) = self.dap.as_mut() else {
            return;
        };
        if let Err(error) = dap.respond_error(request, message) {
            eprintln!("elephc-debug: DAP write failed: {error}");
            std::process::exit(2);
        }
    }

    /// Reports a runtime condition failure to the DAP client (or stderr).
    pub(crate) fn report_condition_error(&mut self, condition: &str, error: &str) {
        let text = format!("Condition '{condition}' failed: {error}\n");
        if self.dap.is_some() {
            self.dap_event("output", json!({"category": "stderr", "output": text}));
        } else {
            eprint!("{text}");
        }
    }

    /// Sends one DAP event, exiting if the transport has gone away.
    fn dap_event(&mut self, event: &str, body: JsonValue) {
        let Some(dap) = self.dap.as_mut() else {
            return;
        };
        if let Err(error) = dap.event(event, body) {
            eprintln!("elephc-debug: DAP write failed: {error}");
            std::process::exit(2);
        }
    }

    /// Returns a diagnostic when a requested source no longer matches the
    /// compiler-recorded SHA-256, or `None` when it is current or unhashed.
    fn stale_source_message(&self, source_id: u64, requested: &str) -> Option<String> {
        let expected = self.source_hashes.get(&source_id)?;
        let recorded = self.sources.get(&source_id)?;
        let bytes = [recorded.as_str(), requested]
            .into_iter()
            .find_map(|path| std::fs::read(path).ok())?;
        if sha256_hex(&bytes) == *expected {
            return None;
        }
        Some(format!(
            "source changed since compilation: {}",
            recorded
        ))
    }

    /// Arms the sites requested by one DAP `setBreakpoints` request.
    fn dap_set_breakpoints(&mut self, request: &JsonValue) {
        let arguments = &request["arguments"];
        let path = arguments
            .get("source")
            .and_then(|source| source.get("path"))
            .and_then(JsonValue::as_str)
            .unwrap_or("");
        let matching_sources: Vec<u64> = self
            .sources
            .iter()
            .filter(|(id, _)| source_matches(&self.sources, **id, path))
            .map(|(id, _)| *id)
            .collect();
        let stale = matching_sources
            .iter()
            .find_map(|id| self.stale_source_message(*id, path));
        let remove: Vec<u32> = self
            .break_sites
            .iter()
            .copied()
            .filter(|id| {
                self.sites
                    .get(id)
                    .is_some_and(|site| source_matches(&self.sources, site.source_id, path))
            })
            .collect();
        for id in remove {
            self.break_sites.remove(&id);
            self.breakpoint_conditions.remove(&id);
        }
        let mut results = Vec::new();
        if let Some(breakpoints) = arguments
            .get("breakpoints")
            .and_then(JsonValue::as_array)
        {
            for breakpoint in breakpoints {
                let line = breakpoint
                    .get("line")
                    .and_then(JsonValue::as_u64)
                    .unwrap_or(0);
                let matching: Vec<u32> = self
                    .sites
                    .iter()
                    .filter(|(_, site)| {
                        site.line == line
                            && source_matches(&self.sources, site.source_id, path)
                    })
                    .map(|(id, _)| *id)
                    .collect();
                let condition = breakpoint
                    .get("condition")
                    .and_then(JsonValue::as_str)
                    .map(str::trim)
                    .filter(|condition| !condition.is_empty());
                let condition_error =
                    condition.and_then(|condition| condition::validate(condition).err());
                let verified =
                    stale.is_none() && condition_error.is_none() && !matching.is_empty();
                let breakpoint_id = matching.first().copied();
                if verified {
                    for id in &matching {
                        self.break_sites.insert(*id);
                        if let Some(condition) = condition {
                            self.breakpoint_conditions.insert(*id, condition.to_string());
                        } else {
                            self.breakpoint_conditions.remove(id);
                        }
                    }
                }
                let mut result = json!({"verified": verified, "line": line});
                if let Some(id) = breakpoint_id.filter(|_| verified) {
                    result["id"] = json!(id);
                }
                if let Some(message) = stale.as_ref().or(condition_error.as_ref()) {
                    result["message"] = json!(message);
                }
                results.push(result);
            }
        }
        self.dap_respond(request, Some(json!({"breakpoints": results})));
    }

    /// Answers `dataBreakpointInfo` for a frame local or an object property.
    fn dap_data_breakpoint_info(&mut self, request: &JsonValue) {
        let arguments = &request["arguments"];
        let reference = arguments
            .get("variablesReference")
            .and_then(JsonValue::as_i64)
            .unwrap_or(0);
        let name = arguments
            .get("name")
            .and_then(JsonValue::as_str)
            .unwrap_or("")
            .trim()
            .trim_start_matches('$');
        let Some(container) = self.variable_refs.get(&reference).copied() else {
            self.dap_respond(
                request,
                Some(json!({
                    "dataId": JsonValue::Null,
                    "description": "Unknown variable container",
                    "accessTypes": [],
                    "canPersist": false,
                })),
            );
            return;
        };
        let (data_id, description) = match container {
            VariableContainer::Frame { thread_id, index } => {
                let Some(frame) = self.activation_for(thread_id, index) else {
                    self.dap_respond(
                        request,
                        Some(json!({
                            "dataId": JsonValue::Null,
                            "description": "Frame is no longer active",
                            "accessTypes": [],
                            "canPersist": false,
                        })),
                    );
                    return;
                };
                let function_id = frame.function_id as u64;
                let local = self
                    .locals
                    .get(&function_id)
                    .and_then(|locals| locals.iter().find(|local| local.name == name));
                match local {
                    Some(local) => {
                        let function = self
                            .functions
                            .get(&function_id)
                            .map(|function| super::display_function_name(&function.name))
                            .unwrap_or("<unknown>");
                        (
                            json!(format!("local:{function_id}:{}", local.name)),
                            format!("Watch ${} in {function}", local.name),
                        )
                    }
                    None => (
                        JsonValue::Null,
                        format!("No watchable local named '${name}' in this frame"),
                    ),
                }
            }
            VariableContainer::Value(value) if value.tag == value::TAG_OBJECT => {
                let plain = strip_property_suffix(name);
                let class = value::object_class_id(value.lo).and_then(|class_id| {
                    self.classes
                        .get(&class_id)
                        .map(|class| (class_id, class))
                });
                match class {
                    Some((class_id, class)) => {
                        // A folded `__debugInfo()` exposes its array key, not the
                        // underlying property name. Resolve either spelling to the
                        // same declared slot so a projected child stays watchable.
                        let property = class
                            .properties
                            .iter()
                            .find(|property| property.name == plain)
                            .or_else(|| {
                                class
                                    .debug_info
                                    .entries
                                    .iter()
                                    .find(|entry| entry.key == plain)
                                    .and_then(|entry| {
                                        let property_name = entry.property.as_deref()?;
                                        class
                                            .properties
                                            .iter()
                                            .find(|property| property.name == property_name)
                                    })
                            });
                        if let Some(property) = property {
                            // A virtual property has no backing slot; reading it
                            // would execute the getter, so there is nothing
                            // read-only to watch. Backed hooked properties still
                            // expose their raw backing slot.
                            if class
                                .property_hooks
                                .get(&property.name)
                                .is_some_and(|hooks| hooks.is_virtual)
                            {
                                (
                                    JsonValue::Null,
                                    format!(
                                        "No watchable backing slot for virtual property {} of {}",
                                        plain, class.name
                                    ),
                                )
                            } else {
                                (
                                    json!(format!(
                                        "object:{}:{}:{}",
                                        value.lo, class_id, property.offset
                                    )),
                                    format!("Watch property {} of {}", plain, class.name),
                                )
                            }
                        } else if let Some(offset) = class.dynamic_offset {
                            // Dynamic properties are string-keyed hash entries,
                            // not declared slots. Match the raw requested name
                            // (the child name is the hash key, not a visibility
                            // suffix).
                            if value::object_dynamic_property_value(
                                value.lo,
                                class_id,
                                offset,
                                name.as_bytes(),
                            )
                            .is_some()
                            {
                                (
                                    json!(format!(
                                        "dynamic:{}:{}:{}:{}",
                                        value.lo,
                                        class_id,
                                        offset,
                                        hex_encode(name.as_bytes())
                                    )),
                                    format!("Watch dynamic property {} of {}", name, class.name),
                                )
                            } else {
                                (
                                    JsonValue::Null,
                                    format!("No watchable property '{name}' on this object"),
                                )
                            }
                        } else {
                            (
                                JsonValue::Null,
                                format!("No watchable property '{name}' on this object"),
                            )
                        }
                    }
                    None => (
                        JsonValue::Null,
                        format!("No watchable property '{name}' on this object"),
                    ),
                }
            }
            VariableContainer::Value(_) => (
                JsonValue::Null,
                "Only declared object properties can be watched".to_string(),
            ),
        };
        self.dap_respond(
            request,
            Some(json!({
                "dataId": data_id,
                "description": description,
                "accessTypes": if data_id.is_string() { json!(["write"]) } else { json!([]) },
                "canPersist": false,
            })),
        );
    }

    /// Replaces the active data-breakpoint write watches.
    fn dap_set_data_breakpoints(&mut self, request: &JsonValue) {
        self.data_breakpoints.clear();
        let mut results = Vec::new();
        if let Some(breakpoints) = request["arguments"]
            .get("breakpoints")
            .and_then(JsonValue::as_array)
        {
            for breakpoint in breakpoints {
                let Some(data_id) = breakpoint.get("dataId").and_then(JsonValue::as_str) else {
                    results.push(json!({
                        "verified": false,
                        "message": "data breakpoint has no dataId",
                    }));
                    continue;
                };
                let access = breakpoint
                    .get("accessType")
                    .and_then(JsonValue::as_str)
                    .unwrap_or("write");
                if access != "write" {
                    results.push(json!({
                        "verified": false,
                        "message": format!(
                            "only write data breakpoints are supported, got '{access}'"
                        ),
                    }));
                    continue;
                }
                if let Some(rest) = data_id.strip_prefix("local:") {
                    let mut parts = rest.splitn(2, ':');
                    let (Some(function_id), Some(local_name)) =
                        (parts.next(), parts.next())
                    else {
                        results.push(json!({
                            "verified": false,
                            "message": format!("malformed dataId '{data_id}'"),
                        }));
                        continue;
                    };
                    let Ok(function_id) = function_id.parse::<u64>() else {
                        results.push(json!({
                            "verified": false,
                            "message": format!("malformed function id in '{data_id}'"),
                        }));
                        continue;
                    };
                    let Some(local) = self
                        .locals
                        .get(&function_id)
                        .and_then(|locals| locals.iter().find(|local| local.name == local_name))
                    else {
                        results.push(json!({
                            "verified": false,
                            "message": format!(
                                "no local '${local_name}' in the watched function"
                            ),
                        }));
                        continue;
                    };
                    let last = self.local_display(function_id, &local.name);
                    self.data_breakpoints.push(DataBreakpoint {
                        target: DataWatchTarget::Local {
                            function_id,
                            local_name: local.name.clone(),
                        },
                        last,
                    });
                    results.push(json!({"verified": true}));
                    continue;
                }
                if let Some(rest) = data_id.strip_prefix("object:") {
                    let mut parts = rest.splitn(3, ':');
                    let (Some(object), Some(class_id), Some(offset)) =
                        (parts.next(), parts.next(), parts.next())
                    else {
                        results.push(json!({
                            "verified": false,
                            "message": format!("malformed dataId '{data_id}'"),
                        }));
                        continue;
                    };
                    let (Ok(object), Ok(class_id), Ok(offset)) = (
                        object.parse::<u64>(),
                        class_id.parse::<u64>(),
                        offset.parse::<usize>(),
                    ) else {
                        results.push(json!({
                            "verified": false,
                            "message": format!("malformed object dataId '{data_id}'"),
                        }));
                        continue;
                    };
                    let property = self
                        .classes
                        .get(&class_id)
                        .and_then(|class| {
                            class
                                .properties
                                .iter()
                                .find(|property| property.offset == offset)
                        })
                        .cloned();
                    let Some(property) = property else {
                        results.push(json!({
                            "verified": false,
                            "message": "the watched property is not in the class layout",
                        }));
                        continue;
                    };
                    let Some(current) =
                        value::object_property_value(object, class_id, &property)
                    else {
                        results.push(json!({
                            "verified": false,
                            "message": "the watched object is no longer live",
                        }));
                        continue;
                    };
                    let last = value::render(&value::describe_with(current, &self.classes));
                    self.data_breakpoints.push(DataBreakpoint {
                        target: DataWatchTarget::Property {
                            object,
                            class_id,
                            property,
                        },
                        last: Some(last),
                    });
                    results.push(json!({"verified": true}));
                    continue;
                }
                if let Some(rest) = data_id.strip_prefix("dynamic:") {
                    let mut parts = rest.splitn(4, ':');
                    let (Some(object), Some(class_id), Some(offset), Some(key_hex)) =
                        (parts.next(), parts.next(), parts.next(), parts.next())
                    else {
                        results.push(json!({
                            "verified": false,
                            "message": format!("malformed dataId '{data_id}'"),
                        }));
                        continue;
                    };
                    let (Ok(object), Ok(class_id), Ok(offset), Some(key)) = (
                        object.parse::<u64>(),
                        class_id.parse::<u64>(),
                        offset.parse::<usize>(),
                        hex_decode(key_hex),
                    ) else {
                        results.push(json!({
                            "verified": false,
                            "message": format!("malformed dynamic dataId '{data_id}'"),
                        }));
                        continue;
                    };
                    let layout_matches = self
                        .classes
                        .get(&class_id)
                        .is_some_and(|class| class.dynamic_offset == Some(offset));
                    if !layout_matches {
                        results.push(json!({
                            "verified": false,
                            "message": "the watched dynamic hash is not in the class layout",
                        }));
                        continue;
                    }
                    let Some(current) =
                        value::object_dynamic_property_value(object, class_id, offset, &key)
                    else {
                        results.push(json!({
                            "verified": false,
                            "message": "the watched dynamic property is no longer live",
                        }));
                        continue;
                    };
                    let last = value::render(&value::describe_with(current, &self.classes));
                    self.data_breakpoints.push(DataBreakpoint {
                        target: DataWatchTarget::Dynamic {
                            object,
                            class_id,
                            dynamic_offset: offset,
                            key,
                        },
                        last: Some(last),
                    });
                    results.push(json!({"verified": true}));
                    continue;
                }
                results.push(json!({
                    "verified": false,
                    "message": format!("malformed dataId '{data_id}'"),
                }));
            }
        }
        self.dap_respond(request, Some(json!({"breakpoints": results})));
    }

    /// Renders a local from the current activation, if it is the watched one.
    fn local_display(&self, function_id: u64, local_name: &str) -> Option<String> {
        let activation = self.activation(0)?;
        if activation.function_id as u64 != function_id || activation.frame == 0 {
            return None;
        }
        let local = self
            .locals
            .get(&function_id)?
            .iter()
            .find(|local| local.name == local_name)?;
        let value = value::local_value(local, activation.frame);
        Some(value::render(&value::describe_with(value, &self.classes)))
    }

    /// Checks watched locals/properties at a site and reports whether any value changed.
    pub(crate) fn check_data_breakpoints(&mut self, frame: usize) -> bool {
        if self.dap.is_none() || self.data_breakpoints.is_empty() {
            return false;
        }
        let Some(activation) = self.activation(0) else {
            return false;
        };
        if activation.frame != frame {
            return false;
        }
        let function_id = activation.function_id as u64;
        let locals = self
            .locals
            .get(&function_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let mut current: Vec<(usize, String)> = Vec::new();
        for (index, watch) in self.data_breakpoints.iter().enumerate() {
            let display = match &watch.target {
                DataWatchTarget::Local {
                    function_id: watched_function,
                    local_name,
                } => {
                    if *watched_function != function_id {
                        continue;
                    }
                    let Some(local) = locals.iter().find(|local| local.name == *local_name)
                    else {
                        continue;
                    };
                    let value = value::local_value(local, frame);
                    value::render(&value::describe_with(value, &self.classes))
                }
                DataWatchTarget::Property {
                    object,
                    class_id,
                    property,
                } => {
                    // The watch follows the object address. Validate the class id
                    // before reading the slot so a freed/reused allocation is
                    // skipped rather than decoded as a property.
                    let Some(value) =
                        value::object_property_value(*object, *class_id, property)
                    else {
                        continue;
                    };
                    value::render(&value::describe_with(value, &self.classes))
                }
                DataWatchTarget::Dynamic {
                    object,
                    class_id,
                    dynamic_offset,
                    key,
                } => {
                    let Some(value) = value::object_dynamic_property_value(
                        *object,
                        *class_id,
                        *dynamic_offset,
                        key,
                    ) else {
                        continue;
                    };
                    value::render(&value::describe_with(value, &self.classes))
                }
            };
            current.push((index, display));
        }
        let mut changed = false;
        for (index, display) in current {
            let watch = &mut self.data_breakpoints[index];
            match &watch.last {
                Some(last) if *last == display => {}
                Some(_) => {
                    watch.last = Some(display);
                    changed = true;
                }
                None => watch.last = Some(display),
            }
        }
        changed
    }

    /// Replaces the function breakpoint table from one DAP request.
    fn dap_set_function_breakpoints(&mut self, request: &JsonValue) {
        self.function_breakpoints.clear();
        let mut results = Vec::new();
        if let Some(breakpoints) = request["arguments"]
            .get("breakpoints")
            .and_then(JsonValue::as_array)
        {
            for breakpoint in breakpoints {
                let name = breakpoint
                    .get("name")
                    .and_then(JsonValue::as_str)
                    .unwrap_or("")
                    .trim();
                let condition = breakpoint
                    .get("condition")
                    .and_then(JsonValue::as_str)
                    .map(str::trim)
                    .filter(|condition| !condition.is_empty());
                let condition_error =
                    condition.and_then(|condition| condition::validate(condition).err());
                let matching: Vec<u32> = self
                    .functions
                    .iter()
                    .filter(|(_, function)| function_name_matches(&function.name, name))
                    .map(|(id, _)| *id as u32)
                    .collect();
                let message = condition_error.or_else(|| {
                    matching
                        .is_empty()
                        .then(|| format!("no function named '{name}'"))
                });
                let verified = message.is_none() && !matching.is_empty();
                let mut result = json!({"verified": verified});
                if let Some(id) = matching.first() {
                    result["id"] = json!(id);
                }
                if let Some(message) = &message {
                    result["message"] = json!(message);
                }
                if verified {
                    for id in matching {
                        self.function_breakpoints
                            .insert(id, condition.map(str::to_string));
                    }
                }
                results.push(result);
            }
        }
        super::FUNCTION_BREAKPOINTS_ACTIVE
            .store(!self.function_breakpoints.is_empty(), std::sync::atomic::Ordering::Relaxed);
        self.dap_respond(request, Some(json!({"breakpoints": results})));
    }

    /// Sends the main thread plus every live fiber/generator as a thread.
    fn dap_threads(&mut self, request: &JsonValue) {
        let mut threads = vec![json!({"id": 1, "name": "main"})];
        for fiber in self.live_fibers() {
            let id = self.fiber_thread(fiber);
            threads.push(json!({"id": id, "name": format!("fiber {id}")}));
        }
        self.dap_respond(request, Some(json!({"threads": threads})));
    }

    /// Sends one thread-like activation stack as DAP stack frames.
    fn dap_stack_trace(&mut self, request: &JsonValue) {
        let arguments = &request["arguments"];
        let start = arguments
            .get("startFrame")
            .and_then(JsonValue::as_u64)
            .unwrap_or(0) as usize;
        let levels = arguments
            .get("levels")
            .and_then(JsonValue::as_u64)
            .unwrap_or(20) as usize;
        let thread_id = arguments
            .get("threadId")
            .and_then(JsonValue::as_i64)
            .unwrap_or_else(|| self.current_thread_id());
        let activation = self.thread_stack(thread_id);
        let total = activation.len();
        let mut frames = Vec::new();
        for (index, frame) in activation
            .iter()
            .rev()
            .enumerate()
            .skip(start)
            .take(levels)
        {
            let function = self.functions.get(&(frame.function_id as u64));
            let name = function
                .map(|function| super::display_function_name(&function.name))
                .unwrap_or("<unknown>");
            let mut dap_frame = json!({
                "id": frame_id(thread_id, index),
                "name": name,
                "line": 1,
                "column": 1,
                "presentationHint": "normal",
            });
            if let Some(site) = frame.last_site.and_then(|id| self.sites.get(&id)) {
                dap_frame["line"] = json!(site.line);
                dap_frame["column"] = json!(site.column);
                if let Some(path) = self.sources.get(&site.source_id) {
                    let resolved = dap_source_path(path);
                    let file = Path::new(&resolved)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or(&resolved);
                    dap_frame["source"] = json!({"name": file, "path": resolved});
                }
            } else if let Some(function) = function {
                if let (Some(source_id), Some(line)) = (function.source_id, function.line_start) {
                    dap_frame["line"] = json!(line);
                    if let Some(path) = self.sources.get(&source_id) {
                        let resolved = dap_source_path(path);
                        let file = Path::new(&resolved)
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or(&resolved);
                        dap_frame["source"] = json!({"name": file, "path": resolved});
                    }
                }
            }
            frames.push(dap_frame);
        }
        self.dap_respond(
            request,
            Some(json!({"stackFrames": frames, "totalFrames": total})),
        );
    }

    /// Sends one Locals scope for the requested frame.
    fn dap_scopes(&mut self, request: &JsonValue) {
        let frame_id = request["arguments"]
            .get("frameId")
            .and_then(JsonValue::as_i64)
            .unwrap_or(0);
        let Some((thread_id, index)) = decode_frame_id(frame_id) else {
            self.dap_respond_error(request, "frame is no longer active");
            return;
        };
        if self.activation_for(thread_id, index).is_none() {
            self.dap_respond_error(request, "frame is no longer active");
            return;
        }
        let reference = self.dap_frame_ref(thread_id, index);
        self.dap_respond(
            request,
            Some(json!({
                "scopes": [{
                    "name": "Locals",
                    "variablesReference": reference,
                    "expensive": false,
                    "presentationHint": "locals",
                }]
            })),
        );
    }

    /// Sends the variables for a frame or nested value reference.
    fn dap_variables(&mut self, request: &JsonValue) {
        let reference = request["arguments"]
            .get("variablesReference")
            .and_then(JsonValue::as_i64)
            .unwrap_or(0);
        let Some(container) = self.variable_refs.get(&reference).copied() else {
            self.dap_respond_error(request, "unknown variablesReference");
            return;
        };
        let variables: Vec<JsonValue> = match container {
            VariableContainer::Frame { thread_id, index } => {
                let Some(frame) = self.activation_for(thread_id, index) else {
                    self.dap_respond(request, Some(json!({"variables": []})));
                    return;
                };
                let function_id = frame.function_id as u64;
                let frame_pointer = frame.frame;
                let entries: Vec<(String, value::Value)> = match self.locals.get(&function_id) {
                    Some(locals) => locals
                        .iter()
                        .filter(|local| !local.name.contains('#'))
                        .map(|local| {
                            (
                                format!("${}", local.name),
                                value::local_value(local, frame_pointer),
                            )
                        })
                        .collect(),
                    None => Vec::new(),
                };
                entries
                    .into_iter()
                    .map(|(name, value)| self.dap_variable(name, value))
                    .collect()
            }
            VariableContainer::Value(value) => value::children_with_annotated(value, &self.classes)
                .into_iter()
                .map(|child| self.dap_object_child(child))
                .collect(),
        };
        self.dap_respond(request, Some(json!({"variables": variables})));
    }

    /// Evaluates a read-only expression in the requested frame.
    fn dap_evaluate(&mut self, request: &JsonValue) {
        let arguments = &request["arguments"];
        let expression = arguments
            .get("expression")
            .and_then(JsonValue::as_str)
            .unwrap_or("");
        let frame_id = arguments
            .get("frameId")
            .and_then(JsonValue::as_i64)
            .unwrap_or(0);
        let Some((thread_id, index)) = decode_frame_id(frame_id) else {
            self.dap_respond_error(request, "frame is no longer active");
            return;
        };
        let Some(frame) = self.activation_for(thread_id, index) else {
            self.dap_respond_error(request, "frame is no longer active");
            return;
        };
        let evaluated = {
            let locals = self
                .locals
                .get(&(frame.function_id as u64))
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            eval::evaluate(expression, locals, frame.frame, &self.classes)
        };
        match evaluated {
            Ok(eval::EvalValue::Borrowed { value, note }) => {
                let description = value::describe_with(value, &self.classes);
                let variables_reference = if description.has_children {
                    self.dap_value_ref(value)
                } else {
                    0
                };
                let mut result = value::render(&description);
                if let Some(note) = note {
                    result.push_str(&note);
                }
                self.dap_respond(
                    request,
                    Some(json!({
                        "result": result,
                        "type": description.type_name,
                        "variablesReference": variables_reference,
                    })),
                );
            }
            Ok(eval::EvalValue::Scalar(scalar)) => {
                let description = value::describe_scalar(&scalar);
                self.dap_respond(
                    request,
                    Some(json!({
                        "result": value::render(&description),
                        "type": description.type_name,
                        "variablesReference": 0,
                    })),
                );
            }
            Err(message) => self.dap_respond_error(request, &message),
        }
    }

    /// Allocates or records one frame variables reference.
    fn dap_frame_ref(&mut self, thread_id: i64, index: usize) -> i64 {
        let reference = self.next_variable_ref;
        self.next_variable_ref += 1;
        self.variable_refs
            .insert(reference, VariableContainer::Frame { thread_id, index });
        reference
    }

    /// Allocates or records one value variables reference.
    fn dap_value_ref(&mut self, value: value::Value) -> i64 {
        let reference = self.next_variable_ref;
        self.next_variable_ref += 1;
        self.variable_refs
            .insert(reference, VariableContainer::Value(value));
        reference
    }

    /// Builds one DAP `Variable` for an object/array child, surfacing the
    /// read-only property-hook caveat without executing any accessor.
    fn dap_object_child(&mut self, child: value::ObjectChild) -> JsonValue {
        // A safe `__debugInfo()` literal was rendered at compile time; it has no
        // live storage to expand or watch, so the value is final.
        if let Some(text) = &child.static_text {
            return json!({
                "name": child.name,
                "value": text,
                "type": child.static_type.unwrap_or_else(|| "mixed".to_string()),
                "variablesReference": 0,
            });
        }
        let Some(note) = child.hook_note else {
            return self.dap_variable(child.name, child.value);
        };
        if note.is_virtual() {
            // There is no backing slot; calling the getter would run user code
            // while the program is stopped, which the debugger never does.
            return json!({
                "name": child.name,
                "value": note.virtual_display(),
                "type": note.php_type,
                "variablesReference": 0,
            });
        }
        let description = value::describe_with(child.value, &self.classes);
        let variables_reference = if description.has_children {
            self.dap_value_ref(child.value)
        } else {
            0
        };
        json!({
            "name": child.name,
            "value": format!(
                "{}{}",
                value::render(&description),
                note.backing_suffix()
            ),
            "type": description.type_name,
            "variablesReference": variables_reference,
        })
    }

    /// Builds one DAP `Variable` and allocates a child reference when needed.
    fn dap_variable(&mut self, name: String, value: value::Value) -> JsonValue {
        let description = value::describe_with(value, &self.classes);
        let variables_reference = if description.has_children {
            self.dap_value_ref(value)
        } else {
            0
        };
        json!({
            "name": name,
            "value": value::render(&description),
            "type": description.type_name,
            "variablesReference": variables_reference,
        })
    }

    /// Records the depth and mode of the next stepping command.
    fn dap_start_step(&mut self, mode: StepMode) {
        let depth = super::ACTIVATIONS.with(|stack| stack.borrow().len());
        self.step = Some(StepState { mode, depth });
    }
}

/// Binds a one-shot mode-0600 Unix socket and waits for the DAP client.
///
/// The socket path is the attach credential: only a process that can open it
/// can drive the waiting debug binary. The listener is removed after the first
/// client connects, so a second adapter cannot race the existing session.
pub(crate) fn listen_for_client(path: &str) -> std::io::Result<DapConnection> {
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        if metadata.file_type().is_socket() {
            let _ = std::fs::remove_file(path);
        } else {
            return Err(IoError::new(
                ErrorKind::AlreadyExists,
                format!("DAP attach path exists and is not a socket: {path}"),
            ));
        }
    }
    // The pid file is written BEFORE the socket is bound. A client that can
    // connect can therefore read the pid without racing the accept cleanup; the
    // CLI `--attach` path reads it before `connect` for the same reason.
    let pid_path = format!("{path}.pid");
    let _ = std::fs::remove_file(&pid_path);
    let mut pid_file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&pid_path)?;
    let written = writeln!(pid_file, "{}", std::process::id()).and_then(|_| pid_file.flush());
    drop(pid_file);
    if let Err(error) = written {
        let _ = std::fs::remove_file(&pid_path);
        return Err(error);
    }
    let listener = match UnixListener::bind(path) {
        Ok(listener) => listener,
        Err(error) => {
            let _ = std::fs::remove_file(&pid_path);
            return Err(error);
        }
    };
    if let Err(error) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)) {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(&pid_path);
        return Err(error);
    }
    let accepted = listener.accept();
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(&pid_path);
    let (stream, _) = accepted?;
    DapConnection::from_stream(stream)
}

/// Returns cryptographically random bytes from the OS entropy source.
fn random_bytes<const N: usize>() -> std::io::Result<[u8; N]> {
    let mut bytes = [0u8; N];
    let mut source = std::fs::File::open("/dev/urandom")?;
    source.read_exact(&mut bytes)?;
    Ok(bytes)
}

/// Computes HMAC-SHA256 (RFC 2104) over `message` with `key`.
fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut block = [0u8; 64];
    if key.len() > 64 {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    let mut outer = Sha256::new();
    for byte in block {
        inner.update([byte ^ 0x36]);
        outer.update([byte ^ 0x5c]);
    }
    inner.update(message);
    outer.update(inner.finalize());
    outer.finalize().into()
}

/// Server proof: HMAC over the domain-separated client and server nonces.
fn tcp_attach_server_tag(key: &[u8], nonce_c: &[u8], nonce_s: &[u8]) -> [u8; 32] {
    let mut message = Vec::with_capacity(1 + nonce_c.len() + nonce_s.len());
    message.push(b'S');
    message.extend_from_slice(nonce_c);
    message.extend_from_slice(nonce_s);
    hmac_sha256(key, &message)
}

/// Client proof: HMAC over the reverse nonce order and a distinct domain byte.
fn tcp_attach_client_tag(key: &[u8], nonce_s: &[u8], nonce_c: &[u8]) -> [u8; 32] {
    let mut message = Vec::with_capacity(1 + nonce_s.len() + nonce_c.len());
    message.push(b'C');
    message.extend_from_slice(nonce_s);
    message.extend_from_slice(nonce_c);
    hmac_sha256(key, &message)
}

/// Compares two tags without an early exit.
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (a, b) in left.iter().zip(right) {
        difference |= a ^ b;
    }
    difference == 0
}

/// Encodes a raw attach key as lowercase hexadecimal.
fn encode_attach_key(key: &[u8]) -> String {
    key.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Writes one owner-only attach file, creating it from scratch.
fn write_attach_file(path: &str, contents: &[u8]) -> std::io::Result<()> {
    let _ = std::fs::remove_file(path);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(contents)?;
    file.flush()
}

/// Removes the key, address and pid files for one TCP attach listener.
fn remove_tcp_attach_files(key_path: &str) {
    let _ = std::fs::remove_file(key_path);
    let _ = std::fs::remove_file(format!("{key_path}.addr"));
    let _ = std::fs::remove_file(format!("{key_path}.pid"));
}

/// Serves one loopback TCP attach endpoint with a mode-0600 key file.
///
/// The runtime generates a fresh 32-byte key, writes it (and its actual
/// `127.0.0.1:PORT` and pid) next to `key_path`, and accepts clients until one
/// proves possession of the key. A rejected client gets one status byte and the
/// listener keeps running, so an operator can retry with the right key. The
/// handshake is mutual: the server proves the key first, then the client proves
/// it; the key itself never crosses the wire.
pub(crate) fn listen_tcp_for_client(addr: &str, key_path: &str) -> std::io::Result<DapConnection> {
    if key_path.is_empty() {
        return Err(IoError::new(
            ErrorKind::InvalidInput,
            "ELEPHC_DEBUG_ATTACH_KEY must name the attach key file",
        ));
    }
    let requested: SocketAddr = addr.parse().map_err(|_| {
        IoError::new(
            ErrorKind::InvalidInput,
            format!("attach address '{addr}' must be IP:PORT"),
        )
    })?;
    if !requested.ip().is_loopback() {
        return Err(IoError::new(
            ErrorKind::InvalidInput,
            "TCP attach only listens on a loopback address",
        ));
    }
    let key = random_bytes::<32>()?;
    write_attach_file(key_path, format!("{}\n", encode_attach_key(&key)).as_bytes())?;
    let listener = match TcpListener::bind(requested) {
        Ok(listener) => listener,
        Err(error) => {
            remove_tcp_attach_files(key_path);
            return Err(error);
        }
    };
    let local = match listener.local_addr() {
        Ok(local) => local,
        Err(error) => {
            remove_tcp_attach_files(key_path);
            return Err(error);
        }
    };
    if let Err(error) = write_attach_file(
        &format!("{key_path}.addr"),
        format!("{local}\n").as_bytes(),
    ) {
        remove_tcp_attach_files(key_path);
        return Err(error);
    }
    if let Err(error) = write_attach_file(
        &format!("{key_path}.pid"),
        format!("{}\n", std::process::id()).as_bytes(),
    ) {
        remove_tcp_attach_files(key_path);
        return Err(error);
    }
    loop {
        let (mut stream, peer) = match listener.accept() {
            Ok(pair) => pair,
            Err(error) => {
                remove_tcp_attach_files(key_path);
                return Err(error);
            }
        };
        if !peer.ip().is_loopback() {
            continue;
        }
        let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
        let authenticated = (|| -> std::io::Result<bool> {
            let mut nonce_c = [0u8; 32];
            stream.read_exact(&mut nonce_c)?;
            let nonce_s = random_bytes::<32>()?;
            stream.write_all(&nonce_s)?;
            stream.write_all(&tcp_attach_server_tag(&key, &nonce_c, &nonce_s))?;
            let mut client_tag = [0u8; 32];
            stream.read_exact(&mut client_tag)?;
            Ok(constant_time_eq(
                &client_tag,
                &tcp_attach_client_tag(&key, &nonce_s, &nonce_c),
            ))
        })();
        match authenticated {
            Ok(true) => {
                let _ = stream.write_all(&[1]);
                let _ = stream.set_read_timeout(None);
                let _ = stream.set_write_timeout(None);
                remove_tcp_attach_files(key_path);
                return DapConnection::from_tcp_stream(stream);
            }
            Ok(false) => {
                let _ = stream.write_all(&[0]);
                // A failed proof is not fatal: keep listening for the correct key.
            }
            Err(_) => {
                // The peer aborted before completing the proof; it already knows
                // the result it produced. Do not write into a closed socket.
            }
        }
    }
}

/// Returns the lowercase hexadecimal SHA-256 digest of `bytes`.
fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Strips the ` (private)` / ` (protected)` suffix DAP children carry.
fn strip_property_suffix(name: &str) -> &str {
    name.strip_suffix(" (private)")
        .or_else(|| name.strip_suffix(" (protected)"))
        .unwrap_or(name)
}

/// Returns lowercase hexadecimal for a byte key carried in a dataId.
fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Decodes a hexadecimal dataId key back to raw property-name bytes.
fn hex_decode(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    let mut bytes = Vec::with_capacity(text.len() / 2);
    for pair in text.as_bytes().chunks_exact(2) {
        let pair = std::str::from_utf8(pair).ok()?;
        bytes.push(u8::from_str_radix(pair, 16).ok()?);
    }
    Some(bytes)
}

/// Matches a DAP function-breakpoint name against a compiler function name.
///
/// PHP function/method names are case-insensitive, and a client may send either
/// the fully qualified name or the short name. Include variants are reduced to
/// the PHP-visible name first.
fn function_name_matches(metadata_name: &str, requested: &str) -> bool {
    let name = super::display_function_name(metadata_name).trim_start_matches('\\');
    let requested = requested.trim().trim_start_matches('\\');
    if name.eq_ignore_ascii_case(requested) {
        return true;
    }
    name.rsplit('\\')
        .next()
        .is_some_and(|short| short.eq_ignore_ascii_case(requested))
}

/// Encodes one DAP frame id as `(thread-like id, depth)`.
///
/// Main keeps the historical plain depth so existing clients and tests continue
/// to use `frameId: 0/1`; fiber ids shift the thread id into the high bits.
fn frame_id(thread_id: i64, depth: usize) -> i64 {
    if thread_id == 1 {
        depth as i64
    } else {
        (thread_id << 16) | depth as i64
    }
}

/// Decodes a DAP frame id back to its thread-like id and depth.
fn decode_frame_id(id: i64) -> Option<(i64, usize)> {
    if id < 0 {
        return None;
    }
    if id < (1 << 16) {
        Some((1, id as usize))
    } else {
        Some((id >> 16, (id & 0xffff) as usize))
    }
}

/// Resolves a recorded source path against the debuggee cwd for a DAP client.
///
/// DAP clients key editor buffers by absolute path, while the compiler may
/// record a relative entry path. Terminal output keeps the recorded spelling;
/// only the DAP `stackTrace` source object is resolved. A synthetic source that
/// does not exist on disk keeps its recorded path.
fn dap_source_path(path: &str) -> String {
    std::fs::canonicalize(path)
        .ok()
        .and_then(|path| path.to_str().map(str::to_string))
        .unwrap_or_else(|| path.to_string())
}

/// Matches a requested DAP source path against the metadata source table.
///
/// DAP clients often send an absolute path while the compiler recorded a
/// relative one, so suffix matching is the useful comparison here.
fn source_matches(
    sources: &HashMap<u64, String>,
    source_id: u64,
    requested: &str,
) -> bool {
    if requested.is_empty() {
        return true;
    }
    let source = sources.get(&source_id).map(String::as_str).unwrap_or("");
    !source.is_empty()
        && (source == requested || source.ends_with(requested) || requested.ends_with(source))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the attach listener writes its pid before accepting a client and
    /// removes both the pid file and the socket after the accept.
    #[test]
    fn listen_for_client_exposes_and_removes_the_pid_file() {
        let dir = std::env::temp_dir().join(format!(
            "elephc-debug-listen-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create attach temp dir");
        let socket = dir.join("app.sock");
        let socket_text = socket.to_str().expect("socket path").to_string();
        let thread_path = socket_text.clone();
        let handle = std::thread::spawn(move || listen_for_client(&thread_path));
        let pid_path = dir.join("app.sock.pid");
        for _ in 0..200 {
            if socket.exists() && pid_path.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(socket.exists(), "attach socket was not created");
        let pid = std::fs::read_to_string(&pid_path).expect("attach pid file");
        assert_eq!(
            pid.trim().parse::<u32>().expect("attach pid"),
            std::process::id()
        );
        let stream = UnixStream::connect(&socket).expect("connect to attach socket");
        let connection = handle.join().expect("listen thread").expect("listen");
        drop(connection);
        drop(stream);
        assert!(!socket.exists(), "attach socket was not removed");
        assert!(!pid_path.exists(), "attach pid file was not removed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Verifies the attach HMAC follows RFC 4231 test case 2.
    #[test]
    fn attach_hmac_matches_rfc4231() {
        let tag = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
        assert_eq!(
            encode_attach_key(&tag),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    /// Decodes a test attach key from the runtime's hexadecimal file.
    fn test_key(text: &str) -> [u8; 32] {
        let text = text.trim();
        assert_eq!(text.len(), 64);
        let mut key = [0u8; 32];
        for (index, pair) in text.as_bytes().chunks_exact(2).enumerate() {
            key[index] = u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap();
        }
        key
    }

    /// Runs the mutual handshake as a client and returns whether it was accepted.
    fn tcp_attach_probe(addr: &str, key: &[u8]) -> std::io::Result<bool> {
        let mut stream = std::net::TcpStream::connect(addr)?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        let nonce_c = [7u8; 32];
        stream.write_all(&nonce_c)?;
        let mut response = [0u8; 64];
        stream.read_exact(&mut response)?;
        let (nonce_s, server_tag) = response.split_at(32);
        if !constant_time_eq(server_tag, &tcp_attach_server_tag(key, &nonce_c, nonce_s)) {
            // Mirror the CLI: abort immediately so the listener can keep serving.
            let _ = stream.write_all(&[0]);
            return Ok(false);
        }
        stream.write_all(&tcp_attach_client_tag(key, nonce_s, &nonce_c))?;
        let mut status = [0u8; 1];
        stream.read_exact(&mut status)?;
        Ok(status[0] == 1)
    }

    /// Verifies a wrong TCP attach key is rejected while the listener keeps
    /// running for the correct key, then removes its key/address/pid files.
    #[test]
    fn tcp_attach_rejects_wrong_key_and_accepts_the_next_client() {
        let dir = std::env::temp_dir().join(format!(
            "elephc-debug-tcp-attach-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create TCP attach temp dir");
        let key_path = dir.join("attach.key");
        let key_path_text = key_path.to_str().expect("key path").to_string();
        let thread_key = key_path_text.clone();
        let handle = std::thread::spawn(move || {
            listen_tcp_for_client("127.0.0.1:0", &thread_key)
        });
        let address_path = format!("{key_path_text}.addr");
        for _ in 0..200 {
            if std::path::Path::new(&address_path).exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let address = std::fs::read_to_string(&address_path)
            .expect("TCP attach address file")
            .trim()
            .to_string();
        let key = test_key(&std::fs::read_to_string(&key_path).expect("attach key file"));

        assert!(
            !tcp_attach_probe(&address, &[0u8; 32]).expect("wrong-key probe"),
            "a wrong key must be rejected"
        );
        assert!(
            tcp_attach_probe(&address, &key).expect("correct-key probe"),
            "the correct key must be accepted after a rejection"
        );
        let connection = handle.join().expect("TCP attach thread").expect("listener");
        drop(connection);
        assert!(!std::path::Path::new(&key_path_text).exists());
        assert!(!std::path::Path::new(&address_path).exists());
        assert!(!std::path::Path::new(&format!("{key_path_text}.pid")).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Verifies the DAP source verifier follows the standard SHA-256 vector.
    #[test]
    fn sha256_hex_matches_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
