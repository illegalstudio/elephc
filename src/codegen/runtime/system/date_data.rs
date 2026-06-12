//! Purpose:
//! Emits fixed date-format lookup tables for the system date helpers.
//! Month and weekday names here are addressed directly by the generated __rt_date formatters.
//!
//! Called from:
//! - `crate::codegen::runtime::system::emit_date_data()` during fixed data emission.
//!
//! Key details:
//! - Data symbol names are consumed directly by __rt_date and must not drift from formatter lookups.

/// Emits `.globl` day-name and month-name lookup tables as NASM/OS X assembler directives.
///
/// Each entry is 12 bytes: 10 characters (null-padded), 1 length byte, 1 zero padding byte.
/// Sunday=0…Saturday=6 (7 entries); January=0…December=11 (12 entries).
///
/// Returns a `String` containing `.globl _day_names` / `_month_names` / `_days_in_month`
/// symbols with `.ascii` and `.byte` directives. The symbol names (`_day_names`,
/// `_month_names`, `_days_in_month`) and their fixed strides (12-byte name entries,
/// 1-byte day counts) must stay in sync with `__rt_date` formatter lookups.
pub(crate) fn emit_date_data() -> String {
    let mut out = String::new();
    // Day names: 7 entries, each 12 bytes (10 chars + 1 length + 1 padding)
    // Sunday=0, Monday=1, ..., Saturday=6
    out.push_str(".globl _day_names\n_day_names:\n");
    let days = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
    for day in &days {
        let mut padded = day.to_string();
        while padded.len() < 10 {
            padded.push('\0');
        }
        out.push_str(&format!("    .ascii \"{}\"\n", padded.replace('\0', "\\0")));
        out.push_str(&format!("    .byte {}\n", day.len()));
        out.push_str("    .byte 0\n");
    }

    // Month names: 12 entries, each 12 bytes (10 chars + 1 length + 1 padding)
    // January=0, ..., December=11
    out.push_str(".globl _month_names\n_month_names:\n");
    let months = ["January", "February", "March", "April", "May", "June",
                  "July", "August", "September", "October", "November", "December"];
    for month in &months {
        let mut padded = month.to_string();
        while padded.len() < 10 {
            padded.push('\0');
        }
        out.push_str(&format!("    .ascii \"{}\"\n", padded.replace('\0', "\\0")));
        out.push_str(&format!("    .byte {}\n", month.len()));
        out.push_str("    .byte 0\n");
    }

    // Days-in-month table: 12 bytes indexed by tm_mon (January=0 … December=11).
    // February holds its common-year length (28); the `t` formatter bumps it to 29
    // in leap years. The symbol name and 1-byte stride must stay in sync with the
    // __rt_date 't' token lookup.
    out.push_str(".globl _days_in_month\n_days_in_month:\n");
    out.push_str("    .byte 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31\n");

    // Composite sub-formats for the `c` (ISO 8601) and `r` (RFC 2822) tokens. The
    // formatter re-runs its main loop over these strings, so they are written with the
    // primitive specifiers it already understands. The escaped `\T` in the ISO form emits
    // a literal `T`. The lengths (13 for `_date_fmt_c`, 16 for `_date_fmt_r`) are hardcoded
    // in the formatter's `c`/`r` setup and must match these strings exactly.
    out.push_str(".globl _date_fmt_c\n_date_fmt_c:\n");
    out.push_str("    .ascii \"Y-m-d\\\\TH:i:sP\"\n");
    out.push_str(".globl _date_fmt_r\n_date_fmt_r:\n");
    out.push_str("    .ascii \"D, d M Y H:i:s O\"\n");

    out
}
