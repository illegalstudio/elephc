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
    for requested in symbols {
        let symbol = if matches!(requested.as_str(), "main" | "_main") {
            [requested.as_str(), if requested == "main" { "_main" } else { "main" }]
                .into_iter().find(|name| globals.contains(name)).unwrap_or(requested.as_str())
        } else { requested.as_str() };
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
        let mut body = Vec::new();
        for line in &lines[start..end] {
            if line.trim_end().ends_with(':') && line.contains("_backtrace_frame_reader_") {
                reader = true;
            } else if line.trim_end().ends_with(':') && line.contains("_backtrace_call_site_") {
                reader = false;
            }
            if reader { continue; }
            body.push(*line);
        }
        append_function_body(&mut output, &body);
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

/// Keeps small bodies whole and bounds context around every ownership call in large bodies.
fn append_function_body(output: &mut String, body: &[&str]) {
    let mut include = vec![body.len() <= 1800; body.len()];
    if body.len() > 1800 {
        include[..32].fill(true);
        include[body.len() - 32..].fill(true);
        for (index, line) in body.iter().enumerate() {
            if is_ownership_call(line) {
                include[index.saturating_sub(16)..(index + 25).min(body.len())].fill(true);
            }
        }
    }
    let mut omitted = 0;
    for (line, include) in body.iter().zip(include) {
        if !include { omitted += 1; continue; }
        if omitted > 0 {
            output.push_str(&format!("... {omitted} assembly lines omitted ...\n"));
            omitted = 0;
        }
        output.push_str(line);
        output.push('\n');
    }
}

/// Recognizes producer, release and PHP-call boundaries whose argument setup must stay visible.
fn is_ownership_call(line: &str) -> bool {
    let mut words = line.split_whitespace();
    let Some(instruction) = words.next() else { return false; };
    if instruction == "blr" { return true; }
    if !matches!(instruction, "bl" | "call") { return false; }
    let Some(target) = words.next() else { return false; };
    target.starts_with("_fn_") || target.starts_with("_method_") || target.starts_with("_static_")
        || target.starts_with("__rt_decref_") || target.starts_with("__rt_incref")
        || target.starts_with("__rt_array_") || target.starts_with("__rt_hash_")
        || target.starts_with("__rt_mixed_") || target.starts_with("__rt_heap_")
        || target.starts_with("__rt_new_") || target.starts_with("__rt_str_persist")
        || target.starts_with("__rt_fopen")
        || matches!(target, "rax" | "rcx" | "rdx" | "rsi" | "rdi" | "r8" | "r9" | "r10" | "r11" | "r12" | "r13" | "r14" | "r15")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Late array producers and releases survive large bodies without including another function.
    #[test]
    fn selected_function_assembly_keeps_late_ownership_calls() {
        let filler = "    nop\n".repeat(2000);
        let assembly = format!(".globl _fn_owned\n.globl _fn_unrelated\n_fn_owned:\n{filler}    mov x0, #4\n    mov x1, #8\n    bl __rt_array_new\n{filler}    bl __rt_decref_array\n    ret\n_fn_unrelated:\nUNRELATED_BODY\n    ret\n");
        let selected = selected_function_assembly(&assembly, &["_fn_owned".into()]);
        for expected in ["mov x0, #4", "mov x1, #8", "bl __rt_array_new", "bl __rt_decref_array"] {
            assert!(selected.contains(expected), "{selected}");
        }
        assert!(selected.contains("assembly lines omitted"), "{selected}");
        assert!(!selected.contains("UNRELATED_BODY"), "{selected}");
        assert!(selected.lines().count() < 200, "{}", selected.lines().count());
    }

    /// Main diagnostics resolve the emitted entry-point spelling on Linux and Darwin.
    #[test]
    fn selected_function_assembly_resolves_main_entry_point() {
        for symbol in ["main", "_main"] {
            let assembly = format!(".globl {symbol}\n{symbol}:\n    bl __rt_decref_mixed\n    ret\n");
            for requested in ["main", "_main"] {
                let selected = selected_function_assembly(&assembly, &[requested.into()]);
                assert!(selected.contains("bl __rt_decref_mixed"), "{selected}");
                assert!(!selected.contains("missing assembly symbol"), "{selected}");
            }
        }
    }
}
