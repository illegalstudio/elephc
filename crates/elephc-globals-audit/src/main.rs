//! Purpose:
//! CLI entry point for the elephc global-state audit tool. Extracts mutable
//! global symbols the compiler/runtime/bridges emit, classifies them against a
//! versioned baseline, and either prints the live table (`--audit`) or checks
//! the baseline for drift (`--check`).
//!
//! Called from:
//! - Manual invocation `cargo run -p elephc-globals-audit -- [--check|--audit]`.
//! - Integration tests in `tests/` that assert `--check` passes and fails on
//!   a deliberately introduced unclassified symbol.
//!
//! Key details:
//! - The tool is read-only against the repository: it never writes the baseline
//!   automatically. Bumping the baseline is a deliberate human action.
//! - `--check` exits non-zero on drift (new unclassified symbol, changed
//!   classification, or a removed symbol still listed in the baseline).

use std::path::PathBuf;
use std::process::ExitCode;

use elephc_globals_audit::{apply_baseline, check, extract_all, load_baseline, Class};

/// Prints usage and returns the exit code for an arg-error.
fn usage() -> ExitCode {
    eprintln!("usage: elephc-globals-audit --check [--target <triple>] [--repo <path>]");
    eprintln!("       elephc-globals-audit --audit [--target <triple>] [--repo <path>]");
    eprintln!();
    eprintln!("  --check   diff the live extraction against the committed baseline; exit");
    eprintln!("            non-zero on new/changed/removed symbols (CI gate).");
    eprintln!("  --audit   print the live classified table (no exit-code gate).");
    eprintln!("  --generate print a baseline TOML body to stdout (seed for review).");
    eprintln!("  --target  baseline target triple dir (default: host).");
    eprintln!("  --repo    repository root to audit (default: current dir).");
    ExitCode::from(2)
}

/// Resolves the audit crate directory from the repo root. The crate lives at
/// `<repo>/crates/elephc-globals-audit` and holds `baseline/<target>.toml`.
fn crate_dir(repo: &std::path::Path) -> PathBuf {
    repo.join("crates").join("elephc-globals-audit")
}

/// Detects the host target triple directory name matching a baseline file.
/// Falls back to the first available baseline if the host has none.
fn default_target(repo: &std::path::Path) -> Option<String> {
    let crate_d = crate_dir(repo);
    let targets = elephc_globals_audit::baseline_targets(&crate_d);
    if targets.is_empty() {
        return None;
    }
    // Prefer a host-matching baseline; otherwise the first (stable) entry.
    let host = host_triple();
    if targets.iter().any(|t| t == &host) {
        Some(host)
    } else {
        Some(targets[0].clone())
    }
}

/// Returns a coarse host triple name matching the baseline file naming.
fn host_triple() -> String {
    // The baselines are keyed by the elephc target triple (macos-aarch64,
    // linux-aarch64, linux-x86_64). Detect from cfg.
    if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        "macos-aarch64".to_string()
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "aarch64") {
        "linux-aarch64".to_string()
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
        "linux-x86_64".to_string()
    } else {
        "unknown".to_string()
    }
}

/// Entry point: parses flags, runs the requested mode, returns the exit code.
fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let mut mode: Option<&str> = None;
    let mut target: Option<String> = None;
    let mut repo: Option<String> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--check" => mode = Some("check"),
            "--audit" => mode = Some("audit"),
            "--generate" => mode = Some("generate"),
            "--target" => {
                i += 1;
                if i < args.len() {
                    target = Some(args[i].clone());
                }
            }
            "--repo" => {
                i += 1;
                if i < args.len() {
                    repo = Some(args[i].clone());
                }
            }
            _ => return usage(),
        }
        i += 1;
    }
    let mode = match mode {
        Some(m) => m,
        None => return usage(),
    };

    let repo_path = repo.map(PathBuf::from).unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let crate_d = crate_dir(&repo_path);
    let target_name = target.or_else(|| default_target(&repo_path)).unwrap_or_else(|| host_triple());
    let baseline_p = elephc_globals_audit::baseline_path(&crate_d, &target_name);

    let live = extract_all(&repo_path);
    let baseline = load_baseline(&baseline_p);

    match mode {
        "generate" => {
            // Emit a baseline TOML body from the FRESH heuristic classification,
            // without applying the existing baseline. This is a seed for human
            // review, not an automatic baseline bump: re-running `--generate`
            // always reflects the current classifier, not a merge with the
            // committed baseline.
            let entries: Vec<(String, Class, String)> = live
                .iter()
                .map(|e| (e.symbol.clone(), e.class, e.notes.clone()))
                .collect();
            let body = elephc_globals_audit::render_baseline(
                entries.iter().map(|(s, c, n)| (s, *c, n)),
            );
            print!("{}", body);
            ExitCode::from(0)
        }
        "audit" => {
            let mut live = live;
            apply_baseline(&mut live, &baseline);
            println!("# elephc global-state audit (target: {})", target_name);
            println!("# symbol\tclass\tsource\tnotes");
            for e in &live {
                println!("{}\t{}\t{}\t{}", e.symbol, e.class.as_str(), e.source, e.notes);
            }
            println!("# live={} baseline={}", live.len(), baseline.len());
            ExitCode::from(0)
        }
        "check" => {
            let mut live = live;
            apply_baseline(&mut live, &baseline);
            let report = check(&live, &baseline);
            if report.ok() {
                println!("globals-audit: OK (live={} baseline={} target={})", report.live_count, report.baseline_count, target_name);
                ExitCode::from(0)
            } else {
                if !report.new_symbols.is_empty() {
                    eprintln!("globals-audit: {} new unclassified symbol(s) — add to baseline {}:", report.new_symbols.len(), baseline_p.display());
                    for e in &report.new_symbols {
                        eprintln!("  + {}  [{}]  {}", e.symbol, e.source, e.notes);
                    }
                }
                if !report.changed.is_empty() {
                    eprintln!("globals-audit: {} re-classified symbol(s):", report.changed.len());
                    for (sym, old, new) in &report.changed {
                        eprintln!("  ~ {}  {} -> {}", sym, old.as_str(), new.as_str());
                    }
                }
                if !report.removed.is_empty() {
                    eprintln!("globals-audit: {} baseline symbol(s) no longer emitted:", report.removed.len());
                    for sym in &report.removed {
                        eprintln!("  - {}", sym);
                    }
                }
                ExitCode::from(1)
            }
        }
        _ => usage(),
    }
}