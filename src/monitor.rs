//! Purpose:
//! Implements `elephc monitor`: sample a compiled program and render a PHP-level
//! profile — a Speedscope file with a helpers-folded PHP view and a cause-annotated
//! runtime view, plus a per-function cause table on stdout.
//!
//! Called from:
//! - `crate::main()` when the first argument is exactly `monitor`.
//!
//! Key details:
//! - The default capture is exact and needs no sampler, so it is identical on
//!   macOS and Linux. Only `--live` and `--attach` read a process from the
//!   outside, which shells out to `/usr/bin/sample` and is therefore macOS-only.
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

/// Parsed `elephc monitor` invocation.
pub(crate) struct MonitorCommand {
    /// A compiled binary, or a `.php` source to compile first. Empty when attaching.
    pub target: String,
    /// A running process to attach to instead of spawning `target`.
    pub attach_pid: Option<u32>,
    /// Refresh the table live, window after window, until the target exits.
    pub live: bool,
    /// Sampling window; `sample` stops early when the program exits.
    pub duration_secs: u32,
    /// Whether `--duration` was actually passed, as opposed to defaulted. The
    /// in-process probe path captures a whole run and cannot honour a window, so
    /// it says so rather than silently ignoring a flag the user chose.
    pub duration_explicit: bool,
    /// Speedscope output path; defaults to `<target>.speedscope.json`.
    pub out: Option<String>,
    /// gzip-compressed pprof export path (Pyroscope / `go tool pprof` interchange).
    pub pprof_out: Option<String>,
    /// Prior capture (a monitor Speedscope file) to diff the run against.
    pub baseline: Option<String>,
    /// Exit non-zero when any function's total share grew by more than this
    /// many percentage points against the baseline.
    /// Path to write Prometheus textfile-format metrics to.
    pub prom_out: Option<String>,
    /// OTLP/HTTP endpoint to export correlated slices to as OpenTelemetry spans.
    pub otlp: Option<String>,
    /// Path to the build key, for reading a running service (`--key`).
    /// Fail the run when a function's share grew by more than this many points.
    pub fail_on_regression: Option<f64>,
    /// Serve the live page over HTTP at this address instead of writing a file.
    pub serve: Option<String>,
    pub probe_key: Option<String>,
    /// Graphviz DOT call-graph export path.
    pub dot_out: Option<String>,
    /// Self-contained interactive HTML call-graph export path.
    pub html_out: Option<String>,
    /// With `--instrument`, also write a Chrome/Perfetto timeline trace here.
    pub trace: Option<String>,
    /// Performance assertions on the exact profile (e.g. `calls:build<=1000`).
    /// Any failure makes `monitor` exit 2 — a CI gate on exact metrics.
    pub asserts: Vec<String>,
    /// With `--instrument`, save the exact capture here for use as a later
    /// `--baseline` (a JSON exact call graph).
    pub save: Option<String>,
    /// Log files of `--web --instrument` services to stitch into one
    /// distributed trace (repeatable). Each file is one service's stderr.
    pub stitch: Vec<String>,
    /// Explicit performance-budget file; without it, the nearest `.elephc`
    /// above the profiled source is picked up automatically.
    pub assert_file: Option<String>,
}

/// Usage text for `elephc monitor`, printed on parameter errors and `--help`.
pub(crate) const MONITOR_USAGE: &str = "Usage: elephc monitor <program|source.php> [--live] [--duration <seconds>] [--out <file.speedscope.json>]
       elephc monitor <program|source.php> [--pprof <file.pb.gz>] [--dot <file.dot>] [--html <file.html>]
       elephc monitor <program|source.php> [--trace <f.json>] [--assert <spec>]... [--assert-file <file>]
       elephc monitor <program|source.php> --live --html <file.html> [--serve <host:port>]
       elephc monitor <program|source.php> [--baseline <prior.json>] [--fail-on-regression <points>]
       elephc monitor <host:port|http://host|https://host> [--key <file>] [--out <f.json>] [--html <f.html>]
       elephc monitor --stitch <serviceA.log> --stitch <serviceB.log> ... [--html <f.html>]
                      [--otlp <http://collector:4318>] [--prometheus <file.prom>]
       elephc monitor --attach <pid> [--live] [--duration <seconds>]

There is one command for every environment. What differs between a laptop and a
production host is not how you profile — it is what you point at:

  a source        elephc monitor shop.php      built with --with-monitoring,
                                               then read
  a binary        elephc monitor ./shop        read, if it carries monitoring
  a running one   elephc monitor host:9411     read through its endpoint, over
                                               http:// or https://
  a local socket  elephc monitor /tmp/p.sock   the same, on the same machine

A program carries monitoring when it was built with --with-monitoring (or
--with-monitoring=<names> for named functions only). Without it there is nothing
to read, and monitor says so and stops rather than quietly returning less. The
capability is dormant until asked: a monitored binary run on its own behaves and
prints exactly like one built without it.

What comes back is an exact per-function profile — inclusive and self time, call
counts, allocations, retained bytes, SQL queries and I/O wait — not an estimate,
and the same numbers whichever of the four targets above produced them.

--live refreshes the table window after window, top-style, until the target
exits (or Ctrl-C). --attach reads an already-running local process; child worker
processes (a --web prefork server's workers) are discovered and merged in both
modes. Those two use an external sampler, which only macOS ships; elsewhere,
point monitor at the program's endpoint instead. Everything else works on Linux
and macOS alike.

--pprof writes the capture as a gzip pprof profile. --dot writes a Graphviz call
graph (`dot -Tsvg`); --html writes a self-contained, interactive call graph (one
node per function, caller->callee edges, hover metrics, search, zoom) that opens
in any browser with no network, with a time/memory/retained/wait/SQL toggle, a
flame view, a bottom-up callers/callees panel, a critical-path overlay, and —
when DB queries ran — a Queries panel listing each distinct statement and how
many times it ran, normalized so an N+1's repeated query folds into one x200
row. With a .php target and its dSYM the page also carries a Source view (key s)
annotating every line of the PHP file with the share that landed on it. With
--live, --html rewrites the page every window and keeps the last 10 captures
navigable — a timeline scrubber, a follow-latest toggle, and a diff-vs-previous
mode that lights up functions that grew hotter. Opened as a file it auto-reloads;
served over http it updates in place (no flicker). --serve <host:port> runs a
tiny HTTP server for exactly that (bind a loopback address unless you mean to
expose the profile).

--trace <file.json> writes a Chrome/Perfetto timeline, openable at
ui.perfetto.dev. The report prints recommendations, and --assert
<metric>:<fn><op><value> (repeatable; op <= >= == < >) gates the build — any
failed assertion exits 2, e.g. --assert calls:build<=1000. Metrics: calls,
allocs, retained, queries, self_ms, incl_ms, wait_ms, time_pct. Use `*` as the
function to assert on the whole run, e.g. --assert queries:*<=50. A project can
keep its budget in a `.elephc` file at its root (found by walking up from the
source, or named with --assert-file): one assertion per line, `#` comments, and
text after a trailing `#` labels the assertion in the report. The report lists
failures first with the counts, and --html adds a Checks panel showing each
assertion, its measured value and its budget.

--save <f.json> stores the capture; passing it later as --baseline prints a
per-function delta table and, with --html, a two-frame diff graph (scrub
baseline<->current, toggle the diff to see what grew). With --fail-on-regression
<points>, a function growing by more than that many percentage points exits with
status 2.

Reading a running service over the network needs the key it was built with:
--key <file>. The connection is authenticated in both directions and the profile
never crosses it unsigned; over https:// the certificate must validate. A --web
service also answers a signed X-Elephc-Query header on a single request, so one
request can be profiled in production without touching the others.

--otlp posts the correlated slices to an OTLP/HTTP collector as OpenTelemetry
spans, so an elephc hop stops being a gap in someone else's trace. --prometheus
writes the per-service stats in the text exposition format, for a textfile
collector. Profiles are not exported over OTLP on purpose: that signal is alpha,
and --pprof already feeds a Collector's pprof receiver.

--stitch <file> (repeatable) captures nothing: it reads the stderr logs of
services built with --web --with-monitoring and correlates their per-request
slices into distributed traces by W3C Trace Context, printing one indented line
per span (service, exact duration, function count, queries, waiting). A span
whose parent is not among the collected logs still renders, as a root, so a
partial collection shows what it has. With --html it also writes a
self-contained waterfall: each span placed by when it opened and sized by how
long it ran, and opening a row shows that service's hottest functions.";

/// Parses the arguments after the `monitor` selector.
pub(crate) fn parse_monitor_args(args: &[String]) -> Result<MonitorCommand, String> {
    let mut target = None;
    let mut attach_pid = None;
    let mut live = false;
    let mut duration_secs = None;
    let mut out = None;
    let mut pprof_out = None;
    let mut baseline = None;
    let mut fail_on_regression = None;
    let mut probe_key = None;
    let mut otlp = None;
    let mut prom_out = None;
    let mut dot_out = None;
    let mut html_out = None;
    let mut serve = None;
    let mut trace = None;
    let mut asserts: Vec<String> = Vec::new();
    let mut save = None;
    let mut stitch: Vec<String> = Vec::new();
    let mut assert_file = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{MONITOR_USAGE}");
                process::exit(0);
            }
            "--live" => live = true,
            "--attach" => {
                let value = it.next().ok_or("--attach needs a pid")?;
                attach_pid = Some(
                    value
                        .parse()
                        .map_err(|_| format!("invalid --attach pid '{value}'"))?,
                );
            }
            "--duration" => {
                let value = it.next().ok_or("--duration needs a value in seconds")?;
                duration_secs = Some(
                    value
                        .parse()
                        .map_err(|_| format!("invalid --duration '{value}'"))?,
                );
            }
            "--out" => {
                out = Some(it.next().ok_or("--out needs a file path")?.clone());
            }
            "--pprof" => {
                pprof_out = Some(it.next().ok_or("--pprof needs a file path")?.clone());
            }
            "--baseline" => {
                baseline = Some(it.next().ok_or("--baseline needs a file path")?.clone());
            }
            "--prometheus" => {
                prom_out = Some(it.next().ok_or("--prometheus needs a file path")?.clone());
            }
            "--otlp" => {
                otlp = Some(it.next().ok_or("--otlp needs an endpoint URL")?.clone());
            }
            "--key" => {
                probe_key = Some(it.next().ok_or("--key needs a file path")?.clone());
            }
            "--dot" => {
                dot_out = Some(it.next().ok_or("--dot needs a file path")?.clone());
            }
            "--html" => {
                html_out = Some(it.next().ok_or("--html needs a file path")?.clone());
            }
            "--serve" => {
                serve = Some(it.next().ok_or("--serve needs a host:port")?.clone());
            }
            "--trace" => {
                trace = Some(it.next().ok_or("--trace needs a file path")?.clone());
            }
            "--assert" => {
                asserts.push(it.next().ok_or("--assert needs a spec like calls:build<=1000")?.clone());
            }
            "--save" => {
                save = Some(it.next().ok_or("--save needs a file path")?.clone());
            }
            "--assert-file" => {
                assert_file = Some(it.next().ok_or("--assert-file needs a file path")?.clone());
            }
            "--stitch" => {
                stitch.push(it.next().ok_or("--stitch needs a file path")?.clone());
            }
            "--fail-on-regression" => {
                let value = it
                    .next()
                    .ok_or("--fail-on-regression needs a percentage-point threshold")?;
                fail_on_regression = Some(
                    value
                        .parse()
                        .map_err(|_| format!("invalid --fail-on-regression '{value}'"))?,
                );
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown monitor option '{other}'"));
            }
            other => {
                if target.replace(other.to_string()).is_some() {
                    return Err("monitor takes exactly one program argument".to_string());
                }
            }
        }
    }
    let target = match (&target, attach_pid) {
        (Some(_), Some(_)) => {
            return Err("--attach and a program argument are mutually exclusive".to_string())
        }
        // --stitch reads logs offline, so it is the one mode with no target.
        (None, None) if !stitch.is_empty() => String::new(),
        (None, None) => return Err("no program given".to_string()),
        (Some(target), None) => target.clone(),
        (None, Some(_)) => String::new(),
    };
    if serve.is_some() && (!live || html_out.is_none()) {
        return Err("--serve requires --live and --html (it serves the live graph)".to_string());
    }
    // Live windows default short so the display breathes; one-shot keeps 5s.
    let duration_explicit = duration_secs.is_some();
    let duration_secs = duration_secs.unwrap_or(if live { 3 } else { 5 });
    Ok(MonitorCommand {
        target,
        attach_pid,
        live,
        duration_secs,
        duration_explicit,
        out,
        pprof_out,
        baseline,
        fail_on_regression,
        probe_key,
        otlp,
        prom_out,
        dot_out,
        html_out,
        serve,
        trace,
        asserts,
        save,
        stitch,
        assert_file,
    })
}

/// Runs the full capture-and-render pipeline; returns the process exit code.
pub(crate) fn run(cmd: MonitorCommand) -> i32 {
    if !cmd.stitch.is_empty() {
        // Offline: read service logs and correlate their slices. Captures
        // nothing itself, so it runs before every capture path.
        return run_stitch(&cmd);
    }
    // `--duration` sizes a sampling window, and only `--live` and `--attach`
    // take windows — everything else measures a whole run exactly. Silently
    // ignoring it would leave someone believing they had bounded a capture.
    if cmd.duration_explicit && !cmd.live && cmd.attach_pid.is_none() {
        eprintln!(
            "elephc monitor: --duration sizes a sampling window and applies to --live and \
             --attach only; this capture measures the whole run exactly."
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
        let target = cmd.target.clone();
        return run_probe_host(&cmd, &target);
    }
    // Which mechanism answers is decided by what the TARGET can do, never by a
    // flag: asking a user to choose between sampling and instrumentation is
    // asking them to know where their program is running, which is exactly the
    // distinction this command exists not to have. A source is compiled with the
    // capability; a binary that carries it is read exactly. `--live` and
    // `--attach` still need the external sampler, so they keep their own path.
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
    // `--live` and `--attach` are the only paths left that read a process from
    // the outside, and the tool that does it ships on macOS alone. Everything
    // else — a source, a binary, a running service — is measured exactly and
    // needs no sampler, which is why this is the only platform branch left.
    if !cfg!(target_os = "macos") {
        if let Some(pid) = cmd.attach_pid {
            eprintln!(
                "elephc monitor: attaching to a running process needs an external sampler, \
                 which only macOS ships. Read it through its endpoint instead: start it \
                 with ELEPHC_PROBE_ADDR=127.0.0.1:9411, then `elephc monitor \
                 127.0.0.1:9411` (pid {pid} is untouched)."
            );
            return 1;
        }
        if cmd.live {
            eprintln!(
                "elephc monitor: --live refreshes from an external sampler, which only macOS \
                 ships. For a live view here, start the program with \
                 ELEPHC_PROBE_ADDR=127.0.0.1:9411, then `elephc monitor 127.0.0.1:9411`."
            );
            return 1;
        }
    }
    if let Some(pid) = cmd.attach_pid {
        return if cmd.live {
            run_live(&cmd, pid, None)
        } else {
            run_once(&cmd, pid, None, None)
        };
    }
    let (binary, php_source) = if cmd.target.ends_with(".php") {
        match compile_php_target(&cmd.target) {
            Ok(path) => (path, Some(PathBuf::from(&cmd.target))),
            Err(error) => {
                eprintln!("elephc monitor: {error}");
                return 1;
            }
        }
    } else {
        (PathBuf::from(&cmd.target), None)
    };
    let mut child = match process::Command::new(&binary).spawn() {
        Ok(child) => child,
        Err(error) => {
            eprintln!("elephc monitor: cannot run {}: {error}", binary.display());
            return 1;
        }
    };
    let root = child.id();
    let code = if cmd.live {
        run_live(&cmd, root, Some(&mut child))
    } else {
        run_once(&cmd, root, Some(&binary), php_source.as_deref())
    };
    // A short-lived program is allowed to finish naturally after a successful
    // one-shot capture. But when sampling failed, or when the live loop ended
    // with the target still up (it only exits once monitoring is over), waiting
    // would hang forever on a long-running target — reap it instead.
    let still_running = child.try_wait().ok().flatten().is_none();
    if still_running && (code != 0 || cmd.live) {
        let _ = child.kill();
    }
    let _ = child.wait();
    code
}

/// One sampling window over the whole process tree, rendered once: the bar
/// table on stdout, the Speedscope file, and the CI summary when applicable.
fn run_once(
    cmd: &MonitorCommand,
    root: u32,
    binary: Option<&Path>,
    php_source: Option<&Path>,
) -> i32 {
    let pids = discover_pids(root);
    let reports = capture_window(&pids, cmd.duration_secs);
    let samples = match samples_from_reports(&reports, binary, php_source) {
        Some(samples) => samples,
        None => {
            eprintln!(
                "elephc monitor: no samples captured — the program may have exited before \
                 sampling started; try a longer-running input"
            );
            return 1;
        }
    };
    let display = render_stacks(&samples);
    let out_path = cmd
        .out
        .clone()
        .unwrap_or_else(|| format!("{}.speedscope.json", cmd.target.trim_end_matches(".php")));
    if !cmd.target.is_empty() || cmd.out.is_some() {
        match write_speedscope(&display, &out_path) {
            Ok(()) => println!("wrote {out_path}"),
            Err(error) => {
                eprintln!("elephc monitor: {error}");
                return 1;
            }
        }
    }
    print!("{}", why_table(&display, pids.len()));
    write_github_summary(&display, pids.len());
    if let Some(pprof_path) = &cmd.pprof_out {
        let stacks = php_folded_stacks(&display);
        let encoded = crate::pprof_encode::encode_folded_profile(&stacks);
        match std::fs::write(pprof_path, encoded) {
            Ok(()) => println!("wrote {pprof_path}"),
            Err(error) => {
                eprintln!("elephc monitor: cannot write {pprof_path}: {error}");
                return 1;
            }
        }
    }
    let graph_title = if cmd.target.is_empty() {
        "elephc profile".to_string()
    } else {
        cmd.target.trim_end_matches(".php").to_string()
    };
    // Per-line attribution needs the dSYM and the source, so it rides the same
    // .php-target path that recovers inlined frames.
    let lines = match (binary, php_source) {
        (Some(binary), Some(source)) => reports
            .iter()
            .find_map(|report| line_profile(&samples, report, binary, source)),
        _ => None,
    };
    if let Err(error) = write_graph_exports(cmd, &display, &graph_title, lines.as_ref()) {
        eprintln!("elephc monitor: {error}");
        return 1;
    }
    if let Some(baseline_path) = &cmd.baseline {
        match diff_against_baseline(&display, baseline_path, cmd.fail_on_regression) {
            Ok(code) => return code,
            Err(error) => {
                eprintln!("elephc monitor: {error}");
                return 1;
            }
        }
    }
    0
}

/// Merges the display stacks into the helpers-folded PHP view: one weighted
/// entry per distinct PHP frame chain, virtual inlined frames included.
fn php_folded_stacks(display: &[(Vec<(String, Kind)>, u64)]) -> Vec<(Vec<String>, u64)> {
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

/// Aggregates the display stacks into a call graph: one node per PHP function
/// (inclusive/exclusive/causes reuse `table_stats`), and one edge per distinct
/// caller->callee adjacency, weighted by the samples whose stack traversed it.
fn build_call_graph(display: &[(Vec<(String, Kind)>, u64)]) -> crate::call_graph::CallGraph {
    use crate::call_graph::{CallGraph, GraphEdge, GraphNode};
    let stats = table_stats(display);
    // Node order is stable and reads top-down: hottest inclusive first, then name.
    let mut names: Vec<&String> = stats.totals.keys().collect();
    names.sort_by(|a, b| stats.totals[*b].cmp(&stats.totals[*a]).then_with(|| a.cmp(b)));
    let index: HashMap<&String, usize> =
        names.iter().enumerate().map(|(i, n)| (*n, i)).collect();
    let nodes: Vec<GraphNode> = names
        .iter()
        .map(|name| {
            let mut causes: Vec<(String, u64)> = stats
                .causes
                .get(*name)
                .map(|m| m.iter().map(|(c, w)| ((*c).to_string(), *w)).collect())
                .unwrap_or_default();
            causes.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            GraphNode {
                name: (*name).clone(),
                inclusive: stats.totals[*name],
                exclusive: stats.selfs.get(*name).copied().unwrap_or(0),
                // Sampling gives no exact count; --counters is the exact path.
                call_count: None,
                // Allocation and I/O counts are exact-only (--instrument).
                alloc_inclusive: 0,
                alloc_exclusive: 0,
                io_inclusive: 0,
                io_exclusive: 0,
                retained_inclusive: 0,
                retained_exclusive: 0,
                wait_inclusive: 0,
                wait_exclusive: 0,
                causes,
            }
        })
        .collect();
    // Edges: adjacent PHP frames within each stack, deduped per stack so a
    // mutually-recursive chain cannot push one edge past the stack's own weight.
    let mut edge_weights: BTreeMap<(usize, usize), u64> = BTreeMap::new();
    for (stack, weight) in display {
        let php: Vec<usize> = stack
            .iter()
            .filter(|(_, k)| matches!(k, Kind::Php | Kind::PhpInlined))
            .filter_map(|(n, _)| index.get(n).copied())
            .collect();
        let mut seen_pairs = HashSet::new();
        for pair in php.windows(2) {
            let (from, to) = (pair[0], pair[1]);
            if from == to {
                continue; // self-recursion loops back on one node; the layered view drops it
            }
            if seen_pairs.insert((from, to)) {
                *edge_weights.entry((from, to)).or_default() += weight;
            }
        }
    }
    let edges = edge_weights
        .into_iter()
        .map(|((from, to), weight)| GraphEdge { from, to, weight, count: None })
        .collect();
    CallGraph {
        nodes,
        edges,
        total: stats.grand,
        queries: Vec::new(),
        lines: None,
        trace: None,
    }
}

/// Writes the DOT and/or HTML call-graph exports when their flags are set.
fn write_graph_exports(
    cmd: &MonitorCommand,
    display: &[(Vec<(String, Kind)>, u64)],
    title: &str,
    lines: Option<&LineProfile>,
) -> Result<(), String> {
    if cmd.dot_out.is_none() && cmd.html_out.is_none() {
        return Ok(());
    }
    let mut graph = build_call_graph(display);
    if let Some(lines) = lines {
        graph.lines = Some(crate::call_graph::SourceLines {
            file: lines.file.clone(),
            source: lines.source.clone(),
            hits: lines
                .hits
                .iter()
                .map(|(line, samples)| (*line, *samples))
                .collect(),
            total: lines.total,
            funcs: Vec::new(),
        });
    }
    if let Some(path) = &cmd.dot_out {
        std::fs::write(path, crate::call_graph::render_dot(&graph))
            .map_err(|e| format!("cannot write {path}: {e}"))?;
        println!("wrote {path}");
    }
    if let Some(path) = &cmd.html_out {
        std::fs::write(path, crate::call_graph::render_html(&graph, title))
            .map_err(|e| format!("cannot write {path}: {e}"))?;
        println!("wrote {path}");
    }
    Ok(())
}

/// A function's share of a capture, keyed by its inlining-agnostic name: a
/// function drifting in or out of the inliner must not read as new or gone.
fn function_shares(display: &[(Vec<(String, Kind)>, u64)]) -> (BTreeMap<String, f64>, u64) {
    let mut totals: BTreeMap<String, u64> = BTreeMap::new();
    let mut grand = 0u64;
    for (stack, weight) in display {
        grand += weight;
        let mut seen = HashSet::new();
        for (name, kind) in stack {
            if !matches!(kind, Kind::Php | Kind::PhpInlined) {
                continue;
            }
            let normalized = name.trim_end_matches(" (inlined)").to_string();
            if seen.insert(normalized.clone()) {
                *totals.entry(normalized).or_default() += weight;
            }
        }
    }
    let shares = totals
        .into_iter()
        .map(|(name, weight)| (name, 100.0 * weight as f64 / grand.max(1) as f64))
        .collect();
    (shares, grand)
}

/// Reads per-function shares back out of a previous monitor Speedscope file
/// (its first profile is the helpers-folded PHP view).
fn baseline_shares(path: &str) -> Result<(BTreeMap<String, f64>, u64), String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read baseline {path}: {error}"))?;
    let doc: serde_json::Value =
        serde_json::from_str(&raw).map_err(|error| format!("invalid baseline {path}: {error}"))?;
    let frames = doc["shared"]["frames"]
        .as_array()
        .ok_or_else(|| format!("baseline {path} has no frame table"))?;
    let names: Vec<String> = frames
        .iter()
        .map(|frame| frame["name"].as_str().unwrap_or("").to_string())
        .collect();
    let profile = doc["profiles"]
        .as_array()
        .and_then(|profiles| profiles.first())
        .ok_or_else(|| format!("baseline {path} has no profiles"))?;
    let samples = profile["samples"]
        .as_array()
        .ok_or_else(|| format!("baseline {path} has no samples"))?;
    let weights = profile["weights"]
        .as_array()
        .ok_or_else(|| format!("baseline {path} has no weights"))?;
    let mut totals: BTreeMap<String, u64> = BTreeMap::new();
    let mut grand = 0u64;
    for (stack, weight) in samples.iter().zip(weights) {
        let weight = weight.as_u64().unwrap_or(0);
        grand += weight;
        let mut seen = HashSet::new();
        for index in stack.as_array().into_iter().flatten() {
            let Some(name) = index.as_u64().and_then(|i| names.get(i as usize)) else {
                continue;
            };
            if name == "<non-PHP>" {
                continue;
            }
            let normalized = name.trim_end_matches(" (inlined)").to_string();
            if seen.insert(normalized.clone()) {
                *totals.entry(normalized).or_default() += weight;
            }
        }
    }
    let shares = totals
        .into_iter()
        .map(|(name, weight)| (name, 100.0 * weight as f64 / grand.max(1) as f64))
        .collect();
    Ok((shares, grand))
}

/// Prints the per-function delta table against a baseline capture and returns
/// the process exit code: 2 when a regression exceeds the threshold, else 0.
fn diff_against_baseline(
    display: &[(Vec<(String, Kind)>, u64)],
    baseline_path: &str,
    fail_on_regression: Option<f64>,
) -> Result<i32, String> {
    let (current, current_samples) = function_shares(display);
    let (baseline, baseline_samples) = baseline_shares(baseline_path)?;
    let mut names: Vec<&String> = current.keys().chain(baseline.keys()).collect();
    names.sort();
    names.dedup();
    let mut rows: Vec<(String, Option<f64>, Option<f64>, f64)> = names
        .into_iter()
        .map(|name| {
            let now = current.get(name).copied();
            let was = baseline.get(name).copied();
            let delta = now.unwrap_or(0.0) - was.unwrap_or(0.0);
            (name.clone(), now, was, delta)
        })
        .filter(|(_, now, was, _)| now.unwrap_or(0.0) >= 0.1 || was.unwrap_or(0.0) >= 0.1)
        .collect();
    rows.sort_by(|a, b| b.3.abs().partial_cmp(&a.3.abs()).unwrap_or(std::cmp::Ordering::Equal));
    println!(
        "
--- vs baseline {baseline_path} ({baseline_samples} samples, this run {current_samples}) ---"
    );
    if current_samples < 500 || baseline_samples < 500 {
        println!("warning: fewer than 500 samples on one side; deltas are noisy");
    }
    let mut worst: Option<(String, f64)> = None;
    for (name, now, was, delta) in rows.iter().take(20) {
        let now_text = now.map_or("    —".to_string(), |v| format!("{v:5.1}%"));
        let was_text = was.map_or("    —".to_string(), |v| format!("{v:5.1}%"));
        let arrow = if *delta > 0.5 {
            "▲"
        } else if *delta < -0.5 {
            "▼"
        } else {
            " "
        };
        println!("{name:<26} {now_text}   was {was_text}   {delta:+5.1} {arrow}");
        if *delta > worst.as_ref().map_or(0.0, |(_, d)| *d) {
            worst = Some((name.clone(), *delta));
        }
    }
    if let (Some(threshold), Some((name, delta))) = (fail_on_regression, worst) {
        if delta > threshold {
            eprintln!(
                "elephc monitor: regression — {name} grew {delta:+.1} points (threshold {threshold})"
            );
            return Ok(2);
        }
    }
    Ok(0)
}

/// The live loop: sample a window, merge the process tree, redraw, repeat
/// until the target goes away. Prints the cumulative table on exit.
fn run_live(cmd: &MonitorCommand, root: u32, mut child: Option<&mut process::Child>) -> i32 {
    use std::io::IsTerminal;
    let interactive = std::io::stdout().is_terminal();
    let started = std::time::Instant::now();
    let mut cumulative: BTreeMap<Vec<(String, Kind)>, u64> = BTreeMap::new();
    let mut previous: HashMap<String, f64> = HashMap::new();
    let mut windows = 0u32;
    let graph_title = if cmd.target.is_empty() {
        "elephc profile".to_string()
    } else {
        cmd.target.trim_end_matches(".php").to_string()
    };
    // Rolling window of the last 10 per-window call graphs for the live HTML.
    let mut html_ring: std::collections::VecDeque<(u128, crate::call_graph::CallGraph)> =
        std::collections::VecDeque::new();
    if let (Some(addr), Some(path)) = (&cmd.serve, &cmd.html_out) {
        match serve_live_file(addr, path.clone()) {
            Ok(local) => eprintln!(
                "elephc monitor: serving live call graph at http://{local}/ (updates in place)"
            ),
            Err(error) => eprintln!("elephc monitor: cannot serve on {addr}: {error}"),
        }
    }
    loop {
        if let Some(child) = child.as_deref_mut() {
            if child.try_wait().ok().flatten().is_some() {
                break;
            }
        }
        let pids = discover_pids(root);
        let reports = capture_window(&pids, cmd.duration_secs);
        let Some(samples) = samples_from_reports(&reports, None, None) else {
            // Attach mode has no child handle: a window with zero reports is
            // how we learn the target is gone.
            break;
        };
        windows += 1;
        let display = render_stacks(&samples);
        for (stack, weight) in &display {
            *cumulative.entry(stack.clone()).or_default() += weight;
        }
        if cmd.html_out.is_some() || cmd.dot_out.is_some() {
            write_live_graphs(cmd, &display, &graph_title, &mut html_ring);
        }
        let frame = live_frame(
            &display,
            &cumulative,
            &mut previous,
            pids.len(),
            cmd.duration_secs,
            started.elapsed(),
        );
        if interactive {
            // Clear and home, like top: the frame replaces the previous one.
            print!("\u{1b}[2J\u{1b}[H{frame}");
            let _ = std::io::stdout().flush();
        } else {
            println!("--- window {windows} ---");
            print!("{frame}");
        }
    }
    if windows > 0 {
        let merged: Vec<(Vec<(String, Kind)>, u64)> = cumulative.into_iter().collect();
        println!("\n=== cumulative ({windows} windows) ===");
        print!("{}", why_table(&merged, 1));
    }
    0
}

/// Live-mode graph export: append this window's call graph to a rolling ring of
/// the last 10 and rewrite the self-refreshing HTML (and, if asked, the latest
/// DOT). Writes are atomic so the auto-reloading page never reads a half file.
fn write_live_graphs(
    cmd: &MonitorCommand,
    display: &[(Vec<(String, Kind)>, u64)],
    title: &str,
    ring: &mut std::collections::VecDeque<(u128, crate::call_graph::CallGraph)>,
) {
    let graph = build_call_graph(display);
    if let Some(path) = &cmd.dot_out {
        let _ = write_atomic(path, &crate::call_graph::render_dot(&graph));
    }
    if let Some(path) = &cmd.html_out {
        ring.push_back((epoch_millis(), graph));
        while ring.len() > 10 {
            ring.pop_front();
        }
        let frames: Vec<(u128, &crate::call_graph::CallGraph)> =
            ring.iter().map(|(ts, g)| (*ts, g)).collect();
        let html = crate::call_graph::render_html_frames(&frames, title, true, cmd.duration_secs, false, &[]);
        let _ = write_atomic(path, &html);
    }
}

/// Writes `contents` to `path` atomically (temp file + rename), so a concurrent
/// reader (the live page reloading) sees either the old or the new file whole.
fn write_atomic(path: &str, contents: &str) -> std::io::Result<()> {
    let tmp = format!("{path}.tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)
}

/// Milliseconds since the Unix epoch, for frame timestamps.
fn epoch_millis() -> u128 {
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
fn serve_live_file(addr: &str, path: String) -> std::io::Result<std::net::SocketAddr> {
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

/// Answers one HTTP request with the current bytes of `path`. Ignores the
/// request target: this server has exactly one resource.
fn serve_one_request(mut stream: std::net::TcpStream, path: &str) -> std::io::Result<()> {
    use std::io::{BufRead, BufReader, Write};
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
    {
        // Consume the request line and headers so the client can send the body
        // and read the response cleanly.
        let mut reader = BufReader::new(&stream);
        let mut line = String::new();
        reader.read_line(&mut line)?;
        loop {
            let mut header = String::new();
            let n = reader.read_line(&mut header)?;
            if n == 0 || header == "\r\n" || header == "\n" {
                break;
            }
        }
    }
    let body = std::fs::read(path).unwrap_or_default();
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(&body)?;
    stream.flush()
}

/// Returns the target process and its direct children (a prefork server's
/// workers), root first. Best-effort: without `pgrep` only the root is sampled.
fn discover_pids(root: u32) -> Vec<u32> {
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

/// Samples every pid of one window in parallel and returns the reports that
/// succeeded — a worker dying mid-window degrades coverage, never the run.
fn capture_window(pids: &[u32], duration_secs: u32) -> Vec<String> {
    let mut jobs = Vec::new();
    for pid in pids {
        let report_path = std::env::temp_dir().join(format!(
            "elephc_monitor_{}_{}.txt",
            process::id(),
            pid
        ));
        let child = process::Command::new("/usr/bin/sample")
            .args([
                pid.to_string(),
                duration_secs.to_string(),
                "-file".to_string(),
                report_path.display().to_string(),
            ])
            .stdout(process::Stdio::null())
            // Kept, not discarded: when the sampler refuses, its own sentence is
            // the only thing that says why, and a caller that sees "no samples"
            // cannot reconstruct it.
            .stderr(process::Stdio::piped())
            .spawn();
        if let Ok(child) = child {
            jobs.push((child, report_path));
        }
    }
    let mut reports = Vec::new();
    let mut refusal = None;
    for (job, report_path) in jobs {
        let done = job.wait_with_output();
        let ok = done.as_ref().map(|o| o.status.success()).unwrap_or(false);
        if ok {
            if let Ok(text) = std::fs::read_to_string(&report_path) {
                reports.push(text);
            }
        } else if refusal.is_none() {
            refusal = done
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stderr).trim().to_string())
                .filter(|s| !s.is_empty());
        }
        let _ = std::fs::remove_file(&report_path);
    }
    // Say it once, not once per window: a --live loop would otherwise bury the
    // table under the same line every few seconds.
    if reports.is_empty() {
        if let Some(message) = refusal {
            static SAID: std::sync::atomic::AtomicBool =
                std::sync::atomic::AtomicBool::new(false);
            if !SAID.swap(true, std::sync::atomic::Ordering::Relaxed) {
                let first = message.lines().next().unwrap_or(&message);
                eprintln!("elephc monitor: the sampler refused: {first}");
            }
        }
    }
    reports
}

/// Parses every report into merged samples, recovering inlined frames when the
/// binary's dSYM and the PHP source are available (one-shot spawn mode only).
fn samples_from_reports(
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

/// Makes a compiled program's path safe to hand to `Command::new`.
///
/// `Command::new("shop")` does not run `./shop`: with no separator in the name the
/// OS searches `PATH`, so spawning fails with `No such file or directory` even
/// though the binary is right there. It appears to work on a machine whose `PATH`
/// carries an empty entry — POSIX reads that as the current directory — which is
/// exactly the kind of accident that hides the bug during development and surfaces
/// it for everyone else. Absolute wins: unambiguous, and it survives any later
/// change of working directory.
/// A name that resolves to no local file is left alone, so `monitor some-tool` can
/// still mean a program on `PATH`.
fn spawnable_path(path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() || !path.exists() {
        return path;
    }
    match std::env::current_dir() {
        Ok(cwd) => cwd.join(path),
        // Without a cwd there is nothing better than an explicit relative path,
        // which at least defeats the PATH search.
        Err(_) => PathBuf::from(".").join(path),
    }
}

/// Compiles a `.php` target with `--debug-info` by re-executing this binary, and
/// returns the produced executable's path (next to the source, like a normal compile).
fn compile_php_target(source: &str) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot locate elephc: {e}"))?;
    let status = process::Command::new(exe)
        .args(["--debug-info", source])
        .status()
        .map_err(|e| format!("cannot run elephc: {e}"))?;
    if !status.success() {
        return Err(format!("compiling {source} failed"));
    }
    Ok(spawnable_path(source.trim_end_matches(".php")))
}

/// Compiles a `.php` target with the monitoring capability embedded.
///
/// One function for what used to be two, because the two mechanisms are no
/// longer two commands: whichever of them ends up reading the program, the build
/// that produces it is the same build.
///
/// Deliberately *without* `--debug-info`: the embedded sampler resolves frames
/// through the symbol table the capability carries, not through DWARF, so debug
/// info buys this path nothing and only makes the compile slower. (It also used
/// to break it outright on ELF, until the inline-thunk section restore in
/// `runtime_wrappers.rs` fixed the underlying layout bug.)
fn compile_php_monitored(source: &str) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot locate elephc: {e}"))?;
    let status = process::Command::new(exe)
        .args(["--with-monitoring", source])
        .status()
        .map_err(|e| format!("cannot run elephc: {e}"))?;
    if !status.success() {
        return Err(format!("compiling {source} with --with-monitoring failed"));
    }
    Ok(spawnable_path(source.trim_end_matches(".php")))
}

/// Explains an empty capture by what the run actually did.
///
/// One message covered every cause: "was the target built with
/// --with-monitoring". For a program that crashed after printing its output —
/// which is what a CI shard showed, on one architecture only — that is a
/// confident diagnosis of the wrong thing, and it sent the investigation at the
/// build flags for an hour. The exit status was there the whole time and nobody
/// looked at it.
fn no_profile_reason(status: &process::ExitStatus, binary: &Path) -> String {
    use std::os::unix::process::ExitStatusExt as _;
    if let Some(signal) = status.signal() {
        return format!(
            "{} was killed by signal {signal} before it could write a profile. \
             The capture is lost because the run is: the profile is written at exit.",
            binary.display()
        );
    }
    match status.code() {
        Some(0) | None => format!(
            "{} ran and exited cleanly but wrote no profile, so the capability \
             never switched on. It carries the monitoring marker, so the build is \
             not the problem — the control channel is.",
            binary.display()
        ),
        Some(code) => format!(
            "{} exited with status {code} and wrote no profile. Whatever went \
             wrong there went wrong before the profile was written.",
            binary.display()
        ),
    }
}

/// Reads a target that carries the monitoring capability exactly: run it, and
/// render the profile it prints to stderr — the deterministic counterpart to
/// sampling. Honors `--dot` / `--html`.
fn run_instrument(cmd: &MonitorCommand) -> i32 {
    let binary = if cmd.target.ends_with(".php") {
        match compile_php_monitored(&cmd.target) {
            Ok(path) => path,
            Err(error) => {
                eprintln!("elephc monitor: {error}");
                return 1;
            }
        }
    } else {
        let path = spawnable_path(&cmd.target);
        // Nothing to switch on if the hooks were never compiled in.
        if !carries_monitoring(&path) {
            eprintln!(
                "elephc monitor: {} carries no monitoring — rebuild it with \
                 --with-monitoring, or point monitor at the .php source",
                path.display()
            );
            return 1;
        }
        path
    };
    // Inherit stdout (the program's own output shows live); capture stderr, where
    // the instrument profile is written at exit.
    let mut command = process::Command::new(&binary);
    command.stderr(process::Stdio::piped());
    // The binary carries the hooks but boots dormant, so being asked is what
    // separates "capable" from "profiling". Asking happens over a socketpair only
    // this process holds the other end of, rather than an environment variable
    // every process on the machine can read.
    let channel = open_control_channel();
    if let Some(channel) = &channel {
        attach_control_channel(&mut command, channel);
    }
    if let Some(trace_path) = &cmd.trace {
        // The runtime writes the Chrome/Perfetto trace to this path at exit.
        command.env("ELEPHC_INSTR_TRACE", trace_path);
    }
    let output = match command
        .spawn()
        .and_then(|child| child.wait_with_output())
    {
        Ok(output) => output,
        Err(error) => {
            eprintln!("elephc monitor: cannot run {}: {error}", binary.display());
            return 1;
        }
    };
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Pass through the program's own diagnostics — and only those. A
    // `--with-monitoring` binary carries both mechanisms, so its stderr also
    // holds the sampler's folded stacks; forwarding those would print raw
    // profiler output as if the program had written it.
    for line in stderr.lines() {
        if !line.starts_with("elephc-instr") && !line.starts_with("elephc-probe") {
            eprintln!("{line}");
        }
    }
    // The runtime writes its warnings to the same stderr the parser consumes,
    // so anything the parser does not recognise would vanish. A truncated
    // profile that says nothing is worse than no profile: pass them through.
    for line in stderr.lines() {
        if let Some(note) = line.strip_prefix("elephc-instr: note: ") {
            eprintln!("elephc monitor: {note}");
        }
    }
    let mut graph = parse_instrument_dump(&stderr);
    if graph.nodes.is_empty() {
        eprintln!("elephc monitor: {}", no_profile_reason(&output.status, &binary));
        return 1;
    }
    print!("{}", instrument_table(&graph));
    let title = cmd.target.trim_end_matches(".php").to_string();
    // The exact capture carries no per-line data, but it can still show the
    // file: every measured function, located, with its cost.
    attach_exact_source(&mut graph, &cmd.target);
    // Assertions come from the project budget file and from --assert, in that
    // order: the file states the standing contract, the flag adds a one-off.
    // Evaluated here, before any export, so the page can carry the verdicts.
    let mut asserts: Vec<(String, Option<String>)> = Vec::new();
    match load_assert_file(cmd.assert_file.as_deref(), &cmd.target) {
        Ok(Some((from_file, path))) => {
            if !from_file.is_empty() {
                println!("assertions: {} from {path}", from_file.len());
            }
            asserts.extend(from_file);
        }
        Ok(None) => {}
        Err(error) => {
            eprintln!("elephc monitor: {error}");
            return 1;
        }
    }
    asserts.extend(cmd.asserts.iter().map(|spec| (spec.clone(), None)));
    let assert_outcomes = evaluate_asserts(&graph, &asserts);
    // The same exports the sampled path offers, from measured time rather than
    // from samples: a Speedscope document and a pprof profile.
    {
        let stacks = exact_stacks(&graph);
        // Same default as the sampled path, deliberately: "the same exports
        // whichever target produced them" is only true if the default is the
        // same one too.
        let out_path = cmd
            .out
            .clone()
            .unwrap_or_else(|| format!("{}.speedscope.json", cmd.target.trim_end_matches(".php")));
        if let Err(error) = write_speedscope(&stacks, &out_path) {
            eprintln!("elephc monitor: {error}");
            return 1;
        }
        println!("wrote {out_path}");
        if let Some(pprof_path) = &cmd.pprof_out {
            let folded = php_folded_stacks(&stacks);
            let encoded = crate::pprof_encode::encode_folded_profile(&folded);
            match std::fs::write(pprof_path, encoded) {
                Ok(()) => println!("wrote {pprof_path}"),
                Err(error) => {
                    eprintln!("elephc monitor: cannot write {pprof_path}: {error}");
                    return 1;
                }
            }
        }
    }
    // Save this capture for use as a later --baseline.
    if let Some(path) = &cmd.save {
        match serde_json::to_string(&graph) {
            Ok(json) => match std::fs::write(path, json) {
                Ok(()) => println!("saved exact capture to {path}"),
                Err(error) => {
                    eprintln!("elephc monitor: cannot save {path}: {error}");
                    return 1;
                }
            },
            Err(error) => {
                eprintln!("elephc monitor: cannot serialize capture: {error}");
                return 1;
            }
        }
    }
    // Load a prior exact capture to diff against.
    //
    // An unreadable baseline is an ERROR, not a warning. This warned and carried
    // on, so `--baseline` on a file it could not parse still exited 0 — and with
    // `--fail-on-regression` that is a CI gate reporting success for a comparison
    // it never made, which is the one thing a gate must never do.
    let mut baseline = None;
    if let Some(path) = &cmd.baseline {
        match load_exact_graph(path) {
            Some(graph) => baseline = Some(graph),
            None => {
                eprintln!("elephc monitor: could not read exact baseline {path}");
                // The likeliest cause by far, and the one the docs used to
                // suggest: a Speedscope export is a different document with
                // different data, not an exact capture.
                if looks_like_speedscope(path) {
                    eprintln!(
                        "  {path} is a Speedscope export, which carries no per-function \
                         measurements to compare against.\n  \
                         Produce the baseline with --save instead:  elephc monitor \
                         <target> --save baseline.json"
                    );
                }
                return 1;
            }
        }
    }
    if let Some(path) = &cmd.dot_out {
        if let Err(error) = std::fs::write(path, crate::call_graph::render_dot(&graph)) {
            eprintln!("elephc monitor: cannot write {path}: {error}");
            return 1;
        }
        println!("wrote {path}");
    }
    if let Some(path) = &cmd.html_out {
        // With a baseline, render two exact frames [baseline, current] so the
        // navigator scrubs between them and the diff mode highlights growth.
        let html = match &baseline {
            Some(base) => crate::call_graph::render_html_frames(
                &[(0, base), (1, &graph)],
                &title,
                false,
                0,
                true,
                &assert_outcomes,
            ),
            None => crate::call_graph::render_html_exact(&graph, &title, &assert_outcomes),
        };
        if let Err(error) = std::fs::write(path, html) {
            eprintln!("elephc monitor: cannot write {path}: {error}");
            return 1;
        }
        println!("wrote {path}");
    }
    if let Some(path) = &cmd.trace {
        if std::path::Path::new(path).exists() {
            println!("wrote {path} — open in https://ui.perfetto.dev or chrome://tracing");
        }
    }
    if let Some(base) = &baseline {
        print!("{}", instrument_delta_table(base, &graph));
    }
    print!("{}", instrument_recommendations(&graph));
    if !assert_outcomes.is_empty() {
        let (report, ok) = assert_report(&assert_outcomes);
        print!("{report}");
        if !ok {
            return 2;
        }
    }
    0
}

/// Surfaces a few Blackfire-style hints from the exact profile: the time
/// hotspot, the allocation hotspot, and functions whose per-call cost suggests
/// call overhead. Silent when nothing crosses a threshold.
fn instrument_recommendations(graph: &crate::call_graph::CallGraph) -> String {
    let root_ns = graph.nodes.iter().map(|n| n.inclusive).max().unwrap_or(1).max(1);
    let total_allocs: u64 = graph.nodes.iter().map(|n| n.alloc_exclusive).sum();
    let mut hints: Vec<String> = Vec::new();
    if let Some(hot) = graph.nodes.iter().max_by_key(|n| n.exclusive) {
        let pct = 100.0 * hot.exclusive as f64 / root_ns as f64;
        if pct >= 20.0 {
            hints.push(format!(
                "• {} is the hotspot — {:.0}% self time ({})",
                hot.name,
                pct,
                fmt_ns(hot.exclusive)
            ));
        }
    }
    if total_allocs > 0 {
        if let Some(am) = graph.nodes.iter().max_by_key(|n| n.alloc_exclusive) {
            let pct = 100.0 * am.alloc_exclusive as f64 / total_allocs as f64;
            if am.alloc_exclusive > 10_000 && pct >= 25.0 {
                hints.push(format!(
                    "• {} allocates the most — {} allocations ({:.0}% of total); cutting allocation here pays off",
                    am.name, am.alloc_exclusive, pct
                ));
            }
        }
    }
    for n in &graph.nodes {
        if let Some(calls) = n.call_count {
            if calls >= 100_000 && n.exclusive > 0 {
                let per = n.exclusive / calls;
                if per < 100 {
                    hints.push(format!(
                        "• {} is called {} times at ~{}ns each — call overhead may dominate; consider hoisting the work or inlining",
                        n.name, calls, per
                    ));
                }
            }
        }
    }
    // Retention hotspot: which function leaves the most objects on the heap.
    // Reported against what it allocated, since retaining most of a large
    // allocation is the signal — a function that allocates and frees is fine.
    if let Some(r) = graph.nodes.iter().max_by_key(|n| n.retained_exclusive) {
        if r.retained_exclusive > 0 && r.alloc_exclusive > 0 {
            let kept = 100.0 * r.retained_exclusive as f64 / r.alloc_exclusive as f64;
            if kept >= 50.0 {
                hints.push(format!(
                    "• {} retains the most — {} of its {} allocations ({:.0}%) are still live when it returns; check for a cache or collection that only grows",
                    r.name, r.retained_exclusive, r.alloc_exclusive, kept
                ));
            }
        }
    }
    // I/O-bound verdict: when most of the run is spent blocked in the driver,
    // optimizing PHP-side work is the wrong lever — say so.
    let root_ns = graph.nodes.iter().map(|n| n.inclusive).max().unwrap_or(0);
    let total_wait: u64 = graph.nodes.iter().map(|n| n.wait_exclusive).sum();
    if root_ns > 0 && total_wait > 0 {
        let pct = 100.0 * total_wait as f64 / root_ns as f64;
        if pct >= 25.0 {
            let worst = graph
                .nodes
                .iter()
                .max_by_key(|n| n.wait_exclusive)
                .filter(|n| n.wait_exclusive > 0);
            let who = worst.map_or(String::new(), |n| {
                format!(" — {} blocks longest ({})", n.name, fmt_ns(n.wait_exclusive))
            });
            hints.push(format!(
                "• the run is I/O-bound: {:.0}% of it ({}) is spent waiting on the database{who}; batching or caching queries will beat any PHP-side tuning",
                pct,
                fmt_ns(total_wait)
            ));
        }
    }
    // Query hotspot: which function issues the most DB queries.
    let total_io: u64 = graph.nodes.iter().map(|n| n.io_exclusive).sum();
    if total_io > 0 {
        if let Some(q) = graph.nodes.iter().max_by_key(|n| n.io_exclusive) {
            if q.io_exclusive > 0 {
                hints.push(format!(
                    "• {} issues the most DB queries — {} of {} total",
                    q.name, q.io_exclusive, total_io
                ));
            }
        }
    }
    // High fan-out edges: a callee invoked many times from one caller. With exact
    // query counts we can tell a definite N+1 from a mere hot helper.
    let io_by_name: std::collections::HashMap<&str, u64> = graph
        .nodes
        .iter()
        .map(|n| (n.name.as_str(), n.io_exclusive))
        .collect();
    let mut fanout: Vec<(&str, &str, u64)> = graph
        .edges
        .iter()
        .filter_map(|e| {
            e.count.filter(|&c| c >= 100).map(|c| {
                (
                    graph.nodes[e.from].name.as_str(),
                    graph.nodes[e.to].name.as_str(),
                    c,
                )
            })
        })
        .collect();
    fanout.sort_by(|a, b| b.2.cmp(&a.2));
    for (from, to, count) in fanout.into_iter().take(3) {
        let callee_io = io_by_name.get(to).copied().unwrap_or(0);
        if callee_io > 0 {
            hints.push(format!(
                "• N+1: {from} calls {to} {count} times and {to} issues {callee_io} DB queries — batch them into one query",
            ));
        } else {
            hints.push(format!(
                "• {from} calls {to} {count} times — if {to} touches the DB, network, or filesystem, that is an N+1; batch or cache it",
            ));
        }
    }
    if hints.is_empty() {
        return String::new();
    }
    let mut out = String::from("\nrecommendations:\n");
    for hint in hints {
        out.push_str(&hint);
        out.push('\n');
    }
    out
}

/// Evaluates `<metric>:<fn><op><value>` assertions against the exact profile.
/// Metrics: `calls`, `allocs` (self), `self_ms` (self time), `time_pct`
/// (inclusive share). Returns the report and whether every assertion passed.
fn evaluate_asserts(
    graph: &crate::call_graph::CallGraph,
    asserts: &[(String, Option<String>)],
) -> Vec<crate::call_graph::AssertOutcome> {
    use crate::call_graph::{AssertOutcome, AssertStatus};
    let root_ns = graph.nodes.iter().map(|n| n.inclusive).max().unwrap_or(1).max(1);
    let mut by_name: HashMap<&str, &crate::call_graph::GraphNode> = HashMap::new();
    for node in &graph.nodes {
        by_name.insert(node.name.as_str(), node);
    }
    let mut outcomes = Vec::new();
    for (spec, label) in asserts {
        let Some((metric, target, op, budget)) = parse_assert(spec) else {
            outcomes.push(AssertOutcome {
                spec: spec.clone(),
                label: label.clone(),
                metric: String::new(),
                target: String::new(),
                op: String::new(),
                budget: 0.0,
                actual: None,
                status: AssertStatus::Error,
                note: Some("cannot parse — expected <metric>:<function><op><value>".to_string()),
            });
            continue;
        };
        // `*` asserts on the whole run rather than one function.
        let actual = if target == "*" {
            assert_run_total(&metric, graph, root_ns)
        } else {
            by_name
                .get(target.as_str())
                .and_then(|node| assert_metric_value(&metric, node, root_ns))
        };
        let mut note = None;
        let status = match actual {
            Some(actual) => {
                let ok = match op.as_str() {
                    "<=" => actual <= budget,
                    ">=" => actual >= budget,
                    "==" => (actual - budget).abs() < 1e-9,
                    "<" => actual < budget,
                    ">" => actual > budget,
                    _ => false,
                };
                if ok { AssertStatus::Pass } else { AssertStatus::Fail }
            }
            None => {
                // Separate the two reasons: an unknown metric is a typo in the
                // budget, a missing function is a fact about the run.
                note = Some(if assert_metric_value(&metric, graph.nodes.first().unwrap_or(&EMPTY_NODE), root_ns).is_none()
                    && assert_run_total(&metric, graph, root_ns).is_none()
                {
                    format!(
                        "unknown metric '{metric}' — known: {}",
                        ASSERT_METRICS.iter().map(|(m, _)| *m).collect::<Vec<_>>().join(", ")
                    )
                } else {
                    format!("'{target}' never ran")
                });
                AssertStatus::Error
            }
        };
        outcomes.push(AssertOutcome {
            spec: spec.clone(),
            label: label.clone(),
            metric,
            target,
            op,
            budget,
            actual,
            status,
            note,
        });
    }
    outcomes
}

/// A node with no measurements, used only to ask whether a metric name exists.
static EMPTY_NODE: std::sync::LazyLock<crate::call_graph::GraphNode> =
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
        causes: Vec::new(),
    });

/// Renders evaluated assertions as the stdout report, and whether all held.
///
/// Failures come first: a gate's output is read when it is red, and the reason
/// it went red should not be somewhere below twenty passing lines.
fn assert_report(outcomes: &[crate::call_graph::AssertOutcome]) -> (String, bool) {
    use crate::call_graph::AssertStatus;
    let passed = outcomes.iter().filter(|o| o.status == AssertStatus::Pass).count();
    let failed = outcomes.iter().filter(|o| o.status == AssertStatus::Fail).count();
    let errored = outcomes.iter().filter(|o| o.status == AssertStatus::Error).count();
    let mut out = format!(
        "\nassertions — {passed} passed, {failed} failed{}\n",
        if errored > 0 { format!(", {errored} not evaluated") } else { String::new() }
    );
    let mut ordered: Vec<&crate::call_graph::AssertOutcome> = outcomes.iter().collect();
    ordered.sort_by_key(|o| match o.status {
        AssertStatus::Fail => 0,
        AssertStatus::Error => 1,
        AssertStatus::Pass => 2,
    });
    for outcome in ordered {
        let tag = match outcome.status {
            AssertStatus::Pass => "PASS",
            AssertStatus::Fail => "FAIL",
            AssertStatus::Error => "SKIP",
        };
        let measured = match outcome.actual {
            Some(actual) => format!("actual {}", trim_number(actual)),
            None => outcome.note.clone().unwrap_or_default(),
        };
        let label = outcome
            .label
            .as_ref()
            .map(|l| format!("  — {l}"))
            .unwrap_or_default();
        out.push_str(&format!("  [{tag}] {} ({measured}){label}\n", outcome.spec));
    }
    (out, failed == 0 && errored == 0)
}

/// Formats a measured value without trailing zeros, so a count reads as `250`
/// rather than `250.000` while a millisecond figure keeps its precision.
fn trim_number(value: f64) -> String {
    if (value - value.round()).abs() < 1e-9 {
        format!("{}", value.round() as i64)
    } else {
        format!("{value:.3}")
    }
}

/// Parses `<metric>:<fn><op><value>` into its parts. `op` is one of `<= >= == < >`.
fn parse_assert(spec: &str) -> Option<(String, String, String, f64)> {
    let (metric, rest) = spec.split_once(':')?;
    for op in ["<=", ">=", "==", "<", ">"] {
        if let Some(pos) = rest.find(op) {
            let fname = rest[..pos].trim();
            let value = rest[pos + op.len()..].trim();
            let value = value.parse::<f64>().ok()?;
            if fname.is_empty() {
                return None;
            }
            return Some((metric.trim().to_string(), fname.to_string(), op.to_string(), value));
        }
    }
    None
}

/// The project file elephc reads, currently holding the performance budget.
/// A dotfile at the project root, found by walking up from the source, the way
/// `.editorconfig` and `.gitignore` are — so where you run the profiler from
/// does not change which budget applies.
pub(crate) const PROJECT_FILE_NAME: &str = ".elephc";

/// Parses a budget file into `(spec, label)` pairs.
///
/// One assertion per line, in the same syntax `--assert` takes, so there is a
/// single thing to learn. `#` starts a comment: on its own line it is ignored,
/// after an assertion it becomes that assertion's label, which is what makes a
/// red gate say *why* the budget exists rather than only which number moved.
fn parse_assert_file(text: &str) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (spec, label) = match line.split_once('#') {
            Some((spec, label)) => (spec.trim(), Some(label.trim().to_string())),
            None => (line, None),
        };
        if !spec.is_empty() {
            out.push((spec.to_string(), label.filter(|l| !l.is_empty())));
        }
    }
    out
}

/// Finds the nearest `.elephc`, starting beside the profiled source and walking
/// up to the filesystem root.
///
/// Walking up is what lets one budget cover a whole project: the file sits at
/// the root next to `composer.json`, and profiling `src/deep/thing.php` still
/// finds it. The search is anchored on the SOURCE, not the working directory,
/// so running the profiler from elsewhere cannot silently change which budget
/// gates the build.
fn find_project_file(target: &str) -> Option<PathBuf> {
    let start = Path::new(target).parent().map(Path::to_path_buf).or_else(|| {
        std::env::current_dir().ok()
    })?;
    let mut dir = if start.as_os_str().is_empty() {
        std::env::current_dir().ok()?
    } else {
        start.canonicalize().unwrap_or(start)
    };
    loop {
        let candidate = dir.join(PROJECT_FILE_NAME);
        if candidate.is_file() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Loads the budget file to use: the explicit `--assert-file`, else the nearest
/// `.elephc` above the profiled source. Returns the assertions and the path
/// they came from, so the report can say which file is gating the build.
fn load_assert_file(
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
    ("time_pct", "inclusive time as a percentage of the run"),
];

/// The value of one assertable metric for a node.
fn assert_metric_value(
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
        "time_pct" => Some(100.0 * node.inclusive as f64 / root_ns as f64),
        _ => None,
    }
}

/// The same metric for the whole run, which is what `*` asserts on.
///
/// Self values sum across functions (that is what makes them a partition), so a
/// run total is their sum; the time metrics come from the root instead, since
/// summing inclusive times would count every caller again.
fn assert_run_total(metric: &str, graph: &crate::call_graph::CallGraph, root_ns: u64) -> Option<f64> {
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
        "time_pct" => Some(100.0),
        _ => None,
    }
}

/// Reads one `key=<u64>` field out of a metrics fragment.
fn instr_field(fragment: &str, key: &str) -> u64 {
    fragment
        .split(key)
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0)
}

/// Same, for a field that can be negative — retained objects (allocated minus
/// freed) go below zero for a function that releases more than it takes.
fn instr_field_i64(fragment: &str, key: &str) -> i64 {
    fragment
        .split(key)
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0)
}

/// Parses `--instrument` stderr lines into an exact call graph: `inclusive`/
/// `exclusive` carry nanoseconds, `call_count` the exact invocation count, and
/// edge weights the callee's inclusive ns under that caller.
fn parse_instrument_dump(text: &str) -> crate::call_graph::CallGraph {
    use crate::call_graph::{CallGraph, GraphEdge, GraphNode};
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut raw_edges: Vec<(String, String, u64, u64)> = Vec::new();
    let mut queries: Vec<(String, u64)> = Vec::new();
    let mut trace: Option<crate::call_graph::TraceContext> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("elephc-instr-trace: ") {
            // "trace=<id> span=<id> parent=<id|-> start=<unix micros>"
            // `start` is absent in captures taken before slices were timestamped.
            let field = |key: &str| -> String {
                rest.split_whitespace()
                    .find_map(|kv| kv.strip_prefix(key))
                    .unwrap_or("")
                    .to_string()
            };
            let parent = field("parent=");
            trace = Some(crate::call_graph::TraceContext {
                trace_id: field("trace="),
                span_id: field("span="),
                parent_span_id: if parent == "-" { String::new() } else { parent },
                start_us: field("start=").parse::<u64>().ok(),
                route: match field("route=").as_str() {
                    "" | "-" => String::new(),
                    encoded => decode_field(encoded),
                },
            });
        } else if let Some(rest) = line.strip_prefix("elephc-instr-query: ") {
            // "<count> <sql text on one line>"
            if let Some((count, sql)) = rest.split_once(' ') {
                if let Ok(count) = count.parse::<u64>() {
                    queries.push((sql.to_string(), count));
                }
            }
        } else if let Some(rest) = line.strip_prefix("elephc-instr-edge: ") {
            // "<caller> -> <callee> count=N ns=Y"
            let Some((endpoints, metrics)) = rest.split_once(" count=") else {
                continue;
            };
            let Some((caller, callee)) = endpoints.split_once(" -> ") else {
                continue;
            };
            let count = metrics
                .split_whitespace()
                .next()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);
            let ns = instr_field(metrics, "ns=");
            raw_edges.push((caller.to_string(), callee.to_string(), count, ns));
        } else if let Some(rest) = line.strip_prefix("elephc-instr: ") {
            // "<name> calls=N incl_ns=X excl_ns=Y" — the "note:" line has no calls= and is skipped.
            let Some((name, metrics)) = rest.split_once(" calls=") else {
                continue;
            };
            let calls = metrics
                .split_whitespace()
                .next()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(0);
            let inclusive = instr_field(metrics, "incl_ns=");
            let exclusive = instr_field(metrics, "excl_ns=");
            let alloc_inclusive = instr_field(metrics, "incl_allocs=");
            let alloc_exclusive = instr_field(metrics, "excl_allocs=");
            let io_inclusive = instr_field(metrics, "incl_io=");
            let io_exclusive = instr_field(metrics, "excl_io=");
            let retained_inclusive = instr_field_i64(metrics, "incl_ret=");
            let retained_exclusive = instr_field_i64(metrics, "excl_ret=");
            let wait_inclusive = instr_field(metrics, "incl_wait=");
            let wait_exclusive = instr_field(metrics, "excl_wait=");
            index.insert(name.to_string(), nodes.len());
            nodes.push(GraphNode {
                name: name.to_string(),
                inclusive,
                exclusive,
                call_count: Some(calls),
                alloc_inclusive,
                alloc_exclusive,
                io_inclusive,
                io_exclusive,
                retained_inclusive,
                retained_exclusive,
                wait_inclusive,
                wait_exclusive,
                causes: Vec::new(),
            });
        }
    }
    let total = nodes.iter().map(|n| n.inclusive).max().unwrap_or(1);
    let edges = raw_edges
        .into_iter()
        .filter_map(|(caller, callee, count, ns)| {
            Some(GraphEdge {
                from: *index.get(&caller)?,
                to: *index.get(&callee)?,
                weight: ns,
                count: Some(count),
            })
        })
        .collect();
    CallGraph {
        nodes,
        edges,
        total,
        queries,
        lines: None,
        trace,
    }
}

/// One profiled request slice, tagged with where it came from.
struct Slice {
    service: String,
    graph: crate::call_graph::CallGraph,
}

/// Splits a `--web --instrument` service log into its per-request dumps.
///
/// Every slice opens with its `elephc-instr-trace:` line (the runtime writes it
/// first), so that line is the record separator. Text before the first one is
/// not a slice — it is whatever else the service logged.
fn split_slices(text: &str) -> Vec<String> {
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
fn escape_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// Renders the per-service stats in the Prometheus text exposition format.
///
/// A file rather than an endpoint: `monitor` runs and exits, so there is nothing
/// for a scraper to poll. The textfile collector is the established way to get
/// numbers from a batch job into Prometheus, and it needs no HTTP server here.
///
/// Percentiles map onto `summary`, which is what that type is for. They are NOT
/// exposed as a histogram: a histogram carries buckets a backend re-aggregates,
/// and we have exact per-request values rather than bucket counts, so presenting
/// them as buckets would invent a resolution the capture does not have.
fn prometheus_text(slices: &[Slice]) -> String {
    let stats = service_stats(slices);
    let mut out = String::new();
    out.push_str(
        "# HELP elephc_requests_total Profiled requests observed in this capture.\n\
         # TYPE elephc_requests_total counter\n",
    );
    for s in &stats {
        out.push_str(&format!("elephc_requests_total{} {}\n", labels(s), s.requests));
    }
    out.push_str(
        "# HELP elephc_request_duration_seconds Exact per-request duration.\n\
         # TYPE elephc_request_duration_seconds summary\n",
    );
    for s in &stats {
        for (quantile, value) in [
            ("0.5", s.p50),
            ("0.9", s.p90),
            ("0.95", s.p95),
            ("0.99", s.p99),
        ] {
            let mut with_quantile = labels(s);
            with_quantile.truncate(with_quantile.len() - 1);
            out.push_str(&format!(
                "elephc_request_duration_seconds{with_quantile},quantile=\"{quantile}\"}} {:.6}\n",
                value as f64 / 1e9
            ));
        }
    }
    out.push_str(
        "# HELP elephc_queries_per_request Mean database queries per request.\n\
         # TYPE elephc_queries_per_request gauge\n",
    );
    for s in &stats {
        out.push_str(&format!(
            "elephc_queries_per_request{} {:.3}\n",
            labels(s),
            s.queries_per_request
        ));
    }
    out
}

/// `{service="…"}` or `{service="…",route="…"}`.
fn labels(s: &ServiceStats) -> String {
    match &s.route {
        Some(route) => format!(
            "{{service=\"{}\",route=\"{}\"}}",
            escape_label(&s.service),
            escape_label(route)
        ),
        None => format!("{{service=\"{}\"}}", escape_label(&s.service)),
    }
}

/// Exports correlated slices to an OTLP/HTTP collector as OpenTelemetry spans.
///
/// elephc already speaks the identity half of tracing — every slice carries the
/// W3C trace id, span id and parent it was told — so a service already belongs
/// to its caller's trace. What was missing is that the spans never reached a
/// backend, leaving an elephc hop as a gap in someone else's waterfall.
///
/// A slice with no timestamp cannot be placed on a timeline, and OTel requires
/// both ends, so those are skipped and counted rather than exported at epoch 0,
/// which would put every one of them in 1970 and quietly poison the trace.
fn export_otlp(slices: &[Slice], endpoint: &str) -> i32 {
    let mut spans = Vec::new();
    let mut undated = 0usize;
    for slice in slices {
        let Some(trace) = slice.graph.trace.as_ref() else {
            continue;
        };
        let Some(start_us) = trace.start_us else {
            undated += 1;
            continue;
        };
        let start_ns = start_us.saturating_mul(1_000);
        let queries: u64 = slice.graph.nodes.iter().map(|n| n.io_exclusive).sum();
        let wait_ns: u64 = slice.graph.nodes.iter().map(|n| n.wait_exclusive).sum();
        let name = if trace.route.is_empty() {
            slice.service.clone()
        } else {
            trace.route.clone()
        };
        let mut string_attributes = vec![("elephc.service".to_string(), slice.service.clone())];
        if !trace.route.is_empty() {
            string_attributes.push(("http.route".to_string(), trace.route.clone()));
        }
        spans.push(crate::otlp::OtlpSpan {
            service: slice.service.clone(),
            trace_id: trace.trace_id.clone(),
            span_id: trace.span_id.clone(),
            parent_span_id: trace.parent_span_id.clone(),
            name,
            start_unix_nano: start_ns,
            end_unix_nano: start_ns.saturating_add(slice.graph.total),
            attributes: vec![
                ("elephc.functions".to_string(), slice.graph.nodes.len() as i64),
                ("elephc.queries".to_string(), queries as i64),
                ("elephc.wait_ns".to_string(), wait_ns as i64),
            ],
            string_attributes,
        });
    }
    if undated > 0 {
        eprintln!(
            "elephc monitor: {undated} slice(s) carry no timestamp and were not exported — \
             a span needs both ends of an interval"
        );
    }
    if spans.is_empty() {
        eprintln!("elephc monitor: nothing to export to {endpoint}");
        return 1;
    }
    let body = crate::otlp::encode_traces(&spans);
    match crate::otlp::post_traces(endpoint, &body) {
        Ok(()) => {
            println!("exported {} span(s) to {endpoint}", spans.len());
            0
        }
        Err(error) => {
            eprintln!("elephc monitor: {error}");
            1
        }
    }
}

/// A `monitor` target that is a running service rather than a file.
///
/// `host:port`, or the same behind an `http://` scheme — the two spellings people
/// actually type. A filesystem path stays a path, so a socket at `/tmp/p.sock` is
/// still a socket and never mistaken for a host.
pub(crate) fn remote_target(spec: &str) -> Option<RemoteTarget> {
    let (tls, rest, default_port) = if let Some(rest) = spec.strip_prefix("https://") {
        (true, rest, 443)
    } else if let Some(rest) = spec.strip_prefix("http://") {
        (false, rest, 80)
    } else {
        (false, spec, 0)
    };
    // Anything after the authority is a path, not part of the address.
    let authority = rest.split('/').next().unwrap_or(rest);
    if authority.starts_with('/') || authority.starts_with('.') || authority.is_empty() {
        return None;
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => {
            if host.is_empty() || port.parse::<u16>().is_err() {
                return None;
            }
            (host.to_string(), port.parse::<u16>().ok()?)
        }
        // A scheme implies its port; a bare name without one is a file, not a host.
        None if default_port != 0 => (authority.to_string(), default_port),
        None => return None,
    };
    Some(RemoteTarget { host, port, tls })
}

/// A running service `monitor` can read, and how to reach it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RemoteTarget {
    pub host: String,
    pub port: u16,
    /// Whether the connection is wrapped in TLS, with the certificate verified
    /// against the platform root store before a single protocol byte is sent.
    pub tls: bool,
}

impl RemoteTarget {
    fn authority(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// Opens the connection, verifying the certificate first when the target is
    /// https.
    ///
    /// Verification is the entire difference between https and a plaintext port:
    /// without it, an attacker in the path answers the handshake and receives a
    /// profile — the shape of the code and the URLs it serves. Refusing an
    /// unverifiable certificate is therefore a hard failure, never a prompt.
    fn connect(&self) -> Result<Box<dyn ReadWrite>, String> {
        let timeout = std::time::Duration::from_secs(10);
        let tcp = std::net::TcpStream::connect(self.authority())
            .map_err(|error| format!("cannot reach {}: {error}", self.authority()))?;
        let _ = tcp.set_read_timeout(Some(timeout));
        let _ = tcp.set_write_timeout(Some(timeout));
        if !self.tls {
            return Ok(Box::new(tcp));
        }

        let roots = rustls::RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let server = rustls::pki_types::ServerName::try_from(self.host.clone())
            .map_err(|_| format!("{} is not a valid server name", self.host))?;
        let connection = rustls::ClientConnection::new(std::sync::Arc::new(config), server)
            .map_err(|error| format!("cannot start TLS to {}: {error}", self.host))?;
        Ok(Box::new(rustls::StreamOwned::new(connection, tcp)))
    }
}

/// Read + Write in one object, so the remote path is written once for both
/// transports rather than duplicated per socket type.
pub(crate) trait ReadWrite: std::io::Read + std::io::Write {}
impl<T: std::io::Read + std::io::Write> ReadWrite for T {}

/// Descriptor the child finds its control channel on.
const CONTROL_FD: i32 = 3;
/// Marker written into the channel before spawning, so it is already buffered
/// when the child looks and no handshake can race the program's own start.
const CONTROL_MAGIC: &[u8] = b"ELEPHC-MONITOR-1";

/// Holds the parent's end of the control channel open for the child's lifetime.
struct ControlChannel {
    parent: i32,
    child: i32,
}

impl Drop for ControlChannel {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.parent);
            libc::close(self.child);
        }
    }
}

/// Creates the socketpair that tells a spawned binary it is being monitored.
///
/// The credential is the channel itself. Only this process holds the other end,
/// so there is nothing for anyone else to copy, find in a log, or replay — unlike
/// an environment variable, which every process on the machine can read, and
/// which therefore has to be signed to be safe at all.
fn open_control_channel() -> Option<ControlChannel> {
    unsafe {
        let mut fds = [0i32; 2];
        if libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, fds.as_mut_ptr()) != 0 {
            return None;
        }
        let channel = ControlChannel {
            parent: fds[0],
            child: fds[1],
        };
        // Written BEFORE the fork, so the marker is waiting in the buffer rather
        // than racing the child's init.
        let wrote = libc::send(
            channel.parent,
            CONTROL_MAGIC.as_ptr() as *const libc::c_void,
            CONTROL_MAGIC.len(),
            0,
        );
        if wrote != CONTROL_MAGIC.len() as isize {
            return None;
        }
        Some(channel)
    }
}

/// Arranges for `channel`'s child end to arrive as `CONTROL_FD` in the spawned
/// process.
///
/// `pre_exec` runs in the forked child between fork and exec, where only
/// async-signal-safe calls are permitted — `dup2` and `close` are. Nothing else
/// happens here for that reason.
fn attach_control_channel(command: &mut process::Command, channel: &ControlChannel) {
    use std::os::unix::process::CommandExt as _;
    let child_fd = channel.child;
    unsafe {
        command.pre_exec(move || {
            if libc::dup2(child_fd, CONTROL_FD) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            // The duplicate is what the child keeps; clear CLOEXEC so it survives
            // the exec that follows.
            if libc::fcntl(CONTROL_FD, libc::F_SETFD, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

/// The marker `--with-monitoring` embeds, searched for in the target's bytes.
const MONITORING_MARKER: &[u8] = b"elephc-monitoring-v1";

/// Whether `path` is a binary built with `--with-monitoring`.
///
/// Read from the FILE, not from the running process: the whole value of the
/// check is telling someone "this binary cannot answer that question" before
/// anything is launched. Running it and reporting an empty profile would read as
/// "your program is fast", which is the worst possible way to be wrong.
fn carries_monitoring(path: &std::path::Path) -> bool {
    // Regular files only. `fs::read` on a character device never returns —
    // `monitor /dev/zero` read until the machine gave out — and on a directory
    // it fails in a way that used to read as "no marker".
    if !std::fs::metadata(path).map(|m| m.is_file()).unwrap_or(false) {
        return false;
    }
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    bytes
        .windows(MONITORING_MARKER.len())
        .any(|window| window == MONITORING_MARKER)
}

/// Refuses a target that was not built to be monitored.
///
/// Whether a target names an existing Unix socket.
///
/// The question is what the path IS, not how it is spelled: a socket answers a
/// profiling endpoint, a regular file is a program to run. Asking the filesystem
/// costs one `stat` and removes a whole class of surprise — a path that does not
/// exist is not a socket either, so it falls through to the file paths and gets
/// their error message instead of a connection failure.
fn is_socket_path(target: &str) -> bool {
    use std::os::unix::fs::FileTypeExt as _;
    std::fs::metadata(target)
        .map(|meta| meta.file_type().is_socket())
        .unwrap_or(false)
}

/// Deliberately strict, with no reduced fallback. An external sampler could still
/// produce time shares for an unequipped binary, but shipping that as a silent
/// downgrade means two different things arrive under one command and the reader
/// has to notice which — the exact ambiguity this whole design removes. One
/// answer, or an error naming the fix.
fn require_monitoring(path: &std::path::Path) -> Result<(), String> {
    // Say what is actually wrong. Every read failure used to collapse into
    // "not built with --with-monitoring", so a typo'd path, a directory, or a
    // permission problem all sent the user off to rebuild a binary that was
    // never the issue — an error that confidently names the wrong cause is
    // worse than one that admits it does not know.
    match std::fs::metadata(path) {
        Ok(meta) if !meta.is_file() => {
            return Err(format!(
                "{} is not a file, so there is nothing to run.",
                path.display()
            ));
        }
        Err(error) => {
            return Err(format!("cannot read {}: {error}", path.display()));
        }
        Ok(_) => {}
    }
    if carries_monitoring(path) {
        return Ok(());
    }
    Err(format!(
        "{} was not built with --with-monitoring, so there is nothing to monitor.\n  \
         Rebuild it:  elephc --with-monitoring <source>.php\n  \
         Or point monitor at the source and let it build:  elephc monitor <source>.php",
        path.display()
    ))
}

/// Renders the probe's per-route I/O counters, when the capture carries any.
///
/// Printed apart from the cause table and labelled, because these numbers are a
/// different KIND from everything around them: the table's shares are sampled at
/// 1000 Hz, while a driver call fires exactly one event, so these counts are
/// exact. Measured on the demo service, a run the sampler saw only 17 times
/// still reported 551 queries — the same 551 `--instrument` reports. Presenting
/// the two without saying which is which is how a profile misleads.
fn probe_io_summary(text: &str) -> String {
    let mut rows = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("elephc-probe-io: ") else {
            continue;
        };
        // "<route> ops=<n> wait_ns=<n>" — the route may contain spaces, so read
        // the counters off the end rather than splitting from the left.
        let Some((route, tail)) = rest.rsplit_once(" ops=") else {
            continue;
        };
        let Some((ops, wait)) = tail.split_once(" wait_ns=") else {
            continue;
        };
        match (ops.trim().parse::<u64>(), wait.trim().parse::<u64>()) {
            (Ok(ops), Ok(wait)) => rows.push((route.to_string(), ops, wait)),
            _ => continue,
        }
    }
    if rows.is_empty() {
        return String::new();
    }
    rows.sort_by(|a, b| b.1.cmp(&a.1));
    let total_ops: u64 = rows.iter().map(|r| r.1).sum();
    let total_wait: u64 = rows.iter().map(|r| r.2).sum();
    let mut out = format!(
        "\nI/O — {total_ops} operation(s), {} waiting. Exact, not sampled: a driver call \
         reports itself,\nso these counts do not depend on how often the profiler looked.\n\n",
        fmt_ns(total_wait)
    );
    for (route, ops, wait) in rows {
        out.push_str(&format!("{route:<40}{ops:>8} ops{:>12}\n", fmt_ns(wait)));
    }
    out.push_str(&probe_alloc_summary(text));
    out
}

/// Renders the probe's sampled allocation attribution.
///
/// Two different claims live here and conflating them would be the mistake: the
/// **total** is exact by construction, because each sample charges the counter's
/// delta since the previous one and those deltas telescope back to the counter
/// itself. What is sampled is the **attribution** — which stack gets charged —
/// since a delta is credited to whichever stack the sample caught. Allocations
/// after the final sample are simply not seen.
///
/// So this answers "where does allocation happen", not "how much did this
/// function allocate". `--instrument` is the mode that answers the second.
fn probe_alloc_summary(text: &str) -> String {
    let mut rows: Vec<(String, u64)> = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("elephc-probe-alloc: ") else {
            continue;
        };
        let Some((stack, allocs)) = rest.rsplit_once(' ') else {
            continue;
        };
        if let Ok(allocs) = allocs.trim().parse::<u64>() {
            rows.push((stack.to_string(), allocs));
        }
    }
    if rows.is_empty() {
        return String::new();
    }
    rows.sort_by(|a, b| b.1.cmp(&a.1));
    let total: u64 = rows.iter().map(|r| r.1).sum();
    let mut out = format!(
        "\nallocations — {total} total, exact; the attribution below is sampled, so it says\n\
         WHERE allocation happens rather than how much each function allocated.\n\n"
    );
    for (stack, allocs) in rows.iter().take(5) {
        let leaf = stack.rsplit(';').find(|f| *f != "<native>").unwrap_or(stack);
        out.push_str(&format!(
            "{leaf:<40}{allocs:>10}  {:>5.1}%\n",
            100.0 * *allocs as f64 / total.max(1) as f64
        ));
    }
    out
}

/// Reverses the runtime's percent-encoding of a trace-line field.
///
/// The runtime encodes because the route comes from an untrusted path and the
/// line is space-separated; decoding here means the operator still reads the
/// real `GET /orders/42` rather than `GET%20/orders/42`. A malformed escape is
/// left as written rather than guessed at.
fn decode_field(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(byte) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Per-service request statistics, the shape an operator pages on.
struct ServiceStats {
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
}

/// Nearest-rank percentile: the value at position ceil(p/100 x n), 1-indexed.
///
/// Chosen over interpolation because it always returns a value some request
/// actually took. Interpolating between two requests invents a duration nobody
/// experienced, which is the wrong trade when the inputs are exact to begin with.
fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = ((p / 100.0) * sorted.len() as f64).ceil().max(1.0) as usize;
    sorted[rank.min(sorted.len()) - 1]
}

/// Aggregates slices into one row per group.
///
/// Grouped by service, or by `service · route` when every slice names a route —
/// which is the breakdown worth paging on, since one slow endpoint is invisible
/// in a service-wide p95. All-or-nothing: mixing routed and unrouted rows would
/// double-count the same requests under two headings.
fn service_stats(slices: &[Slice]) -> Vec<ServiceStats> {
    use std::collections::BTreeMap;
    let routed = !slices.is_empty()
        && slices
            .iter()
            .all(|s| s.graph.trace.as_ref().is_some_and(|t| !t.route.is_empty()));
    let mut by_service: BTreeMap<(String, Option<String>), Vec<&Slice>> = BTreeMap::new();
    for slice in slices {
        let route = routed.then(|| {
            slice
                .graph
                .trace
                .as_ref()
                .map(|t| t.route.clone())
                .unwrap_or_default()
        });
        by_service
            .entry((slice.service.clone(), route))
            .or_default()
            .push(slice);
    }
    by_service
        .into_iter()
        .map(|((service, route), members)| {
            let mut durations: Vec<u64> = members.iter().map(|s| s.graph.total).collect();
            durations.sort_unstable();
            let total_ns: u64 = durations.iter().sum();
            let queries: u64 = members
                .iter()
                .flat_map(|s| s.graph.nodes.iter())
                .map(|n| n.io_exclusive)
                .sum();
            let wait_ns: u64 = members
                .iter()
                .flat_map(|s| s.graph.nodes.iter())
                .map(|n| n.wait_exclusive)
                .sum();
            // The window runs from the first request opening to the last one
            // finishing, the only span a rate can honestly divide by.
            let starts: Option<Vec<u64>> = members
                .iter()
                .map(|s| s.graph.trace.as_ref().and_then(|t| t.start_us))
                .collect();
            let rps = starts.and_then(|starts| {
                let first = *starts.iter().min()?;
                let last = members
                    .iter()
                    .zip(&starts)
                    .map(|(s, start)| start + s.graph.total / 1_000)
                    .max()?;
                (last > first).then(|| members.len() as f64 / ((last - first) as f64 / 1e6))
            });
            ServiceStats {
                service,
                route,
                requests: members.len(),
                p50: percentile(&durations, 50.0),
                p90: percentile(&durations, 90.0),
                p95: percentile(&durations, 95.0),
                p99: percentile(&durations, 99.0),
                rps,
                queries_per_request: queries as f64 / members.len() as f64,
                wait_share: if total_ns == 0 {
                    0.0
                } else {
                    100.0 * wait_ns as f64 / total_ns as f64
                },
            }
        })
        .collect()
}

/// Grades every captured slice against the project's performance budget.
///
/// The budget is the same `.elephc` file a dev run is graded by, and the checks
/// are the same checks — which is the point. A profile taken in production
/// should answer the same questions as one taken on a laptop, or the budget only
/// ever describes the laptop.
///
/// Where it differs from a one-shot run is what "failed" means across many
/// requests: an assertion is reported with the NUMBER of requests that violated
/// it and the worst value seen, because "p99 blew the query budget" and "every
/// request did" call for different actions. With a single slice this degrades to
/// exactly the one-shot report.
fn stitch_assert_report(slices: &[Slice], asserts: &[(String, Option<String>)]) -> (String, bool) {
    if asserts.is_empty() || slices.is_empty() {
        return (String::new(), true);
    }
    // spec -> (label, violations, worst actual, budget, op, evaluated)
    let mut tally: Vec<crate::call_graph::AssertOutcome> = Vec::new();
    let mut violations: Vec<usize> = Vec::new();
    for slice in slices {
        let outcomes = evaluate_asserts(&slice.graph, asserts);
        if tally.is_empty() {
            tally = outcomes;
            violations = vec![0; tally.len()];
            for (index, outcome) in tally.iter().enumerate() {
                if outcome.status != crate::call_graph::AssertStatus::Pass {
                    violations[index] = 1;
                }
            }
            continue;
        }
        for (index, outcome) in outcomes.iter().enumerate() {
            if outcome.status != crate::call_graph::AssertStatus::Pass {
                violations[index] += 1;
            }
            // Keep the worst value seen, which is the one worth acting on.
            let worse = match (tally[index].actual, outcome.actual) {
                (Some(a), Some(b)) => b > a,
                (None, Some(_)) => true,
                _ => false,
            };
            if worse {
                tally[index].actual = outcome.actual;
                tally[index].status = outcome.status;
            }
        }
    }

    let failed = violations.iter().filter(|n| **n > 0).count();
    let mut out = format!(
        "\nchecks — {} of {} assertion(s) violated, over {} request(s)\n",
        failed,
        tally.len(),
        slices.len()
    );
    for (index, outcome) in tally.iter().enumerate() {
        let n = violations[index];
        // SKIP stays distinct from FAIL, exactly as in a one-shot run: an
        // assertion naming a function no request reached says something is wrong
        // with the BUDGET, not the code. It still fails the gate — a budget that
        // silently measures nothing is worse than none — but calling it a
        // violation would send someone looking in the wrong place.
        let mark = match (n, outcome.actual) {
            (0, _) => "PASS",
            (_, None) => "SKIP",
            _ => "FAIL",
        };
        let actual = outcome
            .actual
            .map(|v| format!("worst {}", trim_number(v)))
            .unwrap_or_else(|| format!("'{}' never ran", outcome.target));
        out.push_str(&format!(
            "  [{mark}] {} ({actual}){}{}\n",
            outcome.spec,
            match (n, outcome.actual) {
                (0, _) | (_, None) => String::new(),
                _ => format!(" — {n} of {} request(s)", slices.len()),
            },
            outcome
                .label
                .as_ref()
                .map(|l| format!("  — {l}"))
                .unwrap_or_default(),
        ));
    }
    (out, failed == 0)
}

/// Renders the per-service summary that opens a stitch report.
fn service_summary(slices: &[Slice]) -> String {
    let stats = service_stats(slices);
    if stats.is_empty() {
        return String::new();
    }
    let mut out = String::from("\nper service\n\n");
    out.push_str(&format!(
        "{:<34}{:>5}{:>11}{:>11}{:>11}{:>11}{:>9}{:>9}{:>8}\n",
        "service", "n", "p50", "p90", "p95", "p99", "rps", "q/req", "wait"
    ));
    let mut thin = false;
    for s in &stats {
        thin |= s.requests < 20;
        out.push_str(&format!(
            "{:<34}{:>5}{:>11}{:>11}{:>11}{:>11}{:>9}{:>9}{:>7.0}%\n",
            match &s.route {
                Some(route) => format!("{} · {}", s.service, route),
                None => s.service.clone(),
            },
            s.requests,
            fmt_ns(s.p50),
            fmt_ns(s.p90),
            fmt_ns(s.p95),
            fmt_ns(s.p99),
            s.rps
                .map(|r| format!("{r:.1}"))
                .unwrap_or_else(|| "-".to_string()),
            format!("{:.1}", s.queries_per_request),
            s.wait_share,
        ));
    }
    if thin {
        // Printing "p99" over a handful of requests would dress the maximum up as
        // a distribution. The numbers stay — they are still the right ones — but
        // the reader is told what they are actually looking at.
        out.push_str(
            "\nfewer than 20 requests for a service: its upper percentiles are that \
             service's\nslowest requests, not a distribution.\n",
        );
    }
    out
}

fn stitch_report(slices: &[Slice]) -> String {
    use std::collections::BTreeMap;
    let mut by_trace: BTreeMap<&str, Vec<&Slice>> = BTreeMap::new();
    for slice in slices {
        if let Some(trace) = &slice.graph.trace {
            by_trace.entry(trace.trace_id.as_str()).or_default().push(slice);
        }
    }
    if by_trace.is_empty() {
        return "no distributed traces found — are the services built with --web --instrument?\n"
            .to_string();
    }
    let mut out = service_summary(slices);
    out.push_str(&format!(
        "\ndistributed traces — {} trace(s) over {} slice(s)\n",
        by_trace.len(),
        slices.len()
    ));
    for (trace_id, members) in by_trace {
        let present: HashSet<&str> = members
            .iter()
            .filter_map(|s| s.graph.trace.as_ref())
            .map(|t| t.span_id.as_str())
            .collect();
        let root_total: u64 = members
            .iter()
            .map(|s| s.graph.total)
            .max()
            .unwrap_or(1)
            .max(1);
        // Placed layout needs every member dated — see `render_trace_html`.
        let window = members
            .iter()
            .map(|s| s.graph.trace.as_ref().and_then(|t| t.start_us))
            .collect::<Option<Vec<u64>>>()
            .and_then(|starts| {
                let t0 = *starts.iter().min()?;
                let end = members
                    .iter()
                    .map(|s| {
                        s.graph.trace.as_ref().and_then(|t| t.start_us).unwrap_or(t0)
                            + s.graph.total / 1_000
                    })
                    .max()?;
                (end > t0).then_some((t0, end - t0))
            });
        out.push_str(&format!("\ntrace {trace_id}\n"));
        // Children of each span, then walk from the roots.
        let mut children: HashMap<&str, Vec<&Slice>> = HashMap::new();
        let mut roots: Vec<&Slice> = Vec::new();
        for slice in &members {
            let trace = slice.graph.trace.as_ref().expect("filtered above");
            if trace.parent_span_id.is_empty() || !present.contains(trace.parent_span_id.as_str()) {
                roots.push(slice);
            } else {
                children
                    .entry(trace.parent_span_id.as_str())
                    .or_default()
                    .push(slice);
            }
        }
        let mut stack: Vec<(&Slice, usize)> =
            roots.into_iter().rev().map(|slice| (slice, 0)).collect();
        let mut guard = 0;
        while let Some((slice, depth)) = stack.pop() {
            if guard > 4096 {
                out.push_str("  … trace truncated (cycle or excessive depth)\n");
                break;
            }
            guard += 1;
            let trace = slice.graph.trace.as_ref().expect("filtered above");
            let (offset, share) = match window {
                Some((t0, span_us)) => {
                    let start = trace.start_us.unwrap_or(t0).saturating_sub(t0);
                    (
                        100.0 * start as f64 / span_us as f64,
                        100.0 * (slice.graph.total / 1_000) as f64 / span_us as f64,
                    )
                }
                None => (0.0, 100.0 * slice.graph.total as f64 / root_total as f64),
            };
            let queries: u64 = slice.graph.nodes.iter().map(|n| n.io_exclusive).sum();
            let waited: u64 = slice.graph.nodes.iter().map(|n| n.wait_exclusive).sum();
            out.push_str(&format!(
                "{:indent$}{} {}  {}  {}  {} fn{}{}\n",
                "",
                if depth == 0 { "●" } else { "└─" },
                slice.service,
                fmt_ns(slice.graph.total),
                if window.is_some() { placed_bar(offset, share, 12) } else { bar(share, 12) },
                slice.graph.nodes.len(),
                if queries > 0 {
                    format!("  {queries} queries")
                } else {
                    String::new()
                },
                if waited > 0 {
                    format!("  {} waiting", fmt_ns(waited))
                } else {
                    String::new()
                },
                indent = depth * 2,
            ));
            if let Some(kids) = children.get(trace.span_id.as_str()) {
                for kid in kids.iter().rev() {
                    stack.push((kid, depth + 1));
                }
            }
        }
    }
    out
}

/// Reads `--stitch` service logs and correlates their request slices into
/// distributed traces.
fn run_stitch(cmd: &MonitorCommand) -> i32 {
    let mut slices = Vec::new();
    for path in &cmd.stitch {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) => {
                eprintln!("elephc monitor: cannot read {path}: {error}");
                return 1;
            }
        };
        // Name the service after its log file, which is what the operator
        // recognizes; the profile itself carries no service identity.
        let service = std::path::Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone());
        for chunk in split_slices(&text) {
            let graph = parse_instrument_dump(&chunk);
            if graph.trace.is_some() && !graph.nodes.is_empty() {
                slices.push(Slice {
                    service: service.clone(),
                    graph,
                });
            }
        }
    }
    if slices.is_empty() {
        eprintln!(
            "elephc monitor: no profiled request slices in {} — a service must be built with \
             --web --instrument for its log to carry them",
            cmd.stitch.join(", ")
        );
        return 1;
    }
    print!("{}", stitch_report(&slices));
    // The same budget that grades a dev run grades this capture, so a production
    // profile answers the same questions rather than a reduced set.
    // The budget is found by walking up from the first log, the way a dev run
    // walks up from the source: one file at a project root serves both.
    let source = cmd.stitch.first().cloned().unwrap_or_default();
    let mut asserts: Vec<(String, Option<String>)> = cmd
        .asserts
        .iter()
        .map(|spec| (spec.clone(), None))
        .collect();
    match load_assert_file(cmd.assert_file.as_deref(), &source) {
        Ok(Some((from_file, path))) => {
            if !from_file.is_empty() {
                println!("budget: {path}");
            }
            asserts.extend(from_file);
        }
        Ok(None) => {}
        Err(error) => {
            eprintln!("elephc monitor: {error}");
            return 1;
        }
    }
    let (checks, ok) = stitch_assert_report(&slices, &asserts);
    print!("{checks}");
    if let Some(path) = &cmd.prom_out {
        if let Err(error) = std::fs::write(path, prometheus_text(&slices)) {
            eprintln!("elephc monitor: cannot write {path}: {error}");
            return 1;
        }
        println!("wrote {path}");
    }
    if let Some(endpoint) = &cmd.otlp {
        if export_otlp(&slices, endpoint) != 0 {
            return 1;
        }
    }
    if !ok {
        // Same exit code a dev run uses, so one gate serves both.
        return 2;
    }
    if let Some(path) = &cmd.html_out {
        let spans: Vec<crate::call_graph::TraceSpan> = slices
            .iter()
            .filter_map(|slice| {
                let trace = slice.graph.trace.as_ref()?;
                let root = slice.graph.total.max(1);
                // The heaviest functions of the slice, so a slow hop is
                // diagnosable from the waterfall without opening its profile.
                let mut nodes: Vec<&crate::call_graph::GraphNode> =
                    slice.graph.nodes.iter().collect();
                nodes.sort_by(|a, b| b.exclusive.cmp(&a.exclusive));
                let top = nodes
                    .iter()
                    .take(8)
                    .map(|n| {
                        (
                            n.name.clone(),
                            100.0 * n.inclusive as f64 / root as f64,
                            100.0 * n.exclusive as f64 / root as f64,
                        )
                    })
                    .collect();
                Some(crate::call_graph::TraceSpan {
                    service: slice.service.clone(),
                    trace_id: trace.trace_id.clone(),
                    span_id: trace.span_id.clone(),
                    parent_span_id: trace.parent_span_id.clone(),
                    total_ns: slice.graph.total,
                    functions: slice.graph.nodes.len(),
                    queries: slice.graph.nodes.iter().map(|n| n.io_exclusive).sum(),
                    wait_ns: slice.graph.nodes.iter().map(|n| n.wait_exclusive).sum(),
                    start_us: trace.start_us,
                    top,
                })
            })
            .collect();
        let html = crate::call_graph::render_trace_html(&spans, "elephc distributed trace");
        match std::fs::write(path, html) {
            Ok(()) => println!("wrote {path}"),
            Err(error) => {
                eprintln!("elephc monitor: cannot write {path}: {error}");
                return 1;
            }
        }
    }
    0
}

/// Attaches the profiled source to an exact capture, with each declared
/// function located and carrying what the run measured.
///
/// An exact capture has no per-line data — `--instrument` times whole calls, not
/// statements — but it does know every function's cost, and the declaration
/// ranges say where each one lives. That is enough to read the file as a map of
/// where the time went, which is the point of having the source view at all.
fn attach_exact_source(graph: &mut crate::call_graph::CallGraph, target: &str) {
    let Ok(text) = std::fs::read_to_string(target) else {
        return;
    };
    let root_ns = graph.nodes.iter().map(|n| n.inclusive).max().unwrap_or(1).max(1);
    let measured: HashMap<&str, &crate::call_graph::GraphNode> =
        graph.nodes.iter().map(|n| (n.name.as_str(), n)).collect();
    let funcs: Vec<crate::call_graph::SourceFunc> = php_decl_ranges(&text)
        .into_iter()
        .filter_map(|range| {
            let node = measured.get(range.name.as_str())?;
            Some(crate::call_graph::SourceFunc {
                name: range.name,
                start: range.start,
                end: range.end,
                self_pct: 100.0 * node.exclusive as f64 / root_ns as f64,
                incl_pct: 100.0 * node.inclusive as f64 / root_ns as f64,
                calls: node.call_count.unwrap_or(0),
            })
        })
        .collect();
    graph.lines = Some(crate::call_graph::SourceLines {
        file: target.to_string(),
        source: text.lines().map(str::to_string).collect(),
        hits: Vec::new(),
        total: 0,
        funcs,
    });
}

/// Loads a saved exact call graph (`--save` output) for diffing.
fn load_exact_graph(path: &str) -> Option<crate::call_graph::CallGraph> {
    let json = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&json).ok()
}

/// Prints a per-function delta table of the exact capture against a baseline:
/// inclusive time share and call count, before → after, most-changed first.
fn instrument_delta_table(
    base: &crate::call_graph::CallGraph,
    cur: &crate::call_graph::CallGraph,
) -> String {
    let base_root = base.nodes.iter().map(|n| n.inclusive).max().unwrap_or(1).max(1);
    let cur_root = cur.nodes.iter().map(|n| n.inclusive).max().unwrap_or(1).max(1);
    let pct = |incl: u64, root: u64| 100.0 * incl as f64 / root as f64;
    let mut base_by: HashMap<&str, &crate::call_graph::GraphNode> = HashMap::new();
    for n in &base.nodes {
        base_by.insert(n.name.as_str(), n);
    }
    let mut cur_by: HashMap<&str, &crate::call_graph::GraphNode> = HashMap::new();
    for n in &cur.nodes {
        cur_by.insert(n.name.as_str(), n);
    }
    let mut names: Vec<&str> = base_by.keys().chain(cur_by.keys()).copied().collect();
    names.sort();
    names.dedup();
    // Sort by absolute time-share change, most-moved first.
    names.sort_by(|a, b| {
        let da = (cur_by.get(a).map_or(0.0, |n| pct(n.inclusive, cur_root))
            - base_by.get(a).map_or(0.0, |n| pct(n.inclusive, base_root)))
        .abs();
        let db = (cur_by.get(b).map_or(0.0, |n| pct(n.inclusive, cur_root))
            - base_by.get(b).map_or(0.0, |n| pct(n.inclusive, base_root)))
        .abs();
        db.partial_cmp(&da).unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut out = String::from("\n--- vs baseline (exact) ---\n");
    for name in names {
        let c_pct = cur_by.get(name).map_or(0.0, |n| pct(n.inclusive, cur_root));
        let b_pct = base_by.get(name).map_or(0.0, |n| pct(n.inclusive, base_root));
        let c_calls = cur_by.get(name).and_then(|n| n.call_count).unwrap_or(0);
        let b_calls = base_by.get(name).and_then(|n| n.call_count).unwrap_or(0);
        let short = if name.len() > 24 {
            format!("{}…", &name[..23])
        } else {
            name.to_string()
        };
        out.push_str(&format!(
            "{short:<25} time {b_pct:>5.1}% -> {c_pct:>5.1}% ({:+5.1})  calls {b_calls} -> {c_calls} ({:+})\n",
            c_pct - b_pct,
            c_calls as i64 - b_calls as i64
        ));
    }
    out
}

/// Renders the exact `--instrument` profile as a stdout bar table, hottest
/// inclusive first.
fn instrument_table(graph: &crate::call_graph::CallGraph) -> String {
    let root = graph.nodes.iter().map(|n| n.inclusive).max().unwrap_or(1).max(1);
    let mut nodes: Vec<&crate::call_graph::GraphNode> = graph.nodes.iter().collect();
    nodes.sort_by(|a, b| b.inclusive.cmp(&a.inclusive).then(a.name.cmp(&b.name)));
    let mut out = format!(
        "\nexact profile — {} functions, total {}\n\n",
        graph.nodes.len(),
        fmt_ns(root)
    );
    // The queries column only appears when the program issued any DB query.
    let has_io = graph.nodes.iter().any(|n| n.io_inclusive > 0);
    // Likewise the retained column: only when some function ends with a net
    // heap delta (allocated minus freed), i.e. the run kept or released objects.
    let has_ret = graph.nodes.iter().any(|n| n.retained_inclusive != 0);
    // And the wait column: only when some call actually blocked in a driver.
    let has_wait = graph.nodes.iter().any(|n| n.wait_inclusive > 0);
    for node in nodes {
        let incl = 100.0 * node.inclusive as f64 / root as f64;
        let excl = 100.0 * node.exclusive as f64 / root as f64;
        let calls = node.call_count.unwrap_or(0);
        let name = if node.name.len() > 26 {
            format!("{}…", &node.name[..25])
        } else {
            node.name.clone()
        };
        let queries = if has_io {
            format!("  queries {}", node.io_exclusive)
        } else {
            String::new()
        };
        let retained = if has_ret {
            format!("  retained {:+}", node.retained_exclusive)
        } else {
            String::new()
        };
        // Self time splits into the part spent blocked and the rest (CPU).
        let wait = if has_wait {
            format!(
                "  wait {} cpu {}",
                fmt_ns(node.wait_exclusive),
                fmt_ns(node.exclusive.saturating_sub(node.wait_exclusive))
            )
        } else {
            String::new()
        };
        out.push_str(&format!(
            "{name:<27} {}  incl {incl:>5.1}%  self {excl:>5.1}%  calls {calls}  self {}  allocs {}{queries}{retained}{wait}\n",
            bar(incl, 20),
            fmt_ns(node.exclusive),
            node.alloc_exclusive,
        ));
    }
    out
}

/// Formats nanoseconds as a human duration for the exact table.
fn fmt_ns(ns: u64) -> String {
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
struct Row {
    depth: usize,
    count: u64,
    symbol: String,
    module: String,
    address: Option<u64>,
}

/// One frame of a rebuilt sample stack.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Frame {
    symbol: String,
    address: Option<u64>,
    /// Set on virtual frames recovered from inlined source spans.
    inlined: bool,
}

/// Parses `sample`'s indented call-graph section into depth rows.
///
/// Depth is encoded by the prefix width — two columns per level, counting the
/// `+ ! : |` ancestry decorations as well as spaces. Thread headers carry no
/// `(in module)` suffix and are skipped by the shape check.
fn parse_call_graph(report: &str) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut in_graph = false;
    for line in report.lines() {
        if line.starts_with("Call graph:") {
            in_graph = true;
            continue;
        }
        if in_graph && (line.starts_with("Total number") || line.trim().is_empty()) {
            break;
        }
        if !in_graph {
            continue;
        }
        let prefix_len = line
            .bytes()
            .take_while(|b| matches!(b, b' ' | b'+' | b'!' | b':' | b'|'))
            .count();
        let rest = &line[prefix_len..];
        let Some((count_text, tail)) = rest.split_once(' ') else {
            continue;
        };
        let Ok(count) = count_text.parse::<u64>() else {
            continue;
        };
        let Some((symbol, module_tail)) = tail.split_once("  (in ") else {
            continue;
        };
        let module = module_tail
            .split_once(')')
            .map(|(module, _)| module)
            .unwrap_or("");
        // The bracketed list holds one address per merged offset bucket; the
        // first is the dominant one and enough for line attribution.
        let address = line
            .rsplit_once('[')
            .and_then(|(_, brackets)| brackets.split([',', ']']).next())
            .map(str::trim)
            .and_then(|first| first.strip_prefix("0x"))
            .and_then(|hex| u64::from_str_radix(hex, 16).ok());
        rows.push(Row {
            depth: prefix_len / 2,
            count,
            symbol: symbol.trim().to_string(),
            module: module.to_string(),
            address,
        });
    }
    // Drop loader scaffolding and rebase depths on the program frames.
    rows.retain(|row| row.module != "dyld");
    if let Some(base) = rows.iter().map(|row| row.depth).min() {
        for row in &mut rows {
            row.depth -= base;
        }
    }
    rows
}

/// Converts depth rows into leaf samples: each node contributes its self weight
/// (count minus its children's counts) under its full ancestor stack.
fn build_samples(rows: &[Row]) -> Vec<(Vec<Frame>, u64)> {
    let mut samples = Vec::new();
    let mut stack: Vec<(usize, Frame, i64)> = Vec::new();
    let flush = |stack: &mut Vec<(usize, Frame, i64)>, samples: &mut Vec<(Vec<Frame>, u64)>| {
        let (_, frame, remaining) = stack.pop().expect("flush on empty stack");
        if remaining > 0 {
            let mut frames: Vec<Frame> = stack.iter().map(|(_, frame, _)| frame.clone()).collect();
            frames.push(frame);
            samples.push((frames, remaining as u64));
        }
    };
    for row in rows {
        while stack
            .last()
            .is_some_and(|(depth, _, _)| *depth >= row.depth)
        {
            flush(&mut stack, &mut samples);
        }
        if let Some((_, _, remaining)) = stack.last_mut() {
            *remaining -= row.count as i64;
        }
        stack.push((
            row.depth,
            Frame {
                symbol: row.symbol.clone(),
                address: row.address,
                inlined: false,
            },
            row.count as i64,
        ));
    }
    while !stack.is_empty() {
        flush(&mut stack, &mut samples);
    }
    samples
}

/// Returns whether a sampled symbol is a PHP-level frame (script main, a
/// function, or a method) rather than a runtime helper or synthetic body.
fn is_php_symbol(symbol: &str) -> bool {
    let stem = symbol.trim_start_matches('_');
    stem == "main" || stem.starts_with("fn_") || stem.starts_with("method_")
}

/// Demangles a PHP-level symbol to its source spelling: `fn_hot_u_leaf` →
/// `hot_leaf`, `method_Engine_step` → `Engine::step`, `main` → `{main}`.
/// `_u_` escapes an underscore inside a name; the placeholder swap keeps it
/// from being read as the class/method separator.
fn demangle(symbol: &str) -> String {
    let stem = symbol.trim_start_matches('_');
    if stem == "main" {
        return "{main}".to_string();
    }
    if let Some(rest) = stem.strip_prefix("fn_") {
        return rest.replace("_u_", "_");
    }
    if let Some(rest) = stem.strip_prefix("method_") {
        let protected = rest.replace("_u_", "\u{1}");
        if let Some((class, method)) = protected.split_once('_') {
            return format!(
                "{}::{}",
                class.replace('\u{1}', "_"),
                method.replace('\u{1}', "_")
            );
        }
        return rest.replace("_u_", "_");
    }
    symbol.to_string()
}

/// What a runtime helper is doing, in words a PHP developer can act on.
/// Prefix-matched in order, most specific first.
const CAUSES: &[(&str, &str)] = &[
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
fn cause_for(symbol: &str) -> Option<&'static str> {
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
struct DeclRange {
    name: String,
    start: u32,
    end: u32,
}

/// Extracts function and method declaration ranges from PHP source with a
/// brace scanner. Best-effort by design: braces inside strings can skew a
/// range, which at worst misplaces one virtual frame — never a wrong weight.
fn php_decl_ranges(source: &str) -> Vec<DeclRange> {
    let lines: Vec<&str> = source.lines().collect();
    let mut ranges = Vec::new();
    let mut classes: Vec<(String, u32)> = Vec::new(); // (name, end line)
    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i].trim_start();
        let decl_line = (i + 1) as u32;
        if let Some(name) = declared_name(line, "class ")
            .or_else(|| declared_name(line, "interface "))
            .or_else(|| declared_name(line, "trait "))
        {
            let end = brace_span_end(&lines, i);
            classes.push((name, end));
            i += 1;
            continue;
        }
        if let Some(name) = declared_name(line, "function ") {
            let end = brace_span_end(&lines, i);
            let owner = classes
                .iter()
                .rev()
                .find(|(_, class_end)| decl_line <= *class_end)
                .map(|(class, _)| class.clone());
            let display = match owner {
                Some(class) => format!("{class}::{name}"),
                None => name,
            };
            ranges.push(DeclRange {
                name: display,
                start: decl_line,
                end,
            });
            // Skip past the body so nested closures don't shadow the range.
            i = (end as usize).max(i + 1);
            continue;
        }
        i += 1;
    }
    ranges
}

/// Returns the identifier following `keyword` on the line, tolerating leading
/// modifiers (`public function step`, `final class Engine`). The keyword must
/// sit at a word boundary; anonymous closures yield no name and are skipped.
fn declared_name(line: &str, keyword: &str) -> Option<String> {
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
fn brace_span_end(lines: &[&str], start: usize) -> u32 {
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

/// Resolves sampled addresses to source lines with `atos` against the dSYM.
fn resolve_lines(
    binary: &Path,
    load_address: &str,
    addresses: &[u64],
) -> HashMap<u64, u32> {
    let dsym = binary.with_extension("dSYM");
    if !dsym.exists() || addresses.is_empty() {
        return HashMap::new();
    }
    let mut command = process::Command::new("/usr/bin/atos");
    command
        .arg("-o")
        .arg(&dsym)
        .arg("-l")
        .arg(load_address);
    for address in addresses {
        command.arg(format!("{address:#x}"));
    }
    let Ok(output) = command.output() else {
        return HashMap::new();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut lines = HashMap::new();
    for (address, resolved) in addresses.iter().zip(text.lines()) {
        // Shape: `main (in bench) (bench.php:33)`; helpers have no line suffix.
        if let Some((_, tail)) = resolved.rsplit_once(':') {
            if let Ok(line) = tail.trim_end_matches(')').parse::<u32>() {
                lines.insert(*address, line);
            }
        }
    }
    lines
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

/// Builds the per-line profile for a capture, or `None` without a dSYM/source.
fn line_profile(
    samples: &[(Vec<Frame>, u64)],
    report: &str,
    binary: &Path,
    php_source: &Path,
) -> Option<LineProfile> {
    let source = std::fs::read_to_string(php_source).ok()?;
    let load_address = report
        .lines()
        .find_map(|line| line.strip_prefix("Load Address:").map(str::trim))?;
    // The innermost PHP frame of each stack is the code actually running.
    let leaves: Vec<(u64, u64)> = samples
        .iter()
        .filter_map(|(stack, weight)| {
            stack
                .iter()
                .rev()
                .find(|frame| is_php_symbol(&frame.symbol) && !frame.inlined)
                .and_then(|frame| frame.address)
                .map(|address| (address, *weight))
        })
        .collect();
    if leaves.is_empty() {
        return None;
    }
    let unique: Vec<u64> = leaves
        .iter()
        .map(|(address, _)| *address)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let resolved = resolve_lines(binary, load_address, &unique);
    if resolved.is_empty() {
        return None;
    }
    let (hits, total) = attribute_lines(&leaves, &resolved);
    (total > 0).then(|| LineProfile {
        file: php_source.display().to_string(),
        source: source.lines().map(str::to_string).collect(),
        hits,
        total,
    })
}

/// Sums each leaf sample's weight onto the source line its address resolves to.
/// Addresses the dSYM could not place are dropped from BOTH the per-line counts
/// and the total, so a line's share is over what was actually attributable
/// rather than being quietly diluted by unresolvable samples.
fn attribute_lines(
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

/// Rewrites sample stacks so a PHP frame sampled on a line owned by ANOTHER
/// function's declaration range grows a virtual `(inlined)` child frame — the
/// call boundary the inliner erased, recovered from the source span it kept.
fn inject_inlined_frames(
    samples: &mut [(Vec<Frame>, u64)],
    report: &str,
    binary: &Path,
    php_source: &Path,
) {
    let Ok(source) = std::fs::read_to_string(php_source) else {
        return;
    };
    let ranges = php_decl_ranges(&source);
    if ranges.is_empty() {
        return;
    }
    let Some(load_address) = report
        .lines()
        .find_map(|line| line.strip_prefix("Load Address:").map(str::trim))
    else {
        return;
    };
    let addresses: Vec<u64> = samples
        .iter()
        .flat_map(|(stack, _)| stack.iter())
        .filter(|frame| is_php_symbol(&frame.symbol))
        .filter_map(|frame| frame.address)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let lines = resolve_lines(binary, load_address, &addresses);
    if lines.is_empty() {
        return;
    }
    for (stack, _) in samples.iter_mut() {
        let mut rewritten: Vec<Frame> = Vec::with_capacity(stack.len());
        for frame in stack.iter() {
            let own_name = demangle(&frame.symbol);
            rewritten.push(frame.clone());
            if !is_php_symbol(&frame.symbol) {
                continue;
            }
            let Some(line) = frame.address.and_then(|a| lines.get(&a)) else {
                continue;
            };
            if let Some(owner) = ranges
                .iter()
                .find(|range| range.start <= *line && *line <= range.end)
            {
                if owner.name != own_name {
                    rewritten.push(Frame {
                        symbol: format!("inlined:{}", owner.name),
                        address: None,
                        inlined: true,
                    });
                }
            }
        }
        *stack = rewritten;
    }
}

// --------------------------------------------------------------------------
// Local sampling without /usr/bin/sample
// --------------------------------------------------------------------------


// --------------------------------------------------------------------------
// --probe-host: connect to a running probe endpoint
// --------------------------------------------------------------------------

/// Profiles a running `--probe` binary through its endpoint socket: reads the
/// build key from its `.key` file (or `ELEPHC_PROBE_KEY`), runs the
/// mutual HMAC handshake, receives the folded profile, and renders the same
/// table (plus optional Speedscope/pprof). Needs no macOS sampler.
fn run_probe_host(cmd: &MonitorCommand, socket: &str) -> i32 {
    use std::os::unix::net::UnixStream;
    let key = match resolve_probe_key(cmd, socket) {
        Ok(key) => key,
        Err(error) => {
            eprintln!("elephc monitor: {error}");
            return 1;
        }
    };
    // A path is a local socket; `host:port` is a service somewhere else. The
    // handshake is identical, so the transport is the only thing that differs —
    // which is what lets one command read a binary on this machine and a service
    // on another.
    let mut stream: Box<dyn ReadWrite> = match remote_target(socket) {
        Some(target) => match target.connect() {
            Ok(stream) => stream,
            Err(error) => {
                eprintln!("elephc monitor: {error}");
                return 1;
            }
        },
        None => match UnixStream::connect(socket) {
            Ok(stream) => Box::new(stream),
            Err(error) => {
                eprintln!("elephc monitor: cannot connect to probe at {socket}: {error}");
                return 1;
            }
        },
    };
    let nonce_c = probe_nonce();
    let folded = match elephc_probe::endpoint::wire::client_handshake_and_fetch(
        &mut stream,
        &key,
        &nonce_c,
    ) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("elephc monitor: {error}");
            return 1;
        }
    };
    // Announced only once the peer has PROVEN it holds the same build key.
    // Printing it on connect claimed a relationship that had not been
    // established — and still printed when the TLS certificate was then refused,
    // which reads as "connected, then something odd happened" rather than
    // "never connected to anything trustworthy".
    println!(
        "connected to probe build {}",
        elephc_probe::handshake::fingerprint(&key)
    );
    let display = folded_text_to_display(&folded);
    if display.is_empty() {
        eprintln!("elephc monitor: the probe returned no samples yet — is the process busy?");
        return 1;
    }
    if let Some(out_path) = &cmd.out {
        if let Err(error) = write_speedscope(&display, out_path) {
            eprintln!("elephc monitor: {error}");
            return 1;
        }
        println!("wrote {out_path}");
    }
    if let Some(pprof_path) = &cmd.pprof_out {
        let stacks = php_folded_stacks(&display);
        if let Err(error) = std::fs::write(pprof_path, crate::pprof_encode::encode_folded_profile(&stacks)) {
            eprintln!("elephc monitor: cannot write {pprof_path}: {error}");
            return 1;
        }
        println!("wrote {pprof_path}");
    }
    // A remote probe capture has no local dSYM/source, so no line attribution.
    if let Err(error) = write_graph_exports(cmd, &display, "elephc probe (remote)", None) {
        eprintln!("elephc monitor: {error}");
        return 1;
    }
    print!("{}", why_table(&display, 1));
    print!("{}", probe_io_summary(&folded));
    0
}

/// Resolves the build key for `--probe-host`: `ELEPHC_PROBE_KEY` hex if set,
/// else the `<socket-without-.sock>.key` file, else a `.key`
/// next to the socket path.
fn resolve_probe_key(cmd: &MonitorCommand, socket: &str) -> Result<[u8; 32], String> {
    if let Some(path) = &cmd.probe_key {
        let hex = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read probe key {path}: {error}"))?;
        return parse_hex_key(hex.trim())
            .ok_or_else(|| format!("probe key {path} is not 64 hex characters"));
    }
    if let Ok(hex) = std::env::var("ELEPHC_PROBE_KEY") {
        return parse_hex_key(hex.trim())
            .ok_or_else(|| "ELEPHC_PROBE_KEY is not 64 hex characters".to_string());
    }
    let candidates = [
        format!("{}.key", socket.trim_end_matches(".sock")),
        format!("{socket}.key"),
    ];
    for candidate in &candidates {
        if let Ok(hex) = std::fs::read_to_string(candidate) {
            return parse_hex_key(hex.trim()).ok_or_else(|| {
                format!("probe key sidecar {candidate} is not 64 hex characters")
            });
        }
    }
    Err(format!(
        "no build key: pass --key <file>, set ELEPHC_PROBE_KEY, or place a .key \
         file next to {socket}"
    ))
}

/// Parses 64 hex chars into a 32-byte key.
fn parse_hex_key(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 {
        return None;
    }
    let mut key = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks_exact(2).enumerate() {
        key[i] = u8::from_str_radix(std::str::from_utf8(chunk).ok()?, 16).ok()?;
    }
    Some(key)
}

/// A per-connection client nonce from the OS RNG (time-seeded fallback).
fn probe_nonce() -> [u8; 32] {
    use std::io::Read as _;
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        let mut nonce = [0u8; 32];
        if file.read_exact(&mut nonce).is_ok() {
            return nonce;
        }
    }
    let mut state = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1)
        | 1;
    let mut nonce = [0u8; 32];
    for byte in nonce.iter_mut() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *byte = (state >> 24) as u8;
    }
    nonce
}

/// Parses the endpoint's folded text (`elephc-probe: a;b;c <count>`) into the
/// display stacks the renderers consume. Probe frames are already PHP names or
/// `<native>`, so classification is a name test.
fn folded_text_to_display(text: &str) -> Vec<(Vec<(String, Kind)>, u64)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("elephc-probe: ") else {
            continue;
        };
        let Some((stack_text, count_text)) = rest.rsplit_once(' ') else {
            continue;
        };
        let Ok(count) = count_text.trim().parse::<u64>() else {
            continue;
        };
        let stack: Vec<(String, Kind)> = stack_text
            .split(';')
            .map(|name| {
                let kind = if name == "<native>" {
                    Kind::Native
                } else {
                    Kind::Php
                };
                (name.to_string(), kind)
            })
            .collect();
        if !stack.is_empty() {
            out.push((stack, count));
        }
    }
    out
}

// --------------------------------------------------------------------------
// Rendering
// --------------------------------------------------------------------------

/// A display-ready frame: its user-facing name and what kind of time it is.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Php,
    PhpInlined,
    Helper,
    Native,
}

/// Whether a file is a Speedscope document rather than an exact capture.
///
/// Used only to explain a failure, so it reads the shape rather than validating:
/// a `profiles` array plus the `$schema` Speedscope writes is enough to be sure
/// which of the two files someone reached for.
fn looks_like_speedscope(path: &str) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(doc) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    doc.get("profiles").map(|p| p.is_array()).unwrap_or(false)
        && doc
            .get("$schema")
            .and_then(|s| s.as_str())
            .map(|s| s.contains("speedscope"))
            .unwrap_or(false)
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
const EXACT_STACK_FLOOR_DIVISOR: u64 = 10_000;

/// Hard ceiling on emitted stacks.
///
/// The share floor bounds the count in terms of the capture, but a wide enough
/// graph can still reach it slowly, and no reader has ever needed a hundred
/// thousand distinct stacks. Reaching it stops the DESCENT, never the emission:
/// a frame that stops descending keeps its children's time as its own, so the
/// total stays right and the shape degrades instead of the arithmetic. It is
/// reported when hit, because a silently truncated profile reads exactly like a
/// complete one.
const EXACT_STACK_CAP: usize = 50_000;

/// How deep a single root-to-leaf descent may go before it stops.
///
/// `on_path` prevents cycles but not depth, and the recursion is a real Rust
/// stack: a genuinely deep chain of distinct functions would overflow it, losing
/// a capture that had already been taken.
const EXACT_STACK_MAX_DEPTH: usize = 512;

fn exact_stacks(graph: &crate::call_graph::CallGraph) -> Vec<(Vec<(String, Kind)>, u64)> {
    let mut children: Vec<Vec<(usize, u64)>> = vec![Vec::new(); graph.nodes.len()];
    let mut incoming: Vec<u64> = vec![0; graph.nodes.len()];
    for edge in &graph.edges {
        if edge.from >= graph.nodes.len() || edge.to >= graph.nodes.len() {
            continue;
        }
        children[edge.from].push((edge.to, edge.weight));
        // A self-edge is not a caller. Counting it as one makes a self-recursive
        // function that nothing else calls look reachable, so it is never
        // treated as a root and its whole time vanishes from the export.
        if edge.from != edge.to {
            incoming[edge.to] += edge.weight;
        }
    }
    let mut stacks = Vec::new();
    let mut path: Vec<(String, Kind)> = Vec::new();
    let mut on_path = vec![false; graph.nodes.len()];
    let mut seen = vec![false; graph.nodes.len()];
    // Roots: everything nothing calls. A capture whose every node has a caller
    // (mutual recursion at the top) would otherwise emit nothing at all, so the
    // heaviest node stands in as the root.
    let mut roots: Vec<usize> = (0..graph.nodes.len())
        .filter(|&i| incoming[i] == 0)
        .collect();
    if roots.is_empty() {
        if let Some(top) = (0..graph.nodes.len()).max_by_key(|&i| graph.nodes[i].inclusive) {
            roots.push(top);
        }
    }
    // The floor is a share of the capture, so it means the same thing whatever
    // unit the numbers are in.
    let budget_total: u64 = roots.iter().map(|&r| graph.nodes[r].inclusive).sum();
    let floor = budget_total / EXACT_STACK_FLOOR_DIVISOR;
    for root in roots {
        let inclusive = graph.nodes[root].inclusive;
        exact_walk(
            graph,
            &children,
            root,
            inclusive,
            floor,
            &mut path,
            &mut on_path,
            &mut seen,
            &mut stacks,
        );
    }
    if stacks.len() >= EXACT_STACK_CAP {
        eprintln!(
            "elephc monitor: the call graph produced more than {EXACT_STACK_CAP} distinct \
             stacks; deeper paths are folded into their callers. Totals are unaffected, \
             the flame view is coarser."
        );
    }
    // Anything left over sits in a cycle no root reaches. Its own time is real
    // and belongs in the total, but re-descending from it would count its
    // already-placed callees twice — so it is emitted flat, as itself.
    for (i, node) in graph.nodes.iter().enumerate() {
        if !seen[i] && node.exclusive > 0 {
            stacks.push((vec![(node.name.clone(), Kind::Php)], node.exclusive));
        }
    }
    stacks
}

/// Marks a folded subtree as accounted for, so the leftover pass leaves it alone.
///
/// Iterative rather than recursive: this runs precisely when a graph turned out
/// to be deeper or wider than expected, which is the worst moment to add stack
/// frames of its own.
fn mark_folded(
    children: &[Vec<(usize, u64)>],
    start: usize,
    on_path: &[bool],
    seen: &mut [bool],
) {
    let mut stack = vec![start];
    while let Some(node) = stack.pop() {
        if seen[node] {
            continue;
        }
        seen[node] = true;
        for &(child, _) in &children[node] {
            // A node on the current path is an ancestor, not part of what was
            // folded away; its own frame still emits.
            if !seen[child] && !on_path[child] {
                stack.push(child);
            }
        }
    }
}

/// One node of `exact_stacks`: place `budget` nanoseconds of this function on
/// the current path, then hand each child the time its edge measured.
fn exact_walk(
    graph: &crate::call_graph::CallGraph,
    children: &[Vec<(usize, u64)>],
    node: usize,
    budget: u64,
    floor: u64,
    path: &mut Vec<(String, Kind)>,
    on_path: &mut [bool],
    seen: &mut [bool],
    stacks: &mut Vec<(Vec<(String, Kind)>, u64)>,
) {
    path.push((graph.nodes[node].name.clone(), Kind::Php));
    on_path[node] = true;
    seen[node] = true;
    // The denominator is whichever is larger: the function's own inclusive time,
    // or what its edges actually claim. They should agree, and in every capture
    // measured here they do — but a truncated or corrupted dump can name edges
    // summing past the parent, and then the children walk away with more than
    // the budget while `own` saturates to zero. Taking the larger keeps the
    // proportions and makes "the export totals the run" true by construction
    // rather than true by luck.
    let claimed: u64 = children[node]
        .iter()
        .filter(|(child, _)| !on_path[*child])
        .map(|(_, edge_ns)| *edge_ns)
        .sum();
    let inclusive = graph.nodes[node].inclusive.max(claimed).max(1);
    // This path's share of the function, so a function with several callers is
    // divided among them instead of counted once per caller.
    let scale = |value: u64| -> u64 {
        (u128::from(value) * u128::from(budget) / u128::from(inclusive)) as u64
    };
    let mut spent = 0u64;
    for &(child, edge_ns) in &children[node] {
        if on_path[child] {
            // Recursion: the cycle's remaining time stays here rather than
            // descending forever.
            continue;
        }
        let share = scale(edge_ns);
        // Too small to say anything, too deep to descend safely, or past the
        // ceiling: the time stays here rather than being dropped, so the total
        // is unchanged and only the shape gets coarser.
        if share == 0 {
            continue;
        }
        if share <= floor || path.len() >= EXACT_STACK_MAX_DEPTH || stacks.len() >= EXACT_STACK_CAP
        {
            // Folded into this frame: `spent` deliberately does not grow, so the
            // share stays in `own` below.
            //
            // Everything under it must then be marked accounted-for. The leftover
            // pass emits the self time of every node it never saw, and a folded
            // subtree is exactly a set of nodes nobody saw — so without this its
            // time is counted twice, once inside the caller and once flat. It
            // showed up as +0.0053% on a real capture, which is small enough to
            // have been read as rounding.
            mark_folded(children, child, on_path, seen);
            continue;
        }
        spent = spent.saturating_add(share);
        exact_walk(graph, children, child, share, floor, path, on_path, seen, stacks);
    }
    let own = budget.saturating_sub(spent);
    if own > 0 {
        stacks.push((path.clone(), own));
    }
    on_path[node] = false;
    path.pop();
}

/// Converts raw sample stacks into display stacks of (name, kind).
fn render_stacks(samples: &[(Vec<Frame>, u64)]) -> Vec<(Vec<(String, Kind)>, u64)> {
    samples
        .iter()
        .map(|(stack, weight)| {
            let display = stack
                .iter()
                .map(|frame| {
                    if frame.inlined {
                        let name = frame.symbol.trim_start_matches("inlined:");
                        (format!("{name} (inlined)"), Kind::PhpInlined)
                    } else if is_php_symbol(&frame.symbol) {
                        (demangle(&frame.symbol), Kind::Php)
                    } else if cause_for(&frame.symbol).is_some() {
                        (frame.symbol.clone(), Kind::Helper)
                    } else {
                        (frame.symbol.clone(), Kind::Native)
                    }
                })
                .collect();
            (display, *weight)
        })
        .collect()
}

/// Serializes both views as one Speedscope document.
fn write_speedscope(
    display: &[(Vec<(String, Kind)>, u64)],
    out_path: &str,
) -> Result<(), String> {
    let mut frames: Vec<serde_json::Value> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut frame_index = |name: &str, frames: &mut Vec<serde_json::Value>| -> usize {
        if let Some(&i) = index.get(name) {
            return i;
        }
        let i = frames.len();
        index.insert(name.to_string(), i);
        frames.push(serde_json::json!({ "name": name }));
        i
    };

    let aggregate = |view: &dyn Fn(&[(String, Kind)]) -> Vec<String>| {
        let mut merged: BTreeMap<Vec<String>, u64> = BTreeMap::new();
        for (stack, weight) in display {
            *merged.entry(view(stack)).or_default() += weight;
        }
        merged
    };
    let php_view = aggregate(&|stack: &[(String, Kind)]| {
        let folded: Vec<String> = stack
            .iter()
            .filter(|(_, kind)| matches!(kind, Kind::Php | Kind::PhpInlined))
            .map(|(name, _)| name.clone())
            .collect();
        if folded.is_empty() {
            vec!["<non-PHP>".to_string()]
        } else {
            folded
        }
    });
    let why_view = aggregate(&|stack: &[(String, Kind)]| {
        stack
            .iter()
            .map(|(name, kind)| match kind {
                Kind::Helper => match cause_for(name) {
                    Some(cause) => format!("{name} — {cause}"),
                    None => name.clone(),
                },
                _ => name.clone(),
            })
            .collect()
    });

    let mut profiles = Vec::new();
    for (name, view) in [("PHP (helpers folded)", php_view), ("Why (runtime)", why_view)] {
        let total: u64 = view.values().sum();
        let mut sample_lists = Vec::new();
        let mut weights = Vec::new();
        for (stack, weight) in view {
            sample_lists.push(
                stack
                    .iter()
                    .map(|f| frame_index(f, &mut frames))
                    .collect::<Vec<_>>(),
            );
            weights.push(weight);
        }
        profiles.push(serde_json::json!({
            "type": "sampled",
            "name": name,
            "unit": "none",
            "startValue": 0,
            "endValue": total,
            "samples": sample_lists,
            "weights": weights,
        }));
    }
    let doc = serde_json::json!({
        "$schema": "https://www.speedscope.app/file-format-schema.json",
        "shared": { "frames": frames },
        "profiles": profiles,
        "name": "elephc monitor profile",
    });
    let file =
        std::fs::File::create(out_path).map_err(|e| format!("cannot write {out_path}: {e}"))?;
    let mut writer = std::io::BufWriter::new(file);
    serde_json::to_writer(&mut writer, &doc).map_err(|e| format!("cannot serialize: {e}"))?;
    writer
        .flush()
        .map_err(|e| format!("cannot write {out_path}: {e}"))?;
    Ok(())
}

/// Per-function statistics aggregated from display stacks: total and self
/// weight per PHP function (virtual inlined frames included), plus the runtime
/// causes sampled underneath each.
struct TableStats {
    grand: u64,
    totals: BTreeMap<String, u64>,
    selfs: BTreeMap<String, u64>,
    causes: BTreeMap<String, BTreeMap<&'static str, u64>>,
}

/// Aggregates display stacks into per-function totals, selfs, and causes.
fn table_stats(display: &[(Vec<(String, Kind)>, u64)]) -> TableStats {
    let mut stats = TableStats {
        grand: 0,
        totals: BTreeMap::new(),
        selfs: BTreeMap::new(),
        causes: BTreeMap::new(),
    };
    for (stack, weight) in display {
        stats.grand += weight;
        let php_frames: Vec<&String> = stack
            .iter()
            .filter(|(_, kind)| matches!(kind, Kind::Php | Kind::PhpInlined))
            .map(|(name, _)| name)
            .collect();
        let mut seen = HashSet::new();
        for frame in &php_frames {
            if seen.insert(*frame) {
                *stats.totals.entry((*frame).clone()).or_default() += weight;
            }
        }
        let (leaf, leaf_kind) = stack.last().expect("sample stacks are never empty");
        if matches!(leaf_kind, Kind::Php | Kind::PhpInlined) {
            *stats.selfs.entry(leaf.clone()).or_default() += weight;
        } else if let Some(owner) = php_frames.last() {
            let cause = cause_for(leaf).unwrap_or("other native");
            *stats
                .causes
                .entry((*owner).clone())
                .or_default()
                .entry(cause)
                .or_default() += weight;
        }
    }
    stats
}

/// Renders a percentage as a fixed-width Unicode bar, readable in any log.
fn bar(pct: f64, width: usize) -> String {
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
fn placed_bar(offset_pct: f64, pct: f64, width: usize) -> String {
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

/// Renders the per-function cause table with proportion bars.
fn why_table(display: &[(Vec<(String, Kind)>, u64)], processes: usize) -> String {
    let stats = table_stats(display);
    let grand = stats.grand;
    let process_note = if processes > 1 {
        format!(" · {processes} processes")
    } else {
        String::new()
    };
    let mut out = format!("samples: {grand}{process_note}\n");
    let mut by_weight: Vec<_> = stats.totals.iter().collect();
    by_weight.sort_by_key(|(_, weight)| std::cmp::Reverse(**weight));
    for (function, total) in by_weight {
        let pct = 100.0 * *total as f64 / grand as f64;
        let self_pct = 100.0 * stats.selfs.get(function).copied().unwrap_or(0) as f64 / grand as f64;
        out.push_str(&format!(
            "\n{function:<26} {} {pct:5.1}%  self {self_pct:4.1}%\n",
            bar(pct, 22)
        ));
        let mut cause_rows: Vec<_> = stats
            .causes
            .get(function)
            .map(|map| map.iter().collect())
            .unwrap_or_default();
        cause_rows.sort_by_key(|(_, weight)| std::cmp::Reverse(**weight));
        for (cause, weight) in cause_rows {
            let cause_pct = 100.0 * *weight as f64 / grand as f64;
            out.push_str(&format!(
                "    {cause:<25} {} {cause_pct:5.1}%\n",
                bar(cause_pct, 22)
            ));
        }
    }
    out
}

/// Renders one live frame: the window's hot functions with trend arrows
/// against the previous window, and the cumulative share on the right.
fn live_frame(
    window: &[(Vec<(String, Kind)>, u64)],
    cumulative: &BTreeMap<Vec<(String, Kind)>, u64>,
    previous: &mut HashMap<String, f64>,
    processes: usize,
    window_secs: u32,
    elapsed: std::time::Duration,
) -> String {
    let stats = table_stats(window);
    let cumulative_samples: Vec<(Vec<(String, Kind)>, u64)> =
        cumulative.iter().map(|(k, v)| (k.clone(), *v)).collect();
    let cumulative_stats = table_stats(&cumulative_samples);
    let elapsed_secs = elapsed.as_secs();
    let mut out = format!(
        "elephc monitor — live · {processes} process{} · window {window_secs}s · total {}m{:02}s · {} samples\n",
        if processes > 1 { "es" } else { "" },
        elapsed_secs / 60,
        elapsed_secs % 60,
        cumulative_stats.grand,
    );
    out.push_str(&format!(
        "{:<26} {:<22} {:>6}      {:>6}\n",
        "", "WINDOW", "", "CUMUL"
    ));
    let mut by_weight: Vec<_> = stats.totals.iter().collect();
    by_weight.sort_by_key(|(_, weight)| std::cmp::Reverse(**weight));
    let mut next_previous = HashMap::new();
    for (function, total) in by_weight {
        let pct = 100.0 * *total as f64 / stats.grand as f64;
        let cumulative_pct = cumulative_stats
            .totals
            .get(function)
            .map(|w| 100.0 * *w as f64 / cumulative_stats.grand as f64)
            .unwrap_or(0.0);
        let trend = match previous.get(function) {
            Some(prior) if pct - prior > 2.0 => "▲",
            Some(prior) if prior - pct > 2.0 => "▼",
            Some(_) => "─",
            None => " ",
        };
        next_previous.insert(function.clone(), pct);
        out.push_str(&format!(
            "{function:<26} {} {pct:5.1}% {trend}    {cumulative_pct:5.1}%\n",
            bar(pct, 22)
        ));
        let mut cause_rows: Vec<_> = stats
            .causes
            .get(function)
            .map(|map| map.iter().collect())
            .unwrap_or_default();
        cause_rows.sort_by_key(|(_, weight)| std::cmp::Reverse(**weight));
        for (cause, weight) in cause_rows.into_iter().take(4) {
            let cause_pct = 100.0 * *weight as f64 / stats.grand as f64;
            out.push_str(&format!(
                "    {cause:<25} {} {cause_pct:5.1}%\n",
                bar(cause_pct, 22)
            ));
        }
    }
    *previous = next_previous;
    out
}

/// Appends a Markdown report to `$GITHUB_STEP_SUMMARY` when running in GitHub
/// Actions: a hot-function table plus a Mermaid pie of the runtime causes.
fn write_github_summary(display: &[(Vec<(String, Kind)>, u64)], processes: usize) {
    let Ok(path) = std::env::var("GITHUB_STEP_SUMMARY") else {
        return;
    };
    let stats = table_stats(display);
    let grand = stats.grand;
    let mut md = format!(
        "## elephc monitor\n\n{grand} samples · {processes} process{}\n\n| Function | Total | Self | Top cause |\n|---|---|---|---|\n",
        if processes > 1 { "es" } else { "" }
    );
    let mut by_weight: Vec<_> = stats.totals.iter().collect();
    by_weight.sort_by_key(|(_, weight)| std::cmp::Reverse(**weight));
    for (function, total) in by_weight.iter().take(12) {
        let pct = 100.0 * **total as f64 / grand as f64;
        let self_pct =
            100.0 * stats.selfs.get(*function).copied().unwrap_or(0) as f64 / grand as f64;
        let top_cause = stats
            .causes
            .get(*function)
            .and_then(|map| map.iter().max_by_key(|(_, weight)| **weight))
            .map(|(cause, weight)| {
                format!("{cause} ({:.1}%)", 100.0 * *weight as f64 / grand as f64)
            })
            .unwrap_or_else(|| "—".to_string());
        md.push_str(&format!(
            "| `{function}` | {pct:.1}% | {self_pct:.1}% | {top_cause} |\n"
        ));
    }
    let mut global_causes: BTreeMap<&'static str, u64> = BTreeMap::new();
    for per_function in stats.causes.values() {
        for (cause, weight) in per_function {
            *global_causes.entry(cause).or_default() += weight;
        }
    }
    if !global_causes.is_empty() {
        md.push_str("\n```mermaid\npie title Runtime time by cause\n");
        for (cause, weight) in &global_causes {
            md.push_str(&format!("    \"{cause}\" : {weight}\n"));
        }
        md.push_str("```\n");
    }
    if let Ok(mut file) = std::fs::OpenOptions::new().append(true).open(&path) {
        let _ = writeln!(file, "{md}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The refusal of an unequipped target must not sit behind a platform
    /// branch.
    ///
    /// It did, once: `require_monitoring` was called after the
    /// `cfg!(target_os = "macos")` block, so the same binary that was refused on
    /// a laptop was run and quietly under-reported on a Linux server — an
    /// environment-dependent behaviour in the one command whose whole purpose is
    /// not to have any.
    ///
    /// Nothing that runs can catch this. CI is macOS-only, so a Linux-only
    /// ordering bug is invisible to every test that executes the binary, and the
    /// two branches cannot both be taken in one process. What is left is the
    /// order of the source itself, so that is what this reads — the file as an
    /// interface, because here it is the only witness.
    #[test]
    fn the_capability_gate_runs_before_any_platform_branch() {
        let source = include_str!("monitor.rs");
        let body = source
            .split_once("pub(crate) fn run(cmd: MonitorCommand) -> i32 {")
            .expect("the dispatch function must exist")
            .1;
        let gate = body
            .find("require_monitoring(")
            .expect("the dispatch must refuse an unequipped target");
        let platform = body
            .find("cfg!(target_os = \"macos\")")
            .expect("the dispatch must still have its one platform branch");
        assert!(
            gate < platform,
            "the capability check is inside or after the platform branch, so it \
             would be enforced on macOS and skipped on Linux"
        );
    }

    /// The exact capture's Speedscope/pprof export must account for the run
    /// exactly once — no more, no less.
    ///
    /// The graph shape that breaks a naive converter is here on purpose: `leaf`
    /// has TWO callers, so anything that emits it once per caller at full cost
    /// reports more time than the program had, and every percentage a viewer
    /// draws is then wrong while the file still opens and looks plausible.
    /// `spin` calls itself, which sends a converter with no cycle guard into a
    /// stack overflow rather than a wrong answer.
    ///
    /// The reference is the sum of every function's SELF time, which is what
    /// partitions a run — not the largest inclusive, which is only the deepest
    /// single call.
    #[test]
    fn exact_export_accounts_for_the_run_exactly_once() {
        use crate::call_graph::{CallGraph, GraphEdge, GraphNode};

        fn node(name: &str, inclusive: u64, exclusive: u64) -> GraphNode {
            GraphNode {
                name: name.to_string(),
                inclusive,
                exclusive,
                call_count: None,
                alloc_inclusive: 0,
                alloc_exclusive: 0,
                io_inclusive: 0,
                io_exclusive: 0,
                retained_inclusive: 0,
                retained_exclusive: 0,
                wait_inclusive: 0,
                wait_exclusive: 0,
                causes: Vec::new(),
            }
        }

        // root(1000) ─┬─ left(400) ──┐
        //             └─ right(300) ─┴─ leaf(500 total, reached from both)
        // spin(200) calls itself.
        let graph = CallGraph {
            nodes: vec![
                node("root", 1000, 300),
                node("left", 400, 100),
                node("right", 300, 100),
                node("leaf", 500, 500),
                node("spin", 200, 200),
            ],
            edges: vec![
                GraphEdge { from: 0, to: 1, weight: 400, count: Some(1) },
                GraphEdge { from: 0, to: 2, weight: 300, count: Some(1) },
                GraphEdge { from: 1, to: 3, weight: 300, count: Some(1) },
                GraphEdge { from: 2, to: 3, weight: 200, count: Some(1) },
                GraphEdge { from: 4, to: 4, weight: 100, count: Some(3) },
            ],
            total: 1000,
            queries: Vec::new(),
            lines: None,
            trace: None,
        };

        let stacks = exact_stacks(&graph);
        let exported: u64 = stacks.iter().map(|(_, weight)| *weight).sum();
        let run: u64 = graph.nodes.iter().map(|n| n.exclusive).sum();
        assert_eq!(
            exported, run,
            "the export must sum to the run's self time, not a multiple of it"
        );

        // And `leaf` must appear under both callers, split rather than doubled.
        let leaf_total: u64 = stacks
            .iter()
            .filter(|(path, _)| path.last().map(|(n, _)| n == "leaf").unwrap_or(false))
            .map(|(_, weight)| *weight)
            .sum();
        assert_eq!(leaf_total, 500, "leaf's own time is 500 however many callers reach it");
        let leaf_paths = stacks
            .iter()
            .filter(|(path, _)| path.last().map(|(n, _)| n == "leaf").unwrap_or(false))
            .count();
        assert_eq!(leaf_paths, 2, "leaf should appear once per caller path");
    }

    /// A Speedscope export must be recognizable as the wrong kind of baseline.
    ///
    /// This is the detector behind the only error message that can rescue the
    /// mistake the docs themselves used to recommend — `--out` a Speedscope
    /// file, then hand it to `--baseline`. Without it the user is told the file
    /// is unreadable and left to guess which of two JSON files they wanted.
    #[test]
    fn a_speedscope_export_is_not_mistaken_for_an_exact_capture() {
        let dir = std::env::temp_dir().join(format!("elephc_baseline_{}", process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");

        let speedscope = dir.join("s.json");
        std::fs::write(
            &speedscope,
            r#"{"$schema":"https://www.speedscope.app/file-format-schema.json",
                "profiles":[],"shared":{"frames":[]}}"#,
        )
        .expect("write speedscope");
        assert!(looks_like_speedscope(&speedscope.display().to_string()));

        // An exact capture is JSON too, and must NOT be mistaken for one.
        let exact = dir.join("e.json");
        std::fs::write(&exact, r#"{"nodes":[],"edges":[],"total":0}"#).expect("write exact");
        assert!(!looks_like_speedscope(&exact.display().to_string()));

        // Neither is anything else — a missing file, or JSON of another shape.
        assert!(!looks_like_speedscope(&dir.join("absent.json").display().to_string()));
        let other = dir.join("o.json");
        std::fs::write(&other, r#"{"profiles":[]}"#).expect("write other");
        assert!(
            !looks_like_speedscope(&other.display().to_string()),
            "a bare `profiles` key is not enough to claim a file is Speedscope"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Bounding the export must not cost its arithmetic.
    ///
    /// These two pull against each other, and fixing one broke the other: the
    /// brake folds a subtree's time into its caller, and the leftover pass emits
    /// the self time of every node it never saw — so the first version counted
    /// folded subtrees twice. It measured +0.0053% on a real capture, small
    /// enough to pass for rounding, which is why the count and the total are
    /// asserted in the same test on the same graph.
    ///
    /// The shape is a chain of diamonds — layered dispatch, not adversarial
    /// input — where every level doubles the number of distinct paths. Before
    /// the brake, 52 nodes with a realistic nanosecond budget produced 319,930
    /// stacks; the bound was `log2(root nanoseconds)`, an accident of the units.
    #[test]
    fn bounding_the_export_does_not_cost_its_accounting() {
        use crate::call_graph::{CallGraph, GraphEdge, GraphNode};

        fn node(name: &str, inclusive: u64, exclusive: u64) -> GraphNode {
            GraphNode {
                name: name.to_string(), inclusive, exclusive, call_count: None,
                alloc_inclusive: 0, alloc_exclusive: 0, io_inclusive: 0, io_exclusive: 0,
                retained_inclusive: 0, retained_exclusive: 0,
                wait_inclusive: 0, wait_exclusive: 0, causes: Vec::new(),
            }
        }

        // n diamonds: j_k calls a_k and b_k, both of which call j_{k+1}.
        // Times are consistent — each frame's inclusive covers its own work plus
        // what it hands on — because an impossible graph proves nothing.
        // Each self time is DERIVED as inclusive minus what the frame hands on,
        // so the selfs sum to the run by construction. Hand-picked numbers gave
        // an impossible graph twice, and an impossible graph proves nothing.
        let n = 24usize;
        let run_ns = 10_000_000_000u64; // 10 s, so the floor is a real threshold
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut w = run_ns;
        for k in 0..n {
            let (j, a, b) = (3 * k, 3 * k + 1, 3 * k + 2);
            // j hands everything to its two arms; each arm keeps half of its own.
            nodes.push(node(&format!("j{k}"), w, 0));
            nodes.push(node(&format!("a{k}"), w / 2, w / 4));
            nodes.push(node(&format!("b{k}"), w / 2, w / 4));
            edges.push(GraphEdge { from: j, to: a, weight: w / 2, count: Some(1) });
            edges.push(GraphEdge { from: j, to: b, weight: w / 2, count: Some(1) });
            let next = 3 * (k + 1); // the next junction, or the leaf below
            edges.push(GraphEdge { from: a, to: next, weight: w / 4, count: Some(1) });
            edges.push(GraphEdge { from: b, to: next, weight: w / 4, count: Some(1) });
            w /= 2;
        }
        nodes.push(node("leaf", w, w)); // index 3n, where the last arms point
        let graph = CallGraph {
            nodes, edges, total: run_ns, queries: Vec::new(), lines: None, trace: None,
        };

        let run: u64 = graph.nodes.iter().map(|n| n.exclusive).sum();
        // Repeated halving loses a nanosecond here and there; what matters is
        // that the fixture is a partition, not that it lands on a round number.
        assert!(
            run_ns - run < 100,
            "the fixture itself must partition the run: {run} vs {run_ns}"
        );
        let stacks = exact_stacks(&graph);
        let exported: u64 = stacks.iter().map(|(_, w)| *w).sum();

        assert!(
            stacks.len() < 10 * graph.nodes.len(),
            "{} nodes produced {} stacks — the count is growing with the paths, \
             not with the graph",
            graph.nodes.len(),
            stacks.len()
        );
        // The export distributes the roots' inclusive time, which on a real
        // capture IS the sum of self times; here the two differ by the handful of
        // nanoseconds this fixture's own halving loses. What is being asserted is
        // that bounding the walk moved time into a caller rather than duplicating
        // or dropping it — a defect of that kind moved it by 0.0053% when it was
        // real, which is far outside this margin.
        let drift = exported.abs_diff(run);
        assert!(
            drift < 100,
            "bounding the walk must move time into a caller, never duplicate or \
             drop it: exported {exported} vs run {run} (drift {drift})"
        );
        assert!(
            stacks.iter().all(|(path, _)| path.len() <= EXACT_STACK_MAX_DEPTH),
            "no stack may exceed the depth bound"
        );
    }

    #[test]
    fn probe_key_hex_parses_and_rejects_malformed() {
        let key = parse_hex_key(
            "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        )
        .expect("valid 64-hex key");
        assert_eq!(key[0], 0x00);
        assert_eq!(key[31], 0xff);
        assert!(parse_hex_key("short").is_none());
        assert!(parse_hex_key(&"zz".repeat(32)).is_none());
    }

    #[test]
    fn folded_endpoint_text_becomes_display_stacks() {
        let text = "elephc-probe: <native>;{main};grind 480\n\
                    elephc-probe: <native>;{main};grind;<native> 20\n\
                    elephc-probe-samples: 500\n";
        let display = folded_text_to_display(text);
        assert_eq!(display.len(), 2);
        // First stack: native root, main, grind (all PHP but the native root).
        assert_eq!(display[0].1, 480);
        assert_eq!(display[0].0[0], ("<native>".to_string(), Kind::Native));
        assert_eq!(display[0].0[1], ("{main}".to_string(), Kind::Php));
        assert_eq!(display[0].0[2], ("grind".to_string(), Kind::Php));
        // The table attributes grind's 500 samples (both stacks name it).
        let table = why_table(&display, 1);
        assert!(table.contains("grind"), "{table}");
        assert!(table.contains("100.0%"), "{table}");
        // The `elephc-probe-samples:` trailer is not a stack line.
        assert!(!display.iter().any(|(stack, _)| stack
            .iter()
            .any(|(name, _)| name.contains("samples"))));
    }

    /// A trimmed `sample` report with the exact indentation and decoration
    /// characters the tool emits, including a function split across sibling
    /// nodes by call offset.
    const REPORT: &str = "Analysis of sampling bench (pid 1) every 1 millisecond
Load Address:    0x100000000
----

Call graph:
    100 Thread_1   DispatchQueue_1: com.apple.main-thread  (serial)
      100 start  (in dyld) + 6992  [0x1]
        60 main  (in bench) + 5164  [0x100000010]
        + 40 fn_hot_u_leaf  (in bench) + 308  [0x100000020]
        + ! 30 _rt_mixed_from_value  (in bench) + 196  [0x100000030]
        + ! : 20 _rt_heap_alloc  (in bench) + 628  [0x100000040]
        + 15 fn_hot_u_leaf  (in bench) + 12,4  [0x100000050,0x100000054]
        30 main  (in bench) + 99  [0x100000060]
        + 30 method_Engine_step  (in bench) + 1  [0x100000070]
        10 main  (in bench) + 3  [0x100000080]

Total number in stack (recursive counted multiple, when >=5):
";

    #[test]
    fn parses_depths_and_addresses_from_decorated_prefixes() {
        let rows = parse_call_graph(REPORT);
        // dyld rows dropped, depths rebased on the first program frame.
        assert_eq!(rows[0].symbol, "main");
        assert_eq!(rows[0].depth, 0);
        assert_eq!(rows[0].address, Some(0x100000010));
        assert_eq!(rows[1].symbol, "fn_hot_u_leaf");
        assert_eq!(rows[1].depth, 1);
        assert_eq!(rows[2].symbol, "_rt_mixed_from_value");
        assert_eq!(rows[2].depth, 2);
        assert_eq!(rows[3].symbol, "_rt_heap_alloc");
        assert_eq!(rows[3].depth, 3);
        // The offset-split sibling stays at the same depth as its twin, and a
        // multi-bucket node keeps its first address.
        assert_eq!(rows[4].symbol, "fn_hot_u_leaf");
        assert_eq!(rows[4].depth, 1);
        assert_eq!(rows[4].address, Some(0x100000050));
    }

    #[test]
    fn self_weights_partition_every_parent_count() {
        let rows = parse_call_graph(REPORT);
        let samples = build_samples(&rows);
        let total: u64 = samples.iter().map(|(_, weight)| weight).sum();
        // 100 thread samples minus nothing: every count is accounted for once.
        assert_eq!(total, 100);
        // main's first node keeps 60 - 40 - 15 = 5 self samples.
        assert!(samples.iter().any(|(stack, weight)| *weight == 5
            && stack.len() == 1
            && stack[0].symbol == "main"));
        // The helper chain bottoms out in heap_alloc with 20 self samples.
        assert!(samples.iter().any(|(stack, weight)| *weight == 20
            && stack.last().map(|f| f.symbol.as_str()) == Some("_rt_heap_alloc")));
    }

    /// Builds a one-slice log chunk for `service` with the given trace identity.
    fn slice_log(fn_name: &str, ns: u64, trace: &str, span: &str, parent: &str) -> String {
        format!(
            "elephc-instr-trace: trace={trace} span={span} parent={parent}\n\
             elephc-instr: {fn_name} calls=1 incl_ns={ns} excl_ns={ns} incl_allocs=0 \
             excl_allocs=0 incl_io=0 excl_io=0 incl_ret=0 excl_ret=0 incl_wait=0 excl_wait=0\n"
        )
    }

    /// Builds a slice directly, so a summary can be exercised without a log round trip.
    fn slice_of(service: &str, ns: u64, start_us: Option<u64>, span: &str) -> Slice {
        let mut graph = parse_instrument_dump(&slice_log("handle", ns, "t1", span, "-"));
        if let Some(trace) = graph.trace.as_mut() {
            trace.start_us = start_us;
        }
        Slice {
            service: service.to_string(),
            graph,
        }
    }

    /// The I/O summary must survive a route containing spaces, and must say that
    /// its numbers are of a different kind from the sampled table above it.
    #[test]
    fn the_io_summary_reads_counters_off_the_end_of_the_line() {
        let text = "elephc-probe: a;b 3\n\
                    elephc-probe-samples: 17\n\
                    elephc-probe-io: <untagged> ops=551 wait_ns=3449131\n\
                    elephc-probe-io: GET /a b/c ops=2 wait_ns=1000\n";
        let out = probe_io_summary(text);
        // A route can contain spaces, so the counters are read from the RIGHT;
        // splitting from the left would truncate the route and lose the row.
        assert!(out.contains("GET /a b/c"), "{out}");
        assert!(out.contains("551 ops"), "{out}");
        // Busiest first, so the row worth reading is the one at the top.
        let untagged = out.find("<untagged>").unwrap();
        let other = out.find("GET /a b/c").unwrap();
        assert!(untagged < other, "rows must sort by operation count:\n{out}");
        // The distinction the whole feature rests on.
        assert!(out.contains("Exact, not sampled"), "{out}");

        // Nothing to say when the capture carries no events.
        assert!(probe_io_summary("elephc-probe: a 1\n").is_empty());
        // A malformed line is skipped rather than producing a bogus row.
        assert!(probe_io_summary("elephc-probe-io: x ops=nope wait_ns=1\n").is_empty());
    }

    /// The allocation summary must separate the two claims it makes.
    ///
    /// The total is exact — each sample charges the counter delta since the last,
    /// and those telescope back to the counter. The attribution is sampled. Saying
    /// only one of those, or neither, is what would mislead.
    #[test]
    fn the_allocation_summary_separates_the_exact_total_from_sampled_attribution() {
        let text = "elephc-probe-alloc: a;b;load_price 900\n\
                    elephc-probe-alloc: a;record_audit 100\n";
        let out = probe_alloc_summary(text);
        assert!(out.contains("1000 total, exact"), "{out}");
        assert!(out.contains("attribution below is sampled"), "{out}");
        // Busiest first, named by the innermost PHP frame rather than the raw stack.
        let hot = out.find("load_price").unwrap();
        let cold = out.find("record_audit").unwrap();
        assert!(hot < cold, "{out}");
        assert!(out.contains("90.0%"), "{out}");
        // `<native>` leaves are skipped when naming a row, or every row would read
        // the same and the summary would say nothing.
        let native = probe_alloc_summary("elephc-probe-alloc: a;connect;<native> 5\n");
        assert!(native.contains("connect"), "{native}");
        assert!(probe_alloc_summary("elephc-probe: a 1\n").is_empty());
    }

    /// A path must never be read as a host, or a local socket silently becomes a
    /// network connection attempt — and the reverse would make `monitor host:port`
    /// try to open a file that does not exist.
    #[test]
    fn a_socket_path_is_never_mistaken_for_a_url() {
        let at = |spec: &str| {
            let t = remote_target(spec).expect("should parse");
            (t.host, t.port, t.tls)
        };
        assert_eq!(at("127.0.0.1:9000"), ("127.0.0.1".into(), 9000, false));
        assert_eq!(at("http://api.internal:8080"), ("api.internal".into(), 8080, false));
        // A path after the authority is not part of the address.
        assert_eq!(at("http://api.internal:8080/x"), ("api.internal".into(), 8080, false));
        // A scheme implies its port, and https implies TLS.
        assert_eq!(at("https://foo.example"), ("foo.example".into(), 443, true));
        assert_eq!(at("https://foo.example:8443"), ("foo.example".into(), 8443, true));
        assert_eq!(at("http://foo.example"), ("foo.example".into(), 80, false));

        // Filesystem paths, including one whose directory contains a colon.
        assert!(remote_target("/tmp/probe.sock").is_none());
        assert!(remote_target("./probe.sock").is_none());
        assert!(remote_target("/tmp/a:b/probe.sock").is_none());
        // A bare name is a binary, not a host: no port, no connection.
        assert!(remote_target("shop").is_none());
        assert!(remote_target("shop.php").is_none());
        // Malformed authorities are refused rather than half-parsed.
        assert!(remote_target(":9000").is_none(), "a port with no host");
        assert!(remote_target("host:").is_none(), "a host with no port");
        assert!(remote_target("host:notaport").is_none());
        assert!(remote_target("host:99999").is_none(), "a port past u16");
    }

    /// A crafted route must not be able to forge series in the metrics file.
    ///
    /// The exposition format ends a label value at an unescaped quote and a sample
    /// at a newline, so a path carrying either would write series a scraper trusts.
    /// Backslash is escaped first — escaping it last would re-escape the escapes we
    /// just added and corrupt every value containing one.
    #[test]
    fn prometheus_labels_cannot_be_escaped_out_of() {
        assert_eq!(escape_label(r#"GET /a"b"#), r#"GET /a\"b"#);
        assert_eq!(escape_label("GET /a\nelephc_requests_total 999"),
                   "GET /a\\nelephc_requests_total 999");
        assert_eq!(escape_label(r"back\slash"), r"back\\slash");
        // Ordinary routes are untouched, so escaping costs nothing in the common case.
        assert_eq!(escape_label("GET /orders/42"), "GET /orders/42");
    }

    /// The file must be the format Prometheus actually parses: typed, and with the
    /// quantile label inside the same brace group as the others.
    #[test]
    fn prometheus_output_is_a_valid_summary() {
        let slices: Vec<Slice> = (0..25)
            .map(|i| slice_of("api", 1_000_000 * (i + 1), None, &format!("s{i}")))
            .collect();
        let text = prometheus_text(&slices);
        assert!(text.contains("# TYPE elephc_request_duration_seconds summary"));
        assert!(
            text.contains(r#"elephc_request_duration_seconds{service="api",quantile="0.99"}"#),
            "quantile must join the existing label set, not open a second one:\n{text}"
        );
        // Durations are seconds, as every Prometheus convention requires.
        assert!(text.contains(" 0.025000\n"), "p99 of 25ms should be 0.025 s:\n{text}");
        // Every sample line must carry a value, or the scrape fails wholesale.
        for line in text.lines().filter(|l| !l.starts_with('#') && !l.is_empty()) {
            let value = line.rsplit(' ').next().unwrap_or("");
            assert!(
                value.parse::<f64>().is_ok(),
                "unparseable sample value in {line:?}"
            );
        }
    }

    /// The decoder must give the operator the real path back, and must not turn a
    /// malformed escape into a guess.
    #[test]
    fn route_fields_round_trip_through_the_percent_encoding() {
        assert_eq!(decode_field("GET%20/orders/42"), "GET /orders/42");
        assert_eq!(decode_field("100%25"), "100%");
        assert_eq!(decode_field("a%09b"), "a\tb");
        assert_eq!(decode_field("/plain/path"), "/plain/path");
        // A truncated or non-hex escape is left as written rather than invented.
        assert_eq!(decode_field("half%2"), "half%2");
        assert_eq!(decode_field("bad%zz"), "bad%zz");
    }

    /// Nearest rank, so every percentile is a duration some request actually took.
    ///
    /// Interpolating would invent a number nobody experienced — the wrong trade when
    /// the inputs are exact. The edges matter more than the middle: p0 and p100 are
    /// where an off-by-one silently reports the wrong request as the slow one.
    #[test]
    fn percentiles_are_nearest_rank_and_never_interpolate() {
        let sorted: Vec<u64> = (1..=10).map(|n| n * 10).collect(); // 10..100
        assert_eq!(percentile(&sorted, 50.0), 50);
        assert_eq!(percentile(&sorted, 90.0), 90);
        assert_eq!(percentile(&sorted, 95.0), 100);
        assert_eq!(percentile(&sorted, 99.0), 100);
        // Never below the first element, never past the last, never a value that
        // is not in the input.
        assert_eq!(percentile(&sorted, 0.0), 10);
        assert_eq!(percentile(&sorted, 100.0), 100);
        assert_eq!(percentile(&[], 95.0), 0, "an empty set has no percentile");
        assert_eq!(percentile(&[42], 99.0), 42, "one sample is every percentile");
    }

    /// A rate needs a window, and the summary must refuse to invent one.
    #[test]
    fn the_rate_is_omitted_when_there_is_no_window_to_divide_by() {
        // Undated slices: no window at all.
        let undated = [slice_of("api", 1_000_000, None, "s1"), slice_of("api", 2_000_000, None, "s2")];
        assert!(service_stats(&undated)[0].rps.is_none(), "no timestamps, no rate");

        // Dated but instantaneous: dividing by zero would print `inf`.
        let same_instant = [
            slice_of("api", 0, Some(1_000_000), "s1"),
            slice_of("api", 0, Some(1_000_000), "s2"),
        ];
        assert!(service_stats(&same_instant)[0].rps.is_none(), "zero window, no rate");

        // A real window: two requests over 1s of wall clock.
        let spread = [
            slice_of("api", 0, Some(1_000_000), "s1"),
            slice_of("api", 0, Some(2_000_000), "s2"),
        ];
        let rps = service_stats(&spread)[0].rps.expect("a window exists");
        assert!((rps - 2.0).abs() < 0.01, "expected 2 req/s, got {rps}");
    }

    /// Below 20 requests the upper percentiles ARE the slowest requests, and saying
    /// "p99" without saying so dresses a maximum up as a distribution.
    #[test]
    fn a_thin_sample_says_its_percentiles_are_not_a_distribution() {
        let thin: Vec<Slice> = (0..5)
            .map(|i| slice_of("api", 1_000_000 * (i + 1), None, &format!("s{i}")))
            .collect();
        let note = "not a distribution";
        assert!(service_summary(&thin).contains(note), "{}", service_summary(&thin));

        let thick: Vec<Slice> = (0..25)
            .map(|i| slice_of("api", 1_000_000 * (i + 1), None, &format!("s{i}")))
            .collect();
        assert!(!service_summary(&thick).contains(note));
        // And one service being thin must flag the report even if another is not.
        let mut mixed: Vec<Slice> = (0..25)
            .map(|i| slice_of("api", 1_000_000 * (i + 1), None, &format!("s{i}")))
            .collect();
        mixed.extend((0..3).map(|i| slice_of("edge", 5_000_000, None, &format!("e{i}"))));
        assert!(service_summary(&mixed).contains(note), "a thin service must flag the report");
    }

    #[test]
    fn slices_split_on_the_trace_line_and_ignore_other_log_output() {
        let log = format!(
            "starting up\n{}{}",
            slice_log("a", 10, "t1", "s1", "-"),
            slice_log("a", 20, "t1", "s2", "-")
        );
        let slices = split_slices(&log);
        assert_eq!(slices.len(), 2, "one slice per trace line");
        assert!(slices[0].contains("incl_ns=10"), "{:?}", slices[0]);
        assert!(slices[1].contains("incl_ns=20"), "{:?}", slices[1]);
        // Text before the first trace line belongs to no slice.
        assert!(!slices[0].contains("starting up"));
        // A log with no slices yields none rather than one bogus entry.
        assert!(split_slices("just a log line\n").is_empty());
    }

    #[test]
    fn stitch_nests_spans_by_parent_and_keeps_orphans_as_roots() {
        let mk = |service: &str, chunk: String| Slice {
            service: service.to_string(),
            graph: parse_instrument_dump(&chunk),
        };
        let slices = vec![
            mk("gateway", slice_log("handle", 1_000, "tr", "aaaa", "-")),
            mk("inventory", slice_log("stock", 400, "tr", "bbbb", "aaaa")),
            // Parent never appears in the logs (its service was not collected):
            // it must still render, as a root, not vanish.
            mk("billing", slice_log("charge", 700, "tr", "cccc", "zzzz")),
        ];
        let out = stitch_report(&slices);
        assert!(out.contains("trace tr"), "{out}");
        assert!(out.contains("● gateway"), "root is un-indented: {out}");
        assert!(out.contains("  └─ inventory"), "child is nested: {out}");
        assert!(out.contains("● billing"), "orphan survives as a root: {out}");
        assert!(out.contains("1 trace(s) over 3 slice(s)"), "{out}");
    }

    #[test]
    fn line_attribution_sums_weights_and_drops_unresolvable_addresses() {
        // Two addresses on the same line accumulate; one address the dSYM could
        // not place is excluded from the counts AND from the denominator.
        let leaves = vec![
            (0x1000u64, 40u64),
            (0x1008u64, 20u64),
            (0x1000u64, 10u64),
            (0xdeadu64, 99u64), // unresolvable
        ];
        let mut resolved = HashMap::new();
        resolved.insert(0x1000u64, 7u32);
        resolved.insert(0x1008u64, 12u32);
        let (hits, total) = attribute_lines(&leaves, &resolved);
        assert_eq!(hits.get(&7).copied(), Some(50), "both hits on line 7 sum");
        assert_eq!(hits.get(&12).copied(), Some(20));
        assert_eq!(total, 70, "the unplaceable sample is not counted anywhere");
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn parses_instrument_query_lines_hottest_first() {
        let dump = "\
elephc-instr: get_user calls=200 incl_ns=100 excl_ns=10 incl_allocs=0 excl_allocs=0 incl_io=200 excl_io=0
elephc-instr-query: 200 SELECT name FROM users WHERE id = ?
elephc-instr-query: 1 CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)
elephc-instr-query: 200 INSERT INTO users (name) VALUES (?)
";
        let graph = parse_instrument_dump(dump);
        assert_eq!(graph.queries.len(), 3);
        // The count and the full remainder of the line (with spaces) are kept.
        assert!(graph
            .queries
            .contains(&("SELECT name FROM users WHERE id = ?".to_string(), 200)));
        assert!(graph.queries.contains(&(
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)".to_string(),
            1
        )));
        assert!(graph
            .queries
            .contains(&("INSERT INTO users (name) VALUES (?)".to_string(), 200)));
    }

    #[test]
    fn demangles_php_symbols() {
        assert_eq!(demangle("main"), "{main}");
        assert_eq!(demangle("fn_hot_u_leaf"), "hot_leaf");
        assert_eq!(demangle("method_Engine_step"), "Engine::step");
        // `_u_` inside the class name survives the class/method split.
        assert_eq!(demangle("method_My_u_Class_run"), "My_Class::run");
        assert_eq!(demangle("_rt_heap_alloc"), "_rt_heap_alloc");
    }

    #[test]
    fn causes_translate_helpers_and_ignore_php() {
        assert_eq!(cause_for("_rt_mixed_from_value"), Some("Mixed cell boxing"));
        assert_eq!(cause_for("_rt_heap_alloc"), Some("heap allocation"));
        assert_eq!(cause_for("_rt_mixed_cast_int"), Some("Mixed cell unboxing"));
        assert_eq!(cause_for("_rt_something_new"), Some("runtime helper"));
        assert_eq!(cause_for("fn_hot_u_leaf"), None);
        assert_eq!(cause_for("main"), None);
    }

    #[test]
    fn why_table_attributes_causes_to_the_php_owner() {
        let rows = parse_call_graph(REPORT);
        let samples = build_samples(&rows);
        let table = why_table(&render_stacks(&samples), 1);
        assert!(table.contains("samples: 100"));
        assert!(table.contains("{main}"));
        assert!(table.contains("100.0%"), "{table}");
        assert!(table.contains("hot_leaf"));
        assert!(table.contains(" 55.0%"), "{table}");
        assert!(table.contains("Engine::step"));
        // heap_alloc's 20 self samples surface as a cause under hot_leaf,
        // rendered with a proportion bar.
        assert!(table.contains("heap allocation"));
        assert!(table.contains(" 20.0%"), "{table}");
        assert!(table.contains("Mixed cell boxing"));
        assert!(table.contains(" 10.0%"), "{table}");
        assert!(table.contains('█') && table.contains('░'), "{table}");
    }

    /// The terminal waterfall must separate hops that ran one after the other from
    /// hops that overlapped — the whole point of placing them.
    #[test]
    fn placed_bars_start_where_the_span_started() {
        // Flush left, quarter width.
        assert_eq!(placed_bar(0.0, 25.0, 12), "███░░░░░░░░░");
        // Same duration, but starting halfway through the trace.
        assert_eq!(placed_bar(50.0, 25.0, 12), "······███░░░");
        // Two concurrent hops share an offset, so they line up.
        assert_eq!(placed_bar(25.0, 25.0, 12), placed_bar(25.0, 25.0, 12));
        // A hop too short to round to a cell still gets one: invisible is worse
        // than imprecise, because a missing bar reads as "did not run".
        assert_eq!(placed_bar(0.0, 0.0, 12), "█░░░░░░░░░░░");
        // A span running to the very end does not overflow its width.
        assert_eq!(placed_bar(75.0, 25.0, 12).chars().count(), 12);
        assert_eq!(placed_bar(100.0, 100.0, 12).chars().count(), 12);
    }

    #[test]
    fn bars_scale_with_the_percentage() {
        assert_eq!(bar(0.0, 10), "░░░░░░░░░░");
        assert_eq!(bar(50.0, 10), "█████░░░░░");
        assert_eq!(bar(100.0, 10), "██████████");
        // Out-of-range input clamps instead of panicking on repeat counts.
        assert_eq!(bar(140.0, 4), "████");
    }

    #[test]
    fn multi_process_note_appears_only_when_merging() {
        let rows = parse_call_graph(REPORT);
        let samples = build_samples(&rows);
        let display = render_stacks(&samples);
        assert!(!why_table(&display, 1).contains("processes"));
        assert!(why_table(&display, 4).contains("samples: 100 · 4 processes"));
    }

    #[test]
    fn live_frame_tracks_trends_between_windows() {
        let rows = parse_call_graph(REPORT);
        let samples = build_samples(&rows);
        let display = render_stacks(&samples);
        let mut cumulative: BTreeMap<Vec<(String, Kind)>, u64> = BTreeMap::new();
        for (stack, weight) in &display {
            *cumulative.entry(stack.clone()).or_default() += weight;
        }
        let mut previous = HashMap::new();
        // First window: no prior data, so no arrow yet.
        let first = live_frame(
            &display,
            &cumulative,
            &mut previous,
            2,
            3,
            std::time::Duration::from_secs(63),
        );
        assert!(first.contains("live · 2 processes · window 3s · total 1m03s"));
        assert!(first.contains("CUMUL"));
        assert!(!first.contains('▲') && !first.contains('▼'), "{first}");
        // Second window identical to the first: flat trends.
        let second = live_frame(
            &display,
            &cumulative,
            &mut previous,
            2,
            3,
            std::time::Duration::from_secs(66),
        );
        assert!(second.contains('─'), "{second}");
    }

    #[test]
    fn decl_ranges_cover_functions_and_methods() {
        let source = "<?php
function hot_leaf(int $n): int {
    return $n;
}

class Engine {
    public function step(): int {
        return 1;
    }
}

function call_hot(int $n): int {
    return hot_leaf($n);
}

echo call_hot(1);
";
        let ranges = php_decl_ranges(source);
        assert_eq!(
            ranges,
            vec![
                DeclRange { name: "hot_leaf".into(), start: 2, end: 4 },
                DeclRange { name: "Engine::step".into(), start: 7, end: 9 },
                DeclRange { name: "call_hot".into(), start: 12, end: 14 },
            ]
        );
    }

    #[test]
    fn inlined_frame_is_injected_from_a_foreign_line() {
        // main sampled on a line owned by call_hot's range grows a virtual frame.
        let mut samples = vec![(
            vec![Frame {
                symbol: "main".to_string(),
                address: Some(0x100000010),
                inlined: false,
            }],
            60u64,
        )];
        let ranges = vec![DeclRange {
            name: "call_hot".to_string(),
            start: 32,
            end: 34,
        }];
        let mut lines = HashMap::new();
        lines.insert(0x100000010u64, 33u32);
        // Inline the core of inject_inlined_frames without atos: rewrite manually.
        for (stack, _) in samples.iter_mut() {
            let mut rewritten = Vec::new();
            for frame in stack.iter() {
                rewritten.push(frame.clone());
                if let Some(line) = frame.address.and_then(|a| lines.get(&a)) {
                    if let Some(owner) = ranges
                        .iter()
                        .find(|range| range.start <= *line && *line <= range.end)
                    {
                        if owner.name != demangle(&frame.symbol) {
                            rewritten.push(Frame {
                                symbol: format!("inlined:{}", owner.name),
                                address: None,
                                inlined: true,
                            });
                        }
                    }
                }
            }
            *stack = rewritten;
        }
        let display = render_stacks(&samples);
        assert_eq!(display[0].0[1].0, "call_hot (inlined)");
        assert_eq!(display[0].0[1].1, Kind::PhpInlined);
        let table = why_table(&display, 1);
        assert!(table.contains("call_hot (inlined)"), "{table}");
        assert!(table.contains("100.0%"), "{table}");
    }

    fn instr_graph() -> crate::call_graph::CallGraph {
        use crate::call_graph::{CallGraph, GraphEdge, GraphNode};
        let node = |name: &str, incl, excl, calls, ai, ae| GraphNode {
            name: name.into(),
            inclusive: incl,
            exclusive: excl,
            call_count: Some(calls),
            alloc_inclusive: ai,
            alloc_exclusive: ae,
            io_inclusive: 0,
            io_exclusive: 0,
            retained_inclusive: 0,
            retained_exclusive: 0,
            wait_inclusive: 0,
            wait_exclusive: 0,
            causes: vec![],
        };
        CallGraph {
            nodes: vec![
                node("run", 1_000_000, 1000, 1, 500, 5),
                node("leaf", 990_000, 990_000, 1200, 495, 495),
            ],
            edges: vec![GraphEdge { from: 0, to: 1, weight: 990_000, count: Some(1200) }],
            total: 1_000_000,
            queries: Vec::new(),
            lines: None,
            trace: None,
        }
    }

    #[test]
    fn parses_and_evaluates_assertions() {
        assert_eq!(
            parse_assert("calls:leaf<=1000"),
            Some(("calls".into(), "leaf".into(), "<=".into(), 1000.0))
        );
        assert_eq!(parse_assert("bogus"), None);
        let graph = instr_graph();
        let run = |specs: &[&str]| {
            let owned: Vec<(String, Option<String>)> =
                specs.iter().map(|s| ((*s).to_string(), None)).collect();
            assert_report(&evaluate_asserts(&graph, &owned))
        };
        // leaf has 1200 calls: <=1000 FAILS, <=2000 PASSES.
        let (report, ok) = run(&["calls:leaf<=1000"]);
        assert!(!ok, "{report}");
        assert!(report.contains("[FAIL] calls:leaf<=1000 (actual 1200)"), "{report}");
        assert!(report.contains("0 passed, 1 failed"), "{report}");
        let (report, ok) = run(&["calls:leaf<=2000", "time_pct:run>=90"]);
        assert!(ok, "{report}");
        assert!(report.contains("2 passed, 0 failed"), "{report}");
        // A function the run never reached is reported as unevaluated, not as a
        // failure of the code: the budget names something that is not there.
        let (report, ok) = run(&["calls:ghost<1"]);
        assert!(!ok, "{report}");
        assert!(report.contains("[SKIP]") && report.contains("'ghost' never ran"), "{report}");
        // A typo in the metric names the ones that exist.
        let (report, ok) = run(&["bogons:leaf<1"]);
        assert!(!ok, "{report}");
        assert!(report.contains("unknown metric 'bogons'") && report.contains("retained"), "{report}");
    }

    /// Verifies `*` asserts on the whole run: self values sum across functions,
    /// while a time metric comes from the root rather than summing inclusives.
    #[test]
    fn run_total_assertions_use_the_star_target() {
        let graph = instr_graph();
        let run = |specs: &[&str]| {
            let owned: Vec<(String, Option<String>)> =
                specs.iter().map(|s| ((*s).to_string(), None)).collect();
            assert_report(&evaluate_asserts(&graph, &owned))
        };
        // run=1 call + leaf=1200 calls.
        let (report, ok) = run(&["calls:*<=1201"]);
        assert!(ok, "{report}");
        let (report, ok) = run(&["calls:*<=1200"]);
        assert!(!ok, "{report}");
        assert!(report.contains("(actual 1201)"), "{report}");
        // The whole run is 100% of itself, and its wall clock is the root's.
        let (report, ok) = run(&["time_pct:*<=100", "incl_ms:*<=1"]);
        assert!(ok, "{report}");
    }

    /// Verifies `.elephc` is found by walking up from the SOURCE, so one budget
    /// at a project root covers files nested under it, and the directory the
    /// profiler happens to run from cannot change which budget applies.
    #[test]
    fn project_file_is_found_by_walking_up_from_the_source() {
        let root = std::env::temp_dir().join(format!(
            "elephc_projectfile_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let nested = root.join("src/deep");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&nested).expect("create nested dirs");
        let source = nested.join("thing.php");
        std::fs::write(&source, "<?php").expect("write source");

        // No file anywhere above: nothing is claimed.
        assert!(find_project_file(source.to_str().expect("utf-8 path")).is_none());

        // One at the project root is found from two levels down.
        let at_root = root.join(PROJECT_FILE_NAME);
        std::fs::write(&at_root, "calls:f<=1\n").expect("write project file");
        let found = find_project_file(source.to_str().expect("utf-8 path")).expect("found");
        assert_eq!(found.canonicalize().ok(), at_root.canonicalize().ok());

        // A nearer one wins over the root: the closest budget describes the code.
        let nearer = nested.join(PROJECT_FILE_NAME);
        std::fs::write(&nearer, "calls:g<=2\n").expect("write nearer file");
        let found = find_project_file(source.to_str().expect("utf-8 path")).expect("found");
        assert_eq!(found.canonicalize().ok(), nearer.canonicalize().ok());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Verifies the budget file: one assertion per line, `#` comments dropped,
    /// and a trailing `#` kept as that assertion's label.
    #[test]
    fn assert_file_parses_specs_comments_and_labels() {
        let parsed = parse_assert_file(
            "# performance budget\n\ncalls:PDO::prepare <= 10   # once per request\n\
             queries:* <= 50\n   \n# trailing comment\n",
        );
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].0, "calls:PDO::prepare <= 10");
        assert_eq!(parsed[0].1.as_deref(), Some("once per request"));
        assert_eq!(parsed[1].0, "queries:* <= 50");
        assert_eq!(parsed[1].1, None);
    }

    #[test]
    fn recommends_the_hotspot() {
        let out = instrument_recommendations(&instr_graph());
        assert!(out.contains("leaf is the hotspot"), "{out}");
    }
}
