//! Purpose:
//! The long-running `monitor <service> --apm` collector: polls the service once
//! a second, keeps the history, writes the store, checks budgets, captures
//! exact profiles and refreshes the live view until stopped.
//!
//! Called from:
//! - `apm::run`, for a service target.
//!
//! Key details:
//! - SIGINT/SIGTERM stop it in order, writing the windows it holds.
//! - One collector per service and store, enforced by a lock file.

use super::*;

/// Set by SIGINT/SIGTERM so the collector can write its last window and stop.
static STOP: AtomicBool = AtomicBool::new(false);

/// SIGINT/SIGTERM handler: asks the collector to stop after its current poll.
extern "C" fn request_stop(_: libc::c_int) {
    STOP.store(true, Ordering::Relaxed);
}

/// Routes Ctrl-C and SIGTERM to an orderly stop instead of losing the
/// windows not yet written to the store.
fn install_stop_handler() {
    unsafe {
        libc::signal(libc::SIGINT, request_stop as extern "C" fn(libc::c_int) as libc::sighandler_t);
        libc::signal(libc::SIGTERM, request_stop as extern "C" fn(libc::c_int) as libc::sighandler_t);
    }
}

/// Reads a service's transaction snapshot.
pub(super) fn fetch_snapshot(target: &str, key: &[u8; 32]) -> Result<Snapshot, String> {
    let text = crate::monitor::fetch_service_answer(
        target,
        key,
        elephc_probe::endpoint::WANT_TRANSACTIONS,
        false,
    )?;
    apm::decode(&text).map_err(|error| match error {
        apm::DecodeError::Off => format!(
            "{target} counts no transactions: start the service with --apm (its binary must be \
             built with --with-monitoring)"
        ),
        apm::DecodeError::Busy => format!(
            "{target} was resetting its route table while being read; the next poll reads it again"
        ),
        apm::DecodeError::Malformed(reason) => format!(
            "{target} answered something that is not a transaction snapshot ({reason}); is it \
             running an older elephc?"
        ),
    })
}

/// Empties a service's route table (`--reset-routes`, and the page's action)
/// and returns what the service answered.
pub(super) fn reset_service_routes(target: &str, key: &[u8; 32]) -> Result<String, String> {
    crate::monitor::fetch_service_answer(target, key, elephc_probe::endpoint::WANT_RESET_ROUTES, false)
}

/// What a running collector holds besides its source: the history the views
/// read, the windows not yet written to the store, and the live view's clock.
struct Collector<'a> {
    cmd: &'a MonitorCommand,
    shared: Shared,
    /// Windows not yet written, by source: a store record is one source's,
    /// so programs pushing to one collector never share one (review).
    pending: std::collections::BTreeMap<String, Vec<Window>>,
    resolution_ms: u64,
    live_every: Duration,
    last_print: Instant,
    tty: bool,
    /// The store's one-collector lock, held until the collector stops.
    _claim: Option<std::fs::File>,
}

impl<'a> Collector<'a> {
    /// Claims the store for `name`, starts the capture loop (`capture`: a
    /// service to read exact profiles from) and the monitor page.
    fn start(
        cmd: &'a MonitorCommand,
        name: &str,
        capture: Option<(String, [u8; 32])>,
        store: Option<Store>,
    ) -> Option<Self> {
        // One collector per source and store: two would each write the same
        // transactions and every stored count would double.
        let claim = match &store {
            Some(store) => match store.claim_collector(name) {
                Ok(claim) => Some(claim),
                Err(error) => {
                    eprintln!("elephc monitor: {error}");
                    return None;
                }
            },
            None => None,
        };
        if let Some(store) = &store {
            store.prune_now();
        }
        install_stop_handler();
        let mut history = History::new(name, store, true);
        history.capture = capture.clone();
        history.capture_every = cmd.apm_capture_every;
        history.alerts = alerts::Alerts::new(
            cmd.apm_alerts.iter().filter_map(|spec| alerts::parse_rule(spec).ok()).collect(),
        );
        let shared: Shared = Arc::new((Mutex::new(history), Condvar::new()));
        if let (Some((target, key)), true) = (&capture, cmd.apm_capture_every > 0) {
            spawn_capture_loop(target, *key, cmd.apm_capture_every, shared.clone());
            println!(
                "capturing the exact profile of one request every {}s (slowest per route and failures kept)",
                cmd.apm_capture_every
            );
        }
        if let Some(addr) = &cmd.serve {
            match dashboard::serve(addr, shared.clone()) {
                Ok((local, _)) => println!("monitor page on http://{local}/ (Transactions and Profile)"),
                Err(error) => {
                    eprintln!("elephc monitor: cannot serve on {addr}: {error}");
                    return None;
                }
            }
        }
        if let Some(store) = &shared.0.lock().unwrap_or_else(|e| e.into_inner()).store {
            println!("storing history in {} ({} day retention)", store.dir().display(), store.retention_days());
        }
        if let Some(endpoint) = &cmd.otlp {
            super::otlp_export::start(endpoint, name);
        }
        Some(Self {
            cmd,
            shared,
            pending: std::collections::BTreeMap::new(),
            resolution_ms: cmd.apm_resolution_secs.max(1) * 1_000,
            live_every: Duration::from_secs(u64::from(cmd.duration_secs.max(1))),
            last_print: Instant::now(),
            tty: std::io::stdout().is_terminal(),
            _claim: claim,
        })
    }

    /// Takes in one collected window and the snapshot it ends at: the
    /// history, the budgets, and the store once a resolution's worth is held.
    fn absorb(&mut self, window: Window, snapshot: Snapshot) {
        let pending = self.pending.entry(window.source.clone()).or_default();
        pending.push(window.clone());
        let full = pending.iter().fold(0u64, |total, w| total.saturating_add(w.span_ms)) >= self.resolution_ms;
        let (lock, signal) = &*self.shared;
        let mut history = lock.lock().unwrap_or_else(|e| e.into_inner());
        history.last_error = None;
        if history.pushed {
            history.latest_by_source.insert(window.source.clone(), snapshot.clone());
        }
        let deploy = snapshot.build.as_deref().and_then(|build| history.deploys.note(&window.source, build, window.t));
        history.latest = Some(snapshot);
        let source = window.source.clone();
        history.push(window);
        check_alerts(&mut history, self.cmd, store::now_ms(), false);
        let store = history.store.clone();
        let captures = due_captures(&mut history, store::now_ms());
        signal.notify_all();
        // The store is written after the history's lock is let go: the page,
        // `/metrics` and the event streams read that lock, and a slow disk
        // held them all up (review).
        drop(history);
        if full {
            if let Some(windows) = self.pending.remove(&source) {
                write_record(store.as_ref(), &windows);
            }
        }
        let unsaved = write_captures(store.as_ref(), &captures);
        if !unsaved.is_empty() {
            self.shared.0.lock().unwrap_or_else(|e| e.into_inner()).traces.unsaved(&unsaved);
        }
        if let (Some(store), Some(deploy)) = (store.as_ref(), deploy) {
            if let Err(error) = deploys::append(store.dir(), &deploy) {
                eprintln!("elephc monitor: {error}");
            }
        }
    }

    /// Writes the pending windows of every source whose oldest one ended a
    /// resolution or more ago, however little time they span.
    ///
    /// A short script's run spans milliseconds, and each run is a source of
    /// its own: its window never filled a resolution, and reached the store
    /// only when the source was forgotten, ten minutes later — lost if the
    /// collector stopped meanwhile (review).
    fn write_due(&mut self, now_ms: u64) {
        let due: Vec<String> = self
            .pending
            .iter()
            .filter(|(_, windows)| windows.first().is_some_and(|w| w.t.saturating_add(self.resolution_ms) <= now_ms))
            .map(|(source, _)| source.clone())
            .collect();
        if due.is_empty() {
            return;
        }
        let store = self.shared.0.lock().unwrap_or_else(|e| e.into_inner()).store.clone();
        for source in due {
            if let Some(windows) = self.pending.remove(&source) {
                write_record(store.as_ref(), &windows);
            }
        }
    }

    /// Forgets a source that went away long ago: its pending windows go to
    /// the store, and its last snapshot leaves `/metrics`.
    fn forget(&mut self, source: &str) {
        let mut history = self.shared.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(windows) = self.pending.remove(source) {
            write_record(history.store.as_ref(), &windows);
        }
        history.latest_by_source.remove(source);
    }

    /// A read that failed: shown on the page, and the budgets measured over
    /// the time nothing was seen (a throughput floor fires when the source
    /// dies, as it does when its traffic stops).
    fn unreachable(&self, error: String) {
        let (lock, signal) = &*self.shared;
        let mut history = lock.lock().unwrap_or_else(|e| e.into_inner());
        history.last_error = Some(error);
        check_alerts(&mut history, self.cmd, store::now_ms(), true);
        signal.notify_all();
    }

    /// Redraws the `--live` view when it is due.
    fn print_live(&mut self) {
        let cmd = self.cmd;
        if !cmd.live || self.last_print.elapsed() < self.live_every {
            return;
        }
        self.last_print = Instant::now();
        let history = self.shared.0.lock().unwrap_or_else(|e| e.into_inner());
        let span_ms = self.live_every.as_millis() as u64;
        let newest = history.newest_ms().unwrap_or(0);
        let recent: Vec<&Window> = history.windows.iter().filter(|w| w.t + span_ms > newest).collect();
        let measured = measured_ms(&recent);
        let mut table = render_table(&format!("{} — last window", history.target), &recent, measured);
        // Trends over the last minute, a slot every few seconds.
        let minute: Vec<&Window> = history.windows.iter().filter(|w| w.t + 60_000 > newest).collect();
        table.push_str(&terminal::render_trends(&minute, &hottest_routes(&minute)));
        if let Some(route) = &cmd.apm_route {
            table.push_str(&route_histogram(&recent, route));
        }
        if cmd.apm_capture_every > 0 {
            table.push_str(&terminal::render_traces(&history.traces.list()));
        }
        if !history.alerts.alerts.is_empty() {
            table.push_str(&terminal::render_alerts(&history.alerts, cmd.apm_alert_window));
        }
        if self.tty {
            print!("\x1b[2J\x1b[H{table}");
        } else {
            print!("{table}\n");
        }
        let _ = std::io::Write::flush(&mut std::io::stdout());
    }

    /// Writes what has not reached the store yet: a stop is not a crash.
    fn finish(self) -> i32 {
        let mut history = self.shared.0.lock().unwrap_or_else(|e| e.into_inner());
        for windows in self.pending.values() {
            write_record(history.store.as_ref(), windows);
        }
        // The current hour's captures too: the collector stops before it ends.
        let captures = due_captures(&mut history, u64::MAX);
        write_captures(history.store.as_ref(), &captures);
        drop(history);
        super::otlp_export::flush();
        0
    }
}

/// The captures of the hours that are over and not stored yet, copied so the store is
/// written after the history's lock is let go. None without a store: memory keeps them.
fn due_captures(
    history: &mut History,
    now_ms: u64,
) -> Vec<(super::captures::CaptureRecord, crate::call_graph::CallGraph)> {
    if history.store.is_none() {
        return Vec::new();
    }
    let source = history.target.clone();
    history.traces.unwritten_hours(now_ms).into_iter().map(|trace| (trace.record(&source), trace.graph.clone())).collect()
}

/// Writes captures to the store, reporting a failure without stopping the collector, and
/// returns the ids it could not save, for the next poll to retry.
fn write_captures(
    store: Option<&Store>,
    captures: &[(super::captures::CaptureRecord, crate::call_graph::CallGraph)],
) -> Vec<u64> {
    let Some(store) = store else {
        return Vec::new();
    };
    let mut failed = Vec::new();
    for (record, graph) in captures {
        if let Err(error) = store.append_capture(record, graph) {
            eprintln!("elephc monitor: {error}");
            failed.push(record.id);
        }
    }
    failed
}

/// Writes one source's pending windows to the store as one record, and hands the record
/// to the OTLP export when there is one.
fn write_record(store: Option<&Store>, windows: &[Window]) {
    let Some(record) = coalesce(windows) else {
        return;
    };
    super::otlp_export::send(&record);
    if let Some(store) = store {
        if let Err(error) = store.append_collected(&record) {
            eprintln!("elephc monitor: {error}");
        }
    }
}

/// The long-running collector: polls, keeps the history, writes the store,
/// refreshes the live view, and feeds the dashboard until stopped.
pub(super) fn collect(cmd: &MonitorCommand, key: &[u8; 32], first: Snapshot, store: Option<Store>) -> i32 {
    let Some(mut collector) = Collector::start(cmd, &cmd.target, Some((cmd.target.clone(), *key)), store) else {
        return 1;
    };
    let mut previous = first;
    collector.shared.0.lock().unwrap_or_else(|e| e.into_inner()).latest = Some(previous.clone());
    let mut reported_error = false;
    let mut next_poll = Instant::now() + POLL_INTERVAL;
    while !STOP.load(Ordering::Relaxed) {
        // Sleep in short steps so a stop request is honoured within 100 ms.
        let now = Instant::now();
        if now < next_poll {
            std::thread::sleep((next_poll - now).min(Duration::from_millis(100)));
            continue;
        }
        next_poll += POLL_INTERVAL;
        if next_poll < Instant::now() {
            // The machine slept or a poll stalled: re-anchor rather than burst.
            next_poll = Instant::now() + POLL_INTERVAL;
        }
        let snapshot = match fetch_snapshot(&cmd.target, key) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                if !reported_error {
                    eprintln!("elephc monitor: {error} — retrying every second");
                    reported_error = true;
                }
                collector.unreachable(error);
                continue;
            }
        };
        if reported_error {
            eprintln!("elephc monitor: {} answers again", cmd.target);
            reported_error = false;
        }
        let window = collected_window(&previous, &snapshot, &cmd.target, store::now_ms());
        previous = snapshot.clone();
        collector.absorb(window, snapshot);
        collector.print_live();
    }
    collector.finish()
}

/// One snapshot a program pushed: who sent it, and when it arrived.
pub(super) struct Pushed {
    pub(super) source: String,
    pub(super) snapshot: Snapshot,
    pub(super) received_ms: u64,
    /// The first snapshot of a program that had delivered none before: it
    /// carries everything since the program started, all of it new.
    pub(super) from_start: bool,
}

/// The collector of programs that send their counters (`--listen`): built
/// with `--apm-collector`, they connect here — an application on a phone,
/// a library in a host, a program nothing can connect to. Each connection
/// is a source of its own; their windows share the history.
pub(super) fn collect_pushed(cmd: &MonitorCommand, key: &[u8; 32], address: &str, store: Option<Store>) -> i32 {
    let listener = match std::net::TcpListener::bind(address) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("elephc monitor: cannot listen on {address}: {error}");
            return 1;
        }
    };
    let local = listener.local_addr().map_or_else(|_| address.to_string(), |addr| addr.to_string());
    let name = format!("listening on {local}");
    // No service to read exact profiles from, nor one to reset: the page
    // offers neither.
    let Some(mut collector) = Collector::start(cmd, &name, None, store) else {
        return 1;
    };
    collector.shared.0.lock().unwrap_or_else(|e| e.into_inner()).pushed = true;
    println!(
        "listening on {local} for programs built with --apm-collector (build {})",
        elephc_probe::handshake::fingerprint(key)
    );
    let (sender, receiver) = std::sync::mpsc::channel::<Pushed>();
    spawn_listener(listener, *key, sender);
    // Each source's last snapshot and when it came: a window is what it did
    // since, and a source silent past the reconnect window is forgotten.
    let mut previous: std::collections::HashMap<String, (Snapshot, Instant)> = std::collections::HashMap::new();
    let mut last_received = Instant::now();
    let mut next_tick = Instant::now() + POLL_INTERVAL;
    while !STOP.load(Ordering::Relaxed) {
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(pushed) => {
                last_received = Instant::now();
                let before = previous.insert(pushed.source.clone(), (pushed.snapshot.clone(), Instant::now()));
                if let Some(window) = pushed_window(before.as_ref().map(|(snapshot, _)| snapshot), &pushed) {
                    collector.absorb(window, pushed.snapshot);
                }
                if before.is_none() {
                    println!("{} connected", pushed.source);
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if Instant::now() >= next_tick {
            next_tick = Instant::now() + POLL_INTERVAL;
            // Budgets on a clock, as the polling collector checks them: once
            // nothing has been sent for three sends' time, the silence is
            // time in which no transaction was seen, so a throughput floor
            // fires when the last consumer dies (review).
            let silent = last_received.elapsed();
            if silent > elephc_probe::push::INTERVAL * 3 {
                collector.unreachable(format!("no program has sent its counters for {}s", silent.as_secs()));
            }
            for source in expired_sources(&previous, Instant::now()) {
                previous.remove(&source);
                collector.forget(&source);
            }
            collector.write_due(store::now_ms());
        }
        collector.print_live();
    }
    collector.finish()
}

/// How long a silent source is remembered: a program that reconnects within
/// it continues where it left off; past it, its last snapshot is dropped, so
/// a collector receiving a short job per pid does not grow forever (review).
pub(super) const SOURCE_FORGOTTEN_AFTER: Duration = Duration::from_secs(600);

/// The sources silent for longer than `SOURCE_FORGOTTEN_AFTER` at `now`.
pub(super) fn expired_sources(
    previous: &std::collections::HashMap<String, (Snapshot, Instant)>,
    now: Instant,
) -> Vec<String> {
    previous
        .iter()
        .filter(|(_, (_, seen))| now.saturating_duration_since(*seen) > SOURCE_FORGOTTEN_AFTER)
        .map(|(source, _)| source.clone())
        .collect()
}

/// The window a pushed snapshot adds, given the source's previous one.
///
/// After a previous snapshot, what happened since. A source's first one is
/// counted whole only when the program had delivered nothing before — a
/// short run's single snapshot at exit is all of it; otherwise it is where
/// counting starts here, since a collector that ran before this one may have
/// stored the rest (as with a service a collector starts reading).
pub(super) fn pushed_window(before: Option<&Snapshot>, pushed: &Pushed) -> Option<Window> {
    match before {
        Some(before) => Some(collected_window(before, &pushed.snapshot, &pushed.source, pushed.received_ms)),
        None if pushed.from_start => {
            let mut window = Window::since_start(&pushed.snapshot);
            window.source = pushed.source.clone();
            window.t = pushed.received_ms;
            Some(window)
        }
        None => None,
    }
}

/// Accepts programs on their own threads: each proves the build key, says
/// who it is, then sends a snapshot every few seconds until it goes away.
fn spawn_listener(listener: std::net::TcpListener, key: [u8; 32], sender: std::sync::mpsc::Sender<Pushed>) {
    let _ = std::thread::Builder::new().name("elephc-apm-listen".to_string()).spawn(move || {
        let open = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            // A thread per connection, so a cap: anyone who can reach the
            // port could otherwise hold a thread each without knowing the
            // key (review). Past it, a connection is closed at once.
            if open.fetch_add(1, Ordering::AcqRel) >= MAX_SOURCES {
                open.fetch_sub(1, Ordering::AcqRel);
                continue;
            }
            let sender = sender.clone();
            let spawned = std::thread::Builder::new().name("elephc-apm-source".to_string()).spawn({
                let open = open.clone();
                move || {
                    receive_source(stream, key, sender);
                    open.fetch_sub(1, Ordering::AcqRel);
                }
            });
            if spawned.is_err() {
                open.fetch_sub(1, Ordering::AcqRel);
            }
        }
    });
}

/// The most programs a listening collector serves at once.
pub(super) const MAX_SOURCES: usize = 256;

/// How long a connection may take to prove the key and say hello.
const HANDSHAKE_DEADLINE: Duration = Duration::from_secs(5);

/// One program's connection: the handshake, the hello, then its snapshots.
fn receive_source(mut stream: std::net::TcpStream, key: [u8; 32], sender: std::sync::mpsc::Sender<Pushed>) {
    use elephc_probe::push;
    let peer = stream.peer_addr().map_or_else(|_| "?".to_string(), |addr| addr.ip().to_string());
    // The handshake and the hello within one deadline, whatever the reads:
    // a watchdog closes a connection still in them when it passes, so a
    // client trickling bytes cannot hold its slot either (review).
    let _ = stream.set_read_timeout(Some(HANDSHAKE_DEADLINE));
    let _ = stream.set_write_timeout(Some(HANDSHAKE_DEADLINE));
    let greeted = Arc::new(AtomicBool::new(false));
    if let Ok(watched) = stream.try_clone() {
        let greeted = greeted.clone();
        let _ = std::thread::Builder::new().name("elephc-apm-deadline".to_string()).spawn(move || {
            std::thread::sleep(HANDSHAKE_DEADLINE);
            if !greeted.load(Ordering::Acquire) {
                let _ = watched.shutdown(std::net::Shutdown::Both);
            }
        });
    }
    let mut session = match push::collector_handshake(&mut stream, &key, crate::monitor::remote::probe_nonce()) {
        Ok(session) => session,
        Err(error) => {
            eprintln!("elephc monitor: refused {peer}: {error}");
            return;
        }
    };
    let Some(hello) = session.read_frame(&mut stream).ok().and_then(|frame| push::parse_hello(&frame)) else {
        eprintln!("elephc monitor: {peer} sent no hello");
        return;
    };
    greeted.store(true, Ordering::Release);
    // Three missed sends and the program is taken for gone.
    let _ = stream.set_read_timeout(Some(push::INTERVAL * 3));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
    let source = source_name(&hello.program, &peer, hello.pid, hello.instance);
    let mut from_start = hello.delivered == 0;
    while let Ok(frame) = session.read_frame(&mut stream) {
        let Ok(text) = std::str::from_utf8(&frame) else { break };
        let Ok(snapshot) = apm::decode(text) else { continue };
        let pushed = Pushed { source: source.clone(), snapshot, received_ms: store::now_ms(), from_start };
        from_start = false;
        if sender.send(pushed).is_err() {
            return;
        }
    }
}

/// What a pushing program's windows are labelled with: its name, where it
/// sent from, its pid and its instance number — the same across its
/// reconnections, apart from any other process's, two containers running the
/// same build as pid 1 behind one address included. The label keeps 32 bits of
/// the instance, both halves folded together: a probe that could not draw
/// randomness numbers itself from its start time, whose high half two
/// processes started in the same few seconds share (#2499).
pub(super) fn source_name(program: &str, peer: &str, pid: u32, instance: u64) -> String {
    format!("{program}@{peer}{PUSHED_PID} {pid} #{:08x}", ((instance >> 32) ^ instance) as u32)
}

/// What a pushing program's source name carries before its pid.
const PUSHED_PID: &str = " pid";

/// Whether a stored window's source is a pushing program's (`source_name`)
/// rather than a service a collector read.
pub(crate) fn is_pushed_source(source: &str) -> bool {
    source.split_once('@').is_some_and(|(_, rest)| {
        rest.rsplit_once(PUSHED_PID).is_some_and(|(_, tail)| {
            let Some((pid, instance)) = tail.strip_prefix(' ').and_then(|tail| tail.split_once(" #")) else {
                return false;
            };
            !pid.is_empty()
                && pid.bytes().all(|b| b.is_ascii_digit())
                && instance.len() == 8
                && instance.bytes().all(|b| b.is_ascii_hexdigit())
        })
    })
}

/// The window a poll collected: what the service did between two snapshots,
/// from `target`, ending at `received_ms` — this machine's clock when the
/// snapshot arrived, not the service's (see `Window::t`). Its span stays the
/// service's own measure of the time between its two snapshots.
pub(super) fn collected_window(previous: &Snapshot, current: &Snapshot, target: &str, received_ms: u64) -> Window {
    let mut window = Window::between(previous, current);
    window.source = target.to_string();
    window.t = received_ms;
    window
}

/// Re-measures the budgets at `now_ms` and announces what changed.
///
/// `unreachable` is a poll the service did not answer: the time since its
/// last window counts as time in which no request was seen, so `rps:*>=1`
/// fires when the service dies, as it does when its traffic stops. Every
/// other metric has nothing to say about requests nobody saw, and keeps its
/// state.
pub(super) fn check_alerts(history: &mut History, cmd: &MonitorCommand, now_ms: u64, unreachable: bool) {
    if history.alerts.alerts.is_empty() {
        return;
    }
    let window_ms = cmd.apm_alert_window.saturating_mul(1_000);
    let mut measured = alert_windows(history.alert_ledger.values().flatten(), now_ms, window_ms);
    let since = now_ms.saturating_sub(window_ms);
    let gap = unreachable.then(|| {
        let from = history.newest_ms().unwrap_or(since).max(since);
        Window {
            build: String::new(),
            t: now_ms,
            span_ms: now_ms.saturating_sub(from),
            epoch: history.latest.as_ref().map_or(1, |snapshot| snapshot.epoch.max(1)),
            source: cmd.target.clone(),
            routes: Vec::new(),
            res: None,
            errors: Vec::new(),
        }
    });
    measured.extend(gap.iter());
    let changes = history.alerts.check(&measured, now_ms);
    // Under a redrawn --live screen a hook's output would land in the middle
    // of the table; there it is not shown.
    let quiet_hook = cmd.live && std::io::stdout().is_terminal();
    for change in changes {
        alerts::announce(&cmd.target, &change, cmd.apm_alert_exec.as_deref(), quiet_hook);
    }
}

/// The windows a budget is measured over at `now_ms`: those that lie in the
/// last `window_ms`, give or take one poll.
///
/// A window that only ENDS in it is not enough: the first window after an
/// outage spans the whole outage, and measuring it in full judged minutes of
/// traffic as the last `--alert-window` seconds (review).
pub(super) fn alert_windows<'a>(
    windows: impl IntoIterator<Item = &'a Window>,
    now_ms: u64,
    window_ms: u64,
) -> Vec<&'a Window> {
    let since = now_ms.saturating_sub(window_ms);
    let slack = POLL_INTERVAL.as_millis() as u64;
    windows
        .into_iter()
        .filter(|w| w.t > since && w.t.saturating_sub(w.span_ms).saturating_add(slack) >= since)
        .collect()
}

/// Captures the exact profile of one request every `every` seconds, on its
/// own thread: an exact ask waits for the next request to complete (up to the
/// endpoint's limit), which must not hold up the once-a-second poll.
fn spawn_capture_loop(target: &str, key: [u8; 32], every: u64, shared: Shared) {
    let target = target.to_string();
    let _ = std::thread::Builder::new()
        .name("elephc-apm-capture".to_string())
        .spawn(move || {
            let period = Duration::from_secs(every);
            let mut next = Instant::now() + period;
            while !STOP.load(Ordering::Relaxed) {
                if Instant::now() < next {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
                next = Instant::now() + period;
                let Ok(text) = crate::monitor::fetch_service_answer(
                    &target,
                    &key,
                    elephc_probe::endpoint::WANT_EXACT,
                    true,
                ) else {
                    continue;
                };
                // A slice of a transaction the project's `[apm]` rules exclude is not kept, and
                // the next request is tried soon: a health check polled every second must not
                // take the captures (review).
                if text.lines().any(|line| line == elephc_probe::endpoint::EXCLUDED_MARK) {
                    next = Instant::now() + Duration::from_secs(1);
                    continue;
                }
                let Some((route, status, duration_ns, graph)) = traces::TraceBook::trace_from_slice(&text) else {
                    continue;
                };
                let (lock, signal) = &*shared;
                let mut history = lock.lock().unwrap_or_else(|e| e.into_inner());
                if history.traces.offer(route, status, duration_ns, graph, store::now_ms()) {
                    signal.notify_all();
                }
            }
        });
}

/// Checks the budgets once over everything the service counted since it
/// started; exits 2 when one is broken, so a pipeline can gate on it.
pub(super) fn check_budgets_once(cmd: &MonitorCommand, window: &Window) -> i32 {
    let mut state = alerts::Alerts::new(
        cmd.apm_alerts.iter().filter_map(|spec| alerts::parse_rule(spec).ok()).collect(),
    );
    state.check(&[window], store::now_ms());
    print!("{}", terminal::render_alerts(&state, window.span_ms / 1_000));
    if state.alerts.iter().any(|alert| alert.firing) {
        2
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A short script's run, alone in its source, reaches the store once it
    /// is a resolution old: it never fills a resolution, and waited for its
    /// source to be forgotten ten minutes later (review).
    #[test]
    fn a_short_runs_window_reaches_the_store_once_due() {
        let dir = std::env::temp_dir().join(format!("elephc-apm-write-due-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let words: Vec<String> = ["127.0.0.1:9411", "--apm", "--live"].iter().map(|w| w.to_string()).collect();
        let cmd = crate::monitor::parse_monitor_args(&words).unwrap();
        let store = Store::open(dir.to_str().unwrap(), 2).unwrap();
        let mut collector = Collector::start(&cmd, "listen", None, Some(store.clone())).expect("a collector");
        let now = store::now_ms();
        let run = Window {
            build: String::new(),
            res: None,
            errors: Vec::new(),
            t: now - 2 * collector.resolution_ms,
            span_ms: 5,
            epoch: 1,
            source: "job@127.0.0.1 pid 7 #00000007".into(),
            routes: Vec::new(),
        };
        let snapshot = Snapshot { epoch: 1, time_ms: run.t, routes: Vec::new(), resources: None, build: None, errors: Vec::new(), exits: Vec::new() };
        collector.absorb(run.clone(), snapshot);
        assert!(store.load(0, now + 1).is_empty(), "five milliseconds fill no resolution");
        collector.write_due(run.t + collector.resolution_ms - 1);
        assert!(store.load(0, now + 1).is_empty(), "not due yet");
        collector.write_due(now);
        assert_eq!(store.load(0, now + 1).len(), 1, "written once due");
        assert!(collector.pending.is_empty());
        drop(collector);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
