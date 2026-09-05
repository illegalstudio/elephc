//! Purpose:
//! Profiles a program `monitor` launches — a `.php` source (built first) or a
//! binary. This path records exact instrumented calls, wall time, allocations,
//! retained objects, database queries, and database-driver wait; it does not
//! claim operating-system CPU time or file-I/O events.
//!
//! Called from:
//! - `monitor::main`, when the target is a file rather than an address.
//!
//! Key details:
//! - A binary without the capability is refused, not profiled approximately.
//! - The control channel is established before the spawn; the program reads it
//!   during its own init.

use super::attach::Image;
use super::*;

/// One sampling window over the whole process tree, rendered once: the bar
/// table on stdout, the Speedscope file, and the CI summary when applicable.
pub(crate) fn run_once(
    cmd: &MonitorCommand,
    root: u32,
    binary: Option<&Path>,
    php_source: Option<&Path>,
    image: Option<&Image>,
) -> i32 {
    let pids = discover_pids(root);
    let window = match capture_display(&pids, cmd.duration_secs, binary, php_source, image) {
        Ok(Some(window)) => window,
        // Said plainly, and never as "it may have exited": the target was there,
        // the kernel would not let this process read it, and the message carries
        // the setting to change.
        Err(reason) => {
            eprintln!("elephc monitor: {reason}");
            return 1;
        }
        Ok(None) => {
            eprintln!(
                "elephc monitor: no samples captured — the program may have exited before \
                 sampling started; try a longer-running input"
            );
            return 1;
        }
    };
    let display = window.display;
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
    let lines = match (binary, php_source, &window.source) {
        (Some(binary), Some(source), Some((reports, samples))) => reports
            .iter()
            .find_map(|report| line_profile(samples, report, binary, source)),
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

/// How a live view ended, and what that means for the program it was watching.
///
/// `--live` normally owns its target's lifetime: it launched the program so the
/// operator could watch it, and leaving it behind when the view ends would be
/// its own surprise. Losing the CHANNEL is the exception. That is this tool's
/// plumbing failing, not the program's, and killing a running program because
/// our own socket stopped answering destroys work the operator did not ask us to
/// end — it just tells them which pid is still theirs to stop.
pub(crate) struct LiveOutcome {
    pub(crate) code: i32,
    pub(crate) leave_target_running: bool,
}

/// The live loop: sample a window, merge the process tree, redraw, repeat
/// until the target goes away. Prints the cumulative table on exit.
pub(crate) fn run_live(
    cmd: &MonitorCommand,
    root: u32,
    mut child: Option<&mut process::Child>,
    channel: Option<&ControlChannel>,
    image: Option<&Image>,
) -> LiveOutcome {
    use std::io::IsTerminal;
    let interactive = std::io::stdout().is_terminal();
    let started = std::time::Instant::now();
    let mut cumulative: BTreeMap<Vec<(String, Kind)>, u64> = BTreeMap::new();
    // The previous snapshot, when the target answers over the control channel.
    // Windows are differences between snapshots, so the first one is measured
    // against nothing and reports everything sampled since the program started.
    let mut sampled_before: BTreeMap<Vec<(String, Kind)>, u64> = BTreeMap::new();
    // Whether the child's activation ACK has been taken off the channel. One
    // byte-sequence, sent once; leaving it there would put it in front of the
    // first snapshot reply.
    let mut activated = false;
    // Whether a late window has already been mentioned.
    let mut reported_late = false;
    // Whether the view ended because the channel broke rather than because the
    // program did.
    let mut lost_channel = false;
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
        // A target this process launched can simply be ASKED — it holds the
        // other end of a socketpair nothing else on the machine can open. Only
        // a foreign process needs to be read from the outside, and that is the
        // one tool that ships on macOS alone.
        let display = if let Some(channel) = channel {
            std::thread::sleep(std::time::Duration::from_secs(u64::from(cmd.duration_secs)));
            // The child's activation ACK is sent once, at init, and sits in the
            // buffer until somebody takes it. It has to come off BEFORE the first
            // snapshot reply, because this reader is length-prefixed: it would
            // otherwise read `ELEP` as a length, refuse it, and the loop would
            // read that as a dead channel and stop after one window.
            //
            // Waited for rather than merely attempted. A non-blocking look
            // succeeds whenever the child booted inside the first window, which
            // is every run on an idle machine and not every run under load.
            if !activated {
                activated = await_activation(channel, std::time::Duration::from_secs(2));
            }
            let snapshot = match request_snapshot(channel) {
                Snapshot::Answered(text) => {
                    // An answer proves the child is past init, whether or not
                    // `await_activation` was the one that saw the ACK — it may
                    // have been consumed by the snapshot read itself. Without
                    // this, every later window would spend its deadline waiting
                    // for an ACK that has already been taken.
                    activated = true;
                    text
                }
                Snapshot::Late { activation_seen } => {
                    // The read may have taken the ACK off the front before giving
                    // up on the reply behind it. Recording that is what stops
                    // every later window from opening with the full activation
                    // deadline spent waiting for a message already consumed.
                    activated |= activation_seen;
                    // Slow is not gone. Ending the loop here would REAP a healthy
                    // program: the target only outlives the view while the view is
                    // still running, so a window nobody answered in time would
                    // stop the thing being profiled.
                    //
                    // Said once, because a busy target would otherwise bury its
                    // own table under the same line every window.
                    if !reported_late {
                        reported_late = true;
                        eprintln!(
                            "elephc monitor: the target did not answer within the window; \
                             still watching"
                        );
                    }
                    continue;
                }
                Snapshot::Gone => {
                    // The channel is finished. Whether the PROGRAM is finished is
                    // a different question, and the answer decides whether it
                    // gets stopped: this tool's own plumbing failing is not a
                    // reason to end work the operator is in the middle of.
                    if child.as_deref_mut().is_some_and(|c| c.try_wait().ok().flatten().is_none()) {
                        lost_channel = true;
                        eprintln!(
                            "elephc monitor: lost the channel to the target; it is still running \
                             as pid {root} and is left alone. Reporting what was collected."
                        );
                    }
                    break;
                }
            };
            // Snapshots are cumulative, so the window is what this one has that
            // the last did not. A stack that stopped being sampled contributes
            // nothing rather than a negative.
            // Summed, not collected. `folded_text_to_display` can emit the same
            // display stack twice — one folded line becomes several stacks when
            // it carries a native leaf — and `collect` into a map keeps the LAST
            // of a pair instead of their total, which silently loses samples
            // from the window and from every window after it.
            let mut total: BTreeMap<Vec<(String, Kind)>, u64> = BTreeMap::new();
            for (stack, count) in folded_text_to_display(&snapshot) {
                *total.entry(stack).or_default() += count;
            }
            let window: Vec<(Vec<(String, Kind)>, u64)> = total
                .iter()
                .filter_map(|(stack, count)| {
                    let before = sampled_before.get(stack).copied().unwrap_or(0);
                    count.checked_sub(before).filter(|delta| *delta > 0).map(|delta| (stack.clone(), delta))
                })
                .collect();
            sampled_before = total;
            windows += 1;
            window
        } else {
            let window = match capture_display(&pids, cmd.duration_secs, None, None, image) {
                Ok(Some(window)) => window,
                // Attach mode has no child handle: an empty window is how we
                // learn the target is gone.
                Ok(None) => break,
                // A refusal is not a target that ended. Ending the view silently
                // here is what made `yama/ptrace_scope=1` look like a program
                // that had exited.
                Err(reason) => {
                    eprintln!("elephc monitor: {reason}");
                    break;
                }
            };
            windows += 1;
            window.display
        };
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
    } else {
        // A program shorter than one window left NOTHING behind: the loop sleeps
        // first and asks second, so it was already gone by the first question,
        // and the exit dump it wrote to its own stderr is filtered out of a live
        // view on purpose. Silence and a zero exit reads as a broken profiler
        // rather than as a program that finished, so say which it was.
        eprintln!(
            "elephc monitor: the program finished before the first window; there was nothing \
             to sample. Use a longer-running input, or a shorter --duration."
        );
    }
    LiveOutcome { code: 0, leave_target_running: lost_channel }
}

/// One window of samples, already named and folded, whichever way it was read.
///
/// Two ways exist and they meet here. A process this tool can ask — anything
/// launched or reachable through its endpoint — answers for itself. A process it
/// can only WATCH answers for nothing, and has to be read from the outside:
/// through `/usr/bin/sample` on macOS, and by stopping it with `ptrace` on
/// Linux. `image` is what says which: it exists only for `--attach`, and only
/// where reading a process from the outside is this tool's own job.
///
/// `None` means the window is empty, which is the same thing both ways: no
/// samples landed, and for `--attach` that is how the target's disappearance is
/// noticed at all.
pub(crate) struct Window {
    /// What every consumer downstream reads: named stacks and their weights.
    pub(crate) display: Vec<(Vec<(String, Kind)>, u64)>,
    /// The text a sampler produced, and the frames parsed out of it.
    ///
    /// Kept only because per-line attribution needs both again, and re-deriving
    /// them from `display` is impossible — a name is not an address. `None` for
    /// a window this tool sampled itself, which never had text to begin with.
    pub(crate) source: Option<(Vec<String>, Vec<(Vec<Frame>, u64)>)>,
}

/// One window, and THREE answers rather than two.
///
/// `Err` is "this target cannot be read at all", which is not the same as the
/// `Ok(None)` that means "read it, saw nothing". Attach treats an empty window
/// as proof the target has gone, so folding a refusal into it told operators
/// their still-running program had exited and swallowed the one line that named
/// what to change.
fn capture_display(
    pids: &[u32],
    duration_secs: u32,
    binary: Option<&Path>,
    php_source: Option<&Path>,
    image: Option<&Image>,
) -> Result<Option<Window>, String> {
    #[cfg(target_os = "linux")]
    if let Some(image) = image {
        let display = super::ptrace::attach_window(pids, duration_secs, image)?;
        return Ok((!display.is_empty()).then_some(Window { display, source: None }));
    }
    // Bound so the macOS build does not warn on an argument only Linux reads.
    let _ = &image;
    let reports = capture_window(pids, duration_secs);
    let Some(samples) = samples_from_reports(&reports, binary, php_source) else {
        return Ok(None);
    };
    Ok(Some(Window { display: render_stacks(&samples), source: Some((reports, samples)) }))
}

/// Samples every pid of one window in parallel and returns the reports that
/// succeeded — a worker dying mid-window degrades coverage, never the run.
pub(crate) fn capture_window(pids: &[u32], duration_secs: u32) -> Vec<String> {
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
pub(crate) fn spawnable_path(path: &str) -> PathBuf {
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

/// Explains an empty capture by what the run actually did.
///
/// One message covered every cause: "was the target built with
/// --with-monitoring". For a program that crashed after printing its output —
/// which is what a CI shard showed, on one architecture only — that is a
/// confident diagnosis of the wrong thing, and it sent the investigation at the
/// build flags for an hour. The exit status was there the whole time and nobody
/// looked at it.
pub(crate) fn no_profile_reason(
    status: &process::ExitStatus,
    binary: &Path,
    capture_activated: bool,
) -> String {
    use std::os::unix::process::ExitStatusExt as _;
    if let Some(signal) = status.signal() {
        return format!(
            "{} was killed by signal {signal} before the active capture window could close \
             and publish its profile",
            binary.display()
        );
    }
    match status.code() {
        Some(code) if code != 0 => format!(
            "{} exited with status {code} before the active capture window could close and \
             publish its profile",
            binary.display()
        ),
        _ if !capture_activated => format!(
            "the exact control channel for {} was unavailable or was not acknowledged, so no \
             capture window was activated",
            binary.display()
        ),
        Some(0) | None => format!(
            "{} acknowledged monitoring and exited cleanly, but published no instrumented \
             frames; this is expected only when selective instrumentation selected no function \
             that ran. A full capture should always publish {{main}}, so otherwise its active \
             window did not close or publish correctly",
            binary.display()
        ),
        Some(_) => unreachable!("non-zero statuses were handled above"),
    }
}

/// Reads a target that carries the monitoring capability exactly: run it, and
/// render the profile it prints to stderr — the deterministic counterpart to
/// sampling. Honors `--dot` / `--html`.
pub(crate) fn run_instrument(cmd: &MonitorCommand) -> i32 {
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
    let capture_activated = channel.as_ref().is_some_and(control_channel_activated);
    // Pass through the program's own diagnostics — and only those. A
    // `--with-monitoring` binary carries both mechanisms, so its stderr also
    // holds the sampler's folded stacks; forwarding those would print raw
    // profiler output as if the program had written it.
    for line in stderr.lines() {
        if !is_profiler_line(line) {
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
        eprintln!(
            "elephc monitor: {}",
            no_profile_reason(&output.status, &binary, capture_activated)
        );
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

/// Renders one live frame: the window's hot functions with trend arrows
/// against the previous window, and the cumulative share on the right.
pub(crate) fn live_frame(
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
