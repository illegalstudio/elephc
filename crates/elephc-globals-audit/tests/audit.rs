//! Purpose:
//! Integration tests for the global-state audit crate: baseline `--check`
//! stability, deliberate-drift detection, and the WI-2 raw-`adrp`-on-mutable
//! guard that ties adrp centralization to the audit gate.
//!
//! Called from:
//! - `cargo test -p elephc-globals-audit` through Rust's test harness.
//!
//! Key details:
//! - The baseline tables live in `baseline/<target>.toml` and are extracted at
//!   source level (target-independent), so drift tests run against the host
//!   baseline without spawning the CLI binary.
//! - The adrp guard scans `src/codegen/**` and `src/codegen_ir/**` for raw
//!   `emitter.adrp(` calls outside `src/codegen/abi/symbols.rs`; WI-2 routed
//!   every mutable-symbol adrp through `emit_symbol_address`, so any new raw
//!   adrp is a regression.

use std::path::{Path, PathBuf};

use elephc_globals_audit::{check, extract_all, load_baseline, Class};

/// Returns the repository root inferred from the crate's CARGO_MANIFEST_DIR.
fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/elephc-globals-audit -> repo root.
    manifest
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf)
        .expect("CARGO_MANIFEST_DIR should be crates/elephc-globals-audit")
}

/// Returns the path to the committed baseline for the given target.
fn baseline_for(crate_dir: &Path, target: &str) -> PathBuf {
    elephc_globals_audit::baseline_path(crate_dir, target)
}

/// Verifies that the live extraction of the current source tree matches the
/// committed `macos-aarch64` baseline with no drift. This is the `--check`
/// happy path: no new symbols, no removed symbols, no classification changes.
#[test]
fn check_passes_against_committed_baseline() {
    let repo = repo_root();
    let crate_dir = repo.join("crates/elephc-globals-audit");
    let target = "macos-aarch64";
    let baseline_p = baseline_for(&crate_dir, target);
    let baseline = load_baseline(&baseline_p);
    assert!(
        !baseline.is_empty(),
        "baseline {} must not be empty",
        baseline_p.display()
    );

    let live = extract_all(&repo);
    let report = check(&live, &baseline);
    assert!(
        report.ok(),
        "audit drift against {}: new={:?} changed={:?} removed={:?} live={} baseline={}",
        target,
        report.new_symbols,
        report.changed,
        report.removed,
        report.live_count,
        report.baseline_count
    );
}

/// Verifies that a deliberate new symbol with no baseline entry FAILS the
/// audit check. This is the regression gate: any new mutable global must be
/// classified and added to the baseline before it can land.
#[test]
fn check_fails_on_new_unclassified_symbol() {
    let repo = repo_root();
    let crate_dir = repo.join("crates/elephc-globals-audit");
    let target = "macos-aarch64";
    let baseline_p = baseline_for(&crate_dir, target);
    let baseline = load_baseline(&baseline_p);

    let mut live = extract_all(&repo);
    // Inject a synthetic new symbol that has no baseline entry and is
    // classified UNKNOWN by the heuristic — exactly the case the gate is
    // designed to catch.
    live.push(elephc_globals_audit::SymbolEntry {
        symbol: "_test_synthetic_new_global".to_string(),
        class: Class::Unknown,
        source: "test".to_string(),
        notes: ".comm".to_string(),
    });

    let report = check(&live, &baseline);
    assert!(
        !report.ok(),
        "check must FAIL when a new unclassified symbol is present, but ok()=true (new={:?})",
        report.new_symbols
    );
    assert!(
        report
            .new_symbols
            .iter()
            .any(|s| s.symbol == "_test_synthetic_new_global"),
        "the synthetic symbol must appear in report.new_symbols"
    );
}

/// Verifies that a deliberate classification change (same symbol, different
/// class) FAILS the audit check. This catches silent re-classification when
/// a runtime family shifts from one ownership model to another.
#[test]
fn check_fails_on_changed_classification() {
    let repo = repo_root();
    let crate_dir = repo.join("crates/elephc-globals-audit");
    let target = "macos-aarch64";
    let baseline_p = baseline_for(&crate_dir, target);
    let baseline = load_baseline(&baseline_p);
    assert!(!baseline.is_empty(), "baseline must not be empty");

    let mut live = extract_all(&repo);
    // Pick the first live symbol and flip its class to force a changed entry.
    let flipped_symbol = live[0].symbol.clone();
    live[0].class = match live[0].class {
        Class::Const => Class::Tls,
        _ => Class::Const,
    };

    let report = check(&live, &baseline);
    assert!(
        !report.ok(),
        "check must FAIL when a classification changes, but ok()=true (changed={:?})",
        report.changed
    );
    assert!(
        report
            .changed
            .iter()
            .any(|(s, _, _)| s == &flipped_symbol),
        "the flipped symbol must appear in report.changed"
    );
}

/// Recursively scans `root` for `.rs` files containing raw `emitter.adrp(`
/// calls, skipping `allowed` (the central symbols seam) and recording each
/// match as `<relpath>:<line>: <trimmed line>` in `offenders`.
fn scan_dir(root: &Path, allowed: &Path, repo: &Path, offenders: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_dir(&path, allowed, repo, offenders);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        if path == *allowed {
            continue;
        }
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (i, line) in src.lines().enumerate() {
            if line.contains("emitter.adrp(") {
                let rel = path
                    .strip_prefix(repo)
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| path.display().to_string());
                offenders.push(format!("{}:{}: {}", rel, i + 1, line.trim()));
            }
        }
    }
}

/// Scans `src/codegen/**` and `src/codegen_ir/**` for raw `emitter.adrp(`
/// calls outside `src/codegen/abi/symbols.rs`. WI-2 centralized every
/// mutable-symbol adrp load onto `emit_symbol_address` (and the paged-load
/// helper `emit_load_symbol_to_reg_via_page`), so any new raw `emitter.adrp(`
/// in those trees is a regression that bypasses the central seam.
///
/// This test ties WI-1 (audit gate) to WI-2 (adrp centralization): the only
/// remaining raw adrp calls should be the two inside `symbols.rs` itself (the
/// AArch64 non-PIC `adrp dest, sym@PAGE` + the paged-load helper).
#[test]
fn no_raw_adrp_on_mutable_symbols_outside_symbols_rs() {
    let repo = repo_root();
    let codegen = repo.join("src/codegen");
    let codegen_ir = repo.join("src/codegen_ir");
    let allowed = repo.join("src/codegen/abi/symbols.rs");

    let mut offenders: Vec<String> = Vec::new();

    scan_dir(&codegen, &allowed, &repo, &mut offenders);
    scan_dir(&codegen_ir, &allowed, &repo, &mut offenders);

    assert!(
        offenders.is_empty(),
        "raw `emitter.adrp(` found outside src/codegen/abi/symbols.rs (WI-2 regression). \
         All mutable-symbol adrp loads must go through `emit_symbol_address` or \
         `emit_load_symbol_to_reg_via_page`. Offenders:\n{}",
        offenders.join("\n")
    );
}