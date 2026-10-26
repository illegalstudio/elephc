//! Purpose:
//! Keeps remote ownership diagnostics limited to selected emitted functions and their callers.
//!
//! Called from:
//! - Heap-debug codegen fixtures when a remote assertion fails.
//!
//! Key details:
//! - Global declarations delimit functions; inline backtrace readers are omitted explicitly.

use std::collections::HashSet;

/// Captures named function bodies and direct call windows without dumping the runtime catalog.
pub(crate) fn selected_function_assembly(assembly: &str, symbols: &[String]) -> String {
    let lines: Vec<_> = assembly.lines().collect();
    let globals: HashSet<_> = lines.iter().filter_map(|line| {
        let line = line.trim();
        line.strip_prefix(".globl ").or_else(|| line.strip_prefix(".global ")).map(str::trim)
    }).collect();
    let mut output = String::new();
    for symbol in symbols {
        let label = format!("{symbol}:");
        let Some(start) = lines.iter().position(|line| line.trim().eq_ignore_ascii_case(&label)) else {
            output.push_str(&format!("missing assembly symbol: {symbol}\n"));
            continue;
        };
        let end = (start + 1..lines.len()).find(|index| {
            lines[*index].trim().strip_suffix(':').is_some_and(|label| {
                globals.contains(label) && !label.contains("_backtrace_frame_reader_")
            })
        }).unwrap_or(lines.len());
        let mut reader = false;
        let mut kept = 0;
        for line in &lines[start..end] {
            if line.trim_end().ends_with(':') && line.contains("_backtrace_frame_reader_") {
                reader = true;
            } else if line.trim_end().ends_with(':') && line.contains("_backtrace_call_site_") {
                reader = false;
            }
            if reader { continue; }
            if kept == 1800 {
                output.push_str("assembly body truncated after 1800 relevant lines\n");
                break;
            }
            output.push_str(line);
            output.push('\n');
            kept += 1;
        }
        let mut windows = 0;
        for (index, line) in lines.iter().enumerate() {
            let mut words = line.split_whitespace();
            if matches!(words.next(), Some("bl" | "call"))
                && words.next().is_some_and(|word| word.eq_ignore_ascii_case(symbol))
            {
                output.push_str("call window:\n");
                output.push_str(&lines[index.saturating_sub(24)..(index + 40).min(lines.len())].join("\n"));
                output.push('\n');
                windows += 1;
                if windows == 6 { break; }
            }
        }
    }
    output
}
