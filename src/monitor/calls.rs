//! Purpose:
//! Implements `elephc monitor --calls`: run a `--debug` build with call capture
//! enabled, parse the runtime's serialized call events, and render the PHP
//! argument values in execution order.
//!
//! Called from:
//! - `crate::monitor::run()` when `MonitorCommand::calls` is set.
//!
//! Key details:
//! - The debug runtime already emits a function-enter hook for the activation
//!   stack. Call capture only adds a bounded event ring and a JSON line per call,
//!   so it reuses the debug metadata and the read-only value formatter.
//! - A `.php` source is compiled with `--debug`; a binary must have the
//!   `<stem>.edbg.json` sidecar produced by the same build.
//! - Program stdout stays live. Stderr is captured for events and every
//!   non-event line is passed through unchanged.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::*;

/// How much of each call argument/return value `monitor --calls` captures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CallCaptureMode {
    /// Function/depth events only, no argument or return values.
    None,
    /// Argument and return types, no values.
    Types,
    /// Full scalar values; arrays/objects keep their summary.
    Scalars,
    /// Values including bounded nested array/object children.
    Full,
}

impl CallCaptureMode {
    /// Parses a `--calls=<mode>` value; `all` remains an alias for `full`.
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "none" => Some(Self::None),
            "types" => Some(Self::Types),
            "scalars" => Some(Self::Scalars),
            "full" | "all" | "1" => Some(Self::Full),
            _ => None,
        }
    }

    /// Returns the mode spelling passed to the runtime.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Types => "types",
            Self::Scalars => "scalars",
            Self::Full => "full",
        }
    }
}

/// Compiles one PHP source into a `--debug` executable for call capture.
///
/// With `monitored`, the same build also embeds the exact instrumentation so a
/// single run can publish both its call events and its per-function timing.
fn compile_php_debug(source: &str, monitored: bool) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|error| format!("cannot locate elephc: {error}"))?;
    let mut command = Command::new(exe);
    command.arg("--debug");
    if monitored {
        command.arg("--with-monitoring");
    }
    let status = command
        .arg(source)
        .status()
        .map_err(|error| format!("cannot run elephc: {error}"))?;
    if !status.success() {
        let flags = if monitored {
            "--debug --with-monitoring"
        } else {
            "--debug"
        };
        return Err(format!("compiling {source} with {flags} failed"));
    }
    Ok(spawnable_path(source.trim_end_matches(".php")))
}

/// Returns whether a binary has the debug metadata sidecar a `--debug` build writes.
fn carries_debug_metadata(path: &Path) -> bool {
    path.with_extension("edbg.json").is_file()
}

/// Renders the captured call events as an indented call-value table.
pub(crate) fn render_calls(
    events: &[serde_json::Value],
    dropped: u64,
    mode: CallCaptureMode,
) -> String {
    let calls: Vec<&serde_json::Value> = events
        .iter()
        .filter(|event| super::capture_events::is_call(event))
        .collect();
    let mut out = format!("call values ({}) [{}]:\n", calls.len(), mode.as_str());
    for event in &calls {
        let depth = event
            .get("depth")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(1) as usize;
        let function = event
            .get("function")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("<unknown>");
        let rendered = event
            .get("args")
            .and_then(serde_json::Value::as_array)
            .map(|args| {
                args.iter()
                    .map(|argument| {
                        let name = argument
                            .get("name")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("?");
                        if let Some(value) = argument
                            .get("value")
                            .and_then(serde_json::Value::as_str)
                        {
                            format!("${name} = {value}")
                        } else if let Some(ty) = argument
                            .get("type")
                            .and_then(serde_json::Value::as_str)
                        {
                            format!("${name}: {ty}")
                        } else {
                            format!("${name}")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        let returned = event
            .get("return")
            .and_then(|value| {
                value
                    .get("value")
                    .and_then(serde_json::Value::as_str)
                    .map(|value| format!(" -> {value}"))
                    .or_else(|| {
                        value
                            .get("type")
                            .and_then(serde_json::Value::as_str)
                            .map(|ty| format!(" -> {ty}"))
                    })
            })
            .unwrap_or_default();
        out.push_str(&format!(
            "{}{}({rendered}){returned}\n",
            "  ".repeat(depth.saturating_sub(1)),
            function
        ));
    }
    let dumps: Vec<&serde_json::Value> = events
        .iter()
        .filter(|event| super::capture_events::is_dump(event))
        .collect();
    if !dumps.is_empty() {
        out.push_str(&format!("\ndumps ({}):\n", dumps.len()));
        for dump in dumps {
            let label = dump
                .get("label")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("dump");
            let value = dump
                .get("value")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("<unavailable>");
            out.push_str(&format!(
                "  {}{} = {}\n",
                super::capture_events::segment_prefix(dump),
                label,
                value
            ));
        }
    }
    let segments = super::capture_events::trace_segments(events);
    if !segments.is_empty() {
        out.push_str(&format!("\ntrace segments ({}):\n", segments.len()));
        for segment in segments {
            out.push_str(&format!(
                "  {} — {} ({} call{}, {} dump{})\n",
                segment.name,
                if segment.complete {
                    "complete"
                } else {
                    "incomplete"
                },
                segment.calls,
                if segment.calls == 1 { "" } else { "s" },
                segment.dumps,
                if segment.dumps == 1 { "" } else { "s" }
            ));
        }
    }
    if dropped > 0 {
        out.push_str(&format!(
            "call values truncated: {dropped} events dropped\n"
        ));
    }
    out
}

/// Writes the raw capture events as JSONL, ending with an overflow marker when
/// the runtime event ring dropped any event.
fn write_calls_jsonl(
    path: &str,
    events: &[serde_json::Value],
    dropped: u64,
) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut file = std::fs::File::create(path)?;
    for event in events {
        writeln!(file, "{event}")?;
    }
    if dropped > 0 {
        writeln!(
            file,
            "{}",
            serde_json::json!({"kind": "capture_overflow", "dropped": dropped})
        )?;
    }
    Ok(())
}

/// Runs `monitor --calls` and returns the program's exit code.
///
/// `--exact` combines the exact `--with-monitoring` profile with call capture in
/// one run: the program is built with both capabilities, activated over the same
/// control channel, and its stderr carries both the instrumentation dump and the
/// call events. The exact profile prints first, then the call-value table.
pub(crate) fn run_calls(cmd: &MonitorCommand) -> i32 {
    // Call values are the debug dimension of the exact monitor: every calls-mode
    // run carries the monitoring capability, prints the exact table and can write
    // the same Chrome trace. `--exact` remains accepted but is now implied.
    let combined_exact = true;
    let unsupported: Vec<&str> = [
        ("--live", cmd.live),
        ("--attach", cmd.attach_pid.is_some()),
        ("--duration", cmd.duration_explicit),
        ("--baseline", cmd.baseline.is_some()),
        ("--save", cmd.save.is_some()),
        ("--assert", !cmd.asserts.is_empty()),
        ("--assert-file", cmd.assert_file.is_some()),
        ("--out", cmd.out.is_some()),
        ("--pprof", cmd.pprof_out.is_some()),
        ("--dot", cmd.dot_out.is_some()),
        ("--serve", cmd.serve.is_some()),
        ("--prometheus", cmd.prom_out.is_some()),
        ("--otlp", cmd.otlp.is_some()),
        ("--fail-on-regression", cmd.fail_on_regression.is_some()),
        ("--key", cmd.probe_key.is_some()),
    ]
    .iter()
    .filter(|(_, given)| *given)
    .map(|(name, _)| *name)
    .collect();
    if !unsupported.is_empty() {
        eprintln!(
            "elephc monitor: --calls prints call values; it cannot be combined with {}",
            unsupported.join(", ")
        );
        return 2;
    }
    if remote_target(&cmd.target).is_some() || is_socket_path(&cmd.target) {
        eprintln!(
            "elephc monitor: --calls needs a local --debug source or binary; a running service \
             does not expose call values"
        );
        return 2;
    }
    let binary = if cmd.target.ends_with(".php") {
        match compile_php_debug(&cmd.target, combined_exact) {
            Ok(path) => path,
            Err(error) => {
                eprintln!("elephc monitor: {error}");
                return 1;
            }
        }
    } else {
        let path = spawnable_path(&cmd.target);
        if !carries_debug_metadata(&path) {
            let build = if combined_exact {
                "--debug --with-monitoring"
            } else {
                "--debug"
            };
            eprintln!(
                "elephc monitor: {} has no debug metadata — rebuild it with {build} for --calls",
                path.display()
            );
            return 1;
        }
        if combined_exact && !carries_monitoring(&path) {
            eprintln!(
                "elephc monitor: {} carries no exact instrumentation — rebuild it with \
                 --debug --with-monitoring for --calls --exact/--html",
                path.display()
            );
            return 1;
        }
        path
    };

    // The exact path is activated over a socketpair only this process holds; the
    // call capture rides on ELEPHC_DEBUG_CALLS in the same child.
    let channel = if combined_exact {
        open_control_channel()
    } else {
        None
    };
    let mut command = Command::new(&binary);
    command
        .env("ELEPHC_DEBUG_CALLS", cmd.calls_mode.as_str())
        .env_remove("ELEPHC_DEBUG_SITES")
        .env_remove("ELEPHC_DEBUG_STDIN")
        .env_remove("ELEPHC_DEBUG_DAP")
        .stderr(Stdio::piped());
    if let Some(channel) = &channel {
        attach_control_channel(&mut command, channel);
    }
    if let Some(trace_path) = &cmd.trace {
        command.env("ELEPHC_INSTR_TRACE", trace_path);
    }
    let output = match command.spawn().and_then(|child| child.wait_with_output()) {
        Ok(output) => output,
        Err(error) => {
            eprintln!("elephc monitor: cannot run {}: {error}", binary.display());
            return 1;
        }
    };
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut events = Vec::new();
    let mut dropped = 0u64;
    let mut passthrough = Vec::new();
    for line in stderr.lines() {
        if let Some(event) = line.strip_prefix("elephc-debug-call: ") {
            match serde_json::from_str::<serde_json::Value>(event) {
                Ok(event) => events.push(event),
                Err(error) => eprintln!("elephc monitor: invalid call event: {error}"),
            }
        } else if let Some(count) = line.strip_prefix("elephc-debug-calls-dropped: ") {
            dropped = count.trim().parse::<u64>().unwrap_or(0);
        } else if combined_exact && is_profiler_line(line) {
            // The exact parser needs the raw dump; do not echo it as if the
            // program had written it.
        } else {
            passthrough.push(line);
        }
    }
    for line in passthrough {
        eprintln!("{line}");
    }
    for line in stderr.lines() {
        if let Some(note) = line.strip_prefix("elephc-instr: note: ") {
            eprintln!("elephc monitor: {note}");
        }
    }

    let mut profile_failed = false;
    let mut profile_html: Option<String> = None;
    if combined_exact {
        let capture_activated = channel.as_ref().is_some_and(control_channel_activated);
        let mut graph = parse_instrument_dump(&stderr);
        if graph.nodes.is_empty() {
            eprintln!(
                "elephc monitor: {}",
                no_profile_reason(&output.status, &binary, capture_activated)
            );
            profile_failed = true;
        } else {
            attach_exact_source(&mut graph, &cmd.target);
            profile_html = Some(crate::call_graph::render_html_exact(
                &graph,
                &cmd.target,
                &[],
            ));
            print!("{}", instrument_table(&graph));
            print!("{}", instrument_recommendations(&graph));
        }
    }

    if events.is_empty() {
        eprintln!(
            "elephc monitor: {} exited without publishing call values; rebuild it with --debug \
             and run a program that calls a PHP function",
            binary.display()
        );
        return 1;
    }
    if let Some(path) = &cmd.calls_out {
        if let Err(error) = write_calls_jsonl(path, &events, dropped) {
            eprintln!("elephc monitor: cannot write --calls-out '{path}': {error}");
            return 1;
        }
    }
    if let Some(path) = &cmd.html_out {
        let calls_html = super::call_html::render_calls_html(
            &events,
            dropped,
            cmd.calls_mode,
            &cmd.target,
        );
        let captured_calls = events
            .iter()
            .filter(|event| super::capture_events::is_call(event))
            .count();
        let html = super::call_html::render_unified_html(
            profile_html.as_deref(),
            &calls_html,
            &cmd.target,
            cmd.calls_mode,
            captured_calls,
            dropped,
        );
        if let Err(error) = std::fs::write(path, html) {
            eprintln!("elephc monitor: cannot write --html '{path}': {error}");
            return 1;
        }
        eprintln!("elephc monitor: unified profile + calls panel written to {path}");
    }
    if let Some(path) = &cmd.trace {
        if std::path::Path::new(path).exists() {
            eprintln!("elephc monitor: trace written to {path}");
        }
    }
    print!("{}", render_calls(&events, dropped, cmd.calls_mode));
    if profile_failed {
        1
    } else {
        output.status.code().unwrap_or(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the JSONL export ends with an explicit overflow marker when the
    /// bounded event ring dropped events.
    #[test]
    fn writes_capture_overflow_jsonl_marker() {
        let dir = std::env::temp_dir().join(format!(
            "elephc-calls-jsonl-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("jsonl temp dir");
        let path = dir.join("events.jsonl");
        let events = vec![serde_json::json!({
            "kind": "call",
            "function": "main",
            "depth": 1,
            "args": [],
            "return": null
        })];
        write_calls_jsonl(path.to_str().unwrap(), &events, 3).expect("jsonl write");
        let text = std::fs::read_to_string(&path).expect("jsonl read");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{text}");
        assert_eq!(lines[0], events[0].to_string(), "{text}");
        assert_eq!(
            lines[1],
            "{\"kind\":\"capture_overflow\",\"dropped\":3}",
            "{text}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Renders type-only and no-value events without inventing values.
    #[test]
    fn renders_type_only_and_empty_call_values() {
        let types = vec![serde_json::json!({
            "depth": 1,
            "function": "inspect",
            "args": [{"name": "n", "type": "int"}],
            "return": {"type": "array"}
        })];
        let rendered = render_calls(&types, 0, CallCaptureMode::Types);
        assert!(rendered.contains("inspect($n: int) -> array"), "{rendered}");
        let none = vec![serde_json::json!({
            "depth": 1,
            "function": "inspect",
            "args": [],
            "return": null
        })];
        let rendered = render_calls(&none, 0, CallCaptureMode::None);
        assert!(rendered.contains("inspect()"), "{rendered}");
        assert!(!rendered.contains("$n"), "{rendered}");
    }

    /// Renders one parent and one indented child with scalar/string arguments and
    /// the child's return value.
    #[test]
    fn renders_indented_call_values() {
        let events = vec![
            serde_json::json!({"depth": 1, "function": "main", "args": []}),
            serde_json::json!({
                "depth": 2,
                "function": "greet",
                "args": [
                    {"name": "name", "value": "string(5) \"world\""},
                    {"name": "times", "value": "int(3)"}
                ],
                "return": {"value": "string(15) \"worldworldworld\"", "type": "string"}
            }),
        ];
        let rendered = render_calls(&events, 0, CallCaptureMode::Full);
        assert!(rendered.contains("call values (2) [full]:"), "{rendered}");
        assert!(rendered.contains("main()"), "{rendered}");
        assert!(
            rendered.contains(
                "  greet($name = string(5) \"world\", $times = int(3)) -> string(15) \"worldworldworld\""
            ),
            "{rendered}"
        );
    }
}
