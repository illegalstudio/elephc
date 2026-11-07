//! Purpose:
//! Per-site debug hook and the first interactive runtime session for
//! `--debug` programs. The compiler emits a register-preserving wrapper around
//! `elephc_debug_site_inner`; this crate owns site flags, counters, metadata
//! lookup, the stdin pause loop, and current-frame local inspection.
//!
//! Called from:
//! - the generated wrapper emitted by `src/codegen/debug_sites.rs`;
//! - `elephc_debug_boot` from main's prologue;
//! - `elephc_debug_dump` from main's epilogue.
//!
//! Key details:
//! - `ELEPHC_DEBUG_SITES=1` arms every site and dumps counters at exit.
//! - `ELEPHC_DEBUG_STDIN=1` opens the interactive pause session. Without a
//!   break spec it stops at the first executed site; with `ELEPHC_DEBUG_BREAK`
//!   (`path:line` or `:line`) it arms matching sites.
//! - `ELEPHC_DEBUG_DAP=1` serves DAP on an inherited fd or a one-shot
//!   `ELEPHC_DEBUG_DAP_LISTEN` Unix socket. `SIGUSR1` requests a stop at the
//!   next executed site.
//! - `ELEPHC_DEBUG_POSTMORTEM=<file>` keeps a bounded call ring and writes a
//!   postmortem JSON document on an uncaught exception. A normal exit writes
//!   nothing; native crashes are not promised to flush from a signal handler.
//! - The site hook blocks on stdin only while the program is paused; the
//!   generated wrapper has already saved every caller-saved register.
//! - Current-frame locals are readable for scalar/string slots; arrays/objects
//!   and ref-cell storage are reported without unsafe dereference until the
//!   ownership-aware formatter lands.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::IntoRawFd;
use std::os::unix::net::UnixListener;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Deserialize;

mod condition;
mod dap;
mod dap_output;
mod eval;
mod value;

/// Maximum distinct site counters/flags held in fixed storage.
const MAX_SITES: usize = 1 << 16;

/// Per-site hit counters, indexed by the compiler-assigned site id.
static SITE_COUNTS: [AtomicU32; MAX_SITES] = [const { AtomicU32::new(0) }; MAX_SITES];
/// Sites past the fixed table, reported as one overflow bucket.
static OVERFLOW: AtomicU64 = AtomicU64::new(0);
/// Last frame pointer observed by the hook.
static LAST_FRAME: AtomicUsize = AtomicUsize::new(0);
/// Whether the interactive stdin session is active.
static INTERACTIVE: AtomicBool = AtomicBool::new(false);
/// Set asynchronously by SIGUSR1 to stop at the next executed site.
static PAUSE_REQUESTED: AtomicBool = AtomicBool::new(false);
/// Whether any DAP function breakpoint is configured.
static FUNCTION_BREAKPOINTS_ACTIVE: AtomicBool = AtomicBool::new(false);
/// Whether DAP exception breakpoints are configured.
static EXCEPTION_BREAKS_ACTIVE: AtomicBool = AtomicBool::new(false);
/// Whether `--debug` call-value capture is active (`ELEPHC_DEBUG_CALLS`).
static CALLS_ENABLED: AtomicBool = AtomicBool::new(false);
/// How much of each call argument/return value to capture.
static CALL_MODE: AtomicU8 = AtomicU8::new(CALL_MODE_FULL);
/// Capture only call events (function/depth), no argument or return values.
const CALL_MODE_NONE: u8 = 0;
/// Capture argument/return types, not values.
const CALL_MODE_TYPES: u8 = 1;
/// Capture full scalar values; containers keep their summary.
const CALL_MODE_SCALARS: u8 = 2;
/// Capture values including bounded nested container children (default).
const CALL_MODE_FULL: u8 = 3;
/// Bounded ring of serialized call events; `None` marks an unwritten slot.
static CALL_EVENTS: Mutex<Vec<Option<serde_json::Value>>> = Mutex::new(Vec::new());
/// Total events pushed since the last drain, used to locate the oldest slot.
static CALL_EVENTS_TOTAL: AtomicU64 = AtomicU64::new(0);
/// Call events evicted after the ring reached its cap.
static CALL_EVENTS_DROPPED: AtomicU64 = AtomicU64::new(0);
/// Runtime-configurable ring cap; defaults to `MAX_CALL_EVENTS`.
static CALL_EVENTS_LIMIT: AtomicUsize = AtomicUsize::new(MAX_CALL_EVENTS);
/// Whether call events should be exported to stderr at normal exit.
static CALLS_EXPORT: AtomicBool = AtomicBool::new(false);
/// Whether the site hook should keep each activation's last source site.
static TRACK_SITE_LINES: AtomicBool = AtomicBool::new(false);
/// Monotonic origin for per-call timestamps.
static CALL_CLOCK: OnceLock<Instant> = OnceLock::new();
/// Whether this inherited process is a `--debug --web` worker tree.
static WEB_MODE: AtomicBool = AtomicBool::new(false);
/// Set by the web master boot so the per-request handler boot stays dormant.
static WEB_BOOTED: AtomicBool = AtomicBool::new(false);
/// Whether the current web request owns an accepted DAP session.
static WEB_ARMED: AtomicBool = AtomicBool::new(false);
/// Monotonic per-worker session sequence for advertised sockets.
static WEB_SEQUENCE: AtomicU64 = AtomicU64::new(0);
/// Per-site flag table handed to the web master boot, inherited by every worker.
static WEB_SITE_FLAGS: AtomicUsize = AtomicUsize::new(0);
/// Capacity of `WEB_SITE_FLAGS` (already clamped to `MAX_SITES`).
static WEB_SITE_COUNT: AtomicUsize = AtomicUsize::new(0);
/// Master-configured route/method/header filters and advertisement directory.
static WEB_FILTERS: OnceLock<Option<WebFilters>> = OnceLock::new();

/// One web request selected by the debug broker.
struct WebFilters {
    /// Advertisement directory created by `elephc debug --web`.
    directory: String,
    /// Exact path, `*`, or a trailing-`*` prefix.
    route: String,
    /// Optional HTTP method, compared case-insensitively.
    method: Option<String>,
    /// Optional `Name: value` header filter, name case-insensitive.
    header: Option<(String, String)>,
}
/// Runtime address of the generated program's allocation counter.
static ALLOCS_PTR: AtomicUsize = AtomicUsize::new(0);
/// Runtime address of the generated program's free counter.
static FREES_PTR: AtomicUsize = AtomicUsize::new(0);
/// Source-site ids known to the metadata, indexed by site id. Branch and path
/// counters share the hook id space but are not stack locations.
static SOURCE_SITE_IDS: OnceLock<Vec<bool>> = OnceLock::new();
/// Destination of the postmortem document, when armed.
static POSTMORTEM_PATH: Mutex<Option<String>> = Mutex::new(None);
/// Whether the postmortem document was already written for this process.
static POSTMORTEM_FLUSHED: AtomicBool = AtomicBool::new(false);
/// Maximum call events retained for one debug run.
const MAX_CALL_EVENTS: usize = 10_000;
/// Parsed metadata plus current breakpoint/session state.
static DEBUGGER: Mutex<Option<Debugger>> = Mutex::new(None);

/// One live PHP activation on the current thread.
#[derive(Clone, Copy)]
struct StackFrame {
    function_id: u32,
    frame: usize,
    last_site: Option<u32>,
    /// Index of this activation's event in `CALL_EVENTS`, when call capture is on.
    call_event: Option<usize>,
    /// Heap allocation counter read when this activation was pushed.
    allocs_at_entry: u64,
    /// Heap free counter read when this activation was pushed.
    frees_at_entry: u64,
}

thread_local! {
    /// Per-thread activation stack. The generated enter/exit wrappers push/pop
    /// one frame per compiled PHP function.
    static ACTIVATIONS: RefCell<Vec<StackFrame>> = const { RefCell::new(Vec::new()) };
    /// Consumer stack depth recorded when a coroutine is switched into. Frames
    /// above this depth belong to that coroutine.
    static FIBER_BASES: RefCell<HashMap<usize, usize>> = RefCell::new(HashMap::new());
    /// Frames parked while a coroutine is suspended, keyed by `_fiber_current`.
    static FIBER_STACKS: RefCell<HashMap<usize, Vec<StackFrame>>> = RefCell::new(HashMap::new());
    /// Open named trace segments, outermost first, on the running stack.
    static TRACE_SEGMENTS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// Active coroutine call chain; the last entry is the currently running
    /// fiber, and an empty chain means the main thread.
    static ACTIVE_FIBER_CHAIN: RefCell<Vec<usize>> = RefCell::new(Vec::new());
}

/// Parses `ELEPHC_DEBUG_CALLS`'s value into one capture mode.
fn parse_call_mode(value: &str) -> u8 {
    match value {
        "none" | "0" => CALL_MODE_NONE,
        "types" => CALL_MODE_TYPES,
        "scalars" => CALL_MODE_SCALARS,
        _ => CALL_MODE_FULL,
    }
}

/// Returns the currently open named trace segments for one capture event.
fn current_segment_path() -> Vec<String> {
    TRACE_SEGMENTS.with(|segments| segments.borrow().clone())
}

/// Pushes one capture event into the bounded ring and returns its live slot.
///
/// The ring keeps the newest events: once full, the oldest slot is overwritten
/// and any activation still pointing at it is detached so a later return cannot
/// attach to the replacement event.
fn push_call_event(event: serde_json::Value) -> Option<usize> {
    let limit = CALL_EVENTS_LIMIT.load(Ordering::Relaxed);
    if limit == 0 {
        return None;
    }
    let Ok(mut events) = CALL_EVENTS.lock() else {
        return None;
    };
    let total = CALL_EVENTS_TOTAL.fetch_add(1, Ordering::Relaxed);
    let index = if events.len() < limit {
        let index = events.len();
        events.push(Some(event));
        index
    } else {
        let index = (total as usize) % limit;
        events[index] = Some(event);
        CALL_EVENTS_DROPPED.fetch_add(1, Ordering::Relaxed);
        ACTIVATIONS.with(|stack| {
            for frame in stack.borrow_mut().iter_mut() {
                if frame.call_event == Some(index) {
                    frame.call_event = None;
                }
            }
        });
        FIBER_STACKS.with(|stacks| {
            for stack in stacks.borrow_mut().values_mut() {
                for frame in stack.iter_mut() {
                    if frame.call_event == Some(index) {
                        frame.call_event = None;
                    }
                }
            }
        });
        index
    };
    Some(index)
}

/// Returns the ring's events in execution order, draining the slots.
fn take_call_events(events: &mut Vec<Option<serde_json::Value>>, total: u64) -> Vec<serde_json::Value> {
    let len = events.len();
    if len == 0 {
        return Vec::new();
    }
    let start = if total as usize > len { total as usize % len } else { 0 };
    let mut ordered = Vec::with_capacity(len);
    for offset in 0..len {
        let index = (start + offset) % len;
        if let Some(event) = events[index].take() {
            ordered.push(event);
        }
    }
    ordered
}

/// Pushes one capture event into the bounded ring, counting overflow.
fn push_capture_event(event: serde_json::Value) {
    let _ = push_call_event(event);
}

/// Reads one UTF-8 string argument passed by the generated event call.
fn capture_event_string(pointer: *const u8, length: usize) -> Option<String> {
    if length == 0 {
        return Some(String::new());
    }
    if pointer.is_null() {
        return None;
    }
    Some(String::from_utf8_lossy(unsafe {
        std::slice::from_raw_parts(pointer, length)
    })
    .into_owned())
}

/// Copies a bounded rendering's overflow counts into a call argument/return.
fn add_capture_overflow(target: &mut serde_json::Value, rendered: &value::CaptureRender) {
    if !rendered.truncated {
        return;
    }
    target["truncated"] = serde_json::json!(true);
    let mut overflow = serde_json::json!({});
    if rendered.string_bytes_omitted > 0 {
        overflow["string_bytes"] = serde_json::json!(rendered.string_bytes_omitted);
    }
    if rendered.children_omitted > 0 {
        overflow["children"] = serde_json::json!(rendered.children_omitted);
    }
    if rendered.depth_truncated {
        overflow["depth"] = serde_json::json!(true);
    }
    target["overflow"] = overflow;
}

/// Closes every segment still open when the capture ends and reports it incomplete.
fn close_open_trace_segments() {
    loop {
        let name = TRACE_SEGMENTS.with(|segments| segments.borrow_mut().pop());
        let Some(name) = name else {
            break;
        };
        push_capture_event(serde_json::json!({
            "kind": "segment_end",
            "name": name,
            "depth": ACTIVATIONS.with(|stack| stack.borrow().len()),
            "segments": current_segment_path(),
            "complete": false,
        }));
    }
}

/// Records one explicit `elephc_dump` snapshot or writes it to stderr.
fn record_capture_dump(value: usize, label: Option<&str>, to: &str) {
    let capture = CALLS_ENABLED.load(Ordering::Relaxed);
    let invalid = !matches!(to, "auto" | "stderr" | "terminal" | "html");
    let to_stderr = matches!(to, "stderr")
        || matches!(to, "terminal")
        || (!capture && matches!(to, "auto" | "html"));

    let empty_classes = std::collections::HashMap::new();
    let plain_null = || value::CaptureRender {
        text: "null".to_string(),
        ..value::CaptureRender::default()
    };
    let (rendered, type_name) = match DEBUGGER.lock() {
        Ok(guard) => match guard.as_ref() {
            Some(debugger) => match value::value_from_mixed(value as u64) {
                Some(parsed) => {
                    let description = value::describe_with(parsed, &debugger.classes);
                    (
                        value::render_capture(parsed, &debugger.classes, true),
                        description.type_name,
                    )
                }
                None => (plain_null(), "null".to_string()),
            },
            None => match value::value_from_mixed(value as u64) {
                Some(parsed) => {
                    let description = value::describe_with(parsed, &empty_classes);
                    (
                        value::render_capture(parsed, &empty_classes, true),
                        description.type_name,
                    )
                }
                None => (plain_null(), "null".to_string()),
            },
        },
        Err(_) => (
            value::CaptureRender {
                text: "<unavailable>".to_string(),
                ..value::CaptureRender::default()
            },
            "mixed".to_string(),
        ),
    };

    if capture {
        let label_value = match label {
            Some(label) if !label.is_empty() => serde_json::json!(label),
            _ => serde_json::Value::Null,
        };
        let mut event = serde_json::json!({
            "kind": "dump",
            "depth": ACTIVATIONS.with(|stack| stack.borrow().len()),
            "segments": current_segment_path(),
            "label": label_value,
            "value": rendered.text,
            "type": type_name,
            "to": to,
        });
        add_capture_overflow(&mut event, &rendered);
        push_capture_event(event);
    }
    if invalid {
        eprintln!(
            "elephc_dump: invalid destination '{to}' (expected auto, stderr, terminal or html)"
        );
    } else if to_stderr {
        let segments = current_segment_path();
        let scope = if segments.is_empty() {
            String::new()
        } else {
            format!("[{}] ", segments.join(" > "))
        };
        match label {
            Some(label) if !label.is_empty() => {
                eprintln!("{scope}elephc_dump[{label}]: {}", rendered.text);
            }
            _ => eprintln!("{scope}elephc_dump: {}", rendered.text),
        }
    }
}

/// Stops at the caller's current source site when an interactive/DAP debugger is
/// attached; a no-op outside a debug session.
fn record_debug_break() {
    if !INTERACTIVE.load(Ordering::Relaxed) {
        return;
    }
    let Ok(mut guard) = DEBUGGER.lock() else {
        return;
    };
    let Some(debugger) = guard.as_mut() else {
        return;
    };
    // Source helpers are transparent frames, but keep the lookup defensive: the
    // innermost activation with a known user site is the call site that asked for
    // the stop.
    let site_id = ACTIVATIONS.with(|stack| {
        stack
            .borrow()
            .iter()
            .rev()
            .filter_map(|frame| frame.last_site)
            .find(|site_id| debugger.sites.contains_key(site_id))
    });
    let Some(site_id) = site_id else {
        return;
    };
    debugger.stop_at_first = false;
    debugger.step = None;
    if debugger.dap.is_some() {
        debugger.dap_stopped(site_id, "breakpoint");
    } else {
        debugger.stop(site_id);
    }
}

/// Opens one named capture segment.
fn record_trace_start(name: &str) {
    let capture = CALLS_ENABLED.load(Ordering::Relaxed);
    let interactive = INTERACTIVE.load(Ordering::Relaxed);
    if !capture && !interactive {
        return;
    }
    if name.is_empty() {
        eprintln!("elephc_trace_start: segment name must not be empty");
        return;
    }
    TRACE_SEGMENTS.with(|segments| segments.borrow_mut().push(name.to_string()));
    if capture {
        push_capture_event(serde_json::json!({
            "kind": "segment_start",
            "name": name,
            "depth": ACTIVATIONS.with(|stack| stack.borrow().len()),
            "segments": current_segment_path(),
        }));
    } else {
        eprintln!("elephc-trace: start {}", current_segment_path().join(" > "));
    }
}

/// Closes one named capture segment, warning when it is not the open top.
fn record_trace_end(name: &str) {
    let capture = CALLS_ENABLED.load(Ordering::Relaxed);
    let interactive = INTERACTIVE.load(Ordering::Relaxed);
    if !capture && !interactive {
        return;
    }
    let top = TRACE_SEGMENTS.with(|segments| segments.borrow().last().cloned());
    match top {
        Some(top) if top == name => {
            TRACE_SEGMENTS.with(|segments| {
                segments.borrow_mut().pop();
            });
            if capture {
                push_capture_event(serde_json::json!({
                    "kind": "segment_end",
                    "name": name,
                    "depth": ACTIVATIONS.with(|stack| stack.borrow().len()),
                    "segments": current_segment_path(),
                    "complete": true,
                }));
            } else {
                eprintln!("elephc-trace: end {name}");
            }
        }
        Some(top) => {
            let message =
                format!("elephc_trace_end('{name}') does not match the open segment '{top}'; ignored");
            eprintln!("elephc_trace_end: {message}");
            if capture {
                push_capture_event(serde_json::json!({
                    "kind": "segment_warning",
                    "message": message,
                    "depth": ACTIVATIONS.with(|stack| stack.borrow().len()),
                }));
            }
        }
        None => {
            let message = format!("elephc_trace_end('{name}') has no open segment; ignored");
            eprintln!("elephc_trace_end: {message}");
            if capture {
                push_capture_event(serde_json::json!({
                    "kind": "segment_warning",
                    "message": message,
                    "depth": ACTIVATIONS.with(|stack| stack.borrow().len()),
                }));
            }
        }
    }
}

/// Reports segments still open when an interactive session ends.
fn report_incomplete_trace_segments() {
    loop {
        let name = TRACE_SEGMENTS.with(|segments| segments.borrow_mut().pop());
        let Some(name) = name else {
            break;
        };
        eprintln!("elephc-trace: segment '{name}' closed incomplete");
    }
}

/// Flushes captured stdout/stderr into DAP output events before a fatal report.
///
/// Called through the nullable `elephc_debug_flush_fn` slot by the uncaught
/// exception report. It restores the original descriptors after draining the
/// pump, so the report itself is written to the process stdout as usual.
#[no_mangle]
pub extern "C" fn elephc_debug_flush_output() {
    if let Ok(mut guard) = DEBUGGER.lock() {
        if let Some(debugger) = guard.as_mut() {
            debugger.flush_dap_output();
        }
    }
}

/// Native entry point for one dump or trace-segment capture event.
///
/// Input: kind 0 = dump, 1 = segment start, 2 = segment end; a boxed `Mixed`
/// value; and label/destination string byte ranges. Strings may be empty.
#[no_mangle]
pub extern "C" fn elephc_debug_event(
    kind: i64,
    value: usize,
    label_pointer: *const u8,
    label_length: usize,
    to_pointer: *const u8,
    to_length: usize,
) {
    let label = capture_event_string(label_pointer, label_length);
    let to = capture_event_string(to_pointer, to_length).unwrap_or_else(|| "auto".to_string());
    match kind {
        0 => record_capture_dump(value, label.as_deref(), &to),
        1 => record_trace_start(label.as_deref().unwrap_or("")),
        2 => record_trace_end(label.as_deref().unwrap_or("")),
        3 => record_debug_break(),
        _ => {}
    }
}

/// Metadata document fields consumed by the initial agent.
#[derive(Deserialize)]
struct Metadata {
    sources: Vec<Source>,
    functions: Vec<Function>,
    sites: Vec<Site>,
    locals: Vec<Local>,
    #[serde(default)]
    classes: Vec<value::Class>,
}

#[derive(Deserialize)]
struct Source {
    id: u64,
    path: String,
    sha256: Option<String>,
}

#[derive(Deserialize)]
struct Function {
    id: u64,
    name: String,
    #[serde(default)]
    source_id: Option<u64>,
    #[serde(default)]
    line_start: Option<u64>,
}

#[derive(Deserialize)]
struct Site {
    id: u64,
    function_id: Option<u64>,
    source_id: u64,
    line: u64,
    column: u64,
}

#[derive(Deserialize)]
struct Local {
    function_id: u64,
    name: String,
    frame_offset: usize,
    php_type: String,
    storage: String,
    state_offset: Option<usize>,
    /// Parameter index when this local is a function parameter.
    #[serde(default)]
    param: Option<usize>,
    /// Whether the parameter carries PHP's `#[\SensitiveParameter]` attribute.
    #[serde(default)]
    sensitive: bool,
}

/// Resumption request recorded by a stepping command.
#[derive(Clone, Copy)]
enum StepMode {
    /// Stop at the next site in this frame or a caller (step over calls).
    Next,
    /// Stop at the next site at any depth (step into calls).
    Step,
    /// Stop after this frame returns by stepping at a shallower depth.
    Finish,
}

/// One pending stepping request and the depth it started from.
#[derive(Clone, Copy)]
struct StepState {
    mode: StepMode,
    depth: usize,
    /// Function the step started in, so another function on the same line stops.
    function_id: u32,
    /// Source owning the step's starting line.
    source_id: u64,
    /// Source line the step started on.
    line: u64,
}

/// Returns the PHP-visible function name, stripping the compiler's include-variant prefix.
pub(crate) fn display_function_name(name: &str) -> &str {
    name.strip_prefix("__elephc_include_variant_")
        .and_then(|rest| rest.split_once('_').map(|(_, visible)| visible))
        .unwrap_or(name)
}

/// DAP exception filters selected by `setExceptionBreakpoints`.
#[derive(Clone, Default)]
struct ExceptionFilters {
    /// Stop on a throw that an active handler will catch.
    caught: bool,
    /// Stop on a throw with no active handler.
    uncaught: bool,
    /// Stop on every throw, caught or not.
    throw_all: bool,
    /// Whether `caught` was narrowed to `caught_classes`.
    caught_filtered: bool,
    /// Whether `uncaught` was narrowed to `uncaught_classes`.
    uncaught_filtered: bool,
    /// Whether `throw_all` was narrowed to `throw_classes`.
    throw_filtered: bool,
    /// Class names accepted for caught throws.
    caught_classes: Vec<String>,
    /// Class names accepted for uncaught throws.
    uncaught_classes: Vec<String>,
    /// Class names accepted for every throw.
    throw_classes: Vec<String>,
}

impl ExceptionFilters {
    /// Builds the filter set from DAP filter names without class conditions.
    fn from_names(names: &[String]) -> Self {
        let mut filters = Self::default();
        for name in names {
            match name.as_str() {
                "caught" => filters.caught = true,
                "uncaught" => filters.uncaught = true,
                "throw" | "all" => filters.throw_all = true,
                _ => {}
            }
        }
        filters
    }

    /// Builds class-aware filters from DAP `filterOptions`.
    fn from_options(options: &[(String, Option<String>)]) -> Self {
        let mut filters = Self::default();
        for (name, class) in options {
            let (enabled, filtered, classes) = match name.as_str() {
                "caught" => (
                    &mut filters.caught,
                    &mut filters.caught_filtered,
                    &mut filters.caught_classes,
                ),
                "uncaught" => (
                    &mut filters.uncaught,
                    &mut filters.uncaught_filtered,
                    &mut filters.uncaught_classes,
                ),
                "throw" | "all" => (
                    &mut filters.throw_all,
                    &mut filters.throw_filtered,
                    &mut filters.throw_classes,
                ),
                _ => continue,
            };
            *enabled = true;
            match class
                .as_deref()
                .map(str::trim)
                .filter(|class| !class.is_empty())
            {
                Some(class) => {
                    if !*filtered {
                        *filtered = true;
                        classes.clear();
                    }
                    classes.push(class.to_string());
                }
                None => {
                    *filtered = false;
                    classes.clear();
                }
            }
        }
        filters
    }

    /// Returns whether no exception category is selected.
    fn is_empty(&self) -> bool {
        !self.caught && !self.uncaught && !self.throw_all
    }

    /// Returns whether a throw with this caught state should stop.
    fn should_stop(&self, caught: bool) -> bool {
        self.throw_all || if caught { self.caught } else { self.uncaught }
    }

    /// Returns whether a throw of `class` should stop under the active filters.
    fn should_stop_exception(
        &self,
        caught: bool,
        class: Option<&str>,
        classes: &HashMap<u64, value::Class>,
    ) -> bool {
        let (enabled, filtered, allowed) = if self.throw_all {
            (true, self.throw_filtered, &self.throw_classes)
        } else if caught {
            (self.caught, self.caught_filtered, &self.caught_classes)
        } else {
            (self.uncaught, self.uncaught_filtered, &self.uncaught_classes)
        };
        if !enabled {
            return false;
        }
        if !filtered {
            return true;
        }
        let Some(class) = class else {
            return false;
        };
        allowed
            .iter()
            .any(|filter| exception_class_matches(filter, class, classes))
    }
}

/// Returns whether `class` is `filter` or one of its PHP parents.
fn exception_class_matches(
    filter: &str,
    class: &str,
    classes: &HashMap<u64, value::Class>,
) -> bool {
    if filter.eq_ignore_ascii_case(class) {
        return true;
    }
    let mut current = class.to_string();
    let mut seen = std::collections::HashSet::new();
    loop {
        if !seen.insert(current.to_ascii_lowercase()) {
            return false;
        }
        let parent = classes
            .values()
            .find(|candidate| candidate.name.eq_ignore_ascii_case(&current))
            .map(|candidate| candidate.parent.clone())
            .unwrap_or_default();
        if parent.is_empty() {
            return false;
        }
        if parent.eq_ignore_ascii_case(filter) {
            return true;
        }
        current = parent;
    }
}

/// Runtime session state built from the embedded metadata.
struct Debugger {
    sources: HashMap<u64, String>,
    source_hashes: HashMap<u64, String>,
    functions: HashMap<u64, Function>,
    sites: HashMap<u32, Site>,
    locals: HashMap<u64, Vec<Local>>,
    classes: HashMap<u64, value::Class>,
    break_sites: HashSet<u32>,
    stop_at_first: bool,
    step: Option<StepState>,
    /// Index into the activation stack, `0` being the innermost frame.
    selected_frame: Option<usize>,
    /// DAP transport when the program was launched by `elephc debug --dap`.
    dap: Option<dap::DapConnection>,
    /// stdout/stderr capture active for the current DAP session.
    dap_output: Option<dap_output::DapOutputPump>,
    /// Whether this DAP session belongs to one `--web` request. A disconnect
    /// must resume that request instead of exiting the worker process.
    web_session: bool,
    /// Conditional expressions attached to breakpoint sites.
    breakpoint_conditions: HashMap<u32, String>,
    /// Hit-count conditions attached to breakpoint sites.
    breakpoint_hit_conditions: HashMap<u32, dap::HitCondition>,
    /// Session hit counts for armed breakpoint sites.
    breakpoint_hits: HashMap<u32, u64>,
    /// Exception currently presented to the DAP client, for `exceptionInfo`.
    current_exception: Option<dap::CurrentException>,
    /// Function-id → optional condition for DAP function breakpoints.
    function_breakpoints: HashMap<u32, Option<String>>,
    /// Active write watches for DAP data breakpoints.
    data_breakpoints: Vec<dap::DataBreakpoint>,
    /// Selected DAP exception filters.
    exception_filters: ExceptionFilters,
    /// DAP variable containers indexed by `variablesReference`.
    variable_refs: HashMap<i64, dap::VariableContainer>,
    /// Next `variablesReference` value.
    next_variable_ref: i64,
    /// Whether the DAP launch requested a stop on entry.
    dap_stop_on_entry: bool,
    /// Coroutine pointer → stable DAP thread-like id.
    fiber_threads: HashMap<u64, i64>,
    /// DAP thread-like id → coroutine pointer; 1 is always the main thread.
    thread_fibers: HashMap<i64, u64>,
    /// Next fiber thread-like id; ids start at 2.
    next_fiber_thread: i64,
}

impl Debugger {
    /// Parses the embedded metadata table and builds lookup maps.
    fn from_embedded() -> Result<Self, String> {
        let len = unsafe { elephc_debug_metadata_len } as usize;
        let pointer = unsafe { &elephc_debug_metadata as *const u8 };
        if len == 0 || pointer.is_null() {
            return Err("embedded debug metadata is empty".to_string());
        }
        let bytes = unsafe { std::slice::from_raw_parts(pointer, len) };
        let metadata: Metadata = serde_json::from_slice(bytes)
            .map_err(|error| format!("invalid embedded debug metadata: {error}"))?;
        let mut sources = HashMap::new();
        let mut source_hashes = HashMap::new();
        for source in metadata.sources {
            if let Some(hash) = source.sha256 {
                source_hashes.insert(source.id, hash);
            }
            sources.insert(source.id, source.path);
        }
        let functions = metadata
            .functions
            .into_iter()
            .map(|function| (function.id, function))
            .collect();
        let mut sites = HashMap::new();
        for site in metadata.sites {
            if let Ok(id) = u32::try_from(site.id) {
                sites.insert(id, site);
            }
        }
        let mut locals: HashMap<u64, Vec<Local>> = HashMap::new();
        for local in metadata.locals {
            locals.entry(local.function_id).or_default().push(local);
        }
        let classes = metadata
            .classes
            .into_iter()
            .map(|class| (class.id, class))
            .collect();
        Ok(Self {
            sources,
            source_hashes,
            functions,
            sites,
            locals,
            classes,
            break_sites: HashSet::new(),
            stop_at_first: false,
            step: None,
            selected_frame: None,
            breakpoint_conditions: HashMap::new(),
            breakpoint_hit_conditions: HashMap::new(),
            breakpoint_hits: HashMap::new(),
            current_exception: None,
            function_breakpoints: HashMap::new(),
            data_breakpoints: Vec::new(),
            exception_filters: ExceptionFilters::default(),
            dap: None,
            dap_output: None,
            web_session: false,
            variable_refs: HashMap::new(),
            next_variable_ref: 1,
            dap_stop_on_entry: false,
            fiber_threads: HashMap::new(),
            thread_fibers: HashMap::new(),
            next_fiber_thread: 2,
        })
    }

    /// Attaches one DAP connection and starts forwarding the program's stdout/stderr.
    fn attach_dap(&mut self, connection: dap::DapConnection) {
        unsafe {
            elephc_debug_flush_fn = elephc_debug_flush_output as *const () as usize;
        }
        match dap_output::DapOutputPump::install(connection.output_handle()) {
            Ok(pump) => self.dap_output = Some(pump),
            Err(error) => {
                eprintln!("elephc-debug: cannot capture program output for DAP: {error}");
            }
        }
        self.dap = Some(connection);
    }

    /// Drains captured output and restores the original stdout/stderr.
    fn flush_dap_output(&mut self) {
        if let Some(pump) = self.dap_output.take() {
            drop(pump);
        }
    }

    /// Restores the original stdout/stderr and drops the current DAP channel.
    fn close_dap(&mut self) {
        self.flush_dap_output();
        self.dap = None;
    }

    /// Arms the sites matching `ELEPHC_DEBUG_BREAK` and returns whether any
    /// breakpoint was found.
    fn arm_break_spec(&mut self, spec: &str) -> bool {
        let Some((path, line)) = spec.rsplit_once(':') else {
            return false;
        };
        let Ok(line) = line.parse::<u64>() else {
            return false;
        };
        let mut armed = false;
        for (id, site) in &self.sites {
            if site.line != line {
                continue;
            }
            let source = self.sources.get(&site.source_id).map(String::as_str).unwrap_or("");
            if path.is_empty()
                || source == path
                || source.ends_with(path)
                || path.ends_with(source)
            {
                self.break_sites.insert(*id);
                armed = true;
            }
        }
        armed
    }

    /// Returns the source path for one site.
    fn site_path(&self, site: &Site) -> &str {
        self.sources.get(&site.source_id).map(String::as_str).unwrap_or("<unknown>")
    }

    /// Returns the function name for one site.
    fn site_function(&self, site: &Site) -> &str {
        site.function_id
            .and_then(|id| self.functions.get(&id))
            .map(|function| display_function_name(&function.name))
            .unwrap_or("<unknown>")
    }

    /// Returns the innermost currently running coroutine, or `None` for main.
    fn current_fiber(&self) -> Option<u64> {
        ACTIVE_FIBER_CHAIN.with(|chain| chain.borrow().last().copied().map(|fiber| fiber as u64))
    }

    /// Returns every live coroutine: active ones and suspended/parked ones.
    fn live_fibers(&self) -> Vec<u64> {
        let mut fibers = HashSet::new();
        FIBER_BASES.with(|bases| {
            fibers.extend(bases.borrow().keys().map(|fiber| *fiber as u64));
        });
        FIBER_STACKS.with(|stacks| {
            fibers.extend(stacks.borrow().keys().map(|fiber| *fiber as u64));
        });
        let mut fibers: Vec<u64> = fibers.into_iter().collect();
        fibers.sort_unstable();
        fibers
    }

    /// Returns the stable DAP thread id for a coroutine, assigning one if new.
    fn fiber_thread(&mut self, fiber: u64) -> i64 {
        if let Some(id) = self.fiber_threads.get(&fiber) {
            return *id;
        }
        let id = self.next_fiber_thread;
        self.next_fiber_thread += 1;
        self.fiber_threads.insert(fiber, id);
        self.thread_fibers.insert(id, fiber);
        id
    }

    /// Returns the DAP thread id of the currently stopped coroutine or main.
    fn current_thread_id(&mut self) -> i64 {
        match self.current_fiber() {
            Some(fiber) => self.fiber_thread(fiber),
            None => 1,
        }
    }

    /// Returns the activation stack for one DAP thread-like id.
    ///
    /// Main shows the frames below the outermost active coroutine. A suspended
    /// coroutine shows its caller chain plus its parked frames. The innermost
    /// active coroutine shows the whole live stack.
    fn thread_stack(&self, thread_id: i64) -> Vec<StackFrame> {
        if thread_id == 1 {
            let activations = ACTIVATIONS.with(|stack| stack.borrow().clone());
            let main_len = FIBER_BASES.with(|bases| {
                bases
                    .borrow()
                    .values()
                    .copied()
                    .min()
                    .unwrap_or(usize::MAX)
            });
            return activations[..main_len.min(activations.len())].to_vec();
        }
        let Some(fiber) = self.thread_fibers.get(&thread_id).copied() else {
            return Vec::new();
        };
        let fiber = fiber as usize;
        let activations = ACTIVATIONS.with(|stack| stack.borrow().clone());
        let base = FIBER_BASES.with(|bases| bases.borrow().get(&fiber).copied());
        let saved = FIBER_STACKS.with(|stacks| stacks.borrow().get(&fiber).cloned());
        match (base, saved) {
            (Some(base), Some(saved)) => {
                let mut stack = activations[..base.min(activations.len())].to_vec();
                stack.extend(saved);
                stack
            }
            (Some(base), None) => {
                let next_base = FIBER_BASES.with(|bases| {
                    bases
                        .borrow()
                        .values()
                        .copied()
                        .filter(|candidate| *candidate > base)
                        .min()
                });
                let end = next_base.unwrap_or(activations.len()).min(activations.len());
                activations[..end].to_vec()
            }
            _ => Vec::new(),
        }
    }

    /// Returns the activation at `index` in one DAP thread-like stack.
    fn activation_for(&self, thread_id: i64, index: usize) -> Option<StackFrame> {
        let stack = self.thread_stack(thread_id);
        stack
            .len()
            .checked_sub(index + 1)
            .and_then(|position| stack.get(position).copied())
    }

    /// Blocks at one site and serves the current minimal stdin commands.
    /// Returns the activation at `index`, where `0` is the innermost frame.
    fn activation(&self, index: usize) -> Option<StackFrame> {
        ACTIVATIONS.with(|stack| {
            let stack = stack.borrow();
            stack
                .len()
                .checked_sub(index + 1)
                .and_then(|position| stack.get(position).copied())
        })
    }

    /// Returns the frame selected by `frame N`, defaulting to the innermost.
    fn selected_activation(&self) -> Option<StackFrame> {
        self.activation(self.selected_frame.unwrap_or(0))
    }

    /// Builds one captured call argument with redaction and bounded rendering.
    fn capture_argument(
        &self,
        local: &Local,
        frame: usize,
        mode: u8,
    ) -> serde_json::Value {
        let value = value::local_value(local, frame);
        let description = value::describe_with(value, &self.classes);
        let mut argument = serde_json::json!({
            "name": local.name,
            "type": description.type_name,
        });
        if local.sensitive {
            argument["redacted"] = serde_json::json!(true);
            if mode != CALL_MODE_TYPES {
                argument["value"] = serde_json::json!("<redacted>");
            }
            return argument;
        }
        if mode == CALL_MODE_TYPES {
            return argument;
        }
        let rendered = value::render_capture(value, &self.classes, mode == CALL_MODE_FULL);
        argument["value"] = serde_json::json!(rendered.text);
        add_capture_overflow(&mut argument, &rendered);
        argument
    }

    /// Builds one captured return value with bounded rendering.
    fn capture_return(&self, value: value::Value, mode: u8) -> serde_json::Value {
        let description = value::describe_with(value, &self.classes);
        let mut returned = serde_json::json!({
            "type": description.type_name,
        });
        if mode == CALL_MODE_TYPES {
            return returned;
        }
        let rendered = value::render_capture(value, &self.classes, mode == CALL_MODE_FULL);
        returned["value"] = serde_json::json!(rendered.text);
        add_capture_overflow(&mut returned, &rendered);
        returned
    }

    /// Records one call event with the PHP-formatted values of its parameters.
    fn record_call(&self, function_id: u32, frame: usize) {
        let Some(function) = self.functions.get(&(function_id as u64)) else {
            return;
        };
        // The debug helpers are scaffolding for the capture itself; their own
        // frames would otherwise appear as ordinary calls in the tree.
        if matches!(
            display_function_name(&function.name),
            "elephc_dump" | "elephc_trace_start" | "elephc_trace_end"
        ) {
            return;
        }
        let locals = self
            .locals
            .get(&(function_id as u64))
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let mut params: Vec<&Local> = locals
            .iter()
            .filter(|local| local.param.is_some())
            .collect();
        params.sort_by_key(|local| local.param.unwrap_or(usize::MAX));
        let mode = CALL_MODE.load(Ordering::Relaxed);
        let args: Vec<serde_json::Value> = if mode == CALL_MODE_NONE {
            Vec::new()
        } else {
            params
                .iter()
                .map(|local| self.capture_argument(local, frame, mode))
                .collect()
        };
        let depth = ACTIVATIONS.with(|stack| stack.borrow().len());
        let (allocs, frees) = allocation_counters();
        let caller_site = ACTIVATIONS.with(|stack| {
            let stack = stack.borrow();
            // The callee activation is already on top; the caller is the frame
            // below it, whose last source site is the call expression.
            if stack.len() >= 2 {
                stack[stack.len() - 2].last_site
            } else {
                None
            }
        });
        let mut event = serde_json::json!({
            "kind": "call",
            "depth": depth,
            "function": display_function_name(&function.name),
            "segments": current_segment_path(),
            "args": args,
            "return": serde_json::Value::Null,
            "timestamp_us": call_timestamp_us(),
            "fiber": current_fiber_id(),
            "allocs": allocs,
            "frees": frees,
            "exception": serde_json::Value::Null,
        });
        if let Some(site) = caller_site.and_then(|site_id| self.sites.get(&site_id)) {
            event["caller"] = serde_json::json!({
                "file": self.site_path(site),
                "line": site.line,
            });
        }
        let event_index = push_call_event(event);
        if let Some(index) = event_index {
            ACTIVATIONS.with(|stack| {
                if let Some(frame) = stack.borrow_mut().last_mut() {
                    frame.call_event = Some(index);
                }
            });
        }
    }

    /// Attaches the return value and per-call deltas to the activation's event.
    ///
    /// Timing and allocation churn are useful even in `--calls=none` mode, so the
    /// event is always closed; only the value capture is mode-gated.
    fn record_return(&self, function_id: u32, frame: usize, tag: i64, lo: u64, hi: u64) {
        let mode = CALL_MODE.load(Ordering::Relaxed);
        let (allocs_at_entry, frees_at_entry, event_index) = ACTIVATIONS.with(|stack| {
            let mut stack = stack.borrow_mut();
            let entry = stack
                .iter_mut()
                .rev()
                .find(|entry| entry.function_id == function_id && entry.frame == frame);
            match entry {
                Some(entry) => (
                    entry.allocs_at_entry,
                    entry.frees_at_entry,
                    entry.call_event.take(),
                ),
                None => (0, 0, None),
            }
        });
        let Some(index) = event_index else {
            return;
        };
        let (allocs, frees) = allocation_counters();
        let returned = if tag >= 0 && mode != CALL_MODE_NONE {
            let value = value::Value {
                tag,
                lo,
                hi,
                by_ref: false,
            };
            Some(self.capture_return(value, mode))
        } else {
            None
        };
        if let Ok(mut events) = CALL_EVENTS.lock() {
            if let Some(event) = events.get_mut(index).and_then(Option::as_mut) {
                event["alloc_delta"] = serde_json::json!(allocs.saturating_sub(allocs_at_entry));
                event["free_delta"] = serde_json::json!(frees.saturating_sub(frees_at_entry));
                event["end_timestamp_us"] = serde_json::json!(call_timestamp_us());
                if let Some(returned) = returned {
                    event["return"] = returned;
                }
            }
        }
    }

    /// Attaches the thrown exception to the innermost open call event.
    ///
    /// The throw helper calls the debugger before unwinding, so the throwing
    /// activation and its call event are still live. Postmortem and live captures
    /// therefore carry the same per-call exception state.
    fn record_exception(&self, throwable: usize, caught: bool) {
        let event_index = ACTIVATIONS.with(|stack| {
            stack
                .borrow()
                .iter()
                .rev()
                .find_map(|frame| frame.call_event)
        });
        let Some(index) = event_index else {
            return;
        };
        let details = value::throwable_details(throwable as u64, &self.classes);
        let mut exception = serde_json::json!({
            "class": details
                .as_ref()
                .map(|details| details.class.as_str())
                .unwrap_or("<unknown>"),
            "message": details
                .as_ref()
                .map(|details| details.message.as_str())
                .unwrap_or_default(),
            "code": details.as_ref().map(|details| details.code).unwrap_or(0),
            "caught": caught,
        });
        if details.is_none() {
            exception["class"] = serde_json::json!(format!("object:0x{throwable:x}"));
        }
        if let Ok(mut events) = CALL_EVENTS.lock() {
            if let Some(event) = events.get_mut(index).and_then(Option::as_mut) {
                event["exception"] = exception;
            }
        }
    }

    /// Writes captured call events to stderr in execution order.
    fn dump_calls(&self) {
        close_open_trace_segments();
        let (events, dropped) = CALL_EVENTS
            .lock()
            .map(|mut events| {
                let total = CALL_EVENTS_TOTAL.swap(0, Ordering::Relaxed);
                let events = take_call_events(&mut events, total);
                (events, CALL_EVENTS_DROPPED.swap(0, Ordering::Relaxed))
            })
            .unwrap_or_default();
        // The event ring is drained, so any still-active activation's slot is
        // stale. A later return must not attach to a new event that reused it.
        ACTIVATIONS.with(|stack| {
            for frame in stack.borrow_mut().iter_mut() {
                frame.call_event = None;
            }
        });
        for event in events {
            eprintln!("elephc-debug-call: {event}");
        }
        if dropped > 0 {
            eprintln!("elephc-debug-calls-dropped: {dropped}");
        }
    }

    /// Prints the current thread's PHP activation stack to stderr.
    fn print_stack(&self) {
        let frames = ACTIVATIONS.with(|stack| stack.borrow().clone());
        for (index, frame) in frames.iter().rev().enumerate() {
            let function = self
                .functions
                .get(&(frame.function_id as u64))
                .map(|function| display_function_name(&function.name))
                .unwrap_or("<unknown>");
            if let Some(site_id) = frame.last_site {
                if let Some(site) = self.sites.get(&site_id) {
                    eprintln!(
                        "#{index} {function} at {}:{}",
                        self.site_path(site),
                        site.line
                    );
                    continue;
                }
            }
            eprintln!("#{index} {function} at <no site>");
        }
    }

    /// Presents one breakpoint stop on the terminal client and waits for commands.
    /// Returns whether `site_id` repeats the exact position a step started from.
    ///
    /// Depth is part of the position: a recursive call may enter the same
    /// function on the same line, and `stepIn` must stop in the deeper frame.
    fn same_step_position(&self, step: &StepState, site_id: u32, depth: usize) -> bool {
        if depth != step.depth {
            return false;
        }
        let Some(site) = self.sites.get(&site_id) else {
            return false;
        };
        let function_id = ACTIVATIONS.with(|stack| {
            stack.borrow().last().map(|frame| frame.function_id).unwrap_or(0)
        });
        function_id == step.function_id
            && site.source_id == step.source_id
            && site.line == step.line
    }

    /// Presents one breakpoint stop on the terminal client and waits for commands.
    fn stop(&mut self, site_id: u32) {
        let Some(site) = self.sites.get(&site_id) else {
            return;
        };
        self.selected_frame = Some(0);
        let (depth, function_id) = ACTIVATIONS.with(|stack| {
            let stack = stack.borrow();
            (stack.len(), stack.last().map(|frame| frame.function_id).unwrap_or(0))
        });
        let start_step = |mode: StepMode| StepState {
            mode,
            depth,
            function_id,
            source_id: site.source_id,
            line: site.line,
        };
        eprintln!(
            "Stopped at {}:{}:{} ({})",
            self.site_path(site),
            site.line,
            site.column,
            self.site_function(site)
        );
        loop {
            eprint!("(elephc) ");
            let _ = io::stderr().flush();
            let mut line = String::new();
            if io::stdin().read_line(&mut line).unwrap_or(0) == 0 {
                eprintln!();
                return;
            }
            let command = line.trim();
            match command {
                "" | "c" | "continue" => {
                    self.step = None;
                    return;
                }
                "n" | "next" => {
                    self.step = Some(start_step(StepMode::Next));
                    return;
                }
                "s" | "step" => {
                    self.step = Some(start_step(StepMode::Step));
                    return;
                }
                "f" | "finish" => {
                    self.step = Some(start_step(StepMode::Finish));
                    return;
                }
                "q" | "quit" => std::process::exit(0),
                "h" | "help" => {
                    eprintln!(
                        "commands: continue|c, next|n, step|s, finish|f, bt, locals,\n\
                         p <name>, quit|q"
                    );
                }
                "bt" | "stack" => self.print_stack(),
                "frame" => eprintln!("usage: frame <index> (0 is the innermost frame)"),
                other if other.starts_with("frame ") => {
                    let index = other[6..].trim().parse::<usize>();
                    match index {
                        Ok(index) if self.activation(index).is_some() => {
                            self.selected_frame = Some(index);
                            eprintln!("Selected frame #{index}");
                        }
                        Ok(index) => eprintln!("frame {index} is out of range"),
                        Err(_) => eprintln!("usage: frame <index>"),
                    }
                }
                "locals" => self.print_locals(),
                other if other.starts_with("p ") => {
                    let name = other[2..].trim().trim_start_matches('$');
                    self.print_local(name);
                }
                other => eprintln!("unknown command '{other}' (try help)"),
            }
        }
    }

    /// Prints every named local available for the selected activation.
    fn print_locals(&self) {
        let Some(frame) = self.selected_activation() else {
            eprintln!("no active frame");
            return;
        };
        let Some(locals) = self.locals.get(&(frame.function_id as u64)) else {
            eprintln!("no locals metadata for this function");
            return;
        };
        for local in locals {
            self.print_one_local(local, frame.frame);
        }
    }

    /// Prints one named local from the selected activation.
    fn print_local(&self, name: &str) {
        let Some(frame) = self.selected_activation() else {
            eprintln!("no active frame");
            return;
        };
        let Some(local) = self
            .locals
            .get(&(frame.function_id as u64))
            .and_then(|locals| locals.iter().find(|local| local.name == name))
        else {
            eprintln!("no local '${name}'");
            return;
        };
        self.print_one_local(local, frame.frame);
    }

    /// Reads and prints one local's current value without mutating the program.
    fn print_one_local(&self, local: &Local, frame: usize) {
        let value = value::local_value(local, frame);
        let description = value::describe_with(value, &self.classes);
        eprintln!("{} {}", local.name, value::render(&description));
        if description.has_children {
            for child in value::children_with_annotated(value, &self.classes)
                .into_iter()
                .take(32)
            {
                let child_description = value::describe_with(child.value, &self.classes);
                if let Some(text) = &child.static_text {
                    eprintln!("  {} => {text}", child.name);
                    continue;
                }
                match child.hook_note {
                    Some(note) if note.is_virtual() => {
                        eprintln!("  {} => {}", child.name, note.virtual_display());
                    }
                    Some(note) => {
                        eprintln!(
                            "  {} => {}{}",
                            child.name,
                            value::render(&child_description),
                            note.backing_suffix()
                        );
                    }
                    None => {
                        eprintln!("  {} => {}", child.name, value::render(&child_description));
                    }
                }
            }
        }
    }
}

extern "C" {
    /// Nullable throw-helper slot; set by boot to the debugger exception hook.
    static mut elephc_debug_exception_fn: usize;
    /// Nullable uncaught-report output flush hook; set when a DAP pump starts.
    static mut elephc_debug_flush_fn: usize;
    /// Nullable coroutine switch-in hook; set by boot for fiber-aware stacks.
    static mut elephc_debug_coroutine_enter_fn: usize;
    /// Nullable coroutine suspend hook; set by boot for fiber-aware stacks.
    static mut elephc_debug_coroutine_suspend_fn: usize;
    /// Nullable coroutine termination hook; set by boot for fiber-aware stacks.
    static mut elephc_debug_coroutine_finished_fn: usize;
    /// Nullable per-request web route hook; set by the web master boot.
    static mut elephc_debug_web_request_fn: usize;
    /// Nullable per-request web session teardown; set by the web master boot.
    static mut elephc_debug_web_request_end_fn: usize;
    /// Embedded metadata label emitted by codegen.
    static elephc_debug_metadata: u8;
    /// `.quad` payload holding the metadata byte length.
    static elephc_debug_metadata_len: u64;
}

/// Reads the generated program's monotonic allocation and free counters.
///
/// `elephc_debug_boot` receives the addresses from the generated prologue, so
/// this crate never spells `_gc_allocs` itself: that hardcoded-underscore symbol
/// is only legal in the emitted assembly, and resolving it from Rust breaks ELF
/// links. The words are emitted for every target, so a debug build reports
/// churn without requiring `--with-monitoring`.
fn allocation_counters() -> (u64, u64) {
    /// Reads one boot-published counter address, or zero before boot.
    fn read(slot: &AtomicUsize) -> u64 {
        let address = slot.load(Ordering::Relaxed);
        if address == 0 {
            0
        } else {
            // Boot stores only addresses of the program's two `.comm` words, and
            // the program writes them as plain u64 counters.
            unsafe { *(address as *const u64) }
        }
    }
    (read(&ALLOCS_PTR), read(&FREES_PTR))
}

/// Returns the running coroutine id, or `0` for the main execution.
fn current_fiber_id() -> u64 {
    ACTIVE_FIBER_CHAIN.with(|chain| chain.borrow().last().copied().unwrap_or(0) as u64)
}

/// Returns microseconds since the debug session clock started.
fn call_timestamp_us() -> u64 {
    let origin = *CALL_CLOCK.get_or_init(Instant::now);
    origin.elapsed().as_micros() as u64
}

/// Returns whether `site_id` is a source site rather than a branch/path counter.
fn is_source_site(site_id: u32) -> bool {
    SOURCE_SITE_IDS
        .get()
        .and_then(|sites| sites.get(site_id as usize))
        .copied()
        .unwrap_or(false)
}

/// Async-signal-safe SIGUSR1 handler: only records the pause request.
extern "C" fn handle_pause_signal(_: libc::c_int) {
    PAUSE_REQUESTED.store(true, Ordering::Relaxed);
}

/// Arms the interactive/tracing session from the environment.
#[no_mangle]
pub extern "C" fn elephc_debug_boot(
    flags: *mut u8,
    count: usize,
    allocs_ptr: *const u64,
    frees_ptr: *const u64,
    web_master: i32,
) {
    ALLOCS_PTR.store(allocs_ptr as usize, Ordering::Relaxed);
    FREES_PTR.store(frees_ptr as usize, Ordering::Relaxed);
    if web_master != 0 {
        // A web server boots once, in the master, before `elephc_web_run` forks.
        // Workers inherit the metadata and the request hook; the per-request
        // handler boot below is a no-op in every child.
        WEB_BOOTED.store(true, Ordering::Relaxed);
        let Some(directory) = std::env::var("ELEPHC_DEBUG_WEB_DIR")
            .ok()
            .filter(|directory| !directory.is_empty())
        else {
            // A debug web build without `elephc debug --web` is dormant: a single
            // inherited DAP fd cannot be shared by sibling workers.
            return;
        };
        WEB_MODE.store(true, Ordering::Relaxed);
        let route = std::env::var("ELEPHC_DEBUG_WEB_ROUTE")
            .ok()
            .filter(|route| !route.is_empty())
            .unwrap_or_else(|| "*".to_string());
        let method = std::env::var("ELEPHC_DEBUG_WEB_METHOD")
            .ok()
            .filter(|method| !method.is_empty());
        let header = std::env::var("ELEPHC_DEBUG_WEB_HEADER")
            .ok()
            .filter(|header| !header.is_empty())
            .and_then(|header| {
                let (name, value) = header.split_once(':')?;
                let name = name.trim();
                let value = value.trim();
                if name.is_empty() {
                    None
                } else {
                    Some((name.to_string(), value.to_string()))
                }
            });
        let _ = WEB_FILTERS.set(Some(WebFilters {
            directory,
            route,
            method,
            header,
        }));
        let mut debugger = match Debugger::from_embedded() {
            Ok(debugger) => debugger,
            Err(error) => {
                eprintln!("elephc-debug: {error}");
                return;
            }
        };
        debugger.web_session = false;
        unsafe {
            elephc_debug_exception_fn = elephc_debug_exception_inner as *const () as usize;
            elephc_debug_coroutine_enter_fn =
                elephc_debug_coroutine_enter_inner as *const () as usize;
            elephc_debug_coroutine_suspend_fn =
                elephc_debug_coroutine_suspend_inner as *const () as usize;
            elephc_debug_coroutine_finished_fn =
                elephc_debug_coroutine_finished_inner as *const () as usize;
            elephc_debug_web_request_fn = elephc_debug_web_request as *const () as usize;
            elephc_debug_web_request_end_fn =
                elephc_debug_web_request_end as *const () as usize;
        }
        if let Ok(mut guard) = DEBUGGER.lock() {
            *guard = Some(debugger);
        }
        // Keep the worker dormant until a request matches: arming every site in
        // the master made every unrelated request pay the wrapper and the
        // debugger lock. The selected request arms them for its own lifetime.
        WEB_SITE_FLAGS.store(flags as usize, Ordering::Relaxed);
        WEB_SITE_COUNT.store(count.min(MAX_SITES), Ordering::Relaxed);
        INTERACTIVE.store(false, Ordering::Relaxed);
        return;
    }
    if WEB_BOOTED.load(Ordering::Relaxed) {
        // The web master already installed the metadata and route hook.
        return;
    }
    let tracing = std::env::var_os("ELEPHC_DEBUG_SITES").is_some();
    let interactive = std::env::var_os("ELEPHC_DEBUG_STDIN").is_some();
    let attach_tcp = std::env::var("ELEPHC_DEBUG_ATTACH_ADDR")
        .ok()
        .filter(|address| !address.is_empty());
    let dap = std::env::var_os("ELEPHC_DEBUG_DAP").is_some() || attach_tcp.is_some();
    let calls_env = std::env::var_os("ELEPHC_DEBUG_CALLS");
    let postmortem = std::env::var("ELEPHC_DEBUG_POSTMORTEM")
        .ok()
        .filter(|path| !path.is_empty());
    let calls_export = calls_env.is_some();
    let calls = calls_export || postmortem.is_some();
    if !tracing && !interactive && !dap && !calls {
        return;
    }
    CALLS_ENABLED.store(calls, Ordering::Relaxed);
    CALLS_EXPORT.store(calls_export, Ordering::Relaxed);
    // Caller source sites are part of every call event, so a calls-mode run must
    // keep each activation's last source line even without an interactive stop.
    TRACK_SITE_LINES.store(calls, Ordering::Relaxed);
    let calls_limit = std::env::var("ELEPHC_DEBUG_CALLS_MAX")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(MAX_CALL_EVENTS)
        .clamp(1, MAX_CALL_EVENTS);
    CALL_EVENTS_LIMIT.store(calls_limit, Ordering::Relaxed);
    CALL_MODE.store(
        calls_env
            .as_deref()
            .and_then(|value| value.to_str())
            .map(parse_call_mode)
            .or_else(|| postmortem.as_ref().map(|_| CALL_MODE_NONE))
            .unwrap_or(CALL_MODE_FULL),
        Ordering::Relaxed,
    );
    if let Some(path) = postmortem.as_ref() {
        if let Ok(mut destination) = POSTMORTEM_PATH.lock() {
            *destination = Some(path.clone());
        }
    }
    let mut debugger = match Debugger::from_embedded() {
        Ok(debugger) => debugger,
        Err(error) => {
            eprintln!("elephc-debug: {error}");
            return;
        }
    };
    // The site-id space mixes source sites and branch/path counters; remember
    // which ids are real source locations so the hook can update `last_site`
    // without taking the debugger lock on every line.
    let source_site_count = debugger
        .sites
        .keys()
        .max()
        .map(|id| *id as usize + 1)
        .unwrap_or(0);
    let mut source_sites = vec![false; source_site_count];
    for id in debugger.sites.keys() {
        if let Some(entry) = source_sites.get_mut(*id as usize) {
            *entry = true;
        }
    }
    let _ = SOURCE_SITE_IDS.set(source_sites);
    // Coroutine switches must save/restore their slice of the activation stack
    // or the consumer's frames read as callers of the fiber's next call (and
    // vice versa). Null slots keep a non-session debug run on the cheap path.
    unsafe {
        elephc_debug_coroutine_enter_fn =
            elephc_debug_coroutine_enter_inner as *const () as usize;
        elephc_debug_coroutine_suspend_fn =
            elephc_debug_coroutine_suspend_inner as *const () as usize;
        elephc_debug_coroutine_finished_fn =
            elephc_debug_coroutine_finished_inner as *const () as usize;
    }

    // Tracing, interactive stepping, breakpoints and call capture all need every
    // site to reach the hook; dormant modes are filtered inside `site_inner`.
    if tracing || interactive || dap || calls {
        if !flags.is_null() {
            for index in 0..count.min(MAX_SITES) {
                unsafe {
                    *flags.add(index) = 1;
                }
            }
        }
    }

    if dap || calls {
        // The throw helper calls this slot before choosing a catch handler. DAP
        // uses it for filters; call capture records per-call exception state, and
        // postmortem uses the uncaught branch to flush.
        unsafe {
            elephc_debug_exception_fn = elephc_debug_exception_inner as *const () as usize;
        }
    }
    if dap {
        // A DAP client sends SIGUSR1 to request a pause. The handler only stores
        // an atomic flag; the next site hook performs the actual stop.
        unsafe {
            libc::signal(libc::SIGUSR1, handle_pause_signal as *const () as libc::sighandler_t);
        }
        let connection = if let Some(address) = attach_tcp.as_deref() {
            match std::env::var("ELEPHC_DEBUG_ATTACH_KEY")
                .ok()
                .filter(|path| !path.is_empty())
            {
                Some(key_path) => dap::listen_tcp_for_client(address, &key_path),
                None => Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "ELEPHC_DEBUG_ATTACH_KEY must name the attach key file",
                )),
            }
        } else {
            match std::env::var("ELEPHC_DEBUG_DAP_LISTEN") {
                Ok(path) if !path.is_empty() => dap::listen_for_client(&path),
                _ => {
                    let fd = std::env::var("ELEPHC_DEBUG_FD")
                        .ok()
                        .and_then(|value| value.parse::<i32>().ok())
                        .unwrap_or(3);
                    dap::DapConnection::from_fd(fd)
                }
            }
        };
        match connection {
            Ok(connection) => {
                debugger.attach_dap(connection);
                debugger.stop_at_first = true;
            }
            Err(error) => {
                eprintln!("elephc-debug: cannot open DAP channel: {error}");
                std::process::exit(2);
            }
        }
    }

    if interactive || dap {
        let spec = std::env::var("ELEPHC_DEBUG_BREAK").ok();
        match spec.as_deref() {
            Some(spec) => {
                if !debugger.arm_break_spec(spec) {
                    eprintln!("elephc-debug: no site matched ELEPHC_DEBUG_BREAK='{spec}'");
                }
            }
            None => {
                if interactive {
                    debugger.stop_at_first = true;
                }
            }
        }
    }

    if let Ok(mut guard) = DEBUGGER.lock() {
        *guard = Some(debugger);
    }
    INTERACTIVE.store(interactive || dap, Ordering::Relaxed);
}

/// Arms or disarms every generated site flag for the selected web request.
fn set_web_site_flags(on: bool) {
    let flags = WEB_SITE_FLAGS.load(Ordering::Relaxed) as *mut u8;
    let count = WEB_SITE_COUNT.load(Ordering::Relaxed);
    if flags.is_null() {
        return;
    }
    let value = u8::from(on);
    for index in 0..count {
        unsafe {
            *flags.add(index) = value;
        }
    }
}

/// Returns whether one web request matches the master-configured filters.
///
/// A route is `*` for any request, an exact path, or a trailing-`*` prefix. The
/// method comparison is case-insensitive; a header filter is one exact value
/// whose name is case-insensitive.
fn web_request_matches(
    filters: &WebFilters,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
) -> bool {
    let route_matches = filters.route == "*"
        || filters.route == path
        || filters
            .route
            .strip_suffix('*')
            .is_some_and(|prefix| path.starts_with(prefix));
    if !route_matches {
        return false;
    }
    if let Some(expected) = filters.method.as_deref() {
        if !method.eq_ignore_ascii_case(expected) {
            return false;
        }
    }
    if let Some((name, value)) = filters.header.as_ref() {
        if !headers
            .iter()
            .any(|(header, candidate)| header.eq_ignore_ascii_case(name) && *candidate == value)
        {
            return false;
        }
    }
    true
}

/// Splits a NUL-separated flat header buffer into name/value pairs.
fn parse_web_headers(buffer: &[u8]) -> Vec<(&str, &str)> {
    let Ok(text) = std::str::from_utf8(buffer) else {
        return Vec::new();
    };
    let mut parts = text.split('\0');
    let mut headers = Vec::new();
    loop {
        let Some(name) = parts.next() else {
            break;
        };
        let Some(value) = parts.next() else {
            break;
        };
        if !name.is_empty() {
            headers.push((name, value));
        }
    }
    headers
}

/// Claim file shared by every prefork worker for the current debug run.
const WEB_CLAIM_FILE: &str = "active-session";

/// Returns whether the pid recorded in one claim file is still alive.
fn web_claim_pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
    result == 0 || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Claims the single debug session for this run, removing a dead worker's claim.
fn claim_web_session(directory: &Path) -> bool {
    let path = directory.join(WEB_CLAIM_FILE);
    let mut attempt = 0;
    loop {
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                let _ = write!(file, "{}", std::process::id());
                let _ = file.flush();
                return true;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists && attempt == 0 => {
                attempt = 1;
                let owner = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|text| text.trim().parse::<u32>().ok());
                if owner.is_some_and(web_claim_pid_alive) {
                    return false;
                }
                let _ = std::fs::remove_file(&path);
            }
            Err(_) => return false,
        }
    }
}

/// Releases this worker's claim, leaving a live worker's claim untouched.
fn release_web_session(directory: &Path) {
    let path = directory.join(WEB_CLAIM_FILE);
    let owner = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| text.trim().parse::<u32>().ok());
    if owner == Some(std::process::id()) {
        let _ = std::fs::remove_file(path);
    }
}

/// Releases the claim on every early return until the request owns its DAP session.
struct WebSessionClaim<'a> {
    /// Broker-owned session directory holding the claim file.
    directory: &'a Path,
    /// Cleared once the accepted session must keep the claim until request end.
    active: bool,
}

impl<'a> WebSessionClaim<'a> {
    /// Creates an armed guard for a claim this worker just acquired.
    fn new(directory: &'a Path) -> Self {
        Self {
            directory,
            active: true,
        }
    }
}

impl Drop for WebSessionClaim<'_> {
    /// Releases an active claim when setup returns before the request owns it.
    fn drop(&mut self) {
        if self.active {
            release_web_session(self.directory);
        }
    }
}

/// Opens the per-worker DAP session for one matching web request.
///
/// The worker advertises a mode-0600 Unix socket in the broker's 0700 directory,
/// then blocks until the broker connects (bounded, so a forgotten broker cannot
/// wedge the worker). The accepted stream becomes the request's DAP transport;
/// the per-request handler boot is already a no-op, and the first site stops.
#[no_mangle]
pub extern "C" fn elephc_debug_web_request(
    method: *const u8,
    method_len: usize,
    path: *const u8,
    path_len: usize,
    headers: *const u8,
    headers_len: usize,
) {
    if !WEB_MODE.load(Ordering::Relaxed) || WEB_ARMED.load(Ordering::Relaxed) {
        return;
    }
    if method.is_null() || path.is_null() {
        return;
    }
    let method = unsafe { std::slice::from_raw_parts(method, method_len) };
    let path = unsafe { std::slice::from_raw_parts(path, path_len) };
    let Ok(method) = std::str::from_utf8(method) else {
        return;
    };
    let Ok(path) = std::str::from_utf8(path) else {
        return;
    };
    let header_bytes = if headers.is_null() || headers_len == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(headers, headers_len) }
    };
    let headers = parse_web_headers(header_bytes);
    let Some(filters) = WEB_FILTERS.get().and_then(Option::as_ref) else {
        return;
    };
    if !web_request_matches(filters, method, path, &headers) {
        return;
    }
    if !claim_web_session(Path::new(&filters.directory)) {
        // Another worker already owns the run's one debug session. Returning
        // immediately keeps this request (and its connection) out of the 30 s
        // wait that only the selected worker should pay.
        return;
    }
    let mut claim = WebSessionClaim::new(Path::new(&filters.directory));

    let sequence = WEB_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    // macOS `sun_path` is much shorter than the test/session directory. Keep the
    // socket itself in /tmp with a compact unique name; the announcement JSON
    // stays in the session directory the broker owns.
    let nonce = dap::random_bytes::<4>().map(u32::from_ne_bytes).unwrap_or(0);
    let socket_path = std::path::Path::new("/tmp").join(format!(
        "elephc-web-{}-{sequence}-{nonce}.sock",
        std::process::id()
    ));
    let announce_path = std::path::Path::new(&filters.directory).join(format!(
        "session-{}-{sequence}.json",
        std::process::id()
    ));
    let listener = match UnixListener::bind(&socket_path) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!(
                "elephc-debug: cannot bind web debug socket '{}': {error}",
                socket_path.display()
            );
            return;
        }
    };
    let _ = std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600));
    let announcement = serde_json::json!({
        "pid": std::process::id(),
        "method": method,
        "path": path,
        "socket": socket_path.to_string_lossy(),
    });
    let encoded = match serde_json::to_vec(&announcement) {
        Ok(encoded) => encoded,
        Err(_) => {
            let _ = std::fs::remove_file(&socket_path);
            return;
        }
    };
    if let Err(error) = std::fs::write(&announce_path, encoded) {
        eprintln!(
            "elephc-debug: cannot advertise web debug session '{}': {error}",
            announce_path.display()
        );
        let _ = std::fs::remove_file(&socket_path);
        return;
    }
    let _ = std::fs::set_permissions(&announce_path, std::fs::Permissions::from_mode(0o600));

    // Poll instead of blocking forever: a broker that never connects must not
    // pin this worker's only PHP thread. The broker polls the same directory at
    // 10 ms, so the normal wait is sub-millisecond.
    let _ = listener.set_nonblocking(true);
    let deadline = Instant::now() + Duration::from_secs(30);
    let stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break Some(stream),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    break None;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => {
                eprintln!("elephc-debug: web debug accept failed: {error}");
                break None;
            }
        }
    };
    let _ = std::fs::remove_file(&announce_path);
    let _ = std::fs::remove_file(&socket_path);
    let Some(stream) = stream else {
        return;
    };
    let fd = stream.into_raw_fd();
    let connection = match dap::DapConnection::from_fd(fd) {
        Ok(connection) => connection,
        Err(error) => {
            unsafe {
                libc::close(fd);
            }
            eprintln!("elephc-debug: cannot wrap web debug socket: {error}");
            return;
        }
    };
    let Ok(mut guard) = DEBUGGER.lock() else {
        return;
    };
    let Some(debugger) = guard.as_mut() else {
        return;
    };
    set_web_site_flags(true);
    debugger.dap = Some(connection);
    debugger.web_session = true;
    debugger.stop_at_first = true;
    debugger.step = None;
    debugger.selected_frame = None;
    debugger.break_sites.clear();
    debugger.breakpoint_conditions.clear();
    debugger.breakpoint_hit_conditions.clear();
    debugger.breakpoint_hits.clear();
    debugger.current_exception = None;
    debugger.function_breakpoints.clear();
    debugger.data_breakpoints.clear();
    debugger.variable_refs.clear();
    WEB_ARMED.store(true, Ordering::Relaxed);
    INTERACTIVE.store(true, Ordering::Relaxed);
    // The request now owns the claim until its end hook runs.
    claim.active = false;
}

/// Ends one web request's DAP session, resuming the handler and leaving the
/// worker ready for another request.
#[no_mangle]
pub extern "C" fn elephc_debug_web_request_end() {
    if !WEB_ARMED.swap(false, Ordering::Relaxed) {
        return;
    }
    if let Some(directory) = WEB_FILTERS
        .get()
        .and_then(Option::as_ref)
        .map(|filters| filters.directory.as_str())
    {
        release_web_session(Path::new(directory));
    }
    set_web_site_flags(false);
    if let Ok(mut guard) = DEBUGGER.lock() {
        if let Some(debugger) = guard.as_mut() {
            debugger.close_dap();
            debugger.web_session = false;
            debugger.stop_at_first = false;
            debugger.step = None;
            debugger.selected_frame = None;
            debugger.break_sites.clear();
            debugger.breakpoint_conditions.clear();
            debugger.breakpoint_hit_conditions.clear();
            debugger.breakpoint_hits.clear();
            debugger.current_exception = None;
            debugger.function_breakpoints.clear();
            debugger.data_breakpoints.clear();
            debugger.variable_refs.clear();
        }
    }
    INTERACTIVE.store(false, Ordering::Relaxed);
}

/// Flushes the bounded activation/call ring to the configured postmortem file.
///
/// Called from the uncaught branch of the throw hook. This is ordinary
/// synchronized code, not a signal handler: native crashes and hard fatals that
/// bypass the hook are explicitly outside the guarantee.
fn write_postmortem(throwable: usize) {
    if POSTMORTEM_FLUSHED.swap(true, Ordering::Relaxed) {
        return;
    }
    let Some(path) = POSTMORTEM_PATH
        .lock()
        .ok()
        .and_then(|destination| destination.clone())
    else {
        return;
    };
    let document = {
        let Ok(guard) = DEBUGGER.lock() else {
            return;
        };
        let Some(debugger) = guard.as_ref() else {
            return;
        };
        let details = value::throwable_details(throwable as u64, &debugger.classes);
        let stack: Vec<serde_json::Value> = ACTIVATIONS.with(|activations| {
            activations
                .borrow()
                .iter()
                .rev()
                .map(|frame| {
                    let function = debugger
                        .functions
                        .get(&(frame.function_id as u64))
                        .map(|function| display_function_name(&function.name))
                        .unwrap_or("<unknown>");
                    let site = frame.last_site.and_then(|id| debugger.sites.get(&id));
                    let function_meta = debugger.functions.get(&(frame.function_id as u64));
                    let source_id = site
                        .map(|site| site.source_id)
                        .or_else(|| function_meta.and_then(|function| function.source_id));
                    let file = source_id
                        .and_then(|source_id| debugger.sources.get(&source_id))
                        .cloned()
                        .unwrap_or_default();
                    let line = site
                        .map(|site| site.line)
                        .or_else(|| function_meta.and_then(|function| function.line_start))
                        .unwrap_or(0);
                    serde_json::json!({
                        "function": function,
                        "file": file,
                        "line": line,
                    })
                })
                .collect()
        });
        let throw_site = stack.iter().find_map(|frame| {
            let line = frame.get("line").and_then(serde_json::Value::as_u64)?;
            (line > 0).then(|| {
                (
                    frame
                        .get("file")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    line,
                )
            })
        });
        let (throw_file, throw_line) = throw_site.unwrap_or_default();
        let mut exception = serde_json::json!({
            "class": details
                .as_ref()
                .map(|details| details.class.as_str())
                .unwrap_or("<unknown>"),
            "message": details
                .as_ref()
                .map(|details| details.message.as_str())
                .unwrap_or_default(),
            "code": details.as_ref().map(|details| details.code).unwrap_or(0),
            "file": throw_file,
            "line": throw_line,
        });
        if details.is_none() {
            exception["class"] = serde_json::json!(format!("object:0x{throwable:x}"));
        }
        let (calls, dropped) = CALL_EVENTS
            .lock()
            .map(|events| {
                let total = CALL_EVENTS_TOTAL.load(Ordering::Relaxed);
                let mut ordered = Vec::new();
                let len = events.len();
                if len > 0 {
                    let start = if total as usize > len { total as usize % len } else { 0 };
                    for offset in 0..len {
                        let index = (start + offset) % len;
                        if let Some(event) = events[index].as_ref() {
                            if event.get("function").is_some() {
                                ordered.push(event.clone());
                            }
                        }
                    }
                }
                (ordered, CALL_EVENTS_DROPPED.load(Ordering::Relaxed))
            })
            .unwrap_or_default();
        let captured = calls.len();
        serde_json::json!({
            "format": "elephc-postmortem",
            "version": 1,
            "reason": "uncaught_exception",
            "exception": exception,
            "stack": stack,
            "calls": calls,
            "captured": captured,
            "dropped": dropped,
        })
    };
    match serde_json::to_vec_pretty(&document) {
        Ok(bytes) => {
            if let Err(error) = std::fs::write(&path, bytes) {
                eprintln!("elephc-debug: cannot write postmortem '{path}': {error}");
            }
        }
        Err(error) => eprintln!("elephc-debug: cannot encode postmortem '{path}': {error}"),
    }
}

/// Debugger hook called by the throw helper before choosing a handler.
#[no_mangle]
pub extern "C" fn elephc_debug_exception_inner(caught: u32, throwable: usize) {
    let calls = CALLS_ENABLED.load(Ordering::Relaxed);
    let exception_breaks = EXCEPTION_BREAKS_ACTIVE.load(Ordering::Relaxed);
    if calls || exception_breaks {
        if let Ok(mut guard) = DEBUGGER.lock() {
            if let Some(debugger) = guard.as_mut() {
                if calls {
                    // The throwing activation is still on the stack here, so its
                    // open call event receives the class/message/caught state.
                    debugger.record_exception(throwable, caught != 0);
                }
                if exception_breaks
                    && debugger.dap.is_some()
                    && debugger.exception_filters.should_stop(caught != 0)
                {
                    debugger.dap_exception_stopped(throwable, caught != 0);
                }
            }
        }
    }
    if caught == 0 {
        write_postmortem(throwable);
    }
}

/// Restores a coroutine's parked frames above the current consumer stack.
///
/// The consumer depth is recorded as this coroutine's base; on the next suspend
/// only frames above that base are parked. First start has no saved frames, so
/// the fiber body's enter hook naturally lands above the base.
#[no_mangle]
pub extern "C" fn elephc_debug_coroutine_enter_inner(fiber: usize) {
    if fiber == 0 {
        return;
    }
    let saved = FIBER_STACKS.with(|stacks| stacks.borrow_mut().remove(&fiber));
    ACTIVATIONS.with(|stack| {
        let mut stack = stack.borrow_mut();
        let base = stack.len();
        if let Some(saved) = saved {
            stack.extend(saved);
        }
        FIBER_BASES.with(|bases| {
            bases.borrow_mut().insert(fiber, base);
        });
    });
    ACTIVE_FIBER_CHAIN.with(|chain| {
        let mut chain = chain.borrow_mut();
        if chain.last() != Some(&fiber) {
            chain.push(fiber);
        }
    });
}

/// Parks the frames belonging to a coroutine before its stack switch.
#[no_mangle]
pub extern "C" fn elephc_debug_coroutine_suspend_inner(fiber: usize) {
    if fiber == 0 {
        return;
    }
    let base = FIBER_BASES.with(|bases| bases.borrow().get(&fiber).copied());
    ACTIVATIONS.with(|stack| {
        let mut stack = stack.borrow_mut();
        let base = base.unwrap_or(stack.len()).min(stack.len());
        let saved = stack.split_off(base);
        FIBER_STACKS.with(|stacks| {
            stacks.borrow_mut().insert(fiber, saved);
        });
    });
    ACTIVE_FIBER_CHAIN.with(|chain| {
        let mut chain = chain.borrow_mut();
        if let Some(position) = chain.iter().rposition(|active| *active == fiber) {
            chain.remove(position);
        }
    });
}

/// Drops one coroutine's parked frames and truncates the consumer stack to its base.
fn finish_coroutine(fiber: usize) {
    let base = FIBER_BASES.with(|bases| bases.borrow_mut().remove(&fiber));
    FIBER_STACKS.with(|stacks| {
        stacks.borrow_mut().remove(&fiber);
    });
    if let Some(base) = base {
        ACTIVATIONS.with(|stack| {
            let mut stack = stack.borrow_mut();
            let length = stack.len();
            stack.truncate(base.min(length));
        });
    }
    ACTIVE_FIBER_CHAIN.with(|chain| {
        let mut chain = chain.borrow_mut();
        if let Some(position) = chain.iter().rposition(|active| *active == fiber) {
            chain.remove(position);
        }
    });
}

/// Drops a finished coroutine's saved frames and restores the consumer stack.
#[no_mangle]
pub extern "C" fn elephc_debug_coroutine_finished_inner(fiber: usize) {
    if fiber == 0 {
        return;
    }
    finish_coroutine(fiber);
}

/// Pushes one activation when a compiled PHP function is entered.
#[no_mangle]
pub extern "C" fn elephc_debug_enter_inner(function_id: u32, frame: usize) {
    let calls = CALLS_ENABLED.load(Ordering::Relaxed);
    let (allocs_at_entry, frees_at_entry) = if calls {
        allocation_counters()
    } else {
        (0, 0)
    };
    ACTIVATIONS.with(|stack| {
        stack.borrow_mut().push(StackFrame {
            function_id,
            frame,
            last_site: None,
            call_event: None,
            allocs_at_entry,
            frees_at_entry,
        });
    });
    let function_breakpoints = FUNCTION_BREAKPOINTS_ACTIVE.load(Ordering::Relaxed);
    if !calls && !function_breakpoints {
        return;
    }
    if let Ok(mut guard) = DEBUGGER.lock() {
        let Some(debugger) = guard.as_mut() else {
            return;
        };
        if calls {
            debugger.record_call(function_id, frame);
        }
        if !function_breakpoints || debugger.dap.is_none() {
            return;
        }
        let Some(condition) = debugger.function_breakpoints.get(&function_id).cloned() else {
            return;
        };
        let should_stop = match condition.as_deref() {
            Some(condition) => match debugger.evaluate_condition(condition, frame) {
                Ok(result) => result,
                Err(error) => {
                    debugger.report_condition_error(condition, &error);
                    false
                }
            },
            None => true,
        };
        if should_stop {
            debugger.dap_function_stopped(function_id, frame);
        }
    }
}

/// Pops the matching activation when a compiled PHP function returns.
#[no_mangle]
pub extern "C" fn elephc_debug_exit_inner(
    function_id: u32,
    frame: usize,
    tag: i64,
    lo: u64,
    hi: u64,
) {
    if CALLS_ENABLED.load(Ordering::Relaxed) {
        if let Ok(guard) = DEBUGGER.lock() {
            if let Some(debugger) = guard.as_ref() {
                debugger.record_return(function_id, frame, tag, lo, hi);
            }
        }
    }
    ACTIVATIONS.with(|stack| {
        let mut stack = stack.borrow_mut();
        if let Some(position) = stack
            .iter()
            .rposition(|entry| entry.function_id == function_id && entry.frame == frame)
        {
            stack.truncate(position);
        }
    });
}

/// Records one site hit and serves an interactive stop when requested.
///
/// The generated wrapper preserves every caller-saved register around this
/// call, so a pause may inspect/read the stopped frame.
#[no_mangle]
pub extern "C" fn elephc_debug_site_inner(site_id: u32, frame: usize) {
    LAST_FRAME.store(frame, Ordering::Relaxed);
    if let Some(counter) = SITE_COUNTS.get(site_id as usize) {
        let _ = counter.fetch_add(1, Ordering::Relaxed);
    } else {
        let _ = OVERFLOW.fetch_add(1, Ordering::Relaxed);
    }

    // Postmortem-only runs do not open an interactive session, but the stack
    // still needs each frame's last source site. Only source sites (not the
    // branch/path counters that share this hook) become a stack location.
    if TRACK_SITE_LINES.load(Ordering::Relaxed) && is_source_site(site_id) {
        ACTIVATIONS.with(|stack| {
            if let Some(entry) = stack.borrow_mut().last_mut() {
                entry.last_site = Some(site_id);
            }
        });
    }

    if !INTERACTIVE.load(Ordering::Relaxed) {
        return;
    }
    if let Ok(mut guard) = DEBUGGER.lock() {
        let Some(debugger) = guard.as_mut() else {
            return;
        };
        // Branch-edge guards reuse the site counter but are not source sites.
        // They must not consume a step/pause or replace the frame's last known
        // line; the coverage dump still counted the hit above.
        if !debugger.sites.contains_key(&site_id) {
            return;
        }
        ACTIVATIONS.with(|stack| {
            if let Some(entry) = stack.borrow_mut().last_mut() {
                entry.last_site = Some(site_id);
            }
        });
        let depth = ACTIVATIONS.with(|stack| stack.borrow().len());
        let pause = PAUSE_REQUESTED.swap(false, Ordering::Relaxed);
        let entry = debugger.stop_at_first;
        let step_stop = match debugger.step {
            Some(step) => match step.mode {
                StepMode::Next => {
                    depth < step.depth
                        || (depth == step.depth
                            && !debugger.same_step_position(&step, site_id, depth))
                }
                StepMode::Step => !debugger.same_step_position(&step, site_id, depth),
                StepMode::Finish => depth < step.depth,
            },
            None => false,
        };
        let data_stop = debugger.check_data_breakpoints(frame);
        // A step that has not left its starting line must not immediately stop
        // again on another guard for that same line. Breakpoints resume normal
        // behavior as soon as the step reaches a new line or function.
        let suppress_breakpoint = debugger.step.is_some_and(|step| {
            matches!(step.mode, StepMode::Next | StepMode::Step)
                && debugger.same_step_position(&step, site_id, depth)
        });
        let mut breakpoint_stop = false;
        if !pause
            && !entry
            && !step_stop
            && !suppress_breakpoint
            && debugger.break_sites.contains(&site_id)
        {
            // The hit count advances whenever the armed site is reached, even
            // when the expression or hit condition then skips the stop.
            let hit_condition = debugger.breakpoint_hit_conditions.get(&site_id).copied();
            let hit_ok = {
                let hits = debugger.breakpoint_hits.entry(site_id).or_insert(0);
                *hits += 1;
                hit_condition.map_or(true, |condition| condition.matches(*hits))
            };
            if hit_ok {
                match debugger.breakpoint_conditions.get(&site_id).cloned() {
                    Some(condition) => match debugger.evaluate_condition(&condition, frame) {
                        Ok(true) => breakpoint_stop = true,
                        Ok(false) => {}
                        Err(error) => debugger.report_condition_error(&condition, &error),
                    },
                    None => breakpoint_stop = true,
                }
            }
        }
        let should_stop = pause || entry || data_stop || breakpoint_stop || step_stop;
        if should_stop {
            let reason = if pause {
                "pause"
            } else if entry {
                "entry"
            } else if data_stop {
                "data breakpoint"
            } else if breakpoint_stop {
                "breakpoint"
            } else {
                "step"
            };
            debugger.stop_at_first = false;
            debugger.step = None;
            if debugger.dap.is_some() {
                debugger.dap_stopped(site_id, reason);
            } else {
                debugger.stop(site_id);
            }
        }
    }
}

/// Prints the per-site counters when `ELEPHC_DEBUG_SITES` is set.
#[no_mangle]
pub extern "C" fn elephc_debug_dump() {
    let calls = CALLS_EXPORT.load(Ordering::Relaxed);
    if let Ok(mut guard) = DEBUGGER.lock() {
        if let Some(debugger) = guard.as_mut() {
            debugger.dap_terminated();
            if calls {
                debugger.dump_calls();
            }
        }
    }
    if !calls && INTERACTIVE.load(Ordering::Relaxed) {
        report_incomplete_trace_segments();
    }
    if std::env::var_os("ELEPHC_DEBUG_SITES").is_none() {
        return;
    }
    let mut total = 0u64;
    let mut used = 0usize;
    for (site, counter) in SITE_COUNTS.iter().enumerate() {
        let count = counter.load(Ordering::Relaxed) as u64;
        if count != 0 {
            used += 1;
            total = total.saturating_add(count);
            eprintln!("elephc-debug-site: {site} {count}");
        }
    }
    let overflow = OVERFLOW.load(Ordering::Relaxed);
    if overflow != 0 {
        total = total.saturating_add(overflow);
        eprintln!("elephc-debug-site: overflow {overflow}");
    }
    eprintln!(
        "elephc-debug-sites: total={total} sites={used} frame=0x{:x}",
        LAST_FRAME.load(Ordering::Relaxed)
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies caught/uncaught/throw filters select the right throw categories.
    #[test]
    fn exception_filters_select_caught_uncaught_and_throw() {
        let empty = ExceptionFilters::default();
        assert!(!empty.should_stop(true));
        assert!(!empty.should_stop(false));
        let caught = ExceptionFilters::from_names(&["caught".to_string()]);
        assert!(caught.should_stop(true));
        assert!(!caught.should_stop(false));
        let uncaught = ExceptionFilters::from_names(&["uncaught".to_string()]);
        assert!(!uncaught.should_stop(true));
        assert!(uncaught.should_stop(false));
        let all = ExceptionFilters::from_names(&["throw".to_string()]);
        assert!(all.should_stop(true));
        assert!(all.should_stop(false));
        let both = ExceptionFilters::from_names(&["caught".to_string(), "uncaught".to_string()]);
        assert!(both.should_stop(true));
        assert!(both.should_stop(false));
    }

    /// Verifies the cross-worker web claim is exclusive and reclaims a dead owner.
    #[test]
    fn web_claim_is_exclusive_and_reclaims_dead_owners() {
        let dir = std::env::temp_dir().join(format!(
            "elephc-web-claim-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("claim directory");
        assert!(claim_web_session(&dir), "first request claims the run");
        assert!(!claim_web_session(&dir), "a second worker must not claim it");
        release_web_session(&dir);
        assert!(!dir.join(WEB_CLAIM_FILE).exists(), "owner releases its claim");

        let mut child = std::process::Command::new("true")
            .spawn()
            .expect("dead owner fixture");
        let pid = child.id();
        child.wait().expect("reap dead owner");
        std::fs::write(dir.join(WEB_CLAIM_FILE), pid.to_string())
            .expect("stale claim");
        assert!(claim_web_session(&dir), "a dead owner's claim is reclaimed");
        assert!(web_claim_pid_alive(std::process::id()));
        release_web_session(&dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Verifies class-aware exception filters narrow the selected category.
    #[test]
    fn exception_filters_apply_class_conditions() {
        let classes = HashMap::new();
        let filters = ExceptionFilters::from_options(&[(
            "uncaught".to_string(),
            Some("RuntimeException".to_string()),
        )]);
        assert!(filters.should_stop_exception(
            false,
            Some("RuntimeException"),
            &classes
        ));
        assert!(!filters.should_stop_exception(
            false,
            Some("LogicException"),
            &classes
        ));
        assert!(!filters.should_stop_exception(
            true,
            Some("RuntimeException"),
            &classes
        ));
        let all = ExceptionFilters::from_options(&[("throw".to_string(), None)]);
        assert!(all.should_stop_exception(true, Some("LogicException"), &classes));
        let reset = ExceptionFilters::from_options(&[
            ("uncaught".to_string(), Some("RuntimeException".to_string())),
            ("uncaught".to_string(), None),
        ]);
        assert!(reset.should_stop_exception(false, Some("LogicException"), &classes));
    }

    /// Verifies include-variant symbols display as their PHP-level function names.
    #[test]
    fn include_variant_names_display_php_names() {
        assert_eq!(
            display_function_name("__elephc_include_variant_2c661efa9b6dfad7_add_from_lib"),
            "add_from_lib"
        );
        assert_eq!(display_function_name("main"), "main");
        assert_eq!(display_function_name(r"App\Service::run"), r"App\Service::run");
    }

    /// Verifies web route selection supports exact paths, trailing-`*` prefixes,
    /// case-insensitive methods and one case-insensitive header value.
    #[test]
    fn web_route_filters_match_exact_prefix_method_and_header() {
        let filters = |route: &str, method: Option<&str>, header: Option<(&str, &str)>| WebFilters {
            directory: String::new(),
            route: route.to_string(),
            method: method.map(str::to_string),
            header: header.map(|(name, value)| (name.to_string(), value.to_string())),
        };
        assert!(web_request_matches(
            &filters("*", None, None),
            "GET",
            "/selected",
            &[]
        ));
        assert!(web_request_matches(
            &filters("/selected", None, None),
            "GET",
            "/selected",
            &[]
        ));
        assert!(!web_request_matches(
            &filters("/selected", None, None),
            "GET",
            "/other",
            &[]
        ));
        assert!(web_request_matches(
            &filters("/api*", None, None),
            "GET",
            "/api/users",
            &[]
        ));
        assert!(!web_request_matches(
            &filters("/api*", None, None),
            "GET",
            "/selected",
            &[]
        ));
        assert!(web_request_matches(
            &filters("*", Some("post"), None),
            "POST",
            "/selected",
            &[]
        ));
        assert!(!web_request_matches(
            &filters("*", Some("post"), None),
            "GET",
            "/selected",
            &[]
        ));
        assert!(web_request_matches(
            &filters("*", None, Some(("X-Debug", "1"))),
            "GET",
            "/selected",
            &[("x-debug", "1")]
        ));
        assert!(!web_request_matches(
            &filters("*", None, Some(("X-Debug", "1"))),
            "GET",
            "/selected",
            &[("x-debug", "2")]
        ));
    }

    /// Verifies the flat NUL-separated header buffer keeps name/value pairing.
    #[test]
    fn web_header_buffer_keeps_pairs() {
        assert_eq!(
            parse_web_headers(b"Host\0localhost\0X-Debug\01\0"),
            [("Host", "localhost"), ("X-Debug", "1")]
        );
        assert!(parse_web_headers(b"unterminated").is_empty());
    }

    /// Verifies the named capture modes parse and unknown values keep full capture.
    #[test]
    fn call_capture_modes_parse_named_values() {
        assert_eq!(parse_call_mode("none"), CALL_MODE_NONE);
        assert_eq!(parse_call_mode("types"), CALL_MODE_TYPES);
        assert_eq!(parse_call_mode("scalars"), CALL_MODE_SCALARS);
        assert_eq!(parse_call_mode("full"), CALL_MODE_FULL);
        assert_eq!(parse_call_mode("all"), CALL_MODE_FULL);
        assert_eq!(parse_call_mode("1"), CALL_MODE_FULL);
    }

    /// Verifies the enter/exit hooks push and pop matching activation frames
    /// without disturbing the caller's frame.
    #[test]
    fn activation_stack_pushes_and_pops_matching_frames() {
        elephc_debug_enter_inner(1, 0x1000);
        elephc_debug_enter_inner(2, 0x2000);
        assert_eq!(ACTIVATIONS.with(|stack| stack.borrow().len()), 2);
        elephc_debug_exit_inner(2, 0x2000, -1, 0, 0);
        assert_eq!(ACTIVATIONS.with(|stack| stack.borrow().len()), 1);
        assert_eq!(
            ACTIVATIONS.with(|stack| stack.borrow()[0].function_id),
            1
        );
        elephc_debug_exit_inner(1, 0x1000, -1, 0, 0);
        assert_eq!(ACTIVATIONS.with(|stack| stack.borrow().len()), 0);
    }
}
