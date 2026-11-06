//! Purpose:
//! The OPcache accelerator's own diagnostic channel — php-src's `zend_accel_error`.
//! It is NOT PHP's error reporting: it writes a timestamped, pid-tagged line to
//! stderr (or to `opcache.error_log`), it is gated by `opcache.log_verbosity_level`
//! rather than by `error_reporting`, and a FATAL exits the process outright.
//!
//! Called from:
//! - `crate::script_cache::store` for the file-cache directory refusal.
//! - `crate::ffi::context::__elephc_eval_configure_opcache()` to install the gate.
//!
//! Key details:
//! - The line shape is `zend_accelerator_debug.c`'s, reproduced field for field:
//!   `asctime(localtime(t))` truncated at 24 bytes, ` (<pid>): `, a level word with a
//!   TRAILING SPACE (`Fatal Error `, `Error `, `Warning `, `Message `, `Debug `), the
//!   message, a newline, flushed. VERIFIED against reference PHP 8.5.6:
//!   `Sat Sep 12 08:33:47 2026 (79737): Fatal Error opcache.file_cache must be a full
//!   path of an accessible directory`.
//! - The timestamp comes from libc's `localtime` — the same zone-aware conversion
//!   php-src uses — and is then spelled out by `format_asctime`, a byte-for-byte
//!   reimplementation of C's `asctime`. libc's own `asctime` is NOT called: the `libc`
//!   crate does not expose it on Linux, and it broke the build on two of the five
//!   supported targets when this module was first wired in. Reimplementing it is safe
//!   because `asctime`'s output is fixed by the C standard to a single format string
//!   with the C-locale day and month abbreviations — it is locale-INdependent, so the
//!   only zone-dependent part is `localtime`, which is still libc's.
//! - The gate is `level <= log_verbosity_level`, and the DEFAULT level is 1. FATAL(0)
//!   and ERROR(1) therefore always print; WARNING(2) and below only with an explicit
//!   raise — which is why reference PHP looks silent about `opcache.preload_user`
//!   until you pass `-d opcache.log_verbosity_level=2` (VERIFIED both ways).
//! - Error handling happens EVEN WHEN THE LINE IS NOT LOGGED: php-src runs the
//!   `switch (type)` outside the verbosity guard, so a FATAL at verbosity 0 still
//!   exits. `exit(-2)` is an exit STATUS of 254, which is what reference returns.

use std::cell::RefCell;
use std::io::Write;

/// php-src's `ACCEL_LOG_*` levels, with their exact numeric values: the gate is a
/// `<=` against `opcache.log_verbosity_level`, so the ORDER and the NUMBERS are the
/// contract, not an internal detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub(crate) enum AccelLogLevel {
    /// Logged at every verbosity, and exits the process with status 254.
    Fatal = 0,
    /// Logged at every verbosity.
    #[allow(dead_code)]
    Error = 1,
    /// Needs `opcache.log_verbosity_level >= 2`.
    #[allow(dead_code)]
    Warning = 2,
    /// Needs `>= 3`. Spelled `Message ` in the output, not `Info `.
    #[allow(dead_code)]
    Info = 3,
    /// Needs `>= 4`.
    #[allow(dead_code)]
    Debug = 4,
}

impl AccelLogLevel {
    /// The level word php-src prints, INCLUDING its trailing space.
    const fn label(self) -> &'static str {
        match self {
            Self::Fatal => "Fatal Error ",
            Self::Error => "Error ",
            Self::Warning => "Warning ",
            Self::Info => "Message ",
            Self::Debug => "Debug ",
        }
    }
}

/// The two directives that decide where an accelerator diagnostic goes and whether it
/// is written at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AccelLogConfig {
    /// `opcache.log_verbosity_level`. php-src's default is 1.
    pub(crate) verbosity: i32,
    /// `opcache.error_log`. Empty, or the literal `stderr`, means stderr — php-src
    /// tests all three of those cases before opening a file.
    pub(crate) error_log: String,
}

impl AccelLogConfig {
    /// The configuration a binary without generated OPcache wiring observes: php-src's
    /// own defaults, so a harness linking this archive directly behaves like reference.
    pub(crate) const fn defaults() -> Self {
        Self {
            verbosity: 1,
            error_log: String::new(),
        }
    }
}

thread_local! {
    static ACCEL_LOG_CONFIG: RefCell<AccelLogConfig> = RefCell::new(AccelLogConfig::defaults());
}

/// Installs the compile-time accelerator log configuration for the current thread.
pub(crate) fn set_config(config: AccelLogConfig) {
    ACCEL_LOG_CONFIG.with(|cell| *cell.borrow_mut() = config);
}

/// Returns a copy of the configuration active on the current thread.
pub(crate) fn config() -> AccelLogConfig {
    ACCEL_LOG_CONFIG.with(|cell| cell.borrow().clone())
}

/// Emits one accelerator diagnostic, then performs the level's own error handling.
///
/// Mirrors `zend_accel_error_va_args`: the write is gated by the verbosity, the error
/// handling is NOT. A `Fatal` therefore terminates the process whether or not anything
/// was printed — with status 254, php-src's `exit(-2)` as the shell sees it.
pub(crate) fn accel_error(level: AccelLogLevel, message: &str) -> ! {
    accel_log(level, message);
    // Only `Fatal` reaches this function's `!` return; every other level goes through
    // `accel_log` directly. Keeping the exit here rather than inside `accel_log` is what
    // lets the non-fatal levels return normally.
    std::process::exit(254);
}

/// Emits one accelerator diagnostic and returns, doing no error handling.
///
/// Silently does nothing when `level` is above the configured verbosity. A log file that
/// cannot be opened falls back to stderr rather than being dropped, exactly as php-src does.
pub(crate) fn accel_log(level: AccelLogLevel, message: &str) {
    let config = config();
    if (level as i32) > config.verbosity {
        return;
    }
    let line = format!(
        "{} ({}): {}{}\n",
        local_time_string(),
        std::process::id(),
        level.label(),
        message
    );
    if config.error_log.is_empty() || config.error_log == "stderr" {
        let mut stderr = std::io::stderr();
        let _ = stderr.write_all(line.as_bytes());
        let _ = stderr.flush();
        return;
    }
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&config.error_log)
    {
        Ok(mut file) => {
            let _ = file.write_all(line.as_bytes());
            let _ = file.flush();
        }
        Err(_) => {
            // php-src falls back to stderr on a failed `fopen` rather than losing the line.
            let mut stderr = std::io::stderr();
            let _ = stderr.write_all(line.as_bytes());
            let _ = stderr.flush();
        }
    }
}

/// Returns the 24-character local timestamp php-src prints.
///
/// `asctime(localtime(&t))` with `time_string[24] = 0` — that is the fixed-width C form
/// `Sat Sep 12 08:33:47 2026`, with the trailing newline `asctime` appends chopped off by
/// the truncation.
fn local_time_string() -> String {
    // SAFETY: `time` accepts a null pointer and returns the timestamp; `localtime` returns a
    // pointer to a static `struct tm` owned by libc, which is read out here before any other
    // libc call on this thread can overwrite it. It can answer null (a timestamp it cannot
    // represent), which is why it is checked rather than dereferenced blind.
    let tm = unsafe {
        let now = libc::time(std::ptr::null_mut());
        let tm = libc::localtime(&now);
        if tm.is_null() {
            return String::new();
        }
        *tm
    };
    format_asctime(&tm)
}

/// C's `asctime`, truncated to 24 bytes the way php-src truncates it.
///
/// The C standard fixes `asctime` to exactly one format —
/// `"%.3s %.3s%3d %.2d:%.2d:%.2d %d\n"` over C-locale abbreviations — so writing it out
/// here cannot drift from libc's: there is nothing locale- or platform-dependent left in
/// it once `localtime` has done the zone conversion. Truncating at 24 drops the newline,
/// and also drops the extra digits of a 5-digit year exactly as php-src's fixed buffer does.
///
/// Out-of-range `tm_wday`/`tm_mon` (which libc's own `asctime` has undefined behaviour for)
/// answer an empty string rather than indexing out of bounds.
fn format_asctime(tm: &libc::tm) -> String {
    const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let (Ok(wday), Ok(mon)) = (usize::try_from(tm.tm_wday), usize::try_from(tm.tm_mon)) else {
        return String::new();
    };
    let (Some(day), Some(month)) = (DAYS.get(wday), MONTHS.get(mon)) else {
        return String::new();
    };
    let text = format!(
        "{} {}{:3} {:02}:{:02}:{:02} {}",
        day,
        month,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec,
        tm.tm_year as i64 + 1900,
    );
    match text.char_indices().nth(24) {
        Some((cut, _)) => text[..cut].to_string(),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a `tm` field by field: its layout differs across platforms, and several of
    /// its members (`tm_gmtoff`, `tm_zone`) exist only on some of them.
    fn tm(year: i32, mon: i32, mday: i32, wday: i32, hour: i32, min: i32, sec: i32) -> libc::tm {
        // SAFETY: `libc::tm` is a plain C struct of integers and one pointer; an all-zero
        // value is a valid one (it spells the Unix epoch), and every field the formatter
        // reads is overwritten below.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        tm.tm_year = year - 1900;
        tm.tm_mon = mon;
        tm.tm_mday = mday;
        tm.tm_wday = wday;
        tm.tm_hour = hour;
        tm.tm_min = min;
        tm.tm_sec = sec;
        tm
    }

    /// The exact line reference PHP 8.5.10 printed, minus the pid and message:
    /// `Sat Sep 12 08:33:47 2026 (79737): Fatal Error opcache.file_cache must be ...`.
    #[test]
    fn spells_the_reference_timestamp() {
        let formatted = format_asctime(&tm(2026, 8, 12, 6, 8, 33, 47));
        assert_eq!(formatted, "Sat Sep 12 08:33:47 2026");
        assert_eq!(formatted.len(), 24);
    }

    /// `%3d` on the day of month, NOT `%02d`: single-digit days are SPACE-padded, which is
    /// what puts two spaces between the month and the day. Getting this wrong would shorten
    /// the line by a byte and still look plausible.
    #[test]
    fn pads_a_single_digit_day_with_a_space() {
        assert_eq!(
            format_asctime(&tm(2026, 0, 1, 4, 0, 0, 0)),
            "Thu Jan  1 00:00:00 2026"
        );
    }

    /// The time fields ARE zero-padded, and the whole line stays 24 bytes wide.
    #[test]
    fn zero_pads_the_time_fields() {
        let formatted = format_asctime(&tm(1999, 11, 31, 5, 5, 4, 3));
        assert_eq!(formatted, "Fri Dec 31 05:04:03 1999");
        assert_eq!(formatted.len(), 24);
    }

    /// php-src writes into `char time_string[24]` and NUL-terminates at index 24, so a year
    /// wider than four digits loses its tail rather than widening the line.
    #[test]
    fn truncates_a_five_digit_year_at_24_bytes() {
        assert_eq!(
            format_asctime(&tm(10000, 0, 1, 6, 0, 0, 0)),
            "Sat Jan  1 00:00:00 1000"
        );
    }

    /// libc's `asctime` has undefined behaviour for these; ours answers an empty string,
    /// the same thing `local_time_string` answers for a null `localtime`.
    #[test]
    fn refuses_an_out_of_range_weekday_or_month() {
        assert!(format_asctime(&tm(2026, 8, 12, 7, 0, 0, 0)).is_empty());
        assert!(format_asctime(&tm(2026, 12, 12, 6, 0, 0, 0)).is_empty());
        assert!(format_asctime(&tm(2026, -1, 12, 6, 0, 0, 0)).is_empty());
    }

    /// The live path must still produce a real line on whatever platform the suite runs on.
    #[test]
    fn the_live_clock_produces_a_24_byte_line() {
        assert_eq!(local_time_string().len(), 24);
    }
}
