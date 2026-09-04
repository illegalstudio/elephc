//! Purpose:
//! Parses `elephc monitor`'s arguments into a `MonitorCommand`, and carries the
//! help text. Combinations that cannot be honoured are refused here rather than
//! discovered later — an accepted flag that writes nothing is worse than an error.
//!
//! Called from:
//! - `monitor::main`, on the raw argument list.
//!
//! Key details:
//! - `--exact` cannot be combined with the exporters: an exact remote answer is
//!   the per-function table, and the exports render the sampled capture.
//! - `--serve` needs `--live` and `--html`, since it serves the live graph.

use super::*;

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
    /// Ask a running service for its next EXACT slice rather than the sampled
    /// ring. Only a `--web` service brackets requests, so this has an answer
    /// there and reports its absence anywhere else.
    pub exact: bool,
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

A program monitor LAUNCHES (a source or a binary) is measured from inside: an
exact per-function profile rooted at {main} — wall time, call counts,
allocations, retained objects, SQL query counts, database-driver wait, outgoing
network operations and network wait. The display derives
wall-minus-recorded-DB-wait; it is not an OS CPU clock. File I/O is not counted
or timed. Every local export is available.

A service monitor CONNECTS to answers from its sample ring: CPU-time shares that
sharpen as samples accumulate, sampled allocation attribution, and per-route
stacks. The combined --with-monitoring build has no sampled SQL/wait summary and
the CPU timer cannot see blocked wall time. Add --exact for the measured
per-function table of the next request that completes; that answer is the table
alone, so --exact cannot be combined with --out, --pprof, --dot or --html.

--live refreshes the table window after window, top-style, until the target
exits (or Ctrl-C). --attach reads an already-running local process; child worker
processes (a --web prefork server's workers) are discovered and merged in both
modes. Those two use an external sampler, which only macOS ships; elsewhere,
point monitor at the program's endpoint instead. They report sampled CPU stacks
only: no calls, allocation/retained counts, SQL/file-I/O counts, wait, or route
tags. Everything else works on Linux and macOS alike.

--pprof writes the capture as a gzip pprof profile. --dot writes a Graphviz call
graph (`dot -Tsvg`); --html writes a self-contained, interactive call graph (one
node per function, caller->callee edges, hover metrics, search, zoom) that opens
in any browser with no network, with time, memory, retained, DB wait, SQL,
network and network-wait dimensions, a
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
allocs, retained, queries, self_ms, incl_ms, wait_ms, network,
network_wait_ms, time_pct. Use `*` as the
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
per span (service, exact duration, function count, queries, DB waiting, network
operations and network waiting). A span
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
    let mut exact = false;
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
            "--exact" => {
                exact = true;
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
                // Rejected here rather than let through as a number, because
                // "nan" and "inf" parse: a NaN threshold makes every
                // `growth > threshold` false, so the gate a pipeline believes it
                // set never trips, and the run exits 0 through any regression.
                // A parameterised threshold that arrives empty or misspelled is
                // exactly how that happens.
                let threshold: f64 = value
                    .parse()
                    .ok()
                    .filter(|parsed: &f64| parsed.is_finite())
                    .ok_or_else(|| format!("invalid --fail-on-regression '{value}'"))?;
                fail_on_regression = Some(threshold);
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
    // The exporters read the sampled capture, and `--exact` against a service
    // returns the per-function table only. This used to warn and exit 0, which
    // told automation it had an artifact it did not have.
    if exact {
        let unwritable: Vec<&str> = [
            ("--out", out.is_some()),
            ("--pprof", pprof_out.is_some()),
            ("--dot", dot_out.is_some()),
            ("--html", html_out.is_some()),
        ]
        .iter()
        .filter(|(_, given)| *given)
        .map(|(name, _)| *name)
        .collect();
        if !unwritable.is_empty() {
            return Err(format!(
                "--exact cannot be combined with {}: the exports are rendered from the \
                 sampled capture, and an exact remote answer is the per-function table \
                 only. Drop --exact to export the sampled view, or profile the program \
                 locally, where an exact capture does export.",
                unwritable.join(", ")
            ));
        }
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
        exact,
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

/// Parses `<metric>:<fn><op><value>` into its parts. `op` is one of `<= >= == < >`.
pub(crate) fn parse_assert(spec: &str) -> Option<(String, String, String, f64)> {
    let (metric, rest) = spec.split_once(':')?;
    for op in ["<=", ">=", "==", "<", ">"] {
        if let Some(pos) = rest.find(op) {
            let fname = rest[..pos].trim();
            let value = rest[pos + op.len()..].trim();
            let value = value.parse::<f64>().ok()?;
            // `f64::from_str` accepts "nan" and "inf", and a budget of either
            // silently disables the assertion it belongs to: every comparison
            // against NaN is false, so the check reports a failure it did not
            // measure, and `inf` with `<=` passes whatever the program did.
            // A budget has to be a number to be a budget.
            if !value.is_finite() {
                return None;
            }
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
pub(crate) fn parse_assert_file(text: &str) -> Vec<(String, Option<String>)> {
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
pub(crate) fn find_project_file(target: &str) -> Option<PathBuf> {
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
