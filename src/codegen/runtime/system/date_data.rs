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
/// Returns a `String` containing `.globl _day_names` / `_month_names` symbols with
/// `.ascii` and `.byte` directives. The symbol names (`_day_names`, `_month_names`) and
/// the fixed 12-byte stride must stay in sync with `__rt_date` formatter lookups.
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

    out
}
