//! Purpose:
//! The captures a store keeps, dated: the exact profiles of single requests the collector
//! captured (`--capture-every`), the profiles captured from the page, and the runs a script
//! was monitored through. Kept on disk beside the windows, so the monitor page can show the
//! captures of any period the store still holds, not only those of the collector's memory.
//!
//! Called from:
//! - The collector (`write_due` / `finish`), as each hour of captures closes.
//! - The page's API (`api::traces_json`, `api::page_html`), for a date range or one capture.
//!
//! Key details:
//! - One directory per UTC day, `apm-captures-YYYY-MM-DD/`: an `index.jsonl` with one line per
//!   capture (what the page lists) and one `<id>.json.gz` per capture (its call graph). A day
//!   is one directory to delete when it falls out of retention, with the day's windows.
//! - A capture's id is its time in milliseconds times a thousand plus a sequence number, so
//!   the id alone names the day its graph is in.
//! - The graph is written to a temporary name and renamed, and the index line is appended
//!   after it: a crash leaves no index line pointing at a missing or half-written graph.
//! - Only `apm-captures-YYYY-MM-DD/` directories are ever read or deleted: the directory is
//!   the user's.

use std::io::{Read as _, Write as _};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::store::{day_of, file_name, parse_file_name, Store};

/// What the page lists about one stored capture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct CaptureRecord {
    /// `t * 1000 + sequence`: unique, and names the capture's day.
    pub id: u64,
    /// When it was captured, Unix milliseconds.
    pub t: u64,
    /// `trace` (a request the collector captured), `profile` (captured from the page) or
    /// `run` (a script's run).
    pub kind: String,
    /// The transaction it belongs to.
    pub route: String,
    /// The answer's status, when known.
    pub status: Option<u16>,
    /// Its measured wall time, nanoseconds.
    pub duration_ns: u64,
    /// Functions in its call graph.
    pub functions: usize,
    /// Database queries it ran.
    pub queries: u64,
    /// The service it was captured from: a page over a store several collectors share lists
    /// its own service's captures, as it reads its own windows (review). Empty for a run.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
}

/// The id of a capture taken at `t_ms`, the `sequence`-th of its millisecond.
pub(crate) fn capture_id(t_ms: u64, sequence: u64) -> u64 {
    t_ms.saturating_mul(1_000) + sequence % 1_000
}

/// A fresh id for a capture taken at `t_ms`: its sequence counts this process's captures
/// from an offset of its own, so two captures of one millisecond differ, from one writer or
/// from two sharing the store (review). `append_capture` still moves past an id taken.
pub(crate) fn next_capture_id(t_ms: u64) -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    static OFFSET: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    let offset = *OFFSET.get_or_init(|| {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos() as u64);
        (u64::from(std::process::id()) * 7_919 + nanos) % 1_000
    });
    capture_id(t_ms, offset + NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
}

/// The time a capture id was taken at.
pub(crate) fn capture_time(id: u64) -> u64 {
    id / 1_000
}

/// `apm-captures-YYYY-MM-DD` for a day number.
pub(crate) fn captures_dir_name(day: u64) -> String {
    file_name(day).replacen("apm-", "apm-captures-", 1).trim_end_matches(".jsonl").to_string()
}

/// The day number of a captures directory name, or `None` for anything else.
pub(crate) fn parse_captures_dir_name(name: &str) -> Option<u64> {
    let date = name.strip_prefix("apm-captures-")?;
    parse_file_name(&format!("apm-{date}.jsonl"))
}

impl Store {
    /// The directory holding one day's captures.
    fn captures_dir(&self, day: u64) -> PathBuf {
        self.dir().join(captures_dir_name(day))
    }

    /// Writes one capture: its graph, then its index line.
    pub(crate) fn append_capture(
        &self,
        record: &CaptureRecord,
        graph: &crate::call_graph::CallGraph,
    ) -> Result<u64, String> {
        let dir = self.captures_dir(day_of(record.t));
        std::fs::create_dir_all(&dir).map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
        let json = serde_json::to_vec(graph).map_err(|error| format!("cannot encode a capture: {error}"))?;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(&json).map_err(|error| format!("cannot compress a capture: {error}"))?;
        let compressed = encoder.finish().map_err(|error| format!("cannot compress a capture: {error}"))?;
        // A temporary name of this writer's own: two writers giving one capture the same id
        // wrote the same `.partial`, and one could publish the other's half-written graph
        // (review). Process and a per-process count keep it apart.
        static PARTIALS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let partial = dir.join(format!(
            "{}.{}.{}.json.gz.partial",
            record.id,
            std::process::id(),
            PARTIALS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::write(&partial, compressed).map_err(|error| format!("cannot write {}: {error}", partial.display()))?;
        // Linked to its name, which fails when the name is taken: a capture of the same
        // millisecond by another writer moves to the next free id, never over the other's
        // graph (review).
        let mut record = record.clone();
        let base = record.id;
        loop {
            let path = dir.join(format!("{}.json.gz", record.id));
            match std::fs::hard_link(&partial, &path) {
                Ok(()) => break,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && record.id < capture_id(capture_time(base), 999) => record.id += 1,
                Err(error) => {
                    let _ = std::fs::remove_file(&partial);
                    return Err(format!("cannot write {}: {error}", path.display()));
                }
            }
        }
        let _ = std::fs::remove_file(&partial);
        let mut line = serde_json::to_string(&record).map_err(|error| format!("cannot encode a capture: {error}"))?;
        line.push('\n');
        let index = dir.join("index.jsonl");
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&index)
            .map_err(|error| format!("cannot open {}: {error}", index.display()))?;
        file.write_all(line.as_bytes()).map_err(|error| format!("cannot write {}: {error}", index.display()))?;
        Ok(record.id)
    }

    /// The captures taken in `[from_ms, to_ms]`, oldest first.
    pub(crate) fn captures_in(&self, from_ms: u64, to_ms: u64) -> Vec<CaptureRecord> {
        if from_ms > to_ms {
            return Vec::new();
        }
        let mut out = Vec::new();
        for day in day_of(from_ms)..=day_of(to_ms) {
            let Ok(text) = std::fs::read_to_string(self.captures_dir(day).join("index.jsonl")) else {
                continue;
            };
            out.extend(
                text.lines()
                    .filter_map(|line| serde_json::from_str::<CaptureRecord>(line).ok())
                    .filter(|record| record.t >= from_ms && record.t <= to_ms),
            );
        }
        out.sort_by_key(|record| (record.t, record.id));
        out
    }

    /// One stored capture's record and call graph.
    pub(crate) fn capture(&self, id: u64) -> Option<(CaptureRecord, crate::call_graph::CallGraph)> {
        let t = capture_time(id);
        let record = self.captures_in(t, t).into_iter().find(|record| record.id == id)?;
        let file = std::fs::File::open(self.captures_dir(day_of(t)).join(format!("{id}.json.gz"))).ok()?;
        let mut json = Vec::new();
        flate2::read::GzDecoder::new(file).read_to_end(&mut json).ok()?;
        let graph = serde_json::from_slice(&json).ok()?;
        Some((record, graph))
    }

    /// Deletes the captures of the days before `keep_from`.
    pub(crate) fn prune_captures(&self, keep_from: u64) {
        let Ok(entries) = std::fs::read_dir(self.dir()) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if parse_captures_dir_name(&name).is_some_and(|day| day < keep_from) {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A graph with one function, `name`.
    fn graph(name: &str) -> crate::call_graph::CallGraph {
        let text = format!("elephc-instr: {name} calls=1 incl_ns=1000 excl_ns=1000\n");
        crate::monitor::parse_instrument_dump(&text)
    }

    /// Captures come back by date range and by id, graph included, and only the
    /// `apm-captures-*` directories of past days are pruned.
    #[test]
    fn captures_are_stored_by_day_and_read_back() {
        let dir = std::env::temp_dir().join(format!("elephc-apm-captures-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::open(dir.to_str().unwrap(), 7).unwrap();
        let day = 86_400_000u64;
        let monday = 20_000 * day + 3_600_000;
        let record = |t: u64, route: &str| CaptureRecord {
            id: capture_id(t, 1),
            t,
            kind: "trace".to_string(),
            route: route.to_string(),
            status: Some(200),
            duration_ns: 5_000_000,
            functions: 1,
            queries: 0,
            source: String::new(),
        };
        let early = record(monday, "GET /a");
        let late = record(monday + day, "GET /b");
        store.append_capture(&early, &graph("first")).unwrap();
        store.append_capture(&late, &graph("second")).unwrap();
        assert_eq!(store.captures_in(monday - 1, monday + 1), vec![early.clone()]);
        assert_eq!(store.captures_in(monday, monday + day), vec![early.clone(), late.clone()]);
        let (found, stored) = store.capture(late.id).expect("stored");
        assert_eq!(found, late);
        assert!(stored.nodes.iter().any(|node| node.name == "second"));
        assert!(store.capture(capture_id(monday, 2)).is_none(), "an unknown id");
        std::fs::write(dir.join("apm-captures-notes"), "mine").unwrap();
        store.prune_captures(day_of(monday + day));
        assert!(store.captures_in(monday, monday).is_empty(), "the past day is gone");
        assert_eq!(store.captures_in(monday + day, monday + day), vec![late]);
        assert!(dir.join("apm-captures-notes").exists(), "anything else is the user's");
        // Two writers giving one millisecond's capture the same id: the second moves to the
        // next free one, and each keeps its own graph (review).
        let clash = record(monday + day + 5, "GET /c");
        assert_eq!(store.append_capture(&clash, &graph("one")).unwrap(), clash.id);
        let moved = store.append_capture(&clash, &graph("two")).unwrap();
        assert_eq!(moved, clash.id + 1);
        assert!(store.capture(clash.id).unwrap().1.nodes.iter().any(|node| node.name == "one"));
        assert!(store.capture(moved).unwrap().1.nodes.iter().any(|node| node.name == "two"));
        assert_ne!(next_capture_id(monday), next_capture_id(monday), "fresh ids differ");
        let leftovers = std::fs::read_dir(dir.join(captures_dir_name(day_of(clash.t)))).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".partial")).count();
        assert_eq!(leftovers, 0, "each writer's temporary file is gone once linked");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Directory names round-trip, and nothing else is taken for one.
    #[test]
    fn captures_dir_names_round_trip() {
        assert_eq!(captures_dir_name(20_407), "apm-captures-2025-11-15");
        assert_eq!(parse_captures_dir_name("apm-captures-2025-11-15"), Some(20_407));
        assert_eq!(parse_captures_dir_name("apm-2025-11-15.jsonl"), None);
        assert_eq!(parse_captures_dir_name("apm-captures-2025-13-01"), None);
        assert_eq!(capture_time(capture_id(1_700_000_000_123, 7)), 1_700_000_000_123);
    }
}
