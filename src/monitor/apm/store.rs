//! Purpose:
//! The on-disk history of `monitor --apm --store <dir>`: one JSON line per
//! stored window, one file per UTC day, older days removed past the retention.
//!
//! Called from:
//! - `apm::collect`, which appends a window every resolution period.
//! - `apm::History::windows_in`, for spans longer than memory holds, and the
//!   dashboard when it replays a store with no service attached.
//!
//! Key details:
//! - Append-only lines: a crash can only lose, or truncate, the line being
//!   written, and a reader skips a line it cannot parse instead of failing the
//!   whole day.
//! - A day per file makes retention a file deletion and a range read a handful
//!   of opens, with no index to keep consistent.
//! - Only files named `apm-YYYY-MM-DD.jsonl` are ever read or deleted: the
//!   directory is the user's, and anything else in it is left alone.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use super::Window;

/// Milliseconds in a day.
const DAY_MS: u64 = 86_400_000;

/// A directory of daily window files.
#[derive(Clone)]
pub(crate) struct Store {
    dir: PathBuf,
    retention_days: u32,
    /// Days kept at the resolution they were written at; older ones are rolled up.
    full_days: u32,
    /// Days kept at least by the minute; older ones are rolled up by the hour.
    minute_days: u32,
    /// What has been read of each day file, shared by the clones of one opened
    /// store.
    reads: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<u64, DayRead>>>,
}

/// How far a day file has been read, and what it held, folded by the minute.
///
/// Every reader goes through this — the page's ranges, the runs, the newest
/// window — so a day file is parsed once and then only for what was appended
/// since. Re-parsing whole days on each refresh made a page on a 7-day range
/// parse sixty thousand lines every ten seconds (review).
#[derive(Clone, Default)]
struct DayRead {
    /// The file's identity when it was read: a file deleted by retention and
    /// written again is another file, whatever its length (review).
    file_id: Option<(u64, u64)>,
    /// The file's first bytes when it was read. Linux reuses a freed inode at
    /// once, so a day file deleted and written again can keep its identity:
    /// its first line, which carries a window's end time, tells it apart (CI).
    head: Vec<u8>,
    /// Bytes read, up to the end of the last complete line.
    offset: u64,
    /// The latest `t` among the lines read.
    newest: Option<u64>,
    /// The windows read. A script's runs of one transaction are folded by the
    /// minute — the runs of a per-second cron were otherwise all kept — and so
    /// are a service's windows once the day is over, when nothing reads them
    /// finer than a minute: a past day holds at most 1 440 of each. Today's
    /// service windows keep the store's resolution, which short ranges show.
    windows: Vec<Window>,
    /// Where each minute's folded window is in `windows`.
    minutes: std::collections::HashMap<MinuteKey, usize>,
    /// Whether the service windows are folded too: the day is over.
    past: bool,
    /// Service windows that ended before this (ms) are folded by the minute
    /// even though the day is not over: today's past hours (`TODAY_FINE_MS`).
    fold_before: u64,
    /// How many windows today's cache may hold before its past hours are
    /// folded again; 0 means `TODAY_FOLD_AT`.
    refold_at: usize,
}

/// How far back today's service windows keep the store's resolution in the
/// read cache: an hour, what the short ranges show.
const TODAY_FINE_MS: u64 = 3_600_000;
/// How many windows today's cache holds before its past hours are folded by
/// the minute. A listening collector's 256 programs at a 10 s resolution
/// wrote two million windows by midnight, all kept fine and walked by every
/// read of the page (review).
const TODAY_FOLD_AT: usize = if cfg!(test) { 100 } else { 200_000 };

/// What windows fold together in a day's cache: one source, one run of it (or
/// one script transaction), one minute.
#[derive(Clone, PartialEq, Eq, Hash)]
struct MinuteKey {
    source: String,
    epoch: u64,
    /// The transaction of a script's run; empty for a service window.
    route: String,
    minute: u64,
}

/// A day rolled up by the minute, in the store's rollup record.
const ROLLUP_MINUTE: u8 = 1;
/// A day rolled up by the hour.
const ROLLUP_HOUR: u8 = 2;
/// The store file recording which days were rolled up, and how far.
const ROLLUPS_FILE: &str = "rollups.json";

/// The days rolled up so far, by day number, with their level.
fn read_rollups(dir: &Path) -> std::collections::BTreeMap<u64, u8> {
    std::fs::read_to_string(dir.join(ROLLUPS_FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Records the days rolled up, written aside and renamed.
fn write_rollups(dir: &Path, rollups: &std::collections::BTreeMap<u64, u8>) {
    let Ok(text) = serde_json::to_string(rollups) else {
        return;
    };
    let partial = dir.join(format!("{ROLLUPS_FILE}.partial"));
    if std::fs::write(&partial, text).is_ok() {
        let _ = std::fs::rename(&partial, dir.join(ROLLUPS_FILE));
    }
}

/// Windows folded by `step_ms`: one per source, run of the service, script run's
/// transaction and step, ending at the latest of the windows it holds.
fn roll_up(windows: Vec<Window>, step_ms: u64) -> Vec<Window> {
    let mut index: std::collections::HashMap<MinuteKey, usize> = std::collections::HashMap::new();
    let mut rolled: Vec<Window> = Vec::new();
    for window in windows {
        let key = MinuteKey {
            source: window.source.clone(),
            epoch: window.epoch,
            route: if super::is_run(&window) {
                window.routes.first().map(|route| route.route.clone()).unwrap_or_default()
            } else {
                String::new()
            },
            minute: window.t / step_ms.max(1),
        };
        match index.get(&key) {
            Some(&at) => rolled[at] = super::merge_pair(&rolled[at], &window),
            None => {
                index.insert(key, rolled.len());
                rolled.push(window);
            }
        }
    }
    rolled.sort_by_key(|window| window.t);
    rolled
}

/// Rewrites a day file with its windows rolled up by `step_ms`: written aside and renamed,
/// so a reader sees the old file or the new one; its read cache notices the new file.
fn roll_up_day(path: &Path, step_ms: u64) -> Result<(), String> {
    let text = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let windows: Vec<Window> = text
        .split(|byte| *byte == b'\n')
        .filter_map(|line| serde_json::from_slice::<Window>(line).ok())
        .collect();
    let mut out = String::new();
    for window in roll_up(windows, step_ms) {
        let line = serde_json::to_string(&window).map_err(|error| error.to_string())?;
        out.push_str(&line);
        out.push('\n');
    }
    let partial = path.with_extension("jsonl.partial");
    std::fs::write(&partial, out).map_err(|error| format!("{}: {error}", partial.display()))?;
    std::fs::rename(&partial, path).map_err(|error| format!("{}: {error}", path.display()))
}

/// Milliseconds a cached stored window covers at most.
const CACHE_MINUTE_MS: u64 = 60_000;

impl Store {
    /// Opens (creating it when missing) the store at `dir`.
    ///
    /// Opening deletes nothing: browsing, exporting or recording a script's run
    /// must not cut history a collector keeps for longer than this command's
    /// default retention. The collector prunes (`prune_now`).
    pub(crate) fn open(dir: &str, retention_days: u32) -> Result<Self, String> {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir)
            .map_err(|error| format!("cannot create the apm store {}: {error}", dir.display()))?;
        if !dir.is_dir() {
            return Err(format!("the apm store {} is not a directory", dir.display()));
        }
        let retention_days = retention_days.max(1);
        Ok(Self {
            dir,
            retention_days,
            full_days: retention_days,
            minute_days: retention_days,
            reads: Default::default(),
        })
    }

    /// Rolls up the days a collector's pruning reaches: past `full_days` by the minute,
    /// past `minute_days` by the hour (`--retention-full`, `--retention-minute`). A store
    /// only opened keeps every day as it was written.
    pub(crate) fn with_tiers(mut self, full_days: u32, minute_days: u32) -> Self {
        self.full_days = full_days.min(self.retention_days);
        self.minute_days = minute_days.clamp(self.full_days, self.retention_days);
        self
    }

    /// The directory holding the history.
    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }

    /// How many days of history are kept.
    pub(crate) fn retention_days(&self) -> u32 {
        self.retention_days
    }

    /// Claims the right to collect `target` into this store, for as long as
    /// the returned file stays open.
    ///
    /// Per target, not per store: a store can hold several services and every
    /// script's runs, and only two collectors of the SAME service conflict —
    /// each would write the same requests. A script's run appends one whole
    /// line and needs no claim.
    pub(crate) fn claim_collector(&self, target: &str) -> Result<std::fs::File, String> {
        use fs2::FileExt as _;
        let path = self.dir.join(collector_lock_name(target));
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(|error| format!("cannot open {}: {error}", path.display()))?;
        file.try_lock_exclusive().map_err(|_| {
            format!(
                "another collector already stores {target} in {}; two would count every request twice",
                self.dir.display()
            )
        })?;
        // The claim says how long this collector keeps its history, so a
        // collector of another service sharing the store prunes no day this
        // one still keeps (`live_claims`).
        use std::io::Write as _;
        let _ = file.set_len(0);
        // And how long it keeps each resolution, so another collector's roll-ups fold no day
        // this one keeps finer (review): `<retention> <full days> <minute days>`.
        let _ = writeln!(&file, "{} {} {}", self.retention_days, self.full_days, self.minute_days);
        Ok(file)
    }

    /// Appends one window to the file of the day it ended in.
    pub(crate) fn append(&self, window: &Window) -> Result<(), String> {
        let path = self.dir.join(file_name(day_of(window.t)));
        let mut line = serde_json::to_string(window)
            .map_err(|error| format!("cannot encode an apm window: {error}"))?;
        line.push('\n');
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(&path)
            .map_err(|error| format!("cannot open {}: {error}", path.display()))?;
        // A line a crash cut short has no newline: this one starts on a line of
        // its own, or the reader took both for one damaged line and lost this
        // one too (review). Two writers seeing the same torn tail leave an
        // empty line between them, which the reader skips.
        if ends_torn(&file) {
            line.insert(0, '\n');
        }
        // One write per line: with O_APPEND the line lands whole after whatever
        // is there, so a concurrent reader never sees half of it followed by
        // the next one.
        file.write_all(line.as_bytes())
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
        Ok(())
    }

    /// Appends a collector's window, and prunes when it begins a new day: that
    /// is when the oldest one falls out. By this machine's clock, never the
    /// service's: a service whose clock runs ahead would otherwise date every
    /// stored day out of retention.
    ///
    /// The collector's only: a script's run appends with `append`, and prunes
    /// only when given a retention, so a cron run crossing midnight never cuts
    /// history a collector keeps for longer than the default (review).
    pub(crate) fn append_collected(&self, window: &Window) -> Result<(), String> {
        self.append(window)?;
        if day_of(window.t) != day_of(window.t.saturating_sub(window.span_ms)) {
            self.prune_now();
        }
        Ok(())
    }

    /// Windows that ended in `[from_ms, to_ms]`, oldest first, folded by the
    /// minute (see `DayRead`).
    pub(crate) fn load(&self, from_ms: u64, to_ms: u64) -> Vec<Window> {
        if from_ms > to_ms {
            return Vec::new();
        }
        self.cached(day_of(from_ms)..=day_of(to_ms), |window| window.t >= from_ms && window.t <= to_ms)
    }

    /// The cached windows of `days` that `keep` selects, oldest first, after
    /// reading what was appended to each day file since the last call.
    fn cached(&self, days: std::ops::RangeInclusive<u64>, keep: impl Fn(&Window) -> bool) -> Vec<Window> {
        let stored = self.days();
        let today = day_of(now_ms());
        let mut reads = self.reads.lock().unwrap_or_else(|e| e.into_inner());
        forget_deleted_days(&mut reads, &stored);
        let mut out: Vec<Window> = Vec::new();
        for day in stored.into_iter().filter(|day| days.contains(day)) {
            let read = reads.entry(day).or_default();
            read_day(&self.dir.join(file_name(day)), read);
            if day < today && !read.past {
                fold_past_day(read);
            } else if day == today {
                fold_past_hours(read, now_ms().saturating_sub(TODAY_FINE_MS));
            }
            out.extend(read.windows.iter().filter(|window| keep(window)).cloned());
        }
        out.sort_by_key(|window| window.t);
        out
    }

    /// The end of the newest stored window, if any.
    ///
    /// Each day file is read once and then only for what was appended since:
    /// the monitor page asks on every refresh and event-stream heartbeat, under
    /// the history's lock, and parsing a full day of windows each time held
    /// every other request up behind it. The latest `t` is kept,
    /// not the last line's: several collectors and script runs append to one
    /// file, so its lines are not strictly ordered.
    pub(crate) fn newest_ms(&self) -> Option<u64> {
        let mut days = self.days();
        // Not past tomorrow: a day file a writer with a clock far ahead left
        // would anchor every view of the store to a future with nothing in it,
        // and retention never deletes a future day (review). One day of
        // skew is allowed.
        let tomorrow = day_of(now_ms()) + 1;
        days.retain(|day| *day <= tomorrow);
        days.sort_unstable();
        let mut reads = self.reads.lock().unwrap_or_else(|e| e.into_inner());
        forget_deleted_days(&mut reads, &days);
        days.into_iter().rev().find_map(|day| {
            let read = reads.entry(day).or_default();
            read_day(&self.dir.join(file_name(day)), read);
            read.newest
        })
    }

    /// How many day files' reads are cached, for the test that proves a pruned
    /// day is forgotten.
    #[cfg(test)]
    pub(crate) fn cached_days(&self) -> usize {
        self.reads.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// The script runs that ended in `(from_ms, to_ms]`, oldest first.
    ///
    /// A collector's memory holds only its own service's windows, so the runs
    /// a page shows beside them always come from here, over the whole range —
    /// even one memory covers. Read the way `newest_ms` reads: each day file
    /// once, then only what was appended since.
    pub(crate) fn runs_in(&self, from_ms: u64, to_ms: u64) -> Vec<Window> {
        if from_ms >= to_ms {
            return Vec::new();
        }
        self.cached(day_of(from_ms)..=day_of(to_ms), |window| {
            super::is_run(window) && window.t > from_ms && window.t <= to_ms
        })
    }

    /// Deletes the day files older than the retention, counted back from now.
    pub(crate) fn prune_now(&self) {
        self.prune(now_ms());
    }

    /// Deletes the day files older than the retention, counted back from `now`.
    ///
    /// The longest retention among the collectors that hold a claim on the
    /// store is kept, not only this one's: a collector started with the
    /// default deleted the days another collector of the store was told to
    /// keep (review).
    pub(crate) fn prune(&self, now: u64) {
        // One collector at a time rewrites the shared files: two roll-ups wrote the same
        // `.partial` path, and one could rename the other's half-written file (review).
        // Busy, this one leaves it to the collector already at it.
        let Some(_maintenance) = self.maintenance_lock() else {
            return;
        };
        let today = day_of(now);
        let claims = self.live_claims();
        let retention = self.retention_days.max(claims.0);
        let keep_from = today.saturating_sub(u64::from(retention) - 1);
        for day in self.days() {
            if day < keep_from {
                let _ = std::fs::remove_file(self.dir.join(file_name(day)));
            }
        }
        // A day's captures leave with its windows, and so do its deploys, but each
        // service's latest: the build it serves stays known.
        self.prune_captures(keep_from);
        super::deploys::prune(&self.dir, keep_from.saturating_mul(DAY_MS));
        self.roll_up_days(today, claims);
    }

    /// The maintenance lock, for a test standing in for another collector at it.
    #[cfg(test)]
    pub(crate) fn maintenance_lock_for_test(&self) -> Option<std::fs::File> {
        self.maintenance_lock()
    }

    /// The store's maintenance lock, held while it prunes and rolls up; `None` while another
    /// collector holds it.
    fn maintenance_lock(&self) -> Option<std::fs::File> {
        use fs2::FileExt as _;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.dir.join(".maintenance.lock"))
            .ok()?;
        file.try_lock_exclusive().ok()?;
        Some(file)
    }

    /// Rolls up the days old enough for a coarser resolution, each once: by the minute
    /// past `full_days`, by the hour past `minute_days`. Today and yesterday never are: a
    /// collector may still be writing yesterday's last windows.
    /// The finest any live collector keeps wins: one keeping thirty days at full resolution
    /// is not folded by a default one sharing the store (review).
    fn roll_up_days(&self, today: u64, claims: (u32, u32, u32)) {
        let full_days = self.full_days.max(claims.1);
        let minute_days = self.minute_days.max(claims.2);
        let mut done = read_rollups(&self.dir);
        let before = done.clone();
        for day in self.days() {
            let age = today.saturating_sub(day);
            let level = if age >= u64::from(minute_days).max(2) {
                ROLLUP_HOUR
            } else if age >= u64::from(full_days).max(2) {
                ROLLUP_MINUTE
            } else {
                continue;
            };
            if done.get(&day).copied().unwrap_or(0) >= level {
                continue;
            }
            let step_ms = if level == ROLLUP_HOUR { 3_600_000 } else { 60_000 };
            if roll_up_day(&self.dir.join(file_name(day)), step_ms).is_ok() {
                done.insert(day, level);
            }
        }
        // Days retention deleted leave the record too.
        let days = self.days();
        done.retain(|day, _| days.contains(day));
        if done != before {
            write_rollups(&self.dir, &done);
        }
    }

    /// The longest retention, full-resolution and minute periods the live collectors' claims
    /// on this store state, each on its own; zeros when no claim is held. A claim file whose
    /// lock can be taken belongs to a collector that is gone, and states nothing. A claim
    /// written before the tiers were stated keeps its whole retention at full resolution.
    fn live_claims(&self) -> (u32, u32, u32) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return (0, 0, 0);
        };
        let mut longest = (0, 0, 0);
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !(name.starts_with(".collector-") && name.ends_with(".lock")) {
                continue;
            }
            let Ok(file) = std::fs::File::open(entry.path()) else {
                continue;
            };
            if file.try_lock_shared().is_ok() {
                let _ = file.unlock();
                continue;
            }
            let text = std::fs::read_to_string(entry.path()).unwrap_or_default();
            let fields: Vec<u32> = text.split_whitespace().filter_map(|field| field.parse().ok()).collect();
            let retention = fields.first().copied().unwrap_or(0);
            let full = fields.get(1).copied().unwrap_or(retention);
            let minute = fields.get(2).copied().unwrap_or(retention);
            longest = (longest.0.max(retention), longest.1.max(full), longest.2.max(minute));
        }
        longest
    }

    /// Days (since the Unix epoch) that have a file in the store.
    fn days(&self) -> Vec<u64> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter_map(|entry| parse_file_name(&entry.file_name().to_string_lossy()))
            .collect()
    }
}

#[cfg(test)]
thread_local! {
    /// Bytes `read_day` read on this thread, for the tests that prove a call
    /// reads only what was appended.
    pub(crate) static DAY_BYTES_READ: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    /// The time `now_ms` answers on this thread instead of the wall clock, for
    /// the tests whose windows must all fall on one day whenever they run
    /// (#2513: in the first minute after UTC midnight, "recent" windows
    /// landed on the previous day).
    pub(crate) static PINNED_NOW_MS: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
}

/// Drops what was read of the days whose file is gone — retention deleted it —
/// so a page left open for weeks does not keep every day's runs it ever read.
fn forget_deleted_days(reads: &mut std::collections::HashMap<u64, DayRead>, days: &[u64]) {
    reads.retain(|day, _| days.contains(day));
}

/// The lock file of the collector of `target`: a readable stem, then a hash of
/// the whole target, so two targets the stem makes alike — `/srv/a-b` and
/// `/srv/a_b` — never share a lock and refuse each other (review).
fn collector_lock_name(target: &str) -> String {
    let stem: String = target
        .chars()
        .take(64)
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    // FNV-1a: the same in every run and build, unlike the std hasher's seed.
    let hash = target
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3));
    format!(".collector-{stem}-{hash:016x}.lock")
}

/// Marks a cached day as over and folds its service windows by the minute.
fn fold_past_day(read: &mut DayRead) {
    read.past = true;
    read.minutes.clear();
    for window in std::mem::take(&mut read.windows) {
        fold_into_day(read, window);
    }
}

/// Folds today's service windows older than `before` by the minute, once the
/// cache holds `refold_at` windows; the threshold then doubles past what is
/// left, so a day that stays large is not refolded on every read.
fn fold_past_hours(read: &mut DayRead, before: u64) {
    read.fold_before = before;
    let at = if read.refold_at == 0 { TODAY_FOLD_AT } else { read.refold_at };
    if read.windows.len() < at {
        return;
    }
    read.minutes.clear();
    for window in std::mem::take(&mut read.windows) {
        fold_into_day(read, window);
    }
    read.refold_at = TODAY_FOLD_AT.max(read.windows.len().saturating_mul(2));
}

/// Adds a stored window to a day's cache: folded into the window of its
/// minute, source, run and kind when it is a script's run, the day is over,
/// or it ended before today's fine hour; as a window of its own otherwise.
fn fold_into_day(read: &mut DayRead, window: Window) {
    if !read.past && !super::is_run(&window) && window.t >= read.fold_before {
        read.windows.push(window);
        return;
    }
    let key = MinuteKey {
        source: window.source.clone(),
        epoch: window.epoch,
        route: if super::is_run(&window) {
            window.routes.first().map(|route| route.route.clone()).unwrap_or_default()
        } else {
            String::new()
        },
        minute: window.t / CACHE_MINUTE_MS,
    };
    match read.minutes.get(&key) {
        Some(&index) => {
            let merged = super::merge_pair(&read.windows[index], &window);
            read.windows[index] = merged;
        }
        None => {
            read.minutes.insert(key, read.windows.len());
            read.windows.push(window);
        }
    }
}

/// Whether `file` ends in the middle of a line: not empty, and its last byte
/// is not a newline.
fn ends_torn(file: &std::fs::File) -> bool {
    use std::os::unix::fs::FileExt as _;
    let Ok(meta) = file.metadata() else {
        return false;
    };
    let mut last = [0u8; 1];
    meta.len() > 0 && file.read_at(&mut last, meta.len() - 1).is_ok_and(|read| read == 1) && last[0] != b'\n'
}

/// The identity of an open file: device and inode.
fn file_id(meta: &std::fs::Metadata) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt as _;
    (meta.dev(), meta.ino())
}

/// Continues reading a day file from where `read` stopped, up to its last
/// complete line, folding in the window ends and the windows found.
///
/// A file shorter than what was read, or another file than the one read, was
/// replaced (pruned and written again), so it is read from the start. A line
/// still being written is left for the next call; a line that does not parse —
/// a crash mid-write — is skipped. A missing file reads as empty.
fn read_day(path: &Path, read: &mut DayRead) {
    use std::io::{Read as _, Seek as _, SeekFrom};
    let Ok(mut file) = std::fs::File::open(path) else {
        *read = DayRead::default();
        return;
    };
    let Ok(meta) = file.metadata() else {
        return;
    };
    let len = meta.len();
    let id = file_id(&meta);
    if len < read.offset
        || read.file_id.is_some_and(|known| known != id)
        || (read.offset > 0 && !starts_with(&mut file, &read.head))
    {
        *read = DayRead::default();
    }
    read.file_id = Some(id);
    if len == read.offset || file.seek(SeekFrom::Start(read.offset)).is_err() {
        return;
    }
    let mut bytes = Vec::with_capacity((len - read.offset) as usize);
    if std::io::Read::by_ref(&mut file).take(len - read.offset).read_to_end(&mut bytes).is_err() {
        return;
    }
    #[cfg(test)]
    DAY_BYTES_READ.with(|count| count.set(count.get() + bytes.len() as u64));
    let Some(complete) = bytes.iter().rposition(|byte| *byte == b'\n').map(|end| end + 1) else {
        return;
    };
    if read.offset == 0 {
        read.head = bytes[..complete.min(HEAD_BYTES)].to_vec();
    }
    for line in bytes[..complete].split(|byte| *byte == b'\n') {
        let Ok(window) = serde_json::from_slice::<Window>(line) else {
            continue;
        };
        read.newest = read.newest.max(Some(window.t));
        fold_into_day(read, window);
    }
    read.offset += complete as u64;
}

/// How many of a day file's first bytes identify it (`DayRead::head`).
const HEAD_BYTES: usize = 64;

/// Whether `file` still begins with `head`: one short read at its start.
fn starts_with(file: &mut std::fs::File, head: &[u8]) -> bool {
    use std::io::{Read as _, Seek as _, SeekFrom};
    let mut start = vec![0u8; head.len()];
    file.seek(SeekFrom::Start(0)).is_ok() && file.read_exact(&mut start).is_ok() && start == head
}

/// Unix milliseconds now (in a test, the time `PINNED_NOW_MS` pins).
pub(crate) fn now_ms() -> u64 {
    #[cfg(test)]
    if let Some(pinned) = PINNED_NOW_MS.with(|now| now.get()) {
        return pinned;
    }
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

/// The UTC day (days since 1970-01-01) a timestamp falls in.
pub(crate) fn day_of(ms: u64) -> u64 {
    ms / DAY_MS
}

/// `apm-YYYY-MM-DD.jsonl` for a day number.
pub(crate) fn file_name(day: u64) -> String {
    let (year, month, date) = civil_from_days(day as i64);
    format!("apm-{year:04}-{month:02}-{date:02}.jsonl")
}

/// The day number of a store file name, or `None` for anything else.
pub(crate) fn parse_file_name(name: &str) -> Option<u64> {
    let stem = name.strip_prefix("apm-")?.strip_suffix(".jsonl")?;
    let mut parts = stem.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: u32 = parts.next()?.parse().ok()?;
    let date: u32 = parts.next()?.parse().ok()?;
    // Years a day file can have: the day arithmetic overflows past ~10^16,
    // which a stray file name in the user's directory could reach (review).
    if parts.next().is_some()
        || !(1970..=9999).contains(&year)
        || !(1..=12).contains(&month)
        || !(1..=31).contains(&date)
    {
        return None;
    }
    let day = days_from_civil(year, month, date);
    (day >= 0 && file_name(day as u64) == name).then_some(day as u64)
}

/// Gregorian date of a day number (Howard Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let date = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, date)
}

/// Day number of a Gregorian date (Howard Hinnant's `days_from_civil`).
fn days_from_civil(year: i64, month: u32, date: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year.rem_euclid(400);
    let month = i64::from(month);
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * mp + 2) / 5 + i64::from(date) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod fold_tests {
    use super::*;
    use crate::monitor::apm::{Io, Mem, RouteWindow};

    /// Today's past hours fold by the minute once the cache is large, the
    /// last hour keeping its resolution and every request its count; at any
    /// hour of the day, unlike a test on the clock (review).
    #[test]
    fn past_hours_fold_by_the_minute_and_keep_the_last_hour_fine() {
        let ten_o_clock = 1_000 * DAY_MS + 10 * 3_600_000;
        let window = |t: u64| Window {
            build: String::new(),
            res: None,
            errors: Vec::new(),
            t,
            span_ms: 1_000,
            epoch: 1,
            source: "svc".into(),
            routes: vec![RouteWindow {
                route: "GET /".into(),
                n: 1,
                status: [0, 1, 0, 0, 0],
                sum_ns: 1_000,
                hist: vec![(3, 1)],
                io: Io::default(),
                mem: Mem::default(),
                exits: Vec::new(),
            }],
        };
        let mut read = DayRead::default();
        for second in 0..150 {
            fold_into_day(&mut read, window(ten_o_clock - 2 * 3_600_000 + second * 1_000));
        }
        for second in 0..10 {
            fold_into_day(&mut read, window(ten_o_clock - 60_000 + second * 1_000));
        }
        assert_eq!(read.windows.len(), 160, "kept fine until the cache is large");
        fold_past_hours(&mut read, ten_o_clock - TODAY_FINE_MS);
        let total: u64 = read.windows.iter().flat_map(|w| &w.routes).map(|r| r.n).sum();
        assert_eq!(total, 160, "every request is kept");
        assert_eq!(read.windows.iter().filter(|w| w.t >= ten_o_clock - 60_000).count(), 10, "the last hour stays fine");
        assert!(read.windows.len() <= 14, "two and a half minutes fold into a few windows: {}", read.windows.len());
        let folded = read.windows.len();
        fold_past_hours(&mut read, ten_o_clock - TODAY_FINE_MS);
        assert_eq!(read.windows.len(), folded, "a small cache is left as it is");
    }

    /// A collector's pruning keeps yesterday as written, rolls a day past `--retention-full`
    /// up by the minute and one past `--retention-minute` by the hour, each once, with
    /// every request kept.
    #[test]
    fn old_days_are_rolled_up_by_the_minute_then_the_hour() {
        let dir = std::env::temp_dir().join(format!("elephc-rollup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::open(dir.to_str().unwrap(), 30).unwrap().with_tiers(2, 7);
        let today = 20_000u64;
        let window = |t: u64| Window {
            build: String::new(),
            res: None,
            errors: Vec::new(),
            t,
            span_ms: 1_000,
            epoch: 1,
            source: "svc".into(),
            routes: vec![RouteWindow {
                route: "GET /".into(),
                n: 1,
                status: [0, 1, 0, 0, 0],
                sum_ns: 1_000,
                hist: vec![(3, 1)],
                io: Io::default(),
                mem: Mem::default(),
                exits: Vec::new(),
            }],
        };
        // Three minutes of one-second windows, at ten o'clock, on each day.
        for age in [1u64, 3, 10] {
            let ten = (today - age) * DAY_MS + 10 * 3_600_000;
            for second in 0..180 {
                store.append(&window(ten + second * 1_000)).unwrap();
            }
        }
        store.prune(today * DAY_MS + 1);
        let lines = |age: u64| -> Vec<Window> {
            std::fs::read_to_string(dir.join(file_name(today - age)))
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect()
        };
        let requests = |windows: &[Window]| -> u64 { windows.iter().flat_map(|w| &w.routes).map(|r| r.n).sum() };
        assert_eq!(lines(1).len(), 180, "yesterday is kept as written");
        assert_eq!(lines(3).len(), 3, "by the minute past two days");
        assert_eq!(lines(10).len(), 1, "by the hour past seven days");
        for age in [1, 3, 10] {
            assert_eq!(requests(&lines(age)), 180, "every request kept, {age} days ago");
        }
        let recorded = read_rollups(&dir);
        assert_eq!(recorded.get(&(today - 3)), Some(&ROLLUP_MINUTE));
        assert_eq!(recorded.get(&(today - 10)), Some(&ROLLUP_HOUR));
        // Read back by the store, the rolled-up days hold the same history.
        let loaded = store.load((today - 10) * DAY_MS, today * DAY_MS);
        assert_eq!(requests(&loaded), 540);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
