//! Purpose:
//! Records compile-phase durations and optional notes for CLI timing output.
//! Reports each successful phase to the interactive progress renderer while
//! retaining the full timing table only when the user requests `--timings`.
//!
//! Called from:
//! - `crate::pipeline::compile()` around each major compiler phase.
//!
//! Key details:
//! - Phase durations are always measured for interactive completed-step lines.
//! - Disabled timing skips only table storage/reporting, so pipeline code does
//!   not branch around every measurement.
//! - Each retained phase also records the process's peak physical footprint at its end, so the
//!   table names the phase that set the build's memory high-water mark (macOS keeps freed heap
//!   in the footprint, so the peak is what matters, not the current value).
//! - Each retained phase also records the process CPU time (user + system) spent since the
//!   previous record. Wall time swings with machine load; CPU time is what an optimization moves,
//!   and a parallel phase shows its fan-out as CPU above wall.

use std::time::{Duration, Instant};

/// Compile timing collector for optional performance profiling.
pub(crate) struct CompileTimings {
    enabled: bool,
    started_at: Instant,
    notes: Vec<String>,
    phases: Vec<PhaseSample>,
    cpu_at_last_record: Option<Duration>,
}

/// One retained phase: its name, wall time, CPU time and the peak footprint at its end.
type PhaseSample = (&'static str, Duration, Option<Duration>, Option<u64>);

/// Box-drawing characters used by the interactive and plain timing tables.
struct TableChars {
    horizontal: char,
    vertical: char,
    top_left: char,
    top_join: char,
    top_right: char,
    middle_left: char,
    middle_join: char,
    middle_right: char,
    bottom_left: char,
    bottom_join: char,
    bottom_right: char,
}

const UNICODE_TABLE: TableChars = TableChars {
    horizontal: '─',
    vertical: '│',
    top_left: '┌',
    top_join: '┬',
    top_right: '┐',
    middle_left: '├',
    middle_join: '┼',
    middle_right: '┤',
    bottom_left: '└',
    bottom_join: '┴',
    bottom_right: '┘',
};

const ASCII_TABLE: TableChars = TableChars {
    horizontal: '-',
    vertical: '|',
    top_left: '+',
    top_join: '+',
    top_right: '+',
    middle_left: '+',
    middle_join: '+',
    middle_right: '+',
    bottom_left: '+',
    bottom_join: '+',
    bottom_right: '+',
};

impl CompileTimings {
    /// Creates a new timing collector.
    ///
    /// `enabled` controls whether detailed timing data is retained and reported.
    /// Phase completion still reaches the interactive progress renderer when
    /// disabled. The internal timer starts immediately.
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            enabled,
            started_at: Instant::now(),
            notes: Vec::new(),
            phases: Vec::new(),
            cpu_at_last_record: process_cpu_time(),
        }
    }

    /// Records the elapsed time since `started_at` for the named phase.
    ///
    /// The duration is always forwarded to the interactive progress renderer.
    /// It is retained for the detailed timing report only when collection is
    /// enabled.
    pub(crate) fn record_since(&mut self, phase: &'static str, started_at: Instant) {
        let elapsed = started_at.elapsed();
        crate::progress::finish_phase(phase, elapsed);
        if self.enabled {
            let cpu_now = process_cpu_time();
            let cpu = match (cpu_now, self.cpu_at_last_record) {
                (Some(now), Some(before)) => Some(now.saturating_sub(before)),
                _ => None,
            };
            self.cpu_at_last_record = cpu_now;
            self.phases.push((phase, elapsed, cpu, peak_footprint_bytes()));
        }
    }

    /// Appends an arbitrary informational note to the timing report.
    ///
    /// No-op when timing collection is disabled. The note is printed verbatim
    /// in order below the timing table.
    pub(crate) fn note(&mut self, note: impl Into<String>) {
        if self.enabled {
            self.notes.push(note.into());
        }
    }

    /// Returns elapsed time since this collector was constructed, regardless of
    /// whether timing collection is enabled. Used for the final success line's
    /// elapsed-seconds suffix even when `--timings` was not passed.
    pub(crate) fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// Prints the collected timing report to stderr.
    ///
    /// Output is gated behind the `enabled` flag. The report renders friendly
    /// phase labels, adaptive durations, percentage shares, and a total row.
    /// Interactive runs use Unicode box drawing and bold header/total rows;
    /// plain runs use an unstyled ASCII table suitable for logs and pipes.
    pub(crate) fn report(&self) {
        if !self.enabled {
            return;
        }

        let decorated = crate::progress::is_decorated();
        let total = self.started_at.elapsed();

        eprintln!("{}", style_if(decorated, "Compiler timings"));
        eprintln!("{}", render_timing_table(&self.phases, total, decorated));
        if !self.notes.is_empty() {
            eprintln!();
            eprintln!("{}", style_if(decorated, "Notes"));
            let bullet = if decorated { "•" } else { "-" };
            for note in &self.notes {
                eprintln!("  {bullet} {note}");
            }
        }
    }
}

/// The process's CPU time so far (user plus system, all threads).
fn process_cpu_time() -> Option<Duration> {
    #[cfg(unix)]
    {
        let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
        let status = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
        if status != 0 {
            return None;
        }
        let usage = unsafe { usage.assume_init() };
        let to_duration = |time: libc::timeval| {
            Duration::from_secs(time.tv_sec as u64) + Duration::from_micros(time.tv_usec as u64)
        };
        Some(to_duration(usage.ru_utime) + to_duration(usage.ru_stime))
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// The process's peak physical footprint so far, in bytes, when the platform reports one.
///
/// macOS: `ledger_phys_footprint_peak` from `TASK_VM_INFO` -- the figure `/usr/bin/time -l` prints
/// as "peak memory footprint". Linux: `VmHWM`, the resident high-water mark.
fn peak_footprint_bytes() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        const TASK_VM_INFO: libc::task_flavor_t = 22;
        const PEAK_WORD: usize = 21;
        let mut info = [0u64; 64];
        // The count is in `natural_t` (32-bit) units.
        let mut count = (info.len() * 2) as libc::mach_msg_type_number_t;
        #[allow(deprecated)]
        let status = unsafe {
            libc::task_info(
                libc::mach_task_self(),
                TASK_VM_INFO,
                info.as_mut_ptr().cast(),
                &mut count,
            )
        };
        if status != 0 || (count as usize) < (PEAK_WORD + 1) * 2 {
            return None;
        }
        Some(info[PEAK_WORD])
    }
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        let line = status.lines().find(|line| line.starts_with("VmHWM:"))?;
        let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
        Some(kib * 1024)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// Formats a byte count for the timing table's peak-memory column.
fn format_bytes(bytes: Option<u64>) -> String {
    match bytes {
        Some(bytes) if bytes >= 1 << 30 => format!("{:.2} GB", bytes as f64 / (1u64 << 30) as f64),
        Some(bytes) => format!("{:.0} MB", bytes as f64 / (1u64 << 20) as f64),
        None => "-".to_string(),
    }
}

/// Returns `part`'s share of `total` as a percentage, or `0.0` when `total` is
/// zero (guards the first phase recorded before any measurable time elapses).
fn percentage(part: Duration, total: Duration) -> f64 {
    if total.as_secs_f64() == 0.0 {
        0.0
    } else {
        part.as_secs_f64() / total.as_secs_f64() * 100.0
    }
}

/// Applies bold terminal styling only for decorated interactive runs.
fn style_if(decorated: bool, text: &str) -> String {
    if decorated {
        console::style(text).bold().to_string()
    } else {
        text.to_string()
    }
}

/// Renders the complete timing table using terminal-appropriate borders.
fn render_timing_table(
    phases: &[PhaseSample],
    total: Duration,
    decorated: bool,
) -> String {
    let chars = if decorated {
        &UNICODE_TABLE
    } else {
        &ASCII_TABLE
    };
    let format_cpu = |cpu: Option<Duration>| {
        cpu.map(crate::progress::format_phase_duration)
            .unwrap_or_else(|| "-".to_string())
    };
    let rows: Vec<[String; 5]> = phases
        .iter()
        .map(|(phase, duration, cpu, peak)| {
            [
                crate::progress::phase_label(phase).to_string(),
                crate::progress::format_phase_duration(*duration),
                format_cpu(*cpu),
                format!("{:.1}%", percentage(*duration, total)),
                format_bytes(*peak),
            ]
        })
        .collect();
    let total_duration = crate::progress::format_phase_duration(total);
    let total_cpu = format_cpu(
        phases
            .iter()
            .map(|(_, _, cpu, _)| *cpu)
            .sum::<Option<Duration>>(),
    );
    let total_share = "100.0%";
    let total_peak = format_bytes(phases.iter().filter_map(|(_, _, _, peak)| *peak).max());
    let header = ["Phase", "Duration", "CPU", "Share", "Peak memory"];
    let total_cells = [
        "Total",
        total_duration.as_str(),
        total_cpu.as_str(),
        total_share,
        total_peak.as_str(),
    ];

    let mut widths = [0usize; 5];
    for (column, width) in widths.iter_mut().enumerate() {
        *width = rows
            .iter()
            .map(|row| row[column].chars().count())
            .chain([header[column].len(), total_cells[column].chars().count()])
            .max()
            .unwrap_or(0);
    }

    let mut lines = Vec::with_capacity(rows.len() + 6);
    lines.push(format_table_rule(
        chars,
        &widths,
        chars.top_left,
        chars.top_join,
        chars.top_right,
    ));
    let header_row = format_table_row(chars, header, &widths);
    lines.push(style_if(decorated, &header_row));
    lines.push(format_table_rule(
        chars,
        &widths,
        chars.middle_left,
        chars.middle_join,
        chars.middle_right,
    ));
    let has_phase_rows = !rows.is_empty();
    for row in &rows {
        lines.push(format_table_row(chars, row.each_ref().map(String::as_str), &widths));
    }
    if has_phase_rows {
        lines.push(format_table_rule(
        chars,
        &widths,
            chars.middle_left,
            chars.middle_join,
            chars.middle_right,
        ));
    }
    let total_row = format_table_row(chars, total_cells, &widths);
    lines.push(style_if(decorated, &total_row));
    lines.push(format_table_rule(
        chars,
        &widths,
        chars.bottom_left,
        chars.bottom_join,
        chars.bottom_right,
    ));
    lines.join("\n")
}

/// Formats one table row: the phase left-aligned, the numeric columns right-aligned.
fn format_table_row(chars: &TableChars, cells: [&str; 5], widths: &[usize; 5]) -> String {
    let mut row = String::new();
    for (index, (cell, width)) in cells.iter().zip(widths.iter()).enumerate() {
        row.push(chars.vertical);
        if index == 0 {
            row.push_str(&format!(" {cell:<width$} "));
        } else {
            row.push_str(&format!(" {cell:>width$} "));
        }
    }
    row.push(chars.vertical);
    row
}

/// Formats a horizontal table border or separator for the computed column widths.
fn format_table_rule(
    chars: &TableChars,
    widths: &[usize; 5],
    left: char,
    join: char,
    right: char,
) -> String {
    let segments: Vec<String> = widths
        .iter()
        .map(|width| chars.horizontal.to_string().repeat(width + 2))
        .collect();
    format!("{left}{}{right}", segments.join(&join.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies disabled detailed timings do not retain phase samples.
    #[test]
    fn disabled_timings_do_not_store_phases() {
        let mut timings = CompileTimings::new(false);
        timings.record_since("read", Instant::now());
        assert!(timings.phases.is_empty());
    }

    /// Verifies percentage formatting avoids division by zero.
    #[test]
    fn percentage_of_zero_total_is_zero() {
        assert_eq!(percentage(Duration::from_millis(5), Duration::ZERO), 0.0);
    }

    /// Verifies phase shares are computed against total elapsed time.
    #[test]
    fn percentage_computes_share_of_total() {
        let pct = percentage(Duration::from_millis(25), Duration::from_millis(100));
        assert!((pct - 25.0).abs() < 0.001);
    }

    /// Verifies plain timing titles remain unstyled.
    #[test]
    fn style_if_plain_is_unchanged() {
        assert_eq!(style_if(false, "Compiler timings"), "Compiler timings");
    }

    /// Verifies decorated timing headers retain their original text.
    #[test]
    fn style_if_decorated_contains_original_text() {
        assert!(style_if(true, "Compiler timings").contains("Compiler timings"));
    }

    /// Verifies plain reports use ASCII borders, friendly labels, and percentages.
    #[test]
    fn plain_timing_table_is_ascii_and_complete() {
        let table = render_timing_table(
            &[("read", Duration::from_millis(25), Some(Duration::from_millis(40)), None)],
            Duration::from_millis(100),
            false,
        );
        assert!(table.starts_with('+'));
        assert!(table.ends_with('+'));
        assert!(table.contains("| Reading source "));
        assert!(table.contains("25.00 ms"));
        assert!(table.contains("25.0%"));
        assert!(table.contains("40.00 ms"));
        assert!(table.contains("| Total"));
        assert!(!table.contains('┌'));
    }

    /// Verifies interactive reports use Unicode borders and adaptive durations.
    #[test]
    fn decorated_timing_table_uses_unicode_and_friendly_labels() {
        let table = render_timing_table(
            &[("ir-opt", Duration::from_millis(1_250), None, Some(3 << 30))],
            Duration::from_millis(2_500),
            true,
        );
        assert!(table.starts_with('┌'));
        assert!(table.ends_with('┘'));
        assert!(table.contains('┼'));
        assert!(table.contains("Optimizing EIR"));
        assert!(table.contains("1.25 s"));
        assert!(table.contains("50.0%"));
        assert!(table.contains("3.00 GB"));
    }

    /// Verifies every line in the plain table has the same visible width.
    #[test]
    fn plain_timing_table_columns_align() {
        let table = render_timing_table(
            &[
                ("read", Duration::from_micros(500), Some(Duration::from_millis(3)), Some(5 << 20)),
                ("autoload-run", Duration::from_millis(1_250), None, None),
            ],
            Duration::from_millis(2_000),
            false,
        );
        let widths: Vec<usize> = table.lines().map(|line| line.chars().count()).collect();
        assert!(widths.windows(2).all(|pair| pair[0] == pair[1]));
    }

    /// Verifies an empty report uses one separator between the header and total row.
    #[test]
    fn empty_timing_table_has_no_adjacent_separators() {
        let table = render_timing_table(&[], Duration::ZERO, false);
        let lines: Vec<&str> = table.lines().collect();
        assert_eq!(lines.len(), 5);
        assert!(lines[1].contains("Phase"));
        assert!(lines[3].contains("Total"));
    }

    /// Verifies each recorded phase carries the CPU time spent since the previous record.
    #[test]
    fn recorded_phases_carry_cpu_time() {
        let mut timings = CompileTimings::new(true);
        let started = Instant::now();
        let mut spin = 0u64;
        while started.elapsed() < Duration::from_millis(30) {
            spin = std::hint::black_box(spin.wrapping_add(1));
        }
        timings.record_since("read", started);
        let (_, _, cpu, _) = timings.phases[0];
        #[cfg(unix)]
        assert!(cpu.is_some_and(|cpu| cpu >= Duration::from_millis(10)), "{cpu:?}");
        let table = render_timing_table(&timings.phases, timings.elapsed(), false);
        assert!(table.contains(" CPU |"));
    }
}
