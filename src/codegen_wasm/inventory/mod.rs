//! Purpose:
//! Builds the deterministic, machine-readable current-revision WASM capability
//! inventory (`WASM-COVERAGE-001` / W0) from the EIR identity enums and the
//! `codegen_wasm::capability` classifiers, plus a generated human summary.
//!
//! Called from:
//! - `tools/gen_wasm_inventory.rs` (`cargo run --example gen_wasm_inventory`),
//!   which emits the JSON report and `--summary` text.
//! - `cargo test` through `inventory::tests`.
//!
//! Key details:
//! - Every `Op`, `RuntimeFnId`, `UnaryStringRuntime`, concrete `IrType`,
//!   `Terminator`, and `RuntimeCallTarget` form is enumerated
//!   and classified into exactly one disposition: `supported`, `excluded`, or
//!   `missing`. Totals are derived from those enumerations, never copied from
//!   the spec prose's historical counts.
//! - The committed baseline leaves `metadata.commit`/`dirty` as `None`; the
//!   generator fills them from git for a per-run CI manifest. Schema validation
//!   is structural (`validate_report`) so no external JSON-schema engine is
//!   required.
#![allow(dead_code)]

mod classify;
mod evidence;
mod schema;
#[cfg(test)]
mod tests;

use crate::ir::{Op, RuntimeFnId, UnaryStringRuntime};
use std::collections::BTreeMap;

use classify::*;
use schema::{
    InventoryRow, NormativePins, PhpSrcPin, ReportMetadata, RevisionPin, TestCatalog,
    ToolchainPins,
};

pub use schema::{
    AggregateTotals, Disposition, FamilyTotals, InventoryReport, SCHEMA_ID, FROZEN_SPEC_SHA256,
    GENERATOR_VERSION,
};

/// Derives exact missing-evidence field names for one inventory row.
fn derive_row_evidence_gaps(row: &InventoryRow) -> Vec<&'static str> {
    let mut gaps = Vec::new();
    if row.producers.is_empty() {
        gaps.push("producers");
    }
    if row.execution_modes.is_empty() {
        gaps.push("execution_modes");
    }
    if let Some(evidence) = &row.supported {
        if evidence.backend.is_empty() {
            gaps.push("supported.backend");
        }
        if evidence.lowerer.is_empty() {
            gaps.push("supported.lowerer");
        }
        if evidence.tests.is_empty() {
            gaps.push("supported.tests");
        }
    }
    gaps
}

/// Aggregates a list of rows into per-family supported/excluded/missing totals.
fn family_totals(mut rows: Vec<InventoryRow>) -> FamilyTotals {
    let mut supported = 0usize;
    let mut excluded = 0usize;
    let mut missing = 0usize;
    for row in &mut rows {
        row.evidence_gaps = derive_row_evidence_gaps(row);
        match row.disposition {
            Disposition::Supported => supported += 1,
            Disposition::Excluded => excluded += 1,
            Disposition::Missing => missing += 1,
        }
    }
    FamilyTotals {
        total: rows.len(),
        supported,
        excluded,
        missing,
        rows,
    }
}

/// Builds the deterministic baseline inventory report (no per-revision git
/// metadata). The generator example fills `metadata.commit`/`dirty` from git
/// when emitting a per-run manifest.
pub fn build_report() -> InventoryReport {
    let op_rows: Vec<InventoryRow> = Op::all().iter().copied().map(op_row).collect();
    let runtime_fn_rows: Vec<InventoryRow> =
        RuntimeFnId::all().iter().copied().map(runtime_fn_row).collect();
    let unary_string_rows: Vec<InventoryRow> =
        UnaryStringRuntime::all().iter().copied().map(unary_string_row).collect();
    let terminator_rows: Vec<InventoryRow> = terminator_representatives()
        .iter()
        .map(terminator_row)
        .collect();
    let call_target_rows = runtime_call_target_rows();
    let ir_type_rows: Vec<InventoryRow> =
        ir_type_representatives().into_iter().map(ir_type_row).collect();

    let mut families = BTreeMap::new();
    families.insert("op", family_totals(op_rows));
    families.insert("runtime_fn", family_totals(runtime_fn_rows));
    families.insert("unary_string", family_totals(unary_string_rows));
    families.insert("terminator", family_totals(terminator_rows));
    families.insert("runtime_call_target", family_totals(call_target_rows));
    families.insert("ir_type", family_totals(ir_type_rows));

    let mut total = 0usize;
    let mut supported = 0usize;
    let mut excluded = 0usize;
    let mut missing = 0usize;
    for family in families.values() {
        total += family.total;
        supported += family.supported;
        excluded += family.excluded;
        missing += family.missing;
    }
    let tests = test_catalog();
    let row_evidence_gaps = families
        .values()
        .flat_map(|family| &family.rows)
        .filter(|row| !row.evidence_gaps.is_empty())
        .count();
    let catalog_evidence_gaps = tests.missing_categories.len();
    let evidence_gaps = row_evidence_gaps + catalog_evidence_gaps;
    let (gate, gate_reason) = if missing == 0 && evidence_gaps == 0 {
        (
            "pass",
            "no reachable identity or required evidence remains missing".to_string(),
        )
    } else {
        (
            "fail",
            format!(
                "{missing} reachable identities lack a WASM lowerer; {row_evidence_gaps} rows lack producer/mode/lowerer/test evidence; {catalog_evidence_gaps} required test categories are missing"
            ),
        )
    };

    InventoryReport {
        metadata: ReportMetadata {
            schema: SCHEMA_ID,
            generator_version: GENERATOR_VERSION,
            commit: None,
            dirty: None,
            pins: normative_pins(),
        },
        families,
        shapes: shape_predicates(),
        execution_modes: execution_modes(),
        tests,
        totals: AggregateTotals {
            total,
            supported,
            excluded,
            missing,
            evidence_gaps,
            row_evidence_gaps,
            catalog_evidence_gaps,
            gate,
            gate_reason,
            stale_literal_counts_rejected: true,
        },
    }
}

/// Returns every immutable normative and acceptance-tool pin from the frozen specification.
fn normative_pins() -> NormativePins {
    NormativePins {
        wasm_compliance_sha256: FROZEN_SPEC_SHA256,
        wasm_core_3_0: RevisionPin {
            tag: "wg-3.0",
            commit: "9d36019973201a19f9c9ebb0f10828b2fe2374aa",
        },
        wasi_preview1_commit: "e840fe45e63b4f227a29fa87df94ab3bbe3d5efb",
        php_src: &[
            PhpSrcPin {
                profile: "8.2",
                tag: "php-8.2.33",
                tag_object: "fa98f62b39a612ae88b7be5d5ea9ff9b794b454b",
                tag_commit: "651db3ebfa622cae0c4e6b39766812efbd274ced",
            },
            PhpSrcPin {
                profile: "8.3",
                tag: "php-8.3.33",
                tag_object: "a7413fbd1dd4dccda419ca473ce475f084edaadd",
                tag_commit: "4a55da8cb580ba56887c80a42291ebc676d6fb43",
            },
            PhpSrcPin {
                profile: "8.4",
                tag: "php-8.4.24",
                tag_object: "3cb6f7231aed24c4ae77a0d3ee5aeeb2b968ad30",
                tag_commit: "fb193d3df72d1ca3b5ef58ec9e9b6ef5bdf18bef",
            },
            PhpSrcPin {
                profile: "8.5",
                tag: "php-8.5.9",
                tag_object: "d6bbf3ed631eea9763a2b790653fc91b69f0af7a",
                tag_commit: "dd6e76cce27aaa0ed9f7520648ed1081dfb6af36",
            },
        ],
        toolchain: ToolchainPins {
            rust: "1.95.0",
            wat: "1.252.0",
            wasmparser: "0.252.0",
            wasmer: "7.2.1",
            wasmtime: "47.0.2",
            wasm_tools: "1.254.0",
            node: "26.3.0",
            typescript: "6.0.3",
            npm: "bundled with Node.js 26.3.0",
        },
    }
}

/// Returns the positive/negative/differential/ownership/host test catalog.
fn test_catalog() -> TestCatalog {
    TestCatalog {
        positive: vec![
            "codegen_wasm::tests::echo_integers_writes_to_stdout",
            "codegen_wasm::tests::echo_float_writes_to_stdout",
            "codegen_wasm::tests::echo_string_literal_writes_to_stdout",
            "codegen_wasm::tests::chained_concat_echoes_correctly",
            "codegen_wasm::strict::tests::strict_scalar_equality_opcodes_lower_and_run",
            "codegen_wasm::strict::tests::strict_binary_string_equality_is_length_delimited",
            "codegen::cli::test_cli_wasm_strict_equality_executes_supported_profiles",
            "codegen_wasm::tests::strlen_of_literal_invokes_correctly",
            "codegen_wasm::tests::argc_reports_argument_count",
            "codegen_wasm::tests::exit_with_code_sets_process_status",
        ],
        negative: vec![
            "codegen_wasm::tests::unsupported_op_is_rejected",
            "codegen_wasm::tests::iterable_mutation_without_concrete_storage_fails_closed",
            "codegen_wasm::tests::hash_set_mixed_int_cast_fails_closed",
            "codegen_wasm::tests::hash_set_mixed_float_cast_fails_closed",
            "codegen_wasm::tests::hash_set_mixed_string_cast_fails_closed",
            "codegen_wasm::capability::tests::direct_call_shape_rejects_arity_mismatch_before_lowering",
            "codegen_wasm::capability::tests::int_like_to_string_shape_is_exact",
            "codegen_wasm::capability::tests::strict_compare_shape_is_exact_and_fail_closed",
            "codegen_wasm::capability::tests::rejects_method_on_null_error_without_command_runtime",
        ],
        differential: vec![],
        ownership: vec![
            "codegen_wasm::tests::exit_runs_owned_local_destructors_before_terminating",
            "codegen_wasm::tests::ref_cell_promotion_is_runtime_idempotent_across_branches",
            "codegen_wasm::tests::acquired_ref_cell_return_survives_owner_epilogue",
            "ir_lower::tests::ownership::match_releases_owned_subject_and_conditions_on_each_normal_edge",
            "ir_lower::tests::ownership::match_releases_owned_object_subject_and_condition",
        ],
        host: vec![
            "scripts/test-wasm-hosts.sh",
            ".github/workflows/ci.yml::wasm-host-portability",
        ],
        missing_categories: vec![
            "differential: no durable php-src 8.2/8.3/8.4/8.5 oracle matrix exists yet",
        ],
    }
}

/// Validates a report against the W0 structural schema. Returns a human-readable
/// error list; an empty `Vec` means the report is schema-valid.
pub fn validate_report(report: &InventoryReport) -> Vec<String> {
    let mut errors = Vec::new();
    if report.metadata.schema != SCHEMA_ID {
        errors.push(format!(
            "metadata.schema is {:?}, expected {:?}",
            report.metadata.schema, SCHEMA_ID
        ));
    }
    if report.metadata.generator_version.is_empty() {
        errors.push("metadata.generator_version is empty".to_string());
    }
    if report.metadata.pins != normative_pins() {
        errors.push("metadata.pins does not match the frozen normative/toolchain set".to_string());
    }
    match (&report.metadata.commit, report.metadata.dirty) {
        (None, None) => {}
        (Some(commit), Some(_)) => {
            if commit.len() != 40 || !commit.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                errors.push("metadata.commit is not a full 40-hex Git commit".to_string());
            }
        }
        _ => errors.push(
            "metadata.commit and metadata.dirty must either both be present or both be absent"
                .to_string(),
        ),
    }
    let expected_families = [
        "op",
        "ir_type",
        "runtime_fn",
        "unary_string",
        "terminator",
        "runtime_call_target",
    ];
    for family in expected_families {
        let Some(totals) = report.families.get(family) else {
            errors.push(format!("missing family {family:?}"));
            continue;
        };
        if totals.total != totals.rows.len() {
            errors.push(format!(
                "family {family:?}: total {} != rows.len() {}",
                totals.total,
                totals.rows.len()
            ));
        }
        let (mut s, mut e, mut m) = (0usize, 0usize, 0usize);
        for row in &totals.rows {
            match row.disposition {
                Disposition::Supported => s += 1,
                Disposition::Excluded => e += 1,
                Disposition::Missing => m += 1,
            }
            validate_row(row, family, &mut errors);
        }
        if (s, e, m) != (totals.supported, totals.excluded, totals.missing) {
            errors.push(format!(
                "family {family:?}: derived ({s},{e},{m}) != declared ({},{},{})",
                totals.supported, totals.excluded, totals.missing
            ));
        }
        if totals.total != s + e + m {
            errors.push(format!(
                "family {family:?}: total {} != supported+excluded+missing {}",
                totals.total,
                s + e + m
            ));
        }
    }
    if report.totals.total != report.families.values().map(|f| f.total).sum::<usize>() {
        errors.push("totals.total does not equal the sum of family totals".to_string());
    }
    let sum_supported = report.families.values().map(|f| f.supported).sum::<usize>();
    let sum_excluded = report.families.values().map(|f| f.excluded).sum::<usize>();
    let sum_missing = report.families.values().map(|f| f.missing).sum::<usize>();
    if report.totals.supported != sum_supported
        || report.totals.excluded != sum_excluded
        || report.totals.missing != sum_missing
    {
        errors.push(format!(
            "totals do not match the sum of family totals: declared ({},{},{}) vs derived ({sum_supported},{sum_excluded},{sum_missing})",
            report.totals.supported, report.totals.excluded, report.totals.missing
        ));
    }
    if !report.totals.stale_literal_counts_rejected {
        errors.push("totals.stale_literal_counts_rejected is false".to_string());
    }
    let expected_row_evidence_gaps = report
        .families
        .values()
        .flat_map(|family| &family.rows)
        .filter(|row| !row.evidence_gaps.is_empty())
        .count();
    let expected_catalog_evidence_gaps = report.tests.missing_categories.len();
    let expected_evidence_gaps =
        expected_row_evidence_gaps + expected_catalog_evidence_gaps;
    if report.totals.row_evidence_gaps != expected_row_evidence_gaps {
        errors.push(format!(
            "totals.row_evidence_gaps is {}, expected {expected_row_evidence_gaps}",
            report.totals.row_evidence_gaps
        ));
    }
    if report.totals.catalog_evidence_gaps != expected_catalog_evidence_gaps {
        errors.push(format!(
            "totals.catalog_evidence_gaps is {}, expected {expected_catalog_evidence_gaps}",
            report.totals.catalog_evidence_gaps
        ));
    }
    if report.totals.evidence_gaps != expected_evidence_gaps {
        errors.push(format!(
            "totals.evidence_gaps is {}, expected {expected_evidence_gaps}",
            report.totals.evidence_gaps
        ));
    }
    let expected_gate =
        if report.totals.missing == 0 && report.totals.evidence_gaps == 0 {
            "pass"
        } else {
            "fail"
        };
    if report.totals.gate != expected_gate {
        errors.push(format!(
            "totals.gate is {:?}, expected {expected_gate:?} (missing={})",
            report.totals.gate, report.totals.missing
        ));
    }
    if report.shapes.is_empty() {
        errors.push("shapes inventory is empty".to_string());
    }
    if report.execution_modes.is_empty() {
        errors.push("execution_modes inventory is empty".to_string());
    }
    for family in report.families.values() {
        for row in &family.rows {
            for mode in &row.execution_modes {
                if !report
                    .execution_modes
                    .iter()
                    .any(|candidate| candidate.mode == *mode && candidate.reachable)
                {
                    errors.push(format!(
                        "family {:?} row {:?}: execution mode {:?} is not globally reachable",
                        row.family, row.name, mode
                    ));
                }
            }
        }
    }
    if report.tests.positive.is_empty()
        || report.tests.negative.is_empty()
        || report.tests.ownership.is_empty()
        || report.tests.host.is_empty()
    {
        errors.push(
            "tests catalog is missing a positive/negative/ownership/host identifier".to_string(),
        );
    }
    let differential_gap = report
        .tests
        .missing_categories
        .iter()
        .any(|category| category.starts_with("differential:"));
    if report.tests.differential.is_empty() && !differential_gap {
        errors.push(
            "tests.differential is empty without an explicit differential evidence gap"
                .to_string(),
        );
    }
    if !report.tests.differential.is_empty() && differential_gap {
        errors.push(
            "tests.differential has durable identifiers but remains marked missing".to_string(),
        );
    }
    errors
}

/// Validates a single row's disposition/exclusion/evidence invariants.
fn validate_row(row: &InventoryRow, family: &str, errors: &mut Vec<String>) {
    let expected_evidence_gaps = derive_row_evidence_gaps(row);
    if row.evidence_gaps != expected_evidence_gaps {
        errors.push(format!(
            "family {family:?} row {:?}: evidence_gaps {:?} != derived {:?}",
            row.name, row.evidence_gaps, expected_evidence_gaps
        ));
    }
    let exactly_one =
        row.supported.is_some() as usize + row.excluded.is_some() as usize + row.missing.is_some() as usize;
    let expected = match row.disposition {
        Disposition::Supported => {
            if row.supported.is_none() {
                errors.push(format!(
                    "family {family:?} row {:?}: supported disposition lacks evidence",
                    row.name
                ));
            }
            if row.excluded.is_some() || row.missing.is_some() {
                errors.push(format!(
                    "family {family:?} row {:?}: supported disposition carries excluded/missing data",
                    row.name
                ));
            }
            1
        }
        Disposition::Excluded => {
            let Some(exclusion) = &row.excluded else {
                errors.push(format!(
                    "family {family:?} row {:?}: excluded disposition lacks an exclusion contract",
                    row.name
                ));
                return;
            };
            if exclusion.category.is_empty()
                || exclusion.reason.is_empty()
                || exclusion.owner.is_empty()
                || exclusion.removal_gate.is_empty()
                || exclusion.diagnostic.is_empty()
            {
                errors.push(format!(
                    "family {family:?} row {:?}: exclusion contract has an empty field",
                    row.name
                ));
            }
            if row.supported.is_some() || row.missing.is_some() {
                errors.push(format!(
                    "family {family:?} row {:?}: excluded disposition carries supported/missing data",
                    row.name
                ));
            }
            1
        }
        Disposition::Missing => {
            if row.missing.is_none() {
                errors.push(format!(
                    "family {family:?} row {:?}: missing disposition lacks a gate-fail note",
                    row.name
                ));
            }
            if row.supported.is_some() || row.excluded.is_some() {
                errors.push(format!(
                    "family {family:?} row {:?}: missing disposition carries supported/excluded data",
                    row.name
                ));
            }
            1
        }
    };
    if exactly_one != expected {
        errors.push(format!(
            "family {family:?} row {:?}: carries {} payload field(s), expected {expected}",
            row.name, exactly_one
        ));
    }
    if row.name.is_empty() {
        errors.push(format!("family {family:?}: row with empty name"));
    }
}

/// Renders a compact human-readable summary of the report.
pub fn human_summary(report: &InventoryReport) -> String {
    let mut out = String::new();
    out.push_str("Elephc wasm32-wasi capability inventory (W0)\n");
    out.push_str(&format!(
        "schema: {} | generator: {} | spec sha256: {}\n",
        report.metadata.schema,
        report.metadata.generator_version,
        report.metadata.pins.wasm_compliance_sha256,
    ));
    if let Some(commit) = &report.metadata.commit {
        out.push_str(&format!(
            "commit: {}{} | gate: {}\n",
            commit,
            if report.metadata.dirty == Some(true) { " (dirty)" } else { "" },
            report.totals.gate,
        ));
    } else {
        out.push_str(&format!("gate: {}\n", report.totals.gate));
    }
    out.push_str(&format!(
        "totals: {} identities (supported {}, excluded {}, missing {}, evidence gaps {} = {} row + {} catalog) — {}\n",
        report.totals.total,
        report.totals.supported,
        report.totals.excluded,
        report.totals.missing,
        report.totals.evidence_gaps,
        report.totals.row_evidence_gaps,
        report.totals.catalog_evidence_gaps,
        report.totals.gate_reason,
    ));
    out.push_str("family breakdown:\n");
    for (name, family) in &report.families {
        out.push_str(&format!(
            "  {:22} total {:>4} | supported {:>3} | excluded {:>3} | missing {:>3}\n",
            name, family.total, family.supported, family.excluded, family.missing,
        ));
    }
    out.push_str(&format!("shape predicates: {}\n", report.shapes.len()));
    out.push_str(&format!(
        "execution modes: {}\n",
        report
            .execution_modes
            .iter()
            .map(|m| m.mode)
            .collect::<Vec<_>>()
            .join(", "),
    ));
    out.push_str(
        "historical prose counts (90/236, 4/437, 0/15, 5/8) are not used; totals are derived.",
    );
    out
}
