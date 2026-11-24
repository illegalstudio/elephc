//! Purpose:
//! The transaction-monitoring (APM) data model shared by the runtime that counts
//! requests and the `elephc monitor --apm` client that reads them: the latency
//! histogram's bucket arithmetic, one route's counters, and the text snapshot
//! the probe endpoint serves.
//!
//! Called from:
//! - `crate::note_transaction` / `crate::apm_snapshot_text`, inside the program.
//! - `elephc monitor --apm` (`src/monitor/apm/`), which decodes snapshots,
//!   differences them into windows and derives rates and percentiles.
//!
//! Key details:
//! - Pure: no libc, no shared memory. Both sides compile this one file, so the
//!   encoder and the decoder cannot disagree about a bucket boundary.
//! - Counters are cumulative since the service started. A reader turns two
//!   snapshots into a window by subtraction, which keeps every poll stateless on
//!   the server and makes a missed poll lose resolution rather than data.
//! - The histogram is log-linear: exact below 16 µs, then 8 sub-buckets per
//!   power of two, so a bucket is never wider than 1/8 of its lower bound. A
//!   percentile is reported as the upper bound of the bucket it falls in — a
//!   duration no request exceeded — rather than an interpolation.

/// Status classes counted per route: 1xx, 2xx, 3xx, 4xx, 5xx.
pub const STATUS_CLASSES: usize = 5;

/// What a script run's transaction name starts with, so a run never shares a
/// row with an HTTP route in a store that holds both.
pub const CLI_PREFIX: &str = "CLI ";

/// The transaction a script's run counts under: `CLI <script>`, plus the
/// console command when the first non-option argument looks like one
/// (`CLI console app:import`). Shared by `monitor`, which names a run it
/// launched, and a program built with `--apm-collector`, which names its own.
///
/// Only a command NAME is kept — a letter, then letters, digits, `:`, `_` or
/// `-` — because anything else in that position is data (a path, an id, a
/// date), and one row per value would fill the table with identifiers instead
/// of naming what ran.
pub fn cli_run_name(script: &str, args: &[String]) -> String {
    let file = std::path::Path::new(script)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| script.to_string());
    let file = file.trim_end_matches(".php");
    let looks_like_command = |arg: &str| {
        arg.len() <= 64
            && arg.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && arg.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '-'))
    };
    match args.iter().find(|arg| !arg.starts_with('-')) {
        Some(command) if looks_like_command(command) => format!("{CLI_PREFIX}{file} {command}"),
        _ => format!("{CLI_PREFIX}{file}"),
    }
}
/// Number of latency buckets per route.
pub const HIST_BUCKETS: usize = 256;
/// Sub-buckets per power of two. Eight bounds a bucket's width at 12.5% of its
/// lower edge, which is finer than the run-to-run noise of any real service.
const SUB_BUCKETS: u64 = 8;
/// log2(SUB_BUCKETS).
const SUB_BITS: u32 = 3;

/// Words of fixed counters before a route's histogram: `[count, 1xx, 2xx,
/// 3xx, 4xx, 5xx, sum_ns, db_ops, db_wait_ns, net_ops, net_wait_ns, reserved]`.
pub const HEADER_WORDS: usize = 12;
/// Buckets of a route's per-request memory histogram: powers of two from
/// `MEM_BUCKET0_BYTES` (bucket 0 holds everything up to it) to 2 GiB.
pub const MEM_BUCKETS: usize = 16;
/// The upper edge of memory bucket 0, bytes.
pub const MEM_BUCKET0_BYTES: u64 = 64 * 1024;
/// Words of a route's memory counters, after its latency histogram: the requests
/// measured, their summed peak bytes, then the memory histogram.
pub const MEM_WORDS: usize = 2 + MEM_BUCKETS;
/// Word index of the requests whose memory was measured.
pub const WORD_MEM_N: usize = HEADER_WORDS + HIST_BUCKETS;
/// Word index of their summed peak memory, bytes.
pub const WORD_MEM_SUM: usize = WORD_MEM_N + 1;
/// Word index of the first memory bucket.
pub const WORD_MEM_HIST0: usize = WORD_MEM_N + 2;
/// Words one route occupies in the shared transaction table.
pub const ROUTE_WORDS: usize = HEADER_WORDS + HIST_BUCKETS + MEM_WORDS;

/// The memory bucket a request's peak of `bytes` falls in.
pub fn mem_bucket_of(bytes: u64) -> usize {
    let mut bucket = 0;
    while bucket + 1 < MEM_BUCKETS && bytes > mem_bucket_upper(bucket) {
        bucket += 1;
    }
    bucket
}

/// The upper edge of memory bucket `bucket`, bytes.
pub fn mem_bucket_upper(bucket: usize) -> u64 {
    MEM_BUCKET0_BYTES << bucket.min(MEM_BUCKETS - 1)
}
/// Word index of the request count.
pub const WORD_COUNT: usize = 0;
/// Word index of the first status-class counter (1xx).
pub const WORD_STATUS0: usize = 1;
/// Word index of the summed wall time, nanoseconds.
pub const WORD_SUM_NS: usize = 6;
/// Word index of the first I/O counter; the four are `IO_KINDS` in order.
pub const WORD_IO0: usize = 7;
/// I/O counters per route: database statements, nanoseconds blocked in
/// database drivers, outgoing network operations, nanoseconds blocked on them.
pub const IO_KINDS: usize = 4;
/// `IO_KINDS` index of database statements.
pub const IO_DB_OPS: usize = 0;
/// `IO_KINDS` index of database-driver wait, nanoseconds.
pub const IO_DB_WAIT: usize = 1;
/// `IO_KINDS` index of outgoing network operations.
pub const IO_NET_OPS: usize = 2;
/// `IO_KINDS` index of outgoing-network wait, nanoseconds.
pub const IO_NET_WAIT: usize = 3;

/// First line of every snapshot; the trailing number is the format version.
pub const SNAPSHOT_MAGIC: &str = "elephc-apm 1";
/// The whole answer of a service that was started without `--apm`.
pub const SNAPSHOT_OFF: &str = "elephc-apm off";
/// The whole answer of a service whose counters could not be read whole:
/// resets kept crossing every read. The reader asks again on its next poll
/// rather than receive counters of one run under another's epoch.
pub const SNAPSHOT_BUSY: &str = "elephc-apm busy";

/// The bucket a duration falls in.
///
/// Microsecond resolution: a request is never faster than a microsecond in any
/// way an operator can act on, and starting there keeps the 256 buckets
/// spanning up to ~4.7 hours before the last one saturates.
pub fn bucket_of(duration_ns: u64) -> usize {
    let micros = duration_ns / 1_000;
    if micros < 2 * SUB_BUCKETS {
        return micros as usize;
    }
    let octave = 63 - micros.leading_zeros();
    let sub = (micros >> (octave - SUB_BITS)) & (SUB_BUCKETS - 1);
    let bucket = (octave as u64 - SUB_BITS as u64 + 1) * SUB_BUCKETS + sub;
    (bucket as usize).min(HIST_BUCKETS - 1)
}

/// Inclusive lower edge of `bucket`, in nanoseconds.
pub fn bucket_lower_ns(bucket: usize) -> u64 {
    bucket_edge_micros(bucket) * 1_000
}

/// Exclusive upper edge of `bucket`, in nanoseconds.
///
/// The last bucket also holds everything that overflowed it, so its "upper
/// edge" is the one value here that is a floor, not a bound; at ~4.7 hours a
/// request has stopped being a latency question.
pub fn bucket_upper_ns(bucket: usize) -> u64 {
    bucket_edge_micros(bucket + 1) * 1_000
}

/// Lower edge of `bucket` in microseconds; `bucket == HIST_BUCKETS` is the
/// upper edge of the last one.
fn bucket_edge_micros(bucket: usize) -> u64 {
    let bucket = bucket as u64;
    if bucket < 2 * SUB_BUCKETS {
        return bucket;
    }
    let octave = bucket / SUB_BUCKETS + SUB_BITS as u64 - 1;
    let sub = bucket % SUB_BUCKETS;
    (SUB_BUCKETS + sub) << (octave - SUB_BITS as u64)
}

/// The status class index of an HTTP status, or the 5xx class for anything a
/// client could not have been sent as success.
///
/// A status outside 100..=599 is a server bug by definition — nothing else
/// produces one — so it is counted where server errors are counted rather than
/// dropped, which would make the error rate look better than the service is.
pub fn status_class(status: u16) -> usize {
    match status {
        100..=199 => 0,
        200..=299 => 1,
        300..=399 => 2,
        400..=499 => 3,
        _ => 4,
    }
}

/// One route's cumulative counters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteStats {
    /// The shaped route label (`GET /users/{id}`), or `<untagged>` / `<other>`.
    pub name: String,
    /// Requests that completed.
    pub count: u64,
    /// Requests per status class, 1xx..5xx.
    pub status: [u64; STATUS_CLASSES],
    /// Summed wall time, nanoseconds.
    pub sum_ns: u64,
    /// Database and network work, indexed by the `IO_*` constants.
    pub io: [u64; IO_KINDS],
    /// Requests per latency bucket; always `HIST_BUCKETS` long.
    pub hist: Vec<u64>,
    /// Peak memory per request: `[measured, summed bytes, histogram…]` (`MEM_WORDS`).
    /// Only requests whose PHP ran in a `--web` worker or handler are measured.
    pub mem: [u64; MEM_WORDS],
}

impl RouteStats {
    /// A route with no requests yet.
    pub fn empty(name: &str) -> Self {
        Self {
            name: name.to_string(),
            count: 0,
            status: [0; STATUS_CLASSES],
            sum_ns: 0,
            io: [0; IO_KINDS],
            hist: vec![0; HIST_BUCKETS],
            mem: [0; MEM_WORDS],
        }
    }

    /// Mean peak memory of the measured requests, bytes, or `None` without any.
    pub fn mem_mean(&self) -> Option<u64> {
        (self.mem[0] > 0).then(|| self.mem[1] / self.mem[0])
    }

    /// The nearest-rank `p`-th percentile of the requests' peak memory, as the upper
    /// edge of its bucket, or `None` without measured requests.
    pub fn mem_percentile(&self, p: f64) -> Option<u64> {
        let hist = &self.mem[2..];
        let total: u64 = hist.iter().sum();
        if total == 0 {
            return None;
        }
        let rank = ((p / 100.0) * total as f64).ceil().clamp(1.0, total as f64) as u64;
        let mut seen = 0u64;
        for (bucket, n) in hist.iter().enumerate() {
            seen += n;
            if seen >= rank {
                return Some(mem_bucket_upper(bucket));
            }
        }
        Some(mem_bucket_upper(MEM_BUCKETS - 1))
    }

    /// What happened between `earlier` and `self`.
    ///
    /// Saturating, because the shared table is read without a lock while
    /// workers write it. A request's count is written last and read first, so
    /// a read can hold its status, time and bucket without its count; the next
    /// read adds the count. Nothing is lost as long as every window keeps a
    /// row in which ANY counter moved — a row kept only for its count dropped
    /// the request's status and latency for good, since the next window is
    /// measured from the read that held them (review).
    pub fn since(&self, earlier: &RouteStats) -> RouteStats {
        let mut status = [0; STATUS_CLASSES];
        for (index, slot) in status.iter_mut().enumerate() {
            *slot = self.status[index].saturating_sub(earlier.status[index]);
        }
        let mut io = [0; IO_KINDS];
        for (index, slot) in io.iter_mut().enumerate() {
            *slot = self.io[index].saturating_sub(earlier.io[index]);
        }
        RouteStats {
            name: self.name.clone(),
            count: self.count.saturating_sub(earlier.count),
            status,
            sum_ns: self.sum_ns.saturating_sub(earlier.sum_ns),
            io,
            hist: self
                .hist
                .iter()
                .zip(earlier.hist.iter())
                .map(|(now, before)| now.saturating_sub(*before))
                .collect(),
            mem: std::array::from_fn(|index| self.mem[index].saturating_sub(earlier.mem[index])),
        }
    }

    /// Adds `other`'s counters into `self` (merging windows or duplicate rows).
    ///
    /// Saturating: rows also come from store files, which a damaged or edited
    /// line can fill with any number, and a sum must not overflow on them.
    pub fn absorb(&mut self, other: &RouteStats) {
        self.count = self.count.saturating_add(other.count);
        for index in 0..STATUS_CLASSES {
            self.status[index] = self.status[index].saturating_add(other.status[index]);
        }
        self.sum_ns = self.sum_ns.saturating_add(other.sum_ns);
        for index in 0..IO_KINDS {
            self.io[index] = self.io[index].saturating_add(other.io[index]);
        }
        for (mine, theirs) in self.hist.iter_mut().zip(other.hist.iter()) {
            *mine = mine.saturating_add(*theirs);
        }
        for (mine, theirs) in self.mem.iter_mut().zip(other.mem.iter()) {
            *mine = mine.saturating_add(*theirs);
        }
    }

    /// Requests answered with a 5xx (or an impossible status).
    pub fn server_errors(&self) -> u64 {
        self.status[4]
    }

    /// Requests answered with a 4xx.
    pub fn client_errors(&self) -> u64 {
        self.status[3]
    }

    /// Mean wall time, nanoseconds, or `None` without requests.
    pub fn mean_ns(&self) -> Option<u64> {
        (self.count > 0).then(|| self.sum_ns / self.count)
    }

    /// The nearest-rank `p`-th percentile (0 < p <= 100) as the upper edge of
    /// the bucket holding it, or `None` without requests.
    ///
    /// Ranked over the histogram's own total rather than `count`, so a read
    /// that caught a request between its two increments still ranks a value
    /// the histogram actually holds.
    pub fn percentile_ns(&self, p: f64) -> Option<u64> {
        let total: u64 = self.hist.iter().sum();
        if total == 0 {
            return None;
        }
        let rank = ((p / 100.0) * total as f64).ceil().clamp(1.0, total as f64) as u64;
        let mut seen = 0u64;
        for (bucket, n) in self.hist.iter().enumerate() {
            seen += n;
            if seen >= rank {
                return Some(bucket_upper_ns(bucket));
            }
        }
        Some(bucket_upper_ns(HIST_BUCKETS - 1))
    }
}

/// What the service's whole process tree has used: its master, workers and
/// isolated handlers. CPU is cumulative since the service started; memory and
/// processes are the current figures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Resources {
    /// User plus system CPU time, nanoseconds.
    pub cpu_ns: u64,
    /// Private memory of the live processes, bytes.
    pub memory_bytes: u64,
    /// Live processes, the master included.
    pub processes: u32,
    /// Cores the service may run on (its CPU affinity, and on Linux its
    /// cgroup quota): what its CPU is a share of. 0 when unknown.
    pub cores: u32,
}

/// Every route's counters at one instant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    /// Identifies one run of the service: its start time, Unix nanoseconds.
    /// A different epoch means the counters restarted from zero.
    pub epoch: u64,
    /// When the snapshot was taken, Unix milliseconds.
    pub time_ms: u64,
    /// Routes that served at least one request.
    pub routes: Vec<RouteStats>,
    /// The service's CPU and memory, when it measured them.
    pub resources: Option<Resources>,
    /// The build serving, `<unix seconds> <label>` (when it was built, then its version
    /// or commit), when the build carries one: a new one is a deploy.
    pub build: Option<String>,
    /// The errors failed transactions ended with, grouped (`apm_errors`).
    pub errors: Vec<ErrorStats>,
    /// The runs this process ended, each with its transaction and the exact code it exited
    /// with: a program sending to a collector reports its run in its last send, so the
    /// page can say `exit 255` and not only that it failed.
    pub exits: Vec<(String, i32)>,
}

/// How many failed transactions of one transaction ended with one error: an exception
/// class, thrown from one place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorStats {
    /// The transaction's name, as its row in the routes.
    pub route: String,
    /// The exception's class.
    pub class: String,
    /// Where it was thrown, `file:line`.
    pub location: String,
    /// How many times, since the run of the counters started.
    pub count: u64,
    /// The W3C trace id of its last occurrence, 32 hex digits; empty without one.
    pub trace: String,
}

impl ErrorStats {
    /// Whether two rows are the same error of the same transaction.
    pub fn same(&self, other: &ErrorStats) -> bool {
        self.route == other.route && self.class == other.class && self.location == other.location
    }
}

impl Snapshot {
    /// The errors counted between `earlier` and `self`; all of them when the two belong to
    /// different runs of the service, whose counters started from zero.
    pub fn errors_since(&self, earlier: &Snapshot) -> Vec<ErrorStats> {
        self.errors
            .iter()
            .filter_map(|error| {
                let before = if self.epoch == earlier.epoch {
                    earlier.errors.iter().find(|e| e.same(error)).map_or(0, |e| e.count)
                } else {
                    0
                };
                let count = error.count.saturating_sub(before);
                (count > 0).then(|| ErrorStats { count, ..error.clone() })
            })
            .collect()
    }

    /// The window between `earlier` and `self`, route by route.
    ///
    /// `None` when the two belong to different runs of the service: counters
    /// that restarted cannot be subtracted, and pretending they could would
    /// report a negative window as an empty one.
    pub fn since(&self, earlier: &Snapshot) -> Option<Vec<RouteStats>> {
        if self.epoch != earlier.epoch {
            return None;
        }
        Some(
            self.routes
                .iter()
                .map(|route| match earlier.routes.iter().find(|r| r.name == route.name) {
                    Some(before) => route.since(before),
                    None => route.clone(),
                })
                // ANY counter that moved keeps the row (see `RouteStats::since`):
                // a read can hold a request's status and time before its bucket
                // and count, and dropping that row lost them for good (review).
                .filter(|route| {
                    route.count > 0
                        || route.sum_ns > 0
                        || route.status.iter().any(|n| *n > 0)
                        || route.hist.iter().any(|n| *n > 0)
                        || route.io.iter().any(|n| *n > 0)
                        // And memory, published before the count: a poll between the two
                        // saw a memory-only change, dropped the row, and the next poll
                        // took those counters as its baseline (review).
                        || route.mem.iter().any(|n| *n > 0)
                })
                .collect(),
        )
    }
}

/// Renders a snapshot in the endpoint's text format.
///
/// One `route` line per route: count, the five status classes, summed wall
/// time, the four I/O counters, the sparse histogram (`bucket:n,...`, `-` when
/// empty), then the name, last because it is the one field that contains spaces.
pub fn encode(snapshot: &Snapshot) -> String {
    let mut out = String::new();
    out.push_str(SNAPSHOT_MAGIC);
    out.push('\n');
    out.push_str(&format!("epoch {}\n", snapshot.epoch));
    out.push_str(&format!("time_ms {}\n", snapshot.time_ms));
    if let Some(res) = snapshot.resources {
        out.push_str(&format!("resources {} {} {} {}\n", res.cpu_ns, res.memory_bytes, res.processes, res.cores));
    }
    if let Some(build) = &snapshot.build {
        out.push_str(&format!("build {build}\n"));
    }
    // A run's exit code, then its transaction: `exit <code>\t<route>`.
    for (route, code) in &snapshot.exits {
        out.push_str(&format!("exit {code}\t{route}\n"));
    }
    // Tab-separated: a route, a class and a path can all hold spaces, never a tab.
    for error in &snapshot.errors {
        out.push_str(&format!("error {}\t{}\t{}\t{}", error.count, error.route, error.class, error.location));
        // The trace id of its last occurrence, when there is one, as a fifth field.
        if !error.trace.is_empty() {
            out.push_str(&format!("\t{}", error.trace));
        }
        out.push('\n');
    }
    for route in &snapshot.routes {
        out.push_str(&format!("route {}", route.count));
        for class in route.status {
            out.push_str(&format!(" {class}"));
        }
        out.push_str(&format!(" {}", route.sum_ns));
        for value in route.io {
            out.push_str(&format!(" {value}"));
        }
        out.push(' ');
        let mut first = true;
        for (bucket, n) in route.hist.iter().enumerate() {
            if *n == 0 {
                continue;
            }
            if !first {
                out.push(',');
            }
            first = false;
            out.push_str(&format!("{bucket}:{n}"));
        }
        if first {
            out.push('-');
        }
        out.push(' ');
        out.push_str(&route.name);
        out.push('\n');
        // The route's memory, on a line of its own after it: a reader that does not know
        // the line skips it. Sent as soon as any word moved: bytes and bucket are published
        // before the count.
        if route.mem.iter().any(|word| *word > 0) {
            out.push_str(&format!("mem {} {} {} {}\n", route.mem[0], route.mem[1], sparse(&route.mem[2..]), route.name));
        }
    }
    out
}

/// A histogram as `bucket:n,…`, or `-` when it is empty.
fn sparse(hist: &[u64]) -> String {
    let cells: Vec<String> =
        hist.iter().enumerate().filter(|(_, n)| **n > 0).map(|(bucket, n)| format!("{bucket}:{n}")).collect();
    if cells.is_empty() {
        "-".to_string()
    } else {
        cells.join(",")
    }
}

/// Reads a `bucket:n,…` histogram into `hist`; false on a cell that does not read.
fn read_sparse(text: &str, hist: &mut [u64]) -> bool {
    if text == "-" {
        return true;
    }
    text.split(',').all(|cell| {
        let Some((bucket, n)) = cell.split_once(':') else { return false };
        match (bucket.parse::<usize>(), n.parse::<u64>()) {
            (Ok(bucket), Ok(n)) if bucket < hist.len() => {
                hist[bucket] = n;
                true
            }
            _ => false,
        }
    })
}

/// Parses the fields after `mem `: measured, summed bytes, histogram, then the route's name.
fn decode_mem(rest: &str) -> Option<(String, [u64; MEM_WORDS])> {
    let mut fields = rest.splitn(4, ' ');
    let mut mem = [0; MEM_WORDS];
    mem[0] = fields.next()?.parse().ok()?;
    mem[1] = fields.next()?.parse().ok()?;
    if !read_sparse(fields.next()?, &mut mem[2..]) {
        return None;
    }
    Some((fields.next()?.to_string(), mem))
}

/// Why a snapshot could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// The service runs without `--apm`, so it counts nothing.
    Off,
    /// The counters could not be read whole this time (`SNAPSHOT_BUSY`).
    Busy,
    /// The text is not a snapshot this version understands.
    Malformed(String),
}

/// Parses the endpoint's text format.
pub fn decode(text: &str) -> Result<Snapshot, DecodeError> {
    let mut lines = text.lines();
    match lines.next().map(str::trim) {
        Some(SNAPSHOT_MAGIC) => {}
        Some(SNAPSHOT_OFF) => return Err(DecodeError::Off),
        Some(SNAPSHOT_BUSY) => return Err(DecodeError::Busy),
        other => {
            return Err(DecodeError::Malformed(format!(
                "expected `{SNAPSHOT_MAGIC}`, got `{}`",
                other.unwrap_or("")
            )))
        }
    }
    let malformed = |line: &str| DecodeError::Malformed(format!("unreadable line `{line}`"));
    let mut epoch = None;
    let mut time_ms = None;
    let mut routes = Vec::new();
    let mut resources = None;
    let mut build = None;
    let mut errors = Vec::new();
    let mut exits = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some(value) = line.strip_prefix("epoch ") {
            epoch = Some(value.trim().parse().map_err(|_| malformed(line))?);
        } else if let Some(value) = line.strip_prefix("time_ms ") {
            time_ms = Some(value.trim().parse().map_err(|_| malformed(line))?);
        } else if let Some(rest) = line.strip_prefix("resources ") {
            let mut fields = rest.split_whitespace();
            let mut next = || fields.next().and_then(|value| value.parse::<u64>().ok());
            match (next(), next(), next()) {
                // The core count came later: a service that does not send it
                // reads as 0, "unknown".
                (Some(cpu_ns), Some(memory_bytes), Some(processes)) => {
                    let cores = next().unwrap_or(0) as u32;
                    resources = Some(Resources { cpu_ns, memory_bytes, processes: processes as u32, cores });
                }
                _ => return Err(malformed(line)),
            }
        } else if let Some(rest) = line.strip_prefix("route ") {
            routes.push(decode_route(rest).ok_or_else(|| malformed(line))?);
        } else if let Some(rest) = line.strip_prefix("mem ") {
            let (name, mem) = decode_mem(rest).ok_or_else(|| malformed(line))?;
            if let Some(route) = routes.iter_mut().find(|route: &&mut RouteStats| route.name == name) {
                route.mem = mem;
            }
        } else if let Some(rest) = line.strip_prefix("exit ") {
            match rest.split_once('\t') {
                Some((code, route)) if code.parse::<i32>().is_ok() => exits.push((route.to_string(), code.parse::<i32>().unwrap_or(0))),
                _ => return Err(malformed(line)),
            }
        } else if let Some(rest) = line.strip_prefix("build ") {
            build = Some(rest.trim().to_string()).filter(|build| !build.is_empty());
        } else if let Some(rest) = line.strip_prefix("error ") {
            let mut fields = rest.splitn(5, '\t');
            match (fields.next().and_then(|n| n.parse().ok()), fields.next(), fields.next(), fields.next()) {
                (Some(count), Some(route), Some(class), Some(location)) => errors.push(ErrorStats {
                    route: route.to_string(),
                    class: class.to_string(),
                    location: location.to_string(),
                    count,
                    trace: fields.next().unwrap_or("").to_string(),
                }),
                _ => return Err(malformed(line)),
            }
        }
        // Unknown lines are skipped: a newer server may add fields an older
        // client does not render, and refusing the whole answer for them would
        // turn an upgrade on one side into an outage of the monitoring.
    }
    Ok(Snapshot {
        epoch: epoch.ok_or_else(|| DecodeError::Malformed("missing epoch".into()))?,
        time_ms: time_ms.ok_or_else(|| DecodeError::Malformed("missing time_ms".into()))?,
        routes,
        resources,
        build,
        errors,
        exits,
    })
}

/// Parses the fields after `route `.
fn decode_route(rest: &str) -> Option<RouteStats> {
    // count, five classes, sum, four I/O counters, histogram, then the name.
    let mut fields = rest.splitn(1 + STATUS_CLASSES + 1 + IO_KINDS + 2, ' ');
    let count = fields.next()?.parse().ok()?;
    let mut status = [0u64; STATUS_CLASSES];
    for slot in status.iter_mut() {
        *slot = fields.next()?.parse().ok()?;
    }
    let sum_ns = fields.next()?.parse().ok()?;
    let mut io = [0u64; IO_KINDS];
    for slot in io.iter_mut() {
        *slot = fields.next()?.parse().ok()?;
    }
    let hist_text = fields.next()?;
    let name = fields.next()?.to_string();
    let mut hist = vec![0u64; HIST_BUCKETS];
    if hist_text != "-" {
        for pair in hist_text.split(',') {
            let (bucket, n) = pair.split_once(':')?;
            let bucket: usize = bucket.parse().ok()?;
            *hist.get_mut(bucket)? = n.parse().ok()?;
        }
    }
    Some(RouteStats { name, count, status, sum_ns, io, hist, mem: [0; MEM_WORDS] })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every duration lands in a bucket whose edges contain it, and the bucket
    /// never exceeds 1/8 of its lower edge past the exact range.
    #[test]
    fn a_bucket_contains_its_durations_within_an_eighth() {
        let mut micros = 0u64;
        while micros < 1u64 << 33 {
            let ns = micros * 1_000;
            let bucket = bucket_of(ns);
            assert!(bucket_lower_ns(bucket) <= ns, "{micros}us below bucket {bucket}");
            assert!(ns < bucket_upper_ns(bucket), "{micros}us above bucket {bucket}");
            let lower = bucket_lower_ns(bucket);
            let width = bucket_upper_ns(bucket) - lower;
            if micros >= 16 {
                assert!(width * 8 <= lower, "bucket {bucket} is {width}ns wide at {lower}ns");
            }
            micros = micros + 1 + micros / 37;
        }
    }

    /// Bucket edges are contiguous and strictly increasing, so no duration can
    /// fall between two buckets or into two at once.
    #[test]
    fn bucket_edges_tile_the_line() {
        for bucket in 0..HIST_BUCKETS - 1 {
            assert_eq!(bucket_upper_ns(bucket), bucket_lower_ns(bucket + 1), "bucket {bucket}");
            assert!(bucket_lower_ns(bucket) < bucket_upper_ns(bucket));
        }
        assert_eq!(bucket_of(0), 0);
        assert_eq!(bucket_of(999), 0, "sub-microsecond requests share the first bucket");
        assert_eq!(bucket_of(u64::MAX), HIST_BUCKETS - 1, "overflow saturates");
    }

    /// Status codes map to their class, and anything outside HTTP's range is
    /// counted as a server error rather than dropped.
    #[test]
    fn statuses_fall_into_their_class() {
        assert_eq!(status_class(101), 0);
        assert_eq!(status_class(200), 1);
        assert_eq!(status_class(304), 2);
        assert_eq!(status_class(404), 3);
        assert_eq!(status_class(503), 4);
        assert_eq!(status_class(0), 4);
        assert_eq!(status_class(999), 4);
    }

    /// A route's counters built from parallel lists of durations (µs) and statuses.
    fn route(name: &str, durations_us: &[u64], statuses: &[u16]) -> RouteStats {
        let mut stats = RouteStats::empty(name);
        for (us, status) in durations_us.iter().zip(statuses) {
            stats.count += 1;
            stats.status[status_class(*status)] += 1;
            stats.sum_ns += us * 1_000;
            stats.hist[bucket_of(us * 1_000)] += 1;
        }
        stats
    }

    /// A snapshot survives the text format unchanged, including a route name
    /// with spaces and a route with an empty histogram.
    #[test]
    fn a_snapshot_round_trips_through_text() {
        let snapshot = Snapshot {
            epoch: 1_731_000_000_123_456_789,
            time_ms: 1_731_000_000_999,
            routes: vec![
                RouteStats { io: [12, 3_400_000, 2, 9_000_000], mem: { let mut mem = [0; MEM_WORDS]; mem[0] = 4; mem[1] = 9_000_000; mem[2 + mem_bucket_of(2_000_000)] = 3; mem[2] = 1; mem }, ..route("GET /users/{id}", &[120, 450, 3_000, 9], &[200, 200, 500, 404]) },
                RouteStats::empty("<untagged>"),
            ],
            resources: Some(Resources { cpu_ns: 12_345_678_901, memory_bytes: 187_654_321, processes: 3, cores: 10 }),
            build: Some("1731000000 v1.2.0 3f2a9c1".into()),
            errors: vec![ErrorStats { route: "GET /users/{id}".into(), class: "App\\Gone".into(), location: "/srv/my app/User.php:12".into(), count: 3, trace: "4bf92f3577b34da6a3ce929d0e0e4736".into() }],
            exits: vec![("CLI console app:import".into(), 255)],
        };
        let text = encode(&snapshot);
        assert_eq!(decode(&text), Ok(snapshot.clone()));
        // A service that measured nothing sends no line, and reads back as such.
        let bare = Snapshot { resources: None, build: None, errors: Vec::new(), ..snapshot };
        assert_eq!(decode(&encode(&bare)), Ok(bare));
        // A service from before the core count sends three figures: read, with
        // its cores unknown (0), rather than refused.
        let older = format!("{SNAPSHOT_MAGIC}\nepoch 1\ntime_ms 2\nresources 3000 4096 2\n");
        assert_eq!(
            decode(&older).map(|s| s.resources),
            Ok(Some(Resources { cpu_ns: 3_000, memory_bytes: 4_096, processes: 2, cores: 0 }))
        );
    }

    /// A service without `--apm` answers a distinct sentinel, so the client can
    /// say what to change instead of reporting an empty service.
    #[test]
    fn the_off_answer_is_not_mistaken_for_an_idle_service() {
        assert_eq!(decode(SNAPSHOT_OFF), Err(DecodeError::Off));
        assert_eq!(decode(SNAPSHOT_BUSY), Err(DecodeError::Busy));
        assert!(matches!(decode("garbage"), Err(DecodeError::Malformed(_))));
    }

    /// Lines a newer server adds are ignored rather than failing the answer.
    #[test]
    fn unknown_lines_are_tolerated() {
        let text = format!("{SNAPSHOT_MAGIC}\nepoch 1\ntime_ms 2\nfuture-field 7\n");
        assert_eq!(decode(&text).map(|s| s.epoch), Ok(1));
    }

    /// Percentiles are nearest-rank over the histogram and report the bucket's
    /// upper edge: a duration no ranked request exceeded.
    #[test]
    fn percentiles_rank_the_histogram() {
        let durations: Vec<u64> = (1..=100).map(|i| i * 100).collect();
        let stats = route("GET /", &durations, &[200; 100]);
        let p50 = stats.percentile_ns(50.0).unwrap();
        let p99 = stats.percentile_ns(99.0).unwrap();
        let p100 = stats.percentile_ns(100.0).unwrap();
        assert!((5_000_000..=5_000_000 * 9 / 8 + 1_000).contains(&p50), "{p50}");
        assert!((9_900_000..=9_900_000 * 9 / 8 + 1_000).contains(&p99), "{p99}");
        assert!(p100 > 10_000_000, "the maximum lies below its bucket's upper edge");
        assert_eq!(RouteStats::empty("x").percentile_ns(50.0), None);
    }

    /// A poll between a request's memory and its count sees only the memory: the row
    /// is kept, or the next poll took those counters as its baseline and the
    /// measurement was lost (review).
    #[test]
    fn a_memory_only_change_keeps_its_row() {
        let before = Snapshot { epoch: 7, time_ms: 1_000, routes: vec![route("GET /a", &[100], &[200])], resources: None, build: None, errors: Vec::new(), exits: Vec::new() };
        let mut after = before.clone();
        after.time_ms = 2_000;
        after.routes[0].mem[0] += 1;
        after.routes[0].mem[1] += 4_000_000;
        let window = after.since(&before).unwrap();
        assert_eq!(window.len(), 1, "the memory-only change is kept");
        assert_eq!((window[0].count, window[0].mem[0]), (0, 1));
    }

    /// Two snapshots of one run subtract into the window between them; two runs
    /// do not subtract at all.
    #[test]
    fn windows_are_differences_within_one_run() {
        let before = Snapshot {
            epoch: 7,
            time_ms: 1_000,
            routes: vec![route("GET /a", &[100, 200], &[200, 200])],
            resources: None,
            build: None,
            errors: Vec::new(),
            exits: Vec::new(),
        };
        let mut after = before.clone();
        after.time_ms = 2_000;
        after.routes[0].absorb(&route("GET /a", &[300], &[503]));
        after.routes.push(route("GET /b", &[50], &[200]));
        let window = after.since(&before).unwrap();
        assert_eq!(window.len(), 2);
        assert_eq!(window[0].count, 1);
        assert_eq!(window[0].server_errors(), 1);
        assert_eq!(window[0].sum_ns, 300_000);
        assert_eq!(window[1].name, "GET /b");

        let mut restarted = after.clone();
        restarted.epoch = 8;
        assert_eq!(restarted.since(&before), None);
    }

    /// A read that caught a request between two of its increments clamps the
    /// transient difference to zero instead of wrapping.
    #[test]
    fn a_torn_read_does_not_wrap() {
        let mut ahead = route("GET /a", &[100], &[200]);
        let behind = ahead.clone();
        ahead.hist[bucket_of(100_000)] -= 1;
        let window = behind.since(&ahead);
        assert_eq!(window.hist[bucket_of(100_000)], 1);
        let window = ahead.since(&behind);
        assert!(window.hist.iter().all(|n| *n == 0));
    }

    /// A read can hold a request's status and time before its bucket and
    /// count: the window keeps that row, or the next window, measured from
    /// this read, never sees them (review).
    #[test]
    fn a_window_keeps_a_row_with_only_status_and_time() {
        let before = route("GET /", &[100], &[200]);
        let mut caught = before.clone();
        caught.status[status_class(500)] += 1;
        caught.sum_ns += 2_000_000;
        let snapshot = |row: RouteStats| Snapshot { epoch: 1, time_ms: 0, routes: vec![row], resources: None, build: None, errors: Vec::new(), exits: Vec::new() };
        let window = snapshot(caught).since(&snapshot(before)).unwrap();
        assert_eq!(window.len(), 1, "the row is kept");
        assert_eq!((window[0].count, window[0].status[status_class(500)], window[0].sum_ns), (0, 1, 2_000_000));
    }
}
