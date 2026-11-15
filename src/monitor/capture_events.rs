//! Purpose:
//! Shared readers for the capture event stream consumed by `monitor --calls`:
//! call events, explicit dumps, named trace segments and their nesting.
//!
//! Called from:
//! - `crate::monitor::calls::render_calls` for the text report.
//! - `crate::monitor::call_html::render_calls_html` for the static panel.
//!
//! Key details:
//! - The runtime emits a single JSON event per line. Call events carry a
//!   `function` field; dump/segment events carry a `kind`.
//! - Segment summaries count calls and dumps between the matching start/end
//!   events, including nested segments in their enclosing segment.

/// One named trace segment with its captured contents.
#[derive(Clone, Debug)]
pub(crate) struct TraceSegment {
    /// Segment name as written by the program.
    pub(crate) name: String,
    /// Whether a matching `elephc_trace_end` closed it.
    pub(crate) complete: bool,
    /// Calls started inside the segment, including nested segments.
    pub(crate) calls: usize,
    /// Explicit dumps captured inside the segment.
    pub(crate) dumps: usize,
}

/// Returns whether one event is an ordinary function-call event.
pub(crate) fn is_call(event: &serde_json::Value) -> bool {
    event.get("function").is_some()
}

/// Returns whether one event is an explicit `elephc_dump` snapshot.
pub(crate) fn is_dump(event: &serde_json::Value) -> bool {
    event.get("kind").and_then(serde_json::Value::as_str) == Some("dump")
}

/// Renders a dump or call event's segment path as a display prefix.
pub(crate) fn segment_prefix(event: &serde_json::Value) -> String {
    let path: Vec<&str> = event
        .get("segments")
        .and_then(serde_json::Value::as_array)
        .map(|names| names.iter().filter_map(serde_json::Value::as_str).collect())
        .unwrap_or_default();
    if path.is_empty() {
        String::new()
    } else {
        format!("[{}] ", path.join(" > "))
    }
}

/// Collects named segments and the calls/dumps they contain.
pub(crate) fn trace_segments(events: &[serde_json::Value]) -> Vec<TraceSegment> {
    let mut segments: Vec<TraceSegment> = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    let mut start_indices: Vec<usize> = Vec::new();
    let mut end_indices: Vec<Option<usize>> = Vec::new();
    for (index, event) in events.iter().enumerate() {
        match event.get("kind").and_then(serde_json::Value::as_str) {
            Some("segment_start") => {
                segments.push(TraceSegment {
                    name: event
                        .get("name")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("<unnamed>")
                        .to_string(),
                    complete: false,
                    calls: 0,
                    dumps: 0,
                });
                start_indices.push(index);
                end_indices.push(None);
                open.push(segments.len() - 1);
            }
            Some("segment_end") => {
                let Some(segment) = open.pop() else {
                    continue;
                };
                segments[segment].complete = event
                    .get("complete")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                end_indices[segment] = Some(index);
            }
            _ => {}
        }
    }
    for (index, segment) in segments.iter_mut().enumerate() {
        let start = start_indices[index];
        let end = end_indices[index].unwrap_or(events.len());
        for event in &events[start.saturating_add(1)..end.min(events.len())] {
            if is_call(event) {
                segment.calls += 1;
            } else if is_dump(event) {
                segment.dumps += 1;
            }
        }
    }
    segments
}
