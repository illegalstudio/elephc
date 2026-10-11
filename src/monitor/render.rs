//! Purpose:
//! Turns a capture into something a person reads: the per-function table, the
//! cost breakdown, the live top-style frame, and the recommendations.
//!
//! Called from:
//! - `local::run`, `remote::run` and `sampled::run`, at the end of a capture.
//!
//! Key details:
//! - A hotspot is named only when it dominates, and an N+1 only when both the
//!   call fan-out and the exact query counts say so.
//! - Bars are a linear read of the percentage, so two reports compare by eye.

use super::*;

/// A function's share of a capture, keyed by its inlining-agnostic name: a
/// function drifting in or out of the inliner must not read as new or gone.
pub(crate) fn function_shares(display: &[(Vec<(String, Kind)>, u64)]) -> (BTreeMap<String, f64>, u64) {
    let mut totals: BTreeMap<String, u64> = BTreeMap::new();
    let mut grand = 0u64;
    for (stack, weight) in display {
        grand += weight;
        let mut seen = HashSet::new();
        for (name, kind) in stack {
            if !matches!(kind, Kind::Php | Kind::PhpInlined) {
                continue;
            }
            let normalized = name.trim_end_matches(" (inlined)").to_string();
            if seen.insert(normalized.clone()) {
                *totals.entry(normalized).or_default() += weight;
            }
        }
    }
    let shares = totals
        .into_iter()
        .map(|(name, weight)| (name, 100.0 * weight as f64 / grand.max(1) as f64))
        .collect();
    (shares, grand)
}

/// Reads per-function shares back out of a previous monitor Speedscope file
/// (its first profile is the helpers-folded PHP view).
pub(crate) fn baseline_shares(path: &str) -> Result<(BTreeMap<String, f64>, u64), String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read baseline {path}: {error}"))?;
    let doc: serde_json::Value =
        serde_json::from_str(&raw).map_err(|error| format!("invalid baseline {path}: {error}"))?;
    let frames = doc["shared"]["frames"]
        .as_array()
        .ok_or_else(|| format!("baseline {path} has no frame table"))?;
    let names: Vec<String> = frames
        .iter()
        .map(|frame| frame["name"].as_str().unwrap_or("").to_string())
        .collect();
    let profile = doc["profiles"]
        .as_array()
        .and_then(|profiles| profiles.first())
        .ok_or_else(|| format!("baseline {path} has no profiles"))?;
    let samples = profile["samples"]
        .as_array()
        .ok_or_else(|| format!("baseline {path} has no samples"))?;
    let weights = profile["weights"]
        .as_array()
        .ok_or_else(|| format!("baseline {path} has no weights"))?;
    let mut totals: BTreeMap<String, u64> = BTreeMap::new();
    let mut grand = 0u64;
    for (stack, weight) in samples.iter().zip(weights) {
        let weight = weight.as_u64().unwrap_or(0);
        grand += weight;
        let mut seen = HashSet::new();
        for index in stack.as_array().into_iter().flatten() {
            let Some(name) = index.as_u64().and_then(|i| names.get(i as usize)) else {
                continue;
            };
            if name == "<non-PHP>" {
                continue;
            }
            let normalized = name.trim_end_matches(" (inlined)").to_string();
            if seen.insert(normalized.clone()) {
                *totals.entry(normalized).or_default() += weight;
            }
        }
    }
    let shares = totals
        .into_iter()
        .map(|(name, weight)| (name, 100.0 * weight as f64 / grand.max(1) as f64))
        .collect();
    Ok((shares, grand))
}

/// Prints the per-function delta table against a baseline capture and returns
/// the process exit code: 2 when a regression exceeds the threshold, else 0.
pub(crate) fn diff_against_baseline(
    display: &[(Vec<(String, Kind)>, u64)],
    baseline_path: &str,
    fail_on_regression: Option<f64>,
) -> Result<i32, String> {
    let (current, current_samples) = function_shares(display);
    let (baseline, baseline_samples) = baseline_shares(baseline_path)?;
    let mut names: Vec<&String> = current.keys().chain(baseline.keys()).collect();
    names.sort();
    names.dedup();
    let mut rows: Vec<(String, Option<f64>, Option<f64>, f64)> = names
        .into_iter()
        .map(|name| {
            let now = current.get(name).copied();
            let was = baseline.get(name).copied();
            let delta = now.unwrap_or(0.0) - was.unwrap_or(0.0);
            (name.clone(), now, was, delta)
        })
        .filter(|(_, now, was, _)| now.unwrap_or(0.0) >= 0.1 || was.unwrap_or(0.0) >= 0.1)
        .collect();
    rows.sort_by(|a, b| b.3.abs().partial_cmp(&a.3.abs()).unwrap_or(std::cmp::Ordering::Equal));
    println!(
        "
--- vs baseline {baseline_path} ({baseline_samples} samples, this run {current_samples}) ---"
    );
    if current_samples < 500 || baseline_samples < 500 {
        println!("warning: fewer than 500 samples on one side; deltas are noisy");
    }
    let mut worst: Option<(String, f64)> = None;
    for (name, now, was, delta) in rows.iter().take(20) {
        let now_text = now.map_or("    —".to_string(), |v| format!("{v:5.1}%"));
        let was_text = was.map_or("    —".to_string(), |v| format!("{v:5.1}%"));
        let arrow = if *delta > 0.5 {
            "▲"
        } else if *delta < -0.5 {
            "▼"
        } else {
            " "
        };
        println!("{name:<26} {now_text}   was {was_text}   {delta:+5.1} {arrow}");
        if *delta > worst.as_ref().map_or(0.0, |(_, d)| *d) {
            worst = Some((name.clone(), *delta));
        }
    }
    if let (Some(threshold), Some((name, delta))) = (fail_on_regression, worst) {
        if delta > threshold {
            eprintln!(
                "elephc monitor: regression — {name} grew {delta:+.1} points (threshold {threshold})"
            );
            return Ok(2);
        }
    }
    Ok(0)
}

/// Renders evaluated assertions as the stdout report, and whether all held.
///
/// Failures come first: a gate's output is read when it is red, and the reason
/// it went red should not be somewhere below twenty passing lines.
pub(crate) fn assert_report(outcomes: &[crate::call_graph::AssertOutcome]) -> (String, bool) {
    use crate::call_graph::AssertStatus;
    let passed = outcomes.iter().filter(|o| o.status == AssertStatus::Pass).count();
    let failed = outcomes.iter().filter(|o| o.status == AssertStatus::Fail).count();
    let errored = outcomes.iter().filter(|o| o.status == AssertStatus::Error).count();
    let mut out = format!(
        "\nassertions — {passed} passed, {failed} failed{}\n",
        if errored > 0 { format!(", {errored} not evaluated") } else { String::new() }
    );
    let mut ordered: Vec<&crate::call_graph::AssertOutcome> = outcomes.iter().collect();
    ordered.sort_by_key(|o| match o.status {
        AssertStatus::Fail => 0,
        AssertStatus::Error => 1,
        AssertStatus::Pass => 2,
    });
    for outcome in ordered {
        let tag = match outcome.status {
            AssertStatus::Pass => "PASS",
            AssertStatus::Fail => "FAIL",
            AssertStatus::Error => "SKIP",
        };
        let measured = match outcome.actual {
            Some(actual) => format!("actual {}", trim_number(actual)),
            None => outcome.note.clone().unwrap_or_default(),
        };
        let label = outcome
            .label
            .as_ref()
            .map(|l| format!("  — {l}"))
            .unwrap_or_default();
        out.push_str(&format!("  [{tag}] {} ({measured}){label}\n", outcome.spec));
    }
    (out, failed == 0 && errored == 0)
}

/// Reverses the runtime's percent-encoding of a trace-line field.
///
/// The runtime encodes because the route comes from an untrusted path and the
/// line is space-separated; decoding here means the operator still reads the
/// real `GET /orders/42` rather than `GET%20/orders/42`. A malformed escape is
/// left as written rather than guessed at.
pub(crate) fn decode_field(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(byte) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Demangles a PHP-level symbol to its source spelling: `fn_hot_u_leaf` →
/// `hot_leaf`, `method_Engine_step` → `Engine::step`, `main` → `{main}`.
///
/// Names are decoded with `names::demangle_fqn`, and a method's class/method
/// boundary with `names::split_symbol_fragments`, the inverses of what the
/// compiler emits: the compact `_method_Engine_step` join and the escaped
/// `_method___App_N_My_u_Class___run` one any `_`, `\` or non-ASCII name selects
/// (#922). A symbol neither parses keeps the older best-effort reading, where a
/// placeholder swap stops `_u_` from being read as the separator.
pub(crate) fn demangle(symbol: &str) -> String {
    let stem = symbol.trim_start_matches('_');
    let stem = strip_generator_suffix(stem);
    if stem == "main" {
        return "{main}".to_string();
    }
    if let Some(rest) = stem.strip_prefix("fn_") {
        return crate::names::demangle_fqn(rest).unwrap_or_else(|| rest.replace("_u_", "_"));
    }
    let tail = if stem.starts_with("method_") {
        Some(&stem["method".len()..])
    } else {
        super::is_static_method_symbol(stem).then(|| &stem["static".len()..])
    };
    if let Some(tail) = tail {
        if let Some(name) = demangle_member(tail) {
            return name;
        }
        let rest = &tail[1..];
        let protected = rest.replace("_u_", "\u{1}");
        if let Some((class, method)) = protected.split_once('_') {
            return format!(
                "{}::{}",
                class.replace('\u{1}', "_"),
                method.replace('\u{1}', "_")
            );
        }
        return rest.replace("_u_", "_");
    }
    symbol.to_string()
}

/// Demangles the suffix-free tail of a method symbol to `Class::method` when it is exactly the two
/// fragments `names::method_symbol` / `names::static_method_symbol` joined, in either
/// separator regime; `None` for anything else, such as a suffixed internal label.
fn demangle_member(tail: &str) -> Option<String> {
    let fragments = crate::names::split_symbol_fragments(tail)?;
    let [class, method] = fragments.as_slice() else {
        return None;
    };
    Some(format!(
        "{}::{}",
        crate::names::demangle_fqn(class)?,
        crate::names::demangle_fqn(method)?
    ))
}

/// Removes a generator entry suffix only when the complete encoded PHP name fails to decode.
/// Escaped separators followed by literal `genbody` or `gencb` methods are complete names.
pub(super) fn strip_generator_suffix(stem: &str) -> &str {
    if complete_php_symbol(stem) {
        return stem;
    }
    stem.strip_suffix("__genbody")
        .or_else(|| stem.strip_suffix("__gencb"))
        .filter(|candidate| complete_php_symbol(candidate))
        .unwrap_or(stem)
}

/// Recognizes complete function or two-fragment method names without the classifier's fallback.
fn complete_php_symbol(stem: &str) -> bool {
    if let Some(name) = stem.strip_prefix("fn_") {
        return !name.is_empty() && crate::names::demangle_fqn(name).is_some();
    }
    stem.strip_prefix("method")
        .or_else(|| stem.strip_prefix("static"))
        .is_some_and(|tail| demangle_member(tail).is_some())
}

/// Resolves sampled addresses to source lines with `atos` against the dSYM.
pub(crate) fn resolve_lines(
    binary: &Path,
    load_address: &str,
    addresses: &[u64],
) -> HashMap<u64, u32> {
    let dsym = binary.with_extension("dSYM");
    if !dsym.exists() || addresses.is_empty() {
        return HashMap::new();
    }
    let mut command = process::Command::new("/usr/bin/atos");
    command
        .arg("-o")
        .arg(&dsym)
        .arg("-l")
        .arg(load_address);
    for address in addresses {
        command.arg(format!("{address:#x}"));
    }
    let Ok(output) = command.output() else {
        return HashMap::new();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut lines = HashMap::new();
    for (address, resolved) in addresses.iter().zip(text.lines()) {
        // Shape: `main (in bench) (bench.php:33)`; helpers have no line suffix.
        if let Some((_, tail)) = resolved.rsplit_once(':') {
            if let Ok(line) = tail.trim_end_matches(')').parse::<u32>() {
                lines.insert(*address, line);
            }
        }
    }
    lines
}

/// Builds the per-line profile for a capture, or `None` without a dSYM/source.
pub(crate) fn line_profile(
    samples: &[(Vec<Frame>, u64)],
    report: &str,
    binary: &Path,
    php_source: &Path,
) -> Option<LineProfile> {
    let source = std::fs::read_to_string(php_source).ok()?;
    let load_address = report
        .lines()
        .find_map(|line| line.strip_prefix("Load Address:").map(str::trim))?;
    // The innermost PHP frame of each stack is the code actually running.
    let leaves: Vec<(u64, u64)> = samples
        .iter()
        .filter_map(|(stack, weight)| {
            stack
                .iter()
                .rev()
                .find(|frame| is_php_symbol(&frame.symbol) && !frame.inlined)
                .and_then(|frame| frame.address)
                .map(|address| (address, *weight))
        })
        .collect();
    if leaves.is_empty() {
        return None;
    }
    let unique: Vec<u64> = leaves
        .iter()
        .map(|(address, _)| *address)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let resolved = resolve_lines(binary, load_address, &unique);
    if resolved.is_empty() {
        return None;
    }
    let (hits, total) = attribute_lines(&leaves, &resolved);
    (total > 0).then(|| LineProfile {
        file: php_source.display().to_string(),
        source: source.lines().map(str::to_string).collect(),
        hits,
        total,
    })
}

/// Converts raw sample stacks into display stacks of (name, kind).
pub(crate) fn render_stacks(samples: &[(Vec<Frame>, u64)]) -> Vec<(Vec<(String, Kind)>, u64)> {
    samples
        .iter()
        .map(|(stack, weight)| {
            let display = stack
                .iter()
                .map(|frame| {
                    if frame.inlined {
                        let name = frame.symbol.trim_start_matches("inlined:");
                        (format!("{name} (inlined)"), Kind::PhpInlined)
                    } else if is_php_symbol(&frame.symbol) {
                        (demangle(&frame.symbol), Kind::Php)
                    } else if cause_for(&frame.symbol).is_some() {
                        (frame.symbol.clone(), Kind::Helper)
                    } else {
                        (frame.symbol.clone(), Kind::Native)
                    }
                })
                .collect();
            (display, *weight)
        })
        .collect()
}

/// Aggregates display stacks into per-function totals, selfs, and causes.
pub(crate) fn table_stats(display: &[(Vec<(String, Kind)>, u64)]) -> TableStats {
    let mut stats = TableStats {
        grand: 0,
        totals: BTreeMap::new(),
        selfs: BTreeMap::new(),
        causes: BTreeMap::new(),
    };
    for (stack, weight) in display {
        stats.grand += weight;
        let php_frames: Vec<&String> = stack
            .iter()
            .filter(|(_, kind)| matches!(kind, Kind::Php | Kind::PhpInlined))
            .map(|(name, _)| name)
            .collect();
        let mut seen = HashSet::new();
        for frame in &php_frames {
            if seen.insert(*frame) {
                *stats.totals.entry((*frame).clone()).or_default() += weight;
            }
        }
        let (leaf, leaf_kind) = stack.last().expect("sample stacks are never empty");
        if matches!(leaf_kind, Kind::Php | Kind::PhpInlined) {
            *stats.selfs.entry(leaf.clone()).or_default() += weight;
        } else if let Some(owner) = php_frames.last() {
            let cause = cause_for(leaf).unwrap_or("other native");
            *stats
                .causes
                .entry((*owner).clone())
                .or_default()
                .entry(cause)
                .or_default() += weight;
        }
    }
    stats
}

/// Renders the per-function cause table with proportion bars.
pub(crate) fn why_table(display: &[(Vec<(String, Kind)>, u64)], processes: usize) -> String {
    let stats = table_stats(display);
    let grand = stats.grand;
    let process_note = if processes > 1 {
        format!(" · {processes} processes")
    } else {
        String::new()
    };
    let mut out = format!("samples: {grand}{process_note}\n");
    let mut by_weight: Vec<_> = stats.totals.iter().collect();
    by_weight.sort_by_key(|(_, weight)| std::cmp::Reverse(**weight));
    for (function, total) in by_weight {
        let pct = 100.0 * *total as f64 / grand as f64;
        let self_pct = 100.0 * stats.selfs.get(function).copied().unwrap_or(0) as f64 / grand as f64;
        out.push_str(&format!(
            "\n{function:<26} {} {pct:5.1}%  self {self_pct:4.1}%\n",
            bar(pct, 22)
        ));
        let mut cause_rows: Vec<_> = stats
            .causes
            .get(function)
            .map(|map| map.iter().collect())
            .unwrap_or_default();
        cause_rows.sort_by_key(|(_, weight)| std::cmp::Reverse(**weight));
        for (cause, weight) in cause_rows {
            let cause_pct = 100.0 * *weight as f64 / grand as f64;
            out.push_str(&format!(
                "    {cause:<25} {} {cause_pct:5.1}%\n",
                bar(cause_pct, 22)
            ));
        }
    }
    out
}
