//! Purpose:
//! Standalone W0 inventory exporter that prints the deterministic
//! `codegen_wasm::inventory` report as JSON, plus a `--summary` human summary.
//!
//! Called from:
//! - `cargo run --example gen_wasm_inventory` (W0 evidence generation / CI gate).
//!
//! Key details:
//! - Mirrors `tools/gen_builtins.rs`: declared as an example so it can read the
//!   `elephc` library's `codegen_wasm::inventory` API without linking it into
//!   the `elephc` binary.
//! - The committed baseline leaves `metadata.commit`/`dirty` as `null`; pass
//!   `--with-revision` to fill them from `git` for a per-run CI manifest.

use std::process::Command;

/// Prints the WASM capability inventory JSON (or `--summary` text) to stdout.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let want_summary = args.iter().any(|a| a == "--summary");
    let with_revision = args.iter().any(|a| a == "--with-revision");

    let mut report = elephc::codegen_wasm::inventory::build_report();
    if with_revision {
        let commit = git_output(&["rev-parse", "HEAD"])
            .unwrap_or_else(|error| fail_revision_metadata(&error));
        let dirty = !git_output(&["status", "--porcelain"])
            .unwrap_or_else(|error| fail_revision_metadata(&error))
            .is_empty();
        report.metadata.commit = Some(commit);
        report.metadata.dirty = Some(dirty);
    }

    let errors = elephc::codegen_wasm::inventory::validate_report(&report);
    if !errors.is_empty() {
        eprintln!("WASM inventory schema validation failed:");
        for error in &errors {
            eprintln!("  - {error}");
        }
        std::process::exit(1);
    }

    if want_summary {
        println!("{}", elephc::codegen_wasm::inventory::human_summary(&report));
        return;
    }

    let json = serde_json::to_string_pretty(&report).expect("serialize inventory report");
    println!("{json}");
}

/// Runs a `git` command and returns trimmed UTF-8 stdout.
fn git_output(args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|error| format!("failed to execute git {}: {error}", args.join(" ")))?;
    if !output.status.success() {
        return Err(format!(
            "git {} exited with {}: {}",
            args.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout)
        .map(|stdout| stdout.trim().to_string())
        .map_err(|error| format!("git {} returned non-UTF-8 output: {error}", args.join(" ")))
}

/// Prints a revision-metadata error and terminates without emitting a partial report.
fn fail_revision_metadata(error: &str) -> ! {
    eprintln!("WASM inventory revision metadata failed: {error}");
    std::process::exit(1);
}
