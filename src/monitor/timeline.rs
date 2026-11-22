//! The profile page's timeline: every call where and when it ran, and the
//! heap's bytes along the run.
//!
//! The exact instrumentation records each call's span (its start, its length,
//! its depth on the stack and the heap in bytes when it ended) when
//! `ELEPHC_INSTR_TIMELINE` names a file, and writes them there at exit. This
//! module reads that file back and shapes it for the page: calls too short to
//! show at any zoom the page offers are left out once the run holds more than
//! the page can carry, and the memory curve is reduced to a few thousand
//! points that keep every peak.

use std::path::{Path, PathBuf};

/// The most calls a page carries. A request of a large application makes far
/// more; past this, the shortest go first, so what is left is still every call
/// long enough to draw.
const MAX_SPANS: usize = 60_000;

/// Points kept of the memory curve: a few per pixel of the widest timeline.
const MEMORY_POINTS: usize = 2_400;

/// A unique file for one run to write its timeline to.
pub(crate) fn scratch_path() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "elephc-timeline-{}-{nanos}.txt",
        std::process::id()
    ))
}

/// One recorded call.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Span {
    id: u32,
    depth: u32,
    start_ns: u64,
    duration_ns: u64,
    heap_bytes: u64,
    /// The heap when it started, from the call's own reading (0 in a file written
    /// before it was recorded): the page's memory change is end minus this (review).
    heap_start: u64,
    /// Its length minus its direct callees', all of them: measured before the page's
    /// cut, so a call does not take the work of the short callees the cut drops (review).
    own_ns: u64,
}

/// Reads the run's timeline file and returns the page's `timeline` object, or
/// `None` when the run wrote none (a build without the instrumentation, a
/// crash before exit) or recorded no call.
pub(crate) fn read(path: &Path) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(path).ok()?;
    shape(&text)
}

/// Parses the file's text and shapes it for the page.
fn shape(text: &str) -> Option<serde_json::Value> {
    let mut lines = text.lines();
    if lines.next()? != "elephc-timeline 1" {
        return None;
    }
    let mut names: Vec<String> = Vec::new();
    let mut spans: Vec<Span> = Vec::new();
    let mut dropped = 0u64;
    let mut end_ns = 0u64;
    let mut converted = true;
    for line in lines {
        let (key, rest) = line.split_once(' ').unwrap_or((line, ""));
        match key {
            "converted" => converted = rest.trim() == "1",
            "dropped" => dropped = rest.trim().parse().unwrap_or(0),
            // When the run ended: past the recording limit, after the last recorded call.
            "end" => end_ns = rest.trim().parse().unwrap_or(0),
            "name" => names.push(rest.to_string()),
            "span" => {
                let mut f = rest.split(' ').map(|v| v.parse::<u64>());
                if let (Some(Ok(id)), Some(Ok(depth)), Some(Ok(start)), Some(Ok(len)), Some(Ok(heap))) =
                    (f.next(), f.next(), f.next(), f.next(), f.next())
                {
                    let heap_start = f.next().and_then(Result::ok).unwrap_or(heap);
                    spans.push(Span {
                        id: id as u32,
                        depth: depth as u32,
                        start_ns: start,
                        duration_ns: len,
                        heap_bytes: heap,
                        heap_start,
                        own_ns: len,
                    });
                }
            }
            _ => {}
        }
    }
    if spans.is_empty() {
        return None;
    }
    let total_ns = spans
        .iter()
        .map(|s| s.start_ns + s.duration_ns)
        .max()
        .unwrap_or(0)
        .max(end_ns)
        .max(1);
    let memory = memory_curve(&spans, total_ns);
    own_times(&mut spans);
    let recorded = spans.len();
    let kept = keep_longest(spans, MAX_SPANS);
    // Only the names the kept calls use, renumbered, so a page does not carry
    // the whole program's function table for a run that touched a few.
    let mut index_of: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    let mut used_names: Vec<String> = Vec::new();
    let mut flat: Vec<u64> = Vec::with_capacity(kept.len() * 7);
    for span in &kept {
        let index = *index_of.entry(span.id).or_insert_with(|| {
            used_names.push(
                names
                    .get(span.id as usize)
                    .cloned()
                    .unwrap_or_else(|| format!("#{}", span.id)),
            );
            used_names.len() - 1
        });
        flat.extend([index as u64, span.depth as u64, span.start_ns, span.duration_ns, span.own_ns, span.heap_start, span.heap_bytes]);
    }
    Some(serde_json::json!({
        "totalNs": total_ns,
        "converted": converted,
        "names": used_names,
        // [name index, depth, start ns, duration ns, own ns, heap at start, heap at end]
        // per call, by start.
        "spans": flat,
        // [ns, heap bytes] pairs along the run.
        "memory": memory,
        "recorded": recorded,
        "shown": kept.len(),
        "dropped": dropped,
    }))
}

/// Sets each call's own time: its length minus its direct callees'. Walked in start
/// order, a call's parent is the nearest open call one level up.
fn own_times(spans: &mut [Span]) {
    let mut order: Vec<usize> = (0..spans.len()).collect();
    order.sort_by(|&a, &b| spans[a].start_ns.cmp(&spans[b].start_ns).then(spans[a].depth.cmp(&spans[b].depth)));
    let mut open: Vec<usize> = Vec::new();
    for &i in &order {
        let (depth, start, len) = (spans[i].depth, spans[i].start_ns, spans[i].duration_ns);
        while let Some(&top) = open.last() {
            if spans[top].depth >= depth || spans[top].start_ns + spans[top].duration_ns <= start {
                open.pop();
            } else {
                break;
            }
        }
        if let Some(&parent) = open.last() {
            if spans[parent].depth + 1 == depth {
                spans[parent].own_ns = spans[parent].own_ns.saturating_sub(len);
            }
        }
        open.push(i);
    }
}

/// The calls a page keeps: all of them when they fit, else the longest
/// `max`, in start order. Sorting by start is also what lets the page draw
/// them in one pass.
fn keep_longest(mut spans: Vec<Span>, max: usize) -> Vec<Span> {
    if spans.len() > max {
        spans.sort_by(|a, b| b.duration_ns.cmp(&a.duration_ns));
        spans.truncate(max);
    }
    spans.sort_by(|a, b| a.start_ns.cmp(&b.start_ns).then(a.depth.cmp(&b.depth)));
    spans
}

/// The heap along the run, from the bytes each call ended with, reduced to
/// at most `MEMORY_POINTS` points. Each slice of the run keeps its highest
/// reading, so a peak survives the reduction.
fn memory_curve(spans: &[Span], total_ns: u64) -> Vec<u64> {
    let mut ends: Vec<(u64, u64)> = spans
        .iter()
        .map(|s| (s.start_ns + s.duration_ns, s.heap_bytes))
        .collect();
    ends.sort_by_key(|e| e.0);
    let mut out: Vec<u64> = Vec::new();
    if let Some(first) = ends.first() {
        out.extend([0, first.1]);
    }
    let slice = (total_ns / MEMORY_POINTS as u64).max(1);
    let mut i = 0;
    while i < ends.len() {
        let bucket = ends[i].0 / slice;
        let mut peak = ends[i];
        while i < ends.len() && ends[i].0 / slice == bucket {
            if ends[i].1 >= peak.1 {
                peak = ends[i];
            }
            i += 1;
        }
        out.extend([peak.0, peak.1]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "elephc-timeline 1\nconverted 1\ndropped 2\nname {main}\nname load\n\
                        span 1 1 100 50 4096 1024\nspan 1 1 200 40 8192 4096\nspan 0 0 0 400 2048\n";

    #[test]
    /// The page gets the calls by start, the names they use, the heap curve
    /// from their ends, and how many the run recorded.
    fn the_file_shapes_into_the_page_timeline() {
        let t = shape(FILE).expect("a timeline");
        assert_eq!(t["totalNs"], 400);
        assert_eq!(t["names"], serde_json::json!(["{main}", "load"]));
        assert_eq!(
            t["spans"],
            serde_json::json!([0, 0, 0, 400, 310, 2048, 2048, 1, 1, 100, 50, 50, 1024, 4096, 1, 1, 200, 40, 40, 4096, 8192]),
            "{{main}} owns 400 minus its two callees; each call carries its heap at start and end, a span written without its start reading reads it as its end"
        );
        assert_eq!(t["recorded"], 3);
        assert_eq!(t["dropped"], 2);
        let memory = t["memory"].as_array().unwrap();
        assert_eq!(memory[..2], [serde_json::json!(0), serde_json::json!(4096)]);
        assert!(memory.chunks(2).any(|p| p[1] == 8192), "the peak survives: {memory:?}");
    }

    #[test]
    /// Past the cap the shortest calls go, and what is kept stays in start
    /// order.
    fn the_longest_calls_are_kept_in_start_order() {
        let spans = (0..10)
            .map(|i| Span { id: 0, depth: 0, start_ns: i * 10, duration_ns: i, heap_bytes: 0, heap_start: 0, own_ns: i })
            .collect();
        let kept = keep_longest(spans, 3);
        assert_eq!(kept.iter().map(|s| s.duration_ns).collect::<Vec<_>>(), vec![7, 8, 9]);
    }

    #[test]
    /// A call's own time is measured before the cut: the short callees the page
    /// drops do not become their caller's work (review).
    fn own_time_survives_the_cut() {
        let mut text = String::from("elephc-timeline 1\nconverted 1\nname {main}\nname tiny\nspan 0 0 0 1000 0\n");
        for i in 0..90 {
            text.push_str(&format!("span 1 1 {} 10 0\n", i * 10));
        }
        let spans: Vec<Span> = {
            let mut parsed: Vec<Span> = text.lines().filter_map(|l| l.strip_prefix("span ")).map(|r| {
                let f: Vec<u64> = r.split(' ').map(|v| v.parse().unwrap()).collect();
                Span { id: f[0] as u32, depth: f[1] as u32, start_ns: f[2], duration_ns: f[3], heap_bytes: f[4], heap_start: f[4], own_ns: f[3] }
            }).collect();
            own_times(&mut parsed);
            keep_longest(parsed, 1)
        };
        assert_eq!((spans[0].duration_ns, spans[0].own_ns), (1000, 100), "90 calls of 10 ns were the callees'");
    }

    #[test]
    /// A run past the recording limit still spans its whole length: the file's `end`
    /// sets the total, and the dropped calls are reported (review).
    fn a_long_run_spans_its_end_past_the_recording_limit() {
        let t = shape("elephc-timeline 1\nconverted 1\ndropped 12\nend 9000\nname {main}\nspan 0 0 0 400 0\n").expect("a timeline");
        assert_eq!((t["totalNs"].as_u64(), t["dropped"].as_u64()), (Some(9_000), Some(12)));
    }

    #[test]
    /// Anything that is not a timeline file is refused rather than misread.
    fn a_foreign_file_is_refused() {
        assert!(shape("{\"traceEvents\":[]}").is_none());
        assert!(shape("elephc-timeline 1\nconverted 1\n").is_none());
    }
}
