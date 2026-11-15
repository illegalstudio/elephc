//! Purpose:
//! Renders `elephc monitor --calls --html` as a self-contained call-tree page
//! over the runtime's captured call events.
//!
//! Called from:
//! - `crate::monitor::calls::run_calls()` after the debug program exits.
//!
//! Key details:
//! - The page has no JavaScript or external assets: the captured events are
//!   rendered server-side into nested `<ul>` elements.
//! - Every function name, argument name, value and type is HTML-escaped, because
//!   strings and class names come from the observed program.
//! - A flat preorder stream of `{depth, function, args, return}` events becomes a
//!   tree by tracking the last event at each depth. Missing depths degrade to a
//!   child of the nearest shallower event instead of panicking.

use std::fmt::Write as _;

use super::capture_events::{is_call, is_dump, trace_segments};
use super::CallCaptureMode;

/// Maximum call depth the page renders before collapsing to a marker.
const MAX_RENDER_DEPTH: usize = 128;

/// One call event and its direct children, stored in a flat arena.
struct CallNode<'a> {
    /// Captured event for this call.
    event: &'a serde_json::Value,
    /// Indices of direct children in the same arena.
    children: Vec<usize>,
}

/// Renders one call-capture session as a self-contained HTML page.
pub(crate) fn render_calls_html(
    events: &[serde_json::Value],
    dropped: u64,
    mode: CallCaptureMode,
    target: &str,
) -> String {
    let calls: Vec<&serde_json::Value> = events.iter().filter(|event| is_call(event)).collect();
    let (nodes, roots) = build_call_tree(&calls);
    let mut out = String::new();
    out.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n");
    out.push_str("<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    out.push_str("<title>elephc call capture</title>\n");
    out.push_str(
        "<style>\n\
         :root { color-scheme: light dark; }\n\
         body { font-family: system-ui, -apple-system, Segoe UI, sans-serif; margin: 2rem auto; max-width: 1100px; padding: 0 1rem; line-height: 1.45; }\n\
         h1 { margin-bottom: .2rem; }\n\
         h2 { margin-top: 1.5rem; }\n\
         .summary { margin: .25rem 0 .5rem; }\n\
         .hint, .warning, .truncated { color: GrayText; }\n\
         .warning { color: #b00020; }\n\
         .badge { display: inline-block; padding: 0 .4rem; border: 1px solid rgba(128,128,128,.45); border-radius: 999px; font-size: .85em; }\n\
         code { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }\n\
         ul.call-tree { list-style: none; margin: .2rem 0; padding-left: 1.1rem; border-left: 1px solid rgba(128,128,128,.3); }\n\
         ul.call-tree.root { padding-left: 0; border-left: 0; }\n\
         li.call { margin: .25rem 0; }\n\
         .call-line { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: .92rem; overflow-wrap: anywhere; }\n\
         .fn { font-weight: 650; color: #0a5a9a; }\n\
         .arg .name { color: #7a4a00; }\n\
         .arg code, .return code { background: rgba(128,128,128,.12); padding: 0 .2rem; border-radius: 3px; }\n\
         .arg .type, .return .type { font-style: italic; }\n\
         .return { color: #0a7a0a; }\n\
         .call-meta { color: GrayText; font-size: .8rem; margin: .1rem 0 0 .1rem; }\n\
         .call-meta code { font-size: inherit; }\n\
         .exception { color: #b00020; font-weight: 650; }\n\
         .segment-tag { color: #7a4a00; font-size: .85em; margin-left: .35rem; }\n\
         ul.dumps, ul.segments { list-style: none; padding-left: 0; }\n\
         ul.dumps li, ul.segments li { margin: .3rem 0; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: .92rem; }\n\
         .dump-label { font-weight: 650; color: #0a5a9a; }\n\
         .segment-name { font-weight: 650; color: #7a4a00; }\n\
         .incomplete { color: #b00020; }\n\
         </style>\n",
    );
    out.push_str("</head>\n<body>\n");
    out.push_str("<h1>elephc call capture</h1>\n");
    let _ = writeln!(
        out,
        "<p class=\"summary\">Target <code>{}</code> · mode <span class=\"badge\">{}</span> · <strong>{}</strong> captured calls</p>",
        escape_html(target),
        mode.as_str(),
        calls.len()
    );
    let hint = match mode {
        CallCaptureMode::None => {
            "Function and depth only; argument and return values were not captured."
        }
        CallCaptureMode::Types => "Argument and return types only; values were not captured.",
        CallCaptureMode::Scalars => {
            "Scalar values are shown in full; arrays and objects keep their summary."
        }
        CallCaptureMode::Full => {
            "Values include bounded nested array/object children; property hooks and __debugInfo() are not executed."
        }
    };
    let _ = writeln!(out, "<p class=\"hint\">{hint}</p>");
    if dropped > 0 {
        let _ = writeln!(
            out,
            "<p class=\"warning\">Capture truncated: {dropped} events dropped.</p>"
        );
    }
    out.push_str("<h2>Call tree</h2>\n");
    if roots.is_empty() {
        out.push_str("<p class=\"hint\">No call events were captured.</p>\n");
    } else {
        render_tree(&nodes, &roots, &mut out, 1, true);
    }
    render_dumps(events, &mut out);
    render_segments(events, &mut out);
    out.push_str("</body>\n</html>\n");
    out
}

/// Renders one shell that exposes the exact profile page and the call-capture
/// page together, with Calls/Dumps/Traces tabs linked to the profile view.
///
/// Both existing reports are embedded as `srcdoc` iframes: they keep their own
/// interactive graph/timeline/source panels and call tree unchanged, and the
/// shell adds no network or external assets. The Dumps/Traces tabs activate or
/// scroll the Calls iframe to the matching section.
pub(crate) fn render_unified_html(
    profile: Option<&str>,
    calls: &str,
    target: &str,
    mode: CallCaptureMode,
    captured_calls: usize,
    dropped: u64,
) -> String {
    let mut out = String::new();
    out.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n");
    out.push_str("<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    out.push_str("<title>elephc monitor — profile + calls</title>\n");
    out.push_str(
        "<style>\n\
         :root { color-scheme: light dark; }\n\
         * { box-sizing: border-box; }\n\
         html, body { height: 100%; margin: 0; }\n\
         body { font-family: system-ui, -apple-system, Segoe UI, sans-serif; display: flex; flex-direction: column; }\n\
         header { padding: .55rem .9rem; border-bottom: 1px solid rgba(128,128,128,.35); }\n\
         h1 { font-size: 1rem; margin: 0 0 .25rem; }\n\
         .summary { margin: 0; color: GrayText; font-size: .82rem; }\n\
         .badge { display: inline-block; padding: 0 .4rem; border: 1px solid rgba(128,128,128,.45); border-radius: 999px; font-size: .8em; }\n\
         nav.tabs { display: flex; gap: .25rem; padding: .45rem .9rem; border-bottom: 1px solid rgba(128,128,128,.35); }\n\
         nav.tabs button { font: inherit; padding: .3rem .75rem; border: 1px solid transparent; border-radius: 6px; background: transparent; color: inherit; cursor: pointer; }\n\
         nav.tabs button:hover { background: rgba(128,128,128,.12); }\n\
         nav.tabs button[aria-selected=\"true\"] { border-color: rgba(128,128,128,.45); background: rgba(128,128,128,.16); font-weight: 650; }\n\
         main { flex: 1; min-height: 0; position: relative; }\n\
         section.panel { display: none; position: absolute; inset: 0; }\n\
         section.panel.active { display: block; }\n\
         iframe { width: 100%; height: 100%; border: 0; display: block; }\n\
         .missing { margin: 1.5rem; padding: 1rem; border: 1px solid rgba(128,128,128,.35); border-radius: 8px; }\n\
         </style>\n",
    );
    out.push_str("</head>\n<body>\n");
    out.push_str("<header>\n<h1>elephc monitor</h1>\n");
    let _ = writeln!(
        out,
        "<p class=\"summary\">Target <code>{}</code> · calls <span class=\"badge\">{}</span> · <strong>{captured_calls}</strong> captured calls{}</p>",
        escape_html(target),
        mode.as_str(),
        if dropped > 0 {
            format!(" · <span class=\"badge\">truncated: {dropped} dropped</span>")
        } else {
            String::new()
        }
    );
    out.push_str("</header>\n");
    out.push_str("<nav class=\"tabs\" role=\"tablist\">\n");
    out.push_str("<button type=\"button\" data-tab=\"profile\" role=\"tab\" aria-selected=\"true\">Profile</button>\n");
    out.push_str("<button type=\"button\" data-tab=\"calls\" role=\"tab\" aria-selected=\"false\">Calls</button>\n");
    out.push_str("<button type=\"button\" data-tab=\"dumps\" role=\"tab\" aria-selected=\"false\">Dumps</button>\n");
    out.push_str("<button type=\"button\" data-tab=\"traces\" role=\"tab\" aria-selected=\"false\">Trace segments</button>\n");
    out.push_str("</nav>\n<main>\n");
    out.push_str("<section class=\"panel active\" id=\"profile-panel\" role=\"tabpanel\">\n");
    if let Some(profile) = profile {
        let _ = writeln!(
            out,
            "<iframe id=\"profile-frame\" title=\"Exact profile\" srcdoc=\"{}\"></iframe>",
            escape_html(profile)
        );
    } else {
        out.push_str("<p class=\"missing\">The exact profile is unavailable for this run. Rebuild with <code>--with-monitoring</code> or use <code>--exact</code>.</p>\n");
    }
    out.push_str("</section>\n");
    out.push_str("<section class=\"panel\" id=\"calls-panel\" role=\"tabpanel\">\n");
    let _ = writeln!(
        out,
        "<iframe id=\"calls-frame\" title=\"Call values\" srcdoc=\"{}\"></iframe>",
        escape_html(calls)
    );
    out.push_str("</section>\n</main>\n");
    out.push_str(
        "<script>\n\
         (() => {\n\
           const buttons = Array.from(document.querySelectorAll('[data-tab]'));\n\
           const panels = { profile: document.getElementById('profile-panel'), calls: document.getElementById('calls-panel') };\n\
           const callsFrame = document.getElementById('calls-frame');\n\
           const show = (name) => {\n\
             const panel = name === 'profile' ? 'profile' : 'calls';\n\
             for (const button of buttons) button.setAttribute('aria-selected', String(button.dataset.tab === name));\n\
             for (const [key, element] of Object.entries(panels)) element.classList.toggle('active', key === panel);\n\
           };\n\
           const scrollCalls = (id) => {\n\
             const scroll = () => {\n\
               const element = callsFrame && callsFrame.contentDocument && callsFrame.contentDocument.getElementById(id);\n\
               if (element) element.scrollIntoView({ block: 'start' });\n\
             };\n\
             if (callsFrame && callsFrame.contentDocument && callsFrame.contentDocument.readyState === 'complete') scroll();\n\
             else if (callsFrame) callsFrame.addEventListener('load', scroll, { once: true });\n\
           };\n\
           for (const button of buttons) button.addEventListener('click', () => {\n\
             const name = button.dataset.tab;\n\
             show(name);\n\
             if (name === 'dumps' || name === 'traces') scrollCalls(name);\n\
           });\n\
         })();\n\
         </script>\n",
    );
    out.push_str("</body>\n</html>\n");
    out
}

/// Builds a tree from the runtime's flat preorder call stream.
fn build_call_tree<'a>(events: &'a [&'a serde_json::Value]) -> (Vec<CallNode<'a>>, Vec<usize>) {
    let mut nodes: Vec<CallNode<'_>> = Vec::with_capacity(events.len());
    let mut roots = Vec::new();
    let mut stack: Vec<(usize, usize)> = Vec::new();
    for event in events {
        let event = *event;
        let depth = event
            .get("depth")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1)
            .max(1) as usize;
        let index = nodes.len();
        nodes.push(CallNode {
            event,
            children: Vec::new(),
        });
        while stack
            .last()
            .is_some_and(|(_, parent_depth)| *parent_depth >= depth)
        {
            stack.pop();
        }
        match stack.last() {
            Some((parent, _)) => nodes[*parent].children.push(index),
            None => roots.push(index),
        }
        stack.push((index, depth));
    }
    (nodes, roots)
}

/// Renders one list of sibling calls and their children.
fn render_tree(
    nodes: &[CallNode<'_>],
    indices: &[usize],
    out: &mut String,
    depth: usize,
    root: bool,
) {
    if root {
        out.push_str("<ul class=\"call-tree root\">\n");
    } else {
        out.push_str("<ul class=\"call-tree\">\n");
    }
    for index in indices {
        let node = &nodes[*index];
        let _ = writeln!(out, "<li class=\"call\" data-depth=\"{depth}\">");
        out.push_str(&render_call_line(node.event));
        if !node.children.is_empty() {
            if depth < MAX_RENDER_DEPTH {
                render_tree(nodes, &node.children, out, depth + 1, false);
            } else {
                out.push_str("<p class=\"truncated\">… deeper calls omitted …</p>\n");
            }
        }
        out.push_str("</li>\n");
    }
    out.push_str("</ul>\n");
}

/// Renders one call's function signature, arguments and return value.
fn render_call_line(event: &serde_json::Value) -> String {
    let function = event
        .get("function")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("<unknown>");
    let mut arguments = Vec::new();
    if let Some(args) = event.get("args").and_then(serde_json::Value::as_array) {
        for argument in args {
            let name = escape_html(
                argument
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("?"),
            );
            let redacted = argument
                .get("redacted")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let truncated = argument
                .get("truncated")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let badges = format!(
                "{}{}",
                if redacted {
                    "<span class=\"badge\">redacted</span>"
                } else {
                    ""
                },
                if truncated {
                    "<span class=\"badge\">truncated</span>"
                } else {
                    ""
                }
            );
            if let Some(value) = argument.get("value").and_then(serde_json::Value::as_str) {
                arguments.push(format!(
                    "<span class=\"arg\"><span class=\"name\">${name}</span> = <code>{}</code>{badges}</span>",
                    escape_html(value)
                ));
            } else if let Some(ty) = argument.get("type").and_then(serde_json::Value::as_str) {
                arguments.push(format!(
                    "<span class=\"arg\"><span class=\"name\">${name}</span>: <code class=\"type\">{}</code>{badges}</span>",
                    escape_html(ty)
                ));
            } else {
                arguments.push(format!(
                    "<span class=\"arg\"><span class=\"name\">${name}</span>{badges}</span>"
                ));
            }
        }
    }
    let returned = event
        .get("return")
        .and_then(|returned| {
            let truncated = returned
                .get("truncated")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let badge = if truncated {
                "<span class=\"badge\">truncated</span>"
            } else {
                ""
            };
            returned
                .get("value")
                .and_then(serde_json::Value::as_str)
                .map(|value| {
                    format!(
                        "<span class=\"return\"> → <code>{}</code>{badge}</span>",
                        escape_html(value)
                    )
                })
                .or_else(|| {
                    returned
                        .get("type")
                        .and_then(serde_json::Value::as_str)
                        .map(|ty| {
                            format!(
                                "<span class=\"return\"> → <code class=\"type\">{}</code>{badge}</span>",
                                escape_html(ty)
                            )
                        })
                })
        })
        .unwrap_or_default();
    let segment = event
        .get("segments")
        .and_then(serde_json::Value::as_array)
        .map(|names| {
            names
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<_>>()
                .join(" > ")
        })
        .unwrap_or_default();
    let segment_tag = if segment.is_empty() {
        String::new()
    } else {
        format!(
            "<span class=\"segment-tag\">[{}]</span>",
            escape_html(&segment)
        )
    };
    format!(
        "<div class=\"call-line\"><span class=\"fn\">{}</span><span class=\"args\">({})</span>{segment_tag}{returned}</div>{}",
        escape_html(function),
        arguments.join(", "),
        render_call_meta(event)
    )
}

/// Renders the per-call metadata: caller source site, timestamp, fiber,
/// allocation/free churn and exception state.
///
/// Older capture streams may carry only some fields; each part is optional and
/// the whole line disappears when none is present.
fn render_call_meta(event: &serde_json::Value) -> String {
    let mut parts = Vec::new();
    if let Some(caller) = event.get("caller") {
        let file = caller
            .get("file")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let line = caller
            .get("line")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        if !file.is_empty() && line > 0 {
            parts.push(format!(
                "at <code>{}</code>",
                escape_html(&format!("{file}:{line}"))
            ));
        }
    }
    if let Some(timestamp) = event
        .get("timestamp_us")
        .and_then(serde_json::Value::as_u64)
    {
        parts.push(format!("t=+{:.3} ms", timestamp as f64 / 1000.0));
    }
    if let Some(fiber) = event.get("fiber").and_then(serde_json::Value::as_u64) {
        parts.push(if fiber == 0 {
            "main".to_string()
        } else {
            format!("fiber {fiber}")
        });
    }
    let alloc_delta = event
        .get("alloc_delta")
        .and_then(serde_json::Value::as_u64);
    let free_delta = event
        .get("free_delta")
        .and_then(serde_json::Value::as_u64);
    if let (Some(alloc), Some(free)) = (alloc_delta, free_delta) {
        parts.push(format!("+{alloc} alloc / -{free} free"));
    }
    if let Some(exception) = event.get("exception").filter(|value| !value.is_null()) {
        let class = exception
            .get("class")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("<unknown>");
        let message = exception
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let caught = exception
            .get("caught")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let state = if caught { "caught" } else { "uncaught" };
        let detail = if message.is_empty() {
            format!("exception <code>{}</code> ({state})", escape_html(class))
        } else {
            format!(
                "exception <code>{}</code>: {} ({state})",
                escape_html(class),
                escape_html(message)
            )
        };
        parts.push(format!("<span class=\"exception\">{detail}</span>"));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("<div class=\"call-meta\">{}</div>", parts.join(" · "))
    }
}

/// Renders explicit dump cards captured by the program.
fn render_dumps(events: &[serde_json::Value], out: &mut String) {
    let dumps: Vec<&serde_json::Value> = events.iter().filter(|event| is_dump(event)).collect();
    if dumps.is_empty() {
        return;
    }
    out.push_str("<h2 id=\"dumps\">Dumps</h2>\n<ul class=\"dumps\">\n");
    for dump in dumps {
        let label = dump
            .get("label")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("dump");
        let value = dump
            .get("value")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("<unavailable>");
        let type_name = dump
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("mixed");
        let segment = dump
            .get("segments")
            .and_then(serde_json::Value::as_array)
            .map(|names| {
                names
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" > ")
            })
            .unwrap_or_default();
        let segment_tag = if segment.is_empty() {
            String::new()
        } else {
            format!(
                "<span class=\"segment-tag\">[{}]</span>",
                escape_html(&segment)
            )
        };
        let truncated = dump
            .get("truncated")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let badge = if truncated {
            "<span class=\"badge\">truncated</span>"
        } else {
            ""
        };
        let _ = writeln!(
            out,
            "<li><span class=\"dump-label\">{}</span>{segment_tag} <code>{}</code> <span class=\"type\">{}</span>{badge}</li>",
            escape_html(label),
            escape_html(value),
            escape_html(type_name)
        );
    }
    out.push_str("</ul>\n");
}

/// Renders named trace segments with their captured call/dump counts.
fn render_segments(events: &[serde_json::Value], out: &mut String) {
    let segments = trace_segments(events);
    if segments.is_empty() {
        return;
    }
    out.push_str("<h2 id=\"traces\">Trace segments</h2>\n<ul class=\"segments\">\n");
    for segment in segments {
        let status = if segment.complete {
            "complete"
        } else {
            "incomplete"
        };
        let class = if segment.complete {
            "complete"
        } else {
            "incomplete"
        };
        let _ = writeln!(
            out,
            "<li class=\"segment {class}\"><span class=\"segment-name\">{}</span> <span class=\"badge\">{status}</span> · {} call{} · {} dump{}</li>",
            escape_html(&segment.name),
            segment.calls,
            if segment.calls == 1 { "" } else { "s" },
            segment.dumps,
            if segment.dumps == 1 { "" } else { "s" }
        );
    }
    out.push_str("</ul>\n");
}

/// Escapes one value for HTML text or attribute content.
fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Renders a nested tree, keeps siblings distinct, and escapes observed
    /// strings so a value cannot inject markup.
    #[test]
    fn renders_nested_call_tree_and_escapes_values() {
        let events = vec![
            serde_json::json!({"depth": 1, "function": "main", "args": [], "return": null}),
            serde_json::json!({
                "depth": 2,
                "function": "greet",
                "args": [{"name": "name", "value": "string(7) \"<world>\""}],
                "return": {"value": "string(21) \"<world><world><world>\""}
            }),
            serde_json::json!({
                "depth": 2,
                "function": "add",
                "args": [{"name": "a", "value": "int(2)"}, {"name": "b", "type": "int"}],
                "return": {"type": "int"}
            }),
        ];
        let html = render_calls_html(&events, 2, CallCaptureMode::Full, "main.php");
        assert!(html.starts_with("<!doctype html>"), "{html}");
        assert!(html.contains("<ul class=\"call-tree root\">"), "{html}");
        assert!(html.contains("data-depth=\"1\""), "{html}");
        assert!(html.contains("data-depth=\"2\""), "{html}");
        assert!(html.contains("<span class=\"fn\">main</span>"), "{html}");
        assert!(html.contains("<span class=\"fn\">greet</span>"), "{html}");
        assert!(html.contains("<span class=\"fn\">add</span>"), "{html}");
        assert!(
            html.contains("string(7) &quot;&lt;world&gt;&quot;"),
            "{html}"
        );
        assert!(html.contains("int(2)"), "{html}");
        assert!(
            html.contains("$b</span>: <code class=\"type\">int</code>"),
            "{html}"
        );
        assert!(
            html.contains("→ <code class=\"type\">int</code>"),
            "{html}"
        );
        assert!(
            html.contains("Capture truncated: 2 events dropped."),
            "{html}"
        );
        assert!(!html.contains("<world>"), "unescaped value leaked: {html}");
    }

    /// Verifies dump cards and named trace segments render without becoming
    /// pseudo-calls in the call tree, and that segment contents are counted.
    #[test]
    fn renders_dumps_and_trace_segments() {
        let events = vec![
            serde_json::json!({"kind": "call", "depth": 1, "function": "main", "segments": [], "args": [], "return": null}),
            serde_json::json!({"kind": "segment_start", "name": "parse", "depth": 2, "segments": ["parse"]}),
            serde_json::json!({"kind": "call", "depth": 2, "function": "parse", "segments": ["parse"], "args": [{"name": "n", "value": "int(4)"}], "return": {"value": "int(6)", "type": "int"}}),
            serde_json::json!({"kind": "dump", "depth": 2, "segments": ["parse"], "label": "parsed", "value": "array(2) {[\"sum\"] => int(6)}", "type": "array", "to": "auto"}),
            serde_json::json!({"kind": "segment_end", "name": "parse", "depth": 2, "segments": [], "complete": true}),
        ];
        let html = render_calls_html(&events, 0, CallCaptureMode::Full, "main.php");
        assert!(html.contains("<strong>2</strong> captured calls"), "{html}");
        assert!(!html.contains("<span class=\"fn\">dump</span>"), "{html}");
        assert!(html.contains("<h2 id=\"dumps\">Dumps</h2>"), "{html}");
        assert!(html.contains("parsed"), "{html}");
        assert!(
            html.contains("array(2) {[&quot;sum&quot;] =&gt; int(6)}"),
            "{html}"
        );
        assert!(
            html.contains("<span class=\"segment-tag\">[parse]</span>"),
            "{html}"
        );
        assert!(html.contains("<h2 id=\"traces\">Trace segments</h2>"), "{html}");
        assert!(html.contains("parse"), "{html}");
        assert!(html.contains("complete"), "{html}");
        assert!(html.contains("1 call"), "{html}");
        assert!(html.contains("1 dump"), "{html}");
    }

    /// Verifies the calls panel renders the caller site, timestamp, fiber,
    /// allocation/free churn and exception state captured for one call.
    #[test]
    fn renders_call_context_and_exception_metadata() {
        let events = vec![serde_json::json!({
            "kind": "call",
            "depth": 1,
            "function": "throwing",
            "segments": [],
            "args": [],
            "return": null,
            "caller": {"file": "main.php", "line": 11},
            "timestamp_us": 56,
            "fiber": 7,
            "allocs": 4,
            "frees": 1,
            "alloc_delta": 2,
            "free_delta": 1,
            "exception": {
                "class": "RuntimeException",
                "message": "boom",
                "code": 7,
                "caught": true
            }
        })];
        let html = render_calls_html(&events, 0, CallCaptureMode::None, "main.php");
        assert!(html.contains("call-meta"), "{html}");
        assert!(html.contains("at <code>main.php:11</code>"), "{html}");
        assert!(html.contains("t=+0.056 ms"), "{html}");
        assert!(html.contains("fiber 7"), "{html}");
        assert!(html.contains("+2 alloc / -1 free"), "{html}");
        assert!(
            html.contains(
                "exception <code>RuntimeException</code>: boom (caught)"
            ),
            "{html}"
        );
    }
}
