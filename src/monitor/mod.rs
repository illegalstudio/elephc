//! Purpose:
//! Implements `elephc monitor`: sample a compiled program and render a PHP-level
//! profile — a Speedscope file with a helpers-folded PHP view and a cause-annotated
//! runtime view, plus a per-function cause table on stdout.
//!
//! Called from:
//! - `crate::main()` when the first argument is exactly `monitor`.
//!
//! Key details:
//! - A launched program defaults to exact capture on macOS and Linux. A running
//!   service defaults to its in-process sampled ring; `--exact` selects one
//!   completed request. `--live` asks the program it launched over a socketpair;
//!   `--attach` reads a process it did not launch from the outside, with
//!   `/usr/bin/sample` on macOS and `ptrace` on Linux.
//! - `sample` splits one function into sibling call-graph nodes per sampled call
//!   offset; aggregation is by symbol, never by node identity.
//! - Inlined PHP calls leave no frame, but the inliner preserves callee source
//!   spans: a sampled address inside the caller that resolves (via `atos` and the
//!   dSYM) to a line owned by another function's declaration range becomes a
//!   virtual `name (inlined)` frame. Best-effort: it needs the `.php` source and
//!   the dSYM, and silently degrades to plain frames without them.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process;

mod command;
pub(crate) use command::*;
// Reading an image from the outside, for the one path that has no channel in.
// Parsing only, so it is exercised by ordinary tests on any host — which is what
// makes code whose real use is on another platform reviewable at all.
//
// Its CALLER is Linux's, so on any other host every item here is dead outside
// the tests. Allowed rather than gated away: gating it would take the tests with
// it, and then the parsing that Linux depends on would be checked nowhere but
// Linux — which is the opposite of why it was separated.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod elf;
// Naming what an out-of-process sampler read. Also parsing only, allowed dead on
// non-Linux hosts for the same reason: the syscalls that produce those addresses
// are the one part a host without `ptrace` cannot run, and they are kept apart
// from this so that everything except them stays testable.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod attach;
// The syscalls themselves, and the loop that drives them. Linux only, and the
// one file here no test on this host reaches — which is why it holds nothing
// but them.
#[cfg(target_os = "linux")]
mod ptrace;
mod channel;
pub(crate) use channel::*;
mod local;
pub(crate) use local::*;
mod remote;
pub(crate) use remote::*;
mod sampled;
pub(crate) use sampled::*;
mod exact;
pub(crate) use exact::*;
mod render;
pub(crate) use render::*;
mod exports;
pub(crate) use exports::*;
mod stitch;
pub(crate) use stitch::*;

#[cfg(test)]
mod tests;

/// Names the flags a service target cannot honour, or `None` when none were given.
///
/// `run_probe_host` reads `--exact` and renders every export flag it is given;
/// everything listed here was parsed, stored, and silently never evaluated. A
/// budget handed to a service is the dangerous one: the command exited 0 and the
/// pipeline believed a gate had run.
///
/// What belongs here is decided by reading that function, never by what a flag
/// sounds like it needs — see the exporter comment inside.
///
/// Lives here rather than in the argument parser because the parser cannot tell a
/// service from a program — a socket target is recognised by asking the
/// filesystem, not by its spelling.
pub(crate) fn unhonoured_service_flags(cmd: &MonitorCommand) -> Option<String> {
    let given: Vec<&str> = [
        ("--assert", !cmd.asserts.is_empty()),
        // The same budget, spelled as a file. Left out of the first version of
        // this list, so the defect it was written to close — "a budget handed to
        // a service was never evaluated" — stayed open through its other name.
        ("--assert-file", cmd.assert_file.is_some()),
        ("--baseline", cmd.baseline.is_some()),
        ("--fail-on-regression", cmd.fail_on_regression.is_some()),
        ("--save", cmd.save.is_some()),
        ("--trace", cmd.trace.is_some()),
        ("--prometheus", cmd.prom_out.is_some()),
        ("--otlp", cmd.otlp.is_some()),
        // `--out`, `--pprof`, `--dot` and `--html` are all written by the
        // service path — `run_probe_host` calls `write_speedscope`, the pprof
        // encoder and `write_graph_exports` on the sampled answer — so none of
        // them belongs here. The first version of this list refused the last
        // two on the strength of a claim that the sampled path did not render
        // graphs, which one reading of `run_probe_host` would have disproved:
        // `monitor <addr> --html out.html` exited 2 before connecting and wrote
        // nothing, breaking an export the CLI reference advertises for exactly
        // that target. A refusal is as capable of being wrong as a silent pass.
        //
        // Modes with no meaning against a service: it answers once, through its
        // endpoint. Accepting them ran a one-shot read and called it success.
        ("--live", cmd.live),
        ("--serve", cmd.serve.is_some()),
    ]
    .iter()
    .filter(|(_, given)| *given)
    .map(|(name, _)| *name)
    .collect();
    if given.is_empty() {
        return None;
    }
    Some(given.join(", "))
}

/// Runs the full capture-and-render pipeline; returns the process exit code.
pub(crate) fn run(cmd: MonitorCommand) -> i32 {
    if !cmd.stitch.is_empty() {
        // Offline: read service logs and correlate their slices. Captures
        // nothing itself, so it runs before every capture path.
        return run_stitch(&cmd);
    }
    // `--duration` sizes an external sampling window, and only `--live` and
    // `--attach` take one. Local exact capture measures the launched run; a
    // service answers its cumulative ring or its next exact request. Silently
    // ignoring the flag would leave someone believing either was time-bounded.
    if cmd.duration_explicit && !cmd.live && cmd.attach_pid.is_none() {
        eprintln!(
            "elephc monitor: --duration applies to external --live/--attach windows only; \
             local exact capture measures the launched run, while a service answers its \
             current ring or next exact request."
        );
    }
    // A running service is a legitimate target, not a special mode: `monitor
    // 127.0.0.1:9000` reads a process that is already serving traffic, which is
    // the only way to profile production without restarting it.
    //
    // A local socket is recognised by ASKING THE FILESYSTEM, not by its spelling.
    // Keying on a leading `/` made every absolute path a socket, so
    // `monitor /usr/local/bin/shop` tried to connect to the binary and
    // `monitor /home/me/shop.php` answered with a complaint about a missing build
    // key — an absolute path being the most ordinary thing a user can type.
    if remote_target(&cmd.target).is_some() || is_socket_path(&cmd.target) {
        // Refuse the flags this path cannot honour, rather than accepting them
        // and exiting 0. `run_probe_host` reads `--exact` and writes every
        // export it is given, and does nothing else, so a budget or a baseline
        // handed to a service was parsed, stored, and never evaluated — a CI
        // gate that always passes.
        // The existing `--exact` refusal below was written for exactly this
        // failure ("this used to warn and exit 0, which told automation it had
        // an artifact it did not have"); it covered the exporters and not these.
        if let Some(unhonoured) = unhonoured_service_flags(&cmd) {
            eprintln!(
                "elephc monitor: {unhonoured} cannot be honoured against a running service: \
                 that target answers with a profile, and nothing here evaluates a budget or \
                 writes a baseline. Profile the program locally to use them, or drop them to \
                 read the service."
            );
            return 2;
        }
        let target = cmd.target.clone();
        return run_probe_host(&cmd, &target);
    }
    // Which mechanism answers is decided by what the TARGET can do, never by a
    // flag: asking a user to choose between sampling and instrumentation is
    // asking them to know where their program is running, which is exactly the
    // distinction this command exists not to have. A source is compiled with the
    // capability; a binary that carries it is read exactly. `--live` and
    // `--attach` are not sampled from the outside any more, but they still keep
    // their own path: live ASKS the child it launched over a socketpair, and
    // attach reads a process it never launched, and neither is what
    // `run_instrument` does.
    if !cmd.live && cmd.attach_pid.is_none() {
        let is_source = cmd.target.ends_with(".php");
        if is_source || carries_monitoring(std::path::Path::new(&cmd.target)) {
            return run_instrument(&cmd);
        }
    }
    // Refuse an unequipped target BEFORE any platform branch. Placed after one,
    // the strictness would be macOS-only: the same binary that is refused on a
    // laptop would be run and quietly under-reported on a Linux server, which is
    // precisely the environment-dependent behaviour this command exists to
    // remove. `--attach` is exempt — it never sees a path to check.
    if cmd.attach_pid.is_none() && !cmd.target.ends_with(".php") {
        if let Err(error) = require_monitoring(std::path::Path::new(&cmd.target)) {
            eprintln!("elephc monitor: {error}");
            return 1;
        }
    }
    // `--attach` is the only path that reads a process from the OUTSIDE. It used
    // to be macOS-only, and `--live` was refused beside it for the same stated
    // reason — wrongly, since `--live` LAUNCHES its target and can hand it a
    // socketpair and ask. Attach cannot ask: it is handed a pid already running
    // under someone else's control, so reading it from the outside is not a
    // shortcut here, it is the whole job. macOS has `/usr/bin/sample` for that;
    // on Linux this tool does it itself, with `ptrace`.
    if let Some(pid) = cmd.attach_pid {
        let image = match attach_image(pid) {
            Ok(image) => image,
            Err(reason) => {
                eprintln!("elephc monitor: {reason}");
                return 1;
            }
        };
        return if cmd.live {
            // Attach never launched the target, so it never owns its lifetime
            // and there is nothing here to leave alone or reap.
            run_live(&cmd, pid, None, None, image.as_ref()).code
        } else {
            run_once(&cmd, pid, None, None, image.as_ref())
        };
    }
    // With `--with-monitoring`, not `--debug-info`. Reaching here with a `.php`
    // means `--live`, because the source path returns through `run_instrument`
    // above and `--attach` never has a path at all — and live asks the child for
    // its samples, which a binary compiled without the probe cannot answer. It
    // did not ask for the probe before, so `monitor hot.php --live` launched a
    // program with nothing listening and then waited out every window for an ACK
    // that could not come. The test that was meant to cover this compiled the
    // fixture by hand first and monitored the BINARY, so the one path a user
    // takes was the one path nothing ran.
    let (binary, php_source) = if cmd.target.ends_with(".php") {
        match channel::compile_php_monitored(&cmd.target) {
            Ok(path) => (path, Some(PathBuf::from(&cmd.target))),
            Err(error) => {
                eprintln!("elephc monitor: {error}");
                return 1;
            }
        }
    } else {
        // Through the same resolver the exact path uses. `Command::new("shop")`
        // searches PATH rather than running `./shop`, and that was fixed on one
        // of the two spawn sites: `--live` kept failing with `No such file or
        // directory` for a binary sitting right there, except on machines whose
        // PATH carries an empty entry.
        (spawnable_path(&cmd.target), None)
    };
    // The live path launches the target too, so it can ask it directly rather
    // than read it from the outside. It did not open a channel before, which is
    // the whole reason `--live` needed an external sampler — for a program this
    // process had started itself.
    let mut command = process::Command::new(&binary);
    let channel = if cmd.live { open_polled_control_channel() } else { None };
    if let Some(channel) = &channel {
        attach_control_channel(&mut command, channel);
        // Asking also wakes the EXACT profiler, which writes its table to stderr
        // when the program ends. That is the one thing a live table must not
        // print: the operator would get the raw dump under the view they are
        // watching, as if the program had written it. Captured and filtered, the
        // way the exact path has always filtered the sampler's folded stacks out
        // of a program's own diagnostics.
        command.stderr(process::Stdio::piped());
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            eprintln!("elephc monitor: cannot run {}: {error}", binary.display());
            return 1;
        }
    };
    // The spawn gave the program its own copy; ours would keep the socket alive
    // after it exits and turn the next snapshot request into a permanent wait.
    let mut channel = channel;
    if let Some(channel) = channel.as_mut() {
        channel.release_child();
    }
    // Drained as it arrives rather than at exit: a live view runs for as long as
    // the program does, and a pipe nobody reads fills and blocks the program
    // writing into it.
    if let Some(stderr) = child.stderr.take() {
        std::thread::Builder::new()
            .name("elephc-monitor-stderr".to_string())
            .spawn(move || {
                use std::io::BufRead as _;
                for line in std::io::BufReader::new(stderr).lines().map_while(Result::ok) {
                    if is_profiler_line(&line) {
                        continue;
                    }
                    eprintln!("{line}");
                }
            })
            .ok();
    }
    let root = child.id();
    let (code, leave_target_running) = if cmd.live {
        let outcome = run_live(&cmd, root, Some(&mut child), channel.as_ref(), None);
        (outcome.code, outcome.leave_target_running)
    } else {
        (run_once(&cmd, root, Some(&binary), php_source.as_deref(), None), false)
    };
    let still_running = child.try_wait().ok().flatten().is_none();
    match disposition(still_running, code, cmd.live, leave_target_running) {
        Disposition::Stop => {
            let _ = child.kill();
            let _ = child.wait();
        }
        Disposition::Collect => {
            let _ = child.wait();
        }
        // Not even waited for: waiting is what would hang this process on a
        // program that is still doing its work.
        Disposition::LeaveAlone => {}
    }
    code
}

/// Merges the display stacks into the helpers-folded PHP view: one weighted
/// entry per distinct PHP frame chain, virtual inlined frames included.
pub(crate) fn php_folded_stacks(display: &[(Vec<(String, Kind)>, u64)]) -> Vec<(Vec<String>, u64)> {
    let mut merged: BTreeMap<Vec<String>, u64> = BTreeMap::new();
    for (stack, weight) in display {
        let folded: Vec<String> = stack
            .iter()
            .filter(|(_, kind)| matches!(kind, Kind::Php | Kind::PhpInlined))
            .map(|(name, _)| name.clone())
            .collect();
        let key = if folded.is_empty() {
            vec!["<non-PHP>".to_string()]
        } else {
            folded
        };
        *merged.entry(key).or_default() += weight;
    }
    merged.into_iter().collect()
}

/// Milliseconds since the Unix epoch, for frame timestamps.
pub(crate) fn epoch_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Serves the live `--html` file over HTTP on a background thread so the page,
/// reached over http(s), can re-fetch itself and update in place (no reload).
/// Every GET returns the file's current bytes — the live loop rewrites it
/// atomically, so a request always reads a whole document. Bind to a loopback
/// address unless you intend to expose the profile.
pub(crate) fn serve_live_file(addr: &str, path: String) -> std::io::Result<std::net::SocketAddr> {
    let listener = std::net::TcpListener::bind(addr)?;
    let local = listener.local_addr()?;
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            // Polls are seconds apart; handling one connection at a time is fine.
            let _ = serve_one_request(stream, &path);
        }
    });
    Ok(local)
}

/// Returns the target process and its direct children (a prefork server's
/// workers), root first. Best-effort: without `pgrep` only the root is sampled.
pub(crate) fn discover_pids(root: u32) -> Vec<u32> {
    let mut pids = vec![root];
    if let Ok(output) = process::Command::new("/usr/bin/pgrep")
        .args(["-P", &root.to_string()])
        .output()
    {
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if let Ok(pid) = line.trim().parse::<u32>() {
                pids.push(pid);
            }
        }
    }
    pids
}

/// Parses every report into merged samples, recovering inlined frames when the
/// binary's dSYM and the PHP source are available (one-shot spawn mode only).
pub(crate) fn samples_from_reports(
    reports: &[String],
    binary: Option<&Path>,
    php_source: Option<&Path>,
) -> Option<Vec<(Vec<Frame>, u64)>> {
    let mut merged = Vec::new();
    for report in reports {
        let rows = parse_call_graph(report);
        if rows.is_empty() {
            continue;
        }
        let mut samples = build_samples(&rows);
        if let (Some(binary), Some(source)) = (binary, php_source) {
            inject_inlined_frames(&mut samples, report, binary, source);
        }
        merged.extend(samples);
    }
    (!merged.is_empty()).then_some(merged)
}

/// A node with no measurements, used only to ask whether a metric name exists.
pub(crate) static EMPTY_NODE: std::sync::LazyLock<crate::call_graph::GraphNode> =
    std::sync::LazyLock::new(|| crate::call_graph::GraphNode {
        name: String::new(),
        inclusive: 0,
        exclusive: 0,
        call_count: None,
        alloc_inclusive: 0,
        alloc_exclusive: 0,
        io_inclusive: 0,
        io_exclusive: 0,
        retained_inclusive: 0,
        retained_exclusive: 0,
        wait_inclusive: 0,
        wait_exclusive: 0,
        network_inclusive: 0,
        network_exclusive: 0,
        network_wait_inclusive: 0,
        network_wait_exclusive: 0,
        causes: Vec::new(),
    });

/// Loads the budget file to use: the explicit `--assert-file`, else the nearest
/// `.elephc` above the profiled source. Returns the assertions and the path
/// they came from, so the report can say which file is gating the build.
pub(crate) fn load_assert_file(
    explicit: Option<&str>,
    target: &str,
) -> Result<Option<(Vec<(String, Option<String>)>, String)>, String> {
    let path = match explicit {
        Some(path) => PathBuf::from(path),
        None => match find_project_file(target) {
            Some(found) => found,
            None => return Ok(None),
        },
    };
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    Ok(Some((parse_assert_file(&text), path.display().to_string())))
}

/// Every assertable metric, with the one-line help the CLI and docs print.
/// Keeping the list in one place is what lets an unknown metric report the
/// valid ones instead of just failing.
pub(crate) const ASSERT_METRICS: &[(&str, &str)] = &[
    ("calls", "exact invocation count"),
    ("allocs", "heap allocations attributed to the function itself"),
    ("retained", "allocations still live when it returns (allocated minus freed)"),
    ("queries", "DB queries it issues itself"),
    ("self_ms", "milliseconds of its own time"),
    ("incl_ms", "milliseconds including everything it calls"),
    ("wait_ms", "milliseconds blocked inside a driver call"),
    ("network", "outgoing network operations it issues itself"),
    ("network_wait_ms", "milliseconds blocked in outgoing network work"),
    ("time_pct", "inclusive time as a percentage of the run"),
];

/// The value of one assertable metric for a node.
pub(crate) fn assert_metric_value(
    metric: &str,
    node: &crate::call_graph::GraphNode,
    root_ns: u64,
) -> Option<f64> {
    match metric {
        "calls" => Some(node.call_count.unwrap_or(0) as f64),
        "allocs" => Some(node.alloc_exclusive as f64),
        "retained" => Some(node.retained_exclusive as f64),
        "queries" => Some(node.io_exclusive as f64),
        "self_ms" => Some(node.exclusive as f64 / 1_000_000.0),
        "incl_ms" => Some(node.inclusive as f64 / 1_000_000.0),
        "wait_ms" => Some(node.wait_exclusive as f64 / 1_000_000.0),
        "network" => Some(node.network_exclusive as f64),
        "network_wait_ms" => Some(node.network_wait_exclusive as f64 / 1_000_000.0),
        "time_pct" => Some(100.0 * node.inclusive as f64 / root_ns as f64),
        _ => None,
    }
}

/// The same metric for the whole run, which is what `*` asserts on.
///
/// Self values sum across functions (that is what makes them a partition), so a
/// run total is their sum; the time metrics come from the root instead, since
/// summing inclusive times would count every caller again.
pub(crate) fn assert_run_total(metric: &str, graph: &crate::call_graph::CallGraph, root_ns: u64) -> Option<f64> {
    let sum_excl = |f: fn(&crate::call_graph::GraphNode) -> f64| -> f64 {
        graph.nodes.iter().map(f).sum()
    };
    match metric {
        "calls" => Some(sum_excl(|n| n.call_count.unwrap_or(0) as f64)),
        "allocs" => Some(sum_excl(|n| n.alloc_exclusive as f64)),
        "retained" => Some(sum_excl(|n| n.retained_exclusive as f64)),
        "queries" => Some(sum_excl(|n| n.io_exclusive as f64)),
        "self_ms" | "incl_ms" => Some(root_ns as f64 / 1_000_000.0),
        "wait_ms" => Some(sum_excl(|n| n.wait_exclusive as f64) / 1_000_000.0),
        "network" => Some(sum_excl(|n| n.network_exclusive as f64)),
        "network_wait_ms" => {
            Some(sum_excl(|n| n.network_wait_exclusive as f64) / 1_000_000.0)
        }
        "time_pct" => Some(100.0),
        _ => None,
    }
}

/// Reads one `key=<u64>` field out of a metrics fragment.
pub(crate) fn instr_field(fragment: &str, key: &str) -> u64 {
    fragment
        .split(key)
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0)
}

/// Same, for a field that can be negative — retained objects (allocated minus
/// freed) go below zero for a function that releases more than it takes.
pub(crate) fn instr_field_i64(fragment: &str, key: &str) -> i64 {
    fragment
        .split(key)
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0)
}

/// One profiled request slice, tagged with where it came from.
pub(crate) struct Slice {
    service: String,
    graph: crate::call_graph::CallGraph,
}

/// Splits a `--web --instrument` service log into its per-request dumps.
///
/// Every slice opens with its `elephc-instr-trace:` line (the runtime writes it
/// first), so that line is the record separator. Text before the first one is
/// not a slice — it is whatever else the service logged.
pub(crate) fn split_slices(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        if line.starts_with("elephc-instr-trace: ") {
            out.push(String::new());
        }
        if let Some(current) = out.last_mut() {
            current.push_str(line);
            current.push('\n');
        }
    }
    out
}

/// Orders slices into trace trees and prints one line per span, indented by
/// depth. Returns the rendered report.
///
/// A span whose parent is absent from the collected logs is treated as a root:
/// a partial capture (one service's log missing, or the trace started in a
/// service that is not elephc) must still render what it has rather than
/// silently drop half the trace.
/// Escapes a Prometheus label value.
///
/// Route labels come from untrusted HTTP paths, and the exposition format ends a
/// value at an unescaped quote and a sample at a newline — so a path containing
/// either could forge series in a file the scraper trusts. Backslash first, or
/// the escapes we add would themselves be re-escaped.
pub(crate) fn escape_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// `{service="…"}` or `{service="…",route="…"}`.
pub(crate) fn labels(s: &ServiceStats) -> String {
    match &s.route {
        Some(route) => format!(
            "{{service=\"{}\",route=\"{}\"}}",
            escape_label(&s.service),
            escape_label(route)
        ),
        None => format!("{{service=\"{}\"}}", escape_label(&s.service)),
    }
}

/// Descriptor the child finds its control channel on.
pub(crate) const CONTROL_FD: i32 = 3;
/// Marker written into the channel before spawning, so it is already buffered
/// when the child looks and no handshake can race the program's own start.
pub(crate) const CONTROL_MAGIC: &[u8] = b"ELEPHC-MONITOR-1";
/// The same marker, from a monitor that will POLL the child for snapshots.
///
/// Deliberately the same length: the child's check peeks a fixed sixteen bytes
/// and compares, so a second marker costs it nothing. Mirrors
/// `CONTROL_MAGIC_LIVE` in `elephc-probe`; the two are one protocol and share a
/// name so a `grep` finds the pair.
pub(crate) const CONTROL_MAGIC_LIVE: &[u8] = b"ELEPHC-MONITOR-L";
/// Acknowledgement returned after the child consumed the control marker and
/// activated its embedded monitoring runtime.
pub(crate) const CONTROL_ACK: &[u8] = b"ELEPHC-MONITOR-ACK-1";

/// Holds the parent's end of the control channel open for the child's lifetime.
pub(crate) struct ControlChannel {
    /// This process's end. `request_snapshot` asks over it; nothing else on the
    /// machine can, which is what makes it a credential.
    pub(crate) parent: i32,
    child: i32,
}

impl ControlChannel {
    /// Drops the parent's copy of the CHILD end, once the spawn has handed the
    /// real one to the profiled program.
    ///
    /// Only matters to a caller that reads the channel. While this process holds
    /// a copy, the socket has a writer that never writes — so when the profiled
    /// program exits, `recv` on the parent end does not report EOF, it blocks
    /// forever waiting for us. A `--live` loop asking for a snapshot hung there
    /// instead of noticing the target was gone.
    /// Marks the child end as no longer ours to close, for a caller that closed
    /// it itself. Closing a descriptor twice closes whatever was opened on that
    /// number in between.
    #[cfg(test)]
    fn forget_child(&mut self) {
        self.child = -1;
    }

    fn release_child(&mut self) {
        if self.child >= 0 {
            unsafe {
                libc::close(self.child);
            }
            self.child = -1;
        }
    }
}

impl Drop for ControlChannel {
    /// Closes both ends. The child end is inherited across the spawn, so
    /// leaking either would leave the profiled program holding a channel
    /// nobody reads. Already-released ends are left alone rather than closed
    /// twice — a second `close` on a recycled number closes somebody else's file.
    fn drop(&mut self) {
        unsafe {
            libc::close(self.parent);
            if self.child >= 0 {
                libc::close(self.child);
            }
        }
    }
}

/// The marker `--with-monitoring` embeds, searched for in the target's bytes.
pub(crate) const MONITORING_MARKER: &[u8] = b"elephc-monitoring-v1";

/// Refuses a target that was not built to be monitored.
///
/// Whether a target names an existing Unix socket.
///
/// The question is what the path IS, not how it is spelled: a socket answers a
/// profiling endpoint, a regular file is a program to run. Asking the filesystem
/// costs one `stat` and removes a whole class of surprise — a path that does not
/// exist is not a socket either, so it falls through to the file paths and gets
/// their error message instead of a connection failure.
pub(crate) fn is_socket_path(target: &str) -> bool {
    use std::os::unix::fs::FileTypeExt as _;
    std::fs::metadata(target)
        .map(|meta| meta.file_type().is_socket())
        .unwrap_or(false)
}

/// Per-service request statistics, the shape an operator pages on.
pub(crate) struct ServiceStats {
    service: String,
    /// The endpoint, when every slice named one. Kept apart from `service` so a
    /// metrics label does not have to be re-split out of a display string.
    route: Option<String>,
    requests: usize,
    /// Nearest-rank percentiles over per-request duration, in nanoseconds.
    p50: u64,
    p90: u64,
    p95: u64,
    p99: u64,
    /// Requests per second over the service's own wall-clock window. `None`
    /// when the slices carry no timestamps, or when they all landed inside the
    /// same microsecond — a rate over a zero window is a division, not a fact.
    rps: Option<f64>,
    /// Mean DB queries per request; an N+1 shows here before it shows anywhere else.
    queries_per_request: f64,
    /// Share of request time spent blocked in a driver.
    wait_share: f64,
    /// Mean outgoing network operations per request.
    network_per_request: f64,
    /// Mean seconds per request spent waiting on outgoing network work.
    network_wait_seconds_per_request: f64,
}

/// Nearest-rank percentile: the value at position ceil(p/100 x n), 1-indexed.
///
/// Chosen over interpolation because it always returns a value some request
/// actually took. Interpolating between two requests invents a duration nobody
/// experienced, which is the wrong trade when the inputs are exact to begin with.
pub(crate) fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = ((p / 100.0) * sorted.len() as f64).ceil().max(1.0) as usize;
    sorted[rank.min(sorted.len()) - 1]
}

/// Loads a saved exact call graph (`--save` output) for diffing.
pub(crate) fn load_exact_graph(path: &str) -> Option<crate::call_graph::CallGraph> {
    let json = std::fs::read_to_string(path).ok()?;
    let mut graph: crate::call_graph::CallGraph = serde_json::from_str(&json).ok()?;
    // Profiles saved before network metrics became inclusive/exclusive deserialize their
    // direct values through serde aliases. An exclusive value with no inclusive partner
    // can only be that legacy shape, so restore the minimum valid inclusive value.
    for node in &mut graph.nodes {
        if node.network_inclusive == 0 && node.network_exclusive > 0 {
            node.network_inclusive = node.network_exclusive;
        }
        if node.network_wait_inclusive == 0 && node.network_wait_exclusive > 0 {
            node.network_wait_inclusive = node.network_wait_exclusive;
        }
    }
    Some(graph)
}

/// Formats nanoseconds as a human duration for the exact table.
pub(crate) fn fmt_ns(ns: u64) -> String {
    if ns >= 1_000_000_000 {
        format!("{:.2} s", ns as f64 / 1e9)
    } else if ns >= 1_000_000 {
        format!("{:.1} ms", ns as f64 / 1e6)
    } else if ns >= 1_000 {
        format!("{:.1} µs", ns as f64 / 1e3)
    } else {
        format!("{ns} ns")
    }
}

/// One parsed call-graph line: nesting depth, sample count, the frame symbol,
/// and the first sampled address of the node's bucket.
#[derive(Debug, PartialEq)]
pub(crate) struct Row {
    depth: usize,
    count: u64,
    symbol: String,
    module: String,
    address: Option<u64>,
}

/// One frame of a rebuilt sample stack.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Frame {
    symbol: String,
    address: Option<u64>,
    /// Set on virtual frames recovered from inlined source spans.
    inlined: bool,
}

/// Returns whether a sampled symbol is a PHP-level frame (script main, a
/// function, or a method) rather than a runtime helper or synthetic body.
pub(crate) fn is_php_symbol(symbol: &str) -> bool {
    let stem = symbol.trim_start_matches('_');
    stem == "main" || stem.starts_with("fn_") || stem.starts_with("method_")
}

/// What a runtime helper is doing, in words a PHP developer can act on.
/// Prefix-matched in order, most specific first.
pub(crate) const CAUSES: &[(&str, &str)] = &[
    ("rt_mixed_from_value", "Mixed cell boxing"),
    ("rt_array_to_mixed", "Mixed cell boxing"),
    ("rt_str_to_mixed", "Mixed cell boxing"),
    ("rt_heap_alloc", "heap allocation"),
    ("rt_mixed_cast_", "Mixed cell unboxing"),
    ("rt_mixed_unbox", "Mixed cell unboxing"),
    ("rt_mixed_numeric", "dynamic Mixed arithmetic"),
    ("rt_mixed_free", "memory release"),
    ("rt_heap_free", "memory release"),
    ("rt_incref", "reference counting"),
    ("rt_decref", "reference counting"),
    ("rt_release", "reference counting"),
    ("rt_concat", "string concatenation"),
    ("rt_str_", "string operation"),
    ("rt_array_", "array operation"),
    ("rt_hash_", "hash operation"),
];

/// Returns the cause label for a runtime helper symbol, if it is one.
pub(crate) fn cause_for(symbol: &str) -> Option<&'static str> {
    let stem = symbol.trim_start_matches('_');
    if !stem.starts_with("rt_") {
        return None;
    }
    for (prefix, cause) in CAUSES {
        if stem.starts_with(prefix) {
            return Some(cause);
        }
    }
    Some("runtime helper")
}

// --------------------------------------------------------------------------
// Inlined-frame recovery
// --------------------------------------------------------------------------

/// A function's declaration range in the PHP source, `[start, end]` inclusive.
#[derive(Debug, PartialEq)]
pub(crate) struct DeclRange {
    name: String,
    start: u32,
    end: u32,
}

/// Returns the identifier following `keyword` on the line, tolerating leading
/// modifiers (`public function step`, `final class Engine`). The keyword must
/// sit at a word boundary; anonymous closures yield no name and are skipped.
pub(crate) fn declared_name(line: &str, keyword: &str) -> Option<String> {
    let index = line.find(keyword)?;
    if index > 0
        && !line[..index]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_whitespace())
    {
        return None;
    }
    let rest = line[index + keyword.len()..].trim_start_matches(['&', ' ']);
    let name: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// Returns the 1-based line where the brace block opened at/after `start` closes.
pub(crate) fn brace_span_end(lines: &[&str], start: usize) -> u32 {
    let mut depth = 0i32;
    let mut opened = false;
    for (index, line) in lines.iter().enumerate().skip(start) {
        for byte in line.bytes() {
            match byte {
                b'{' => {
                    depth += 1;
                    opened = true;
                }
                b'}' => depth -= 1,
                _ => {}
            }
        }
        if opened && depth <= 0 {
            return (index + 1) as u32;
        }
    }
    lines.len() as u32
}

/// Per-line self cost, recovered from the sampled addresses via the dSYM.
///
/// A sample is charged to the source line of the innermost PHP frame on its
/// stack — the line that was actually executing — so the totals are the same
/// self-time distribution the function table shows, only at line granularity.
/// This is **sampled**, not exact: `--instrument` times whole calls, and an
/// exact per-line timer would need a clock read per line, which would cost far
/// more than the thing it measures. The dSYM is what makes the sampled version
/// free.
pub(crate) struct LineProfile {
    /// The PHP source file these lines belong to.
    pub file: String,
    /// Its text, split into lines (1-based when displayed).
    pub source: Vec<String>,
    /// Samples charged to each 1-based line number.
    pub hits: HashMap<u32, u64>,
    /// Total samples charged to any line (the denominator for a line's share).
    pub total: u64,
}

/// Sums each leaf sample's weight onto the source line its address resolves to.
/// Addresses the dSYM could not place are dropped from BOTH the per-line counts
/// and the total, so a line's share is over what was actually attributable
/// rather than being quietly diluted by unresolvable samples.
pub(crate) fn attribute_lines(
    leaves: &[(u64, u64)],
    resolved: &HashMap<u64, u32>,
) -> (HashMap<u32, u64>, u64) {
    let mut hits: HashMap<u32, u64> = HashMap::new();
    let mut total = 0;
    for (address, weight) in leaves {
        if let Some(line) = resolved.get(address) {
            *hits.entry(*line).or_insert(0) += weight;
            total += weight;
        }
    }
    (hits, total)
}

/// Parses 64 hex chars into a 32-byte key.
pub(crate) fn parse_hex_key(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 {
        return None;
    }
    let mut key = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks_exact(2).enumerate() {
        key[i] = u8::from_str_radix(std::str::from_utf8(chunk).ok()?, 16).ok()?;
    }
    Some(key)
}

/// What `--attach` needs to read a process it was only handed a pid for, or the
/// sentence explaining why this host cannot.
///
/// `None` is not a failure: macOS reads a process through `/usr/bin/sample`,
/// which resolves its own symbols, so there is nothing to prepare. Linux reads
/// the process itself and needs the image first. Anywhere else there is no way
/// in at all, and saying so — with the way that DOES work on every host — beats
/// an `EPERM` from a syscall the reader did not know was being made.
fn attach_image(pid: u32) -> Result<Option<attach::Image>, String> {
    #[cfg(target_os = "linux")]
    {
        attach::image_for(pid).map(Some).map_err(|error| {
            // The hint only goes on a refusal. Appended to every failure it named
            // a cause that had not occurred: an absent pid answered "No such file
            // or directory (the kernel's yama/ptrace_scope is 1, …)", which sends
            // an operator to change a setting that was never in the way.
            match (error.denied, ptrace::attach_refusal_hint()) {
                (true, Some(hint)) => format!("{} ({hint})", error.reason),
                _ => error.reason,
            }
        })
    }
    #[cfg(target_os = "macos")]
    {
        let _ = pid;
        Ok(None)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Err(format!(
            "attaching to a running process is not implemented on this platform. Read it \
             through its endpoint instead: start it with ELEPHC_PROBE_ADDR=127.0.0.1:9411, \
             then `elephc monitor 127.0.0.1:9411` (pid {pid} is untouched)."
        ))
    }
}

/// What becomes of a target this process launched, now that the capture is over.
enum Disposition {
    /// Stop it, then collect it.
    Stop,
    /// Collect it: it has finished, or is about to on its own.
    Collect,
    /// Leave it running and do not wait for it.
    LeaveAlone,
}

/// Decides between them from the four facts that matter, in one place, so the
/// rule can be read and tested rather than inferred from where it sits.
///
/// A short-lived program is allowed to finish on its own after a successful
/// one-shot capture. A capture that FAILED, and every live view, stops one that
/// is still up: `--live` runs until monitoring is over, so waiting on a
/// long-running target would hang forever.
///
/// `lost_channel` is the exception that outranks all of it, and the reason is
/// whose fault it is. The view ended because THIS tool's plumbing stopped
/// answering, not because the program did anything, and stopping a program over
/// our own quiet socket ends work the operator never asked us to end. It stays
/// up, uncollected — they have its pid.
fn disposition(still_running: bool, code: i32, live: bool, lost_channel: bool) -> Disposition {
    if !still_running {
        return Disposition::Collect;
    }
    if lost_channel {
        return Disposition::LeaveAlone;
    }
    if code != 0 || live {
        return Disposition::Stop;
    }
    Disposition::Collect
}

/// A display-ready frame: its user-facing name and what kind of time it is.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Kind {
    Php,
    PhpInlined,
    Helper,
    Native,
}

/// Turns an exact capture into the stack form the Speedscope and pprof writers
/// take, so those exports work on a measured profile and not only a sampled one.
///
/// They very nearly did not. `--out` and `--pprof` were wired to the sampled
/// path alone, and when the exact profile became the default they silently
/// stopped producing anything — including for the CI regression gate, which is
/// documented as `--out` a baseline and `--baseline` it back.
///
/// The conversion walks callers to callees and emits one stack per path, giving
/// each path the nanoseconds the edge itself measured. Where a function has
/// several callers, its self time is split among them in proportion to those
/// edge times rather than duplicated — so the weights still sum to the run,
/// which is the property a profile viewer's percentages depend on. Recursion is
/// bounded by the path itself: a function already on the stack is not descended
/// into again, and its remaining time stays on the frame that reached it.
/// Below this share of the run, a path is folded into its caller instead of
/// being descended into.
///
/// One stack is emitted per distinct root-to-leaf path, and shared callees are
/// reached once per caller, so a chain of diamonds — layered dispatch, ordinary
/// framework shape — doubles the count at every level. It stopped only because
/// the budget halves each time and integer division eventually reaches zero,
/// which is a brake made of the units: measured on a 52-node chain, a budget of
/// 10^6 gave 2,951 stacks and the same graph with a realistic 10-second
/// nanosecond budget gave 319,930. The bound was `log2(root nanoseconds)`, which
/// is not a bound anyone chose.
///
/// A path carrying less than this fraction of the capture tells a reader
/// nothing, so it stops there and the time stays on the frame that reached it —
/// the total is unaffected, which is the property everything downstream rests on.
///
/// Measured against the ROOTS' time, not the sum of self times: the budget being
/// divided comes from inclusive time, and a floor derived from anything else can
/// round to zero and never engage — which is exactly what a first attempt at
/// this did.
pub(crate) const EXACT_STACK_FLOOR_DIVISOR: u64 = 10_000;

/// Hard ceiling on emitted stacks.
///
/// The share floor bounds the count in terms of the capture, but a wide enough
/// graph can still reach it slowly, and no reader has ever needed a hundred
/// thousand distinct stacks. Reaching it stops the DESCENT, never the emission:
/// a frame that stops descending keeps its children's time as its own, so the
/// total stays right and the shape degrades instead of the arithmetic. It is
/// reported when hit, because a silently truncated profile reads exactly like a
/// complete one.
pub(crate) const EXACT_STACK_CAP: usize = 50_000;

/// How deep a single root-to-leaf descent may go before it stops.
///
/// `on_path` prevents cycles but not depth, and the recursion is a real Rust
/// stack: a genuinely deep chain of distinct functions would overflow it, losing
/// a capture that had already been taken.
pub(crate) const EXACT_STACK_MAX_DEPTH: usize = 512;

/// Per-function statistics aggregated from display stacks: total and self
/// weight per PHP function (virtual inlined frames included), plus the runtime
/// causes sampled underneath each.
pub(crate) struct TableStats {
    grand: u64,
    totals: BTreeMap<String, u64>,
    selfs: BTreeMap<String, u64>,
    causes: BTreeMap<String, BTreeMap<&'static str, u64>>,
}

/// Renders a percentage as a fixed-width Unicode bar, readable in any log.
pub(crate) fn bar(pct: f64, width: usize) -> String {
    let filled = ((pct / 100.0) * width as f64).round().clamp(0.0, width as f64) as usize;
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

/// A bar that starts where the span started, for the terminal waterfall.
///
/// The HTML trace places spans on a shared axis; the text report is the default
/// output, so leaving it flush-left would keep exactly the ambiguity the axis
/// removes — two sequential hops and two concurrent ones drawing identically.
/// Lead cells are `·` (before this span existed), the span itself `█`, the
/// remainder `░`. A span always gets at least one cell, so a very short hop is
/// visible rather than rounded away.
pub(crate) fn placed_bar(offset_pct: f64, pct: f64, width: usize) -> String {
    let cells = width as f64;
    // Always leave a cell for the span itself: a hop starting at the very end of
    // the window would otherwise push the bar one cell wide, misaligning the
    // column for every row under it.
    let lead = (((offset_pct / 100.0) * cells).round().clamp(0.0, cells) as usize)
        .min(width.saturating_sub(1));
    let room = width - lead;
    let fill = (((pct / 100.0) * cells).round().clamp(0.0, cells) as usize).clamp(1, room.max(1));
    let rest = width.saturating_sub(lead + fill);
    format!("{}{}{}", "·".repeat(lead), "█".repeat(fill), "░".repeat(rest))
}
