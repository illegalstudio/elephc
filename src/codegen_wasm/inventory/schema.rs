//! Purpose:
//! Serde schema types, normative pins, and the structural W0 schema validator
//! for the wasm32-wasi capability inventory.
//!
//! Called from:
//! - `super::build_report`/`super::human_summary` and `tools/gen_wasm_inventory`.
//!
//! Key details:
//! - Every report field is `Serialize`-only; the committed JSON artifact is
//!   validated structurally by `validate_report` instead of round-tripping.
//! - `SCHEMA_ID`, `GENERATOR_VERSION`, and `FROZEN_SPEC_SHA256` are the stable
//!   pins recorded in every emitted report.
#![allow(dead_code)]

use serde::Serialize;
use std::collections::BTreeMap;

/// Schema identifier embedded in every emitted report so consumers can reject
/// an incompatible revision before interpreting the payload.
pub const SCHEMA_ID: &str = "elephc.wasm-inventory.v4";

/// Generator version recorded in the report metadata. Bump when the schema or
/// disposition classification changes in a way that alters the report shape.
pub const GENERATOR_VERSION: &str = "w0-4";

/// Frozen `docs/specs/wasm-compliance.md` SHA-256 recorded as a normative pin.
pub const FROZEN_SPEC_SHA256: &str =
    "5865de45b4c9b4f9e3d6d11f4bb5b9fa088eafaec5a350e0142c1a66e3fad061";

/// Capability disposition assigned to exactly one identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Disposition {
    /// An active `codegen_wasm` lowerer exists for this identity.
    Supported,
    /// A native-only Elephc extension intentionally excluded from
    /// `wasm32-wasi` through this report and a matching CLI diagnostic.
    Excluded,
    /// Ordinary PHP reachable from the public frontend whose WASM lowerer is
    /// absent. Fails the gate until implemented or explicitly excluded.
    Missing,
}

/// Stable exclusion contract for a native-only identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Exclusion {
    /// Machine-readable exclusion category (e.g. `native-ffi-ptr`).
    pub category: &'static str,
    /// Human-readable reason this identity is excluded from `wasm32-wasi`.
    pub reason: &'static str,
    /// Owning work surface for re-enabling the identity on WASM.
    pub owner: &'static str,
    /// Condition that must hold before the identity may be re-enabled.
    pub removal_gate: &'static str,
    /// Exact stable diagnostic fragment that already rejects the identity today.
    pub diagnostic: String,
}

/// Lowerer and test evidence for a supported identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SupportedEvidence {
    /// Backend module that lowers this identity.
    pub backend: &'static str,
    /// Specific lowerer function or group within the backend.
    pub lowerer: &'static str,
    /// Test identifiers that exercise this identity on WASM.
    pub tests: &'static [&'static str],
}

/// One inventory row: a stable identity with exactly one disposition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InventoryRow {
    /// Stable backend-neutral name from the enum's own naming function.
    pub name: String,
    /// Enum family that owns this identity.
    pub family: &'static str,
    /// Rust enum that defines this identity.
    pub enum_name: &'static str,
    /// Exactly one capability disposition.
    pub disposition: Disposition,
    /// PHP source constructs or builtins that produce this identity.
    ///
    /// An empty list is an explicit evidence gap; generators must never invent
    /// a generic producer merely to satisfy the schema.
    pub producers: Vec<String>,
    /// Public execution modes through which this identity reaches the WASM
    /// capability audit or lowerer.
    pub execution_modes: Vec<&'static str>,
    /// Exact evidence fields still missing for this row.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub evidence_gaps: Vec<&'static str>,
    /// Evidence present when `disposition == Supported`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supported: Option<SupportedEvidence>,
    /// Exclusion contract present when `disposition == Excluded`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excluded: Option<Exclusion>,
    /// Gate-fail note present when `disposition == Missing`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing: Option<&'static str>,
}

/// Per-family totals derived from the row list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FamilyTotals {
    /// Total identities enumerated for this family.
    pub total: usize,
    /// Identities with an active WASM lowerer.
    pub supported: usize,
    /// Native-only identities excluded through this report.
    pub excluded: usize,
    /// Ordinary-PHP identities whose WASM lowerer is absent.
    pub missing: usize,
    /// Enumerated rows for this family.
    pub rows: Vec<InventoryRow>,
}

/// One shape predicate enforced before WAT staging.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShapePredicate {
    /// Stable predicate name from `codegen_wasm::capability`.
    pub name: &'static str,
    /// Disposition of the predicate (currently always `enforced`).
    pub disposition: &'static str,
}

/// Public execution mode that can reach the WASM backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExecutionMode {
    /// Mode name (`command` or `npm`).
    pub mode: &'static str,
    /// Whether the mode is reachable from the public frontend today.
    pub reachable: bool,
}

/// One immutable upstream revision identified by release tag and commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RevisionPin {
    /// Human-readable immutable release tag.
    pub tag: &'static str,
    /// Full upstream Git commit.
    pub commit: &'static str,
}

/// One immutable php-src oracle profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PhpSrcPin {
    /// Elephc `--php-version` profile.
    pub profile: &'static str,
    /// php-src release tag.
    pub tag: &'static str,
    /// Full annotated-tag object ID stored at `refs/tags/<tag>`.
    pub tag_object: &'static str,
    /// Full peeled commit ID executed by the oracle.
    pub tag_commit: &'static str,
}

/// Exact acceptance-tool versions required by the frozen specification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolchainPins {
    /// Rust toolchain.
    pub rust: &'static str,
    /// `wat` crate.
    pub wat: &'static str,
    /// Production `wasmparser` crate.
    pub wasmparser: &'static str,
    /// Wasmer CLI/runtime.
    pub wasmer: &'static str,
    /// Wasmtime CLI/runtime.
    pub wasmtime: &'static str,
    /// `wasm-tools` validator.
    pub wasm_tools: &'static str,
    /// Node.js runtime.
    pub node: &'static str,
    /// TypeScript compiler.
    pub typescript: &'static str,
    /// npm coupling rule; npm is supplied by the pinned Node.js distribution.
    pub npm: &'static str,
}

/// Normative and toolchain pins recorded with the report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NormativePins {
    /// Frozen `docs/specs/wasm-compliance.md` SHA-256.
    pub wasm_compliance_sha256: &'static str,
    /// WebAssembly Core 3.0 tag and commit.
    pub wasm_core_3_0: RevisionPin,
    /// WASI Preview 1 commit.
    pub wasi_preview1_commit: &'static str,
    /// Exact php-src oracle profiles.
    pub php_src: &'static [PhpSrcPin],
    /// Exact acceptance toolchain.
    pub toolchain: ToolchainPins,
}

/// Metadata block. The committed baseline leaves the per-revision `commit` and
/// `dirty` fields `None`; `tools/gen_wasm_inventory` fills them from git when
/// emitting a per-run manifest retained by CI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReportMetadata {
    /// Schema identifier (`SCHEMA_ID`).
    pub schema: &'static str,
    /// Generator version (`GENERATOR_VERSION`).
    pub generator_version: &'static str,
    /// Elephc commit the report was generated from (`None` in the baseline).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// Dirty-tree flag (`None` in the baseline).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dirty: Option<bool>,
    /// Normative and toolchain pins.
    pub pins: NormativePins,
}

/// Aggregate totals across every family.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AggregateTotals {
    /// Total identities across every family.
    pub total: usize,
    /// Supported identities across every family.
    pub supported: usize,
    /// Excluded identities across every family.
    pub excluded: usize,
    /// Missing identities across every family.
    pub missing: usize,
    /// Total row-level plus catalog-level evidence gaps.
    pub evidence_gaps: usize,
    /// Rows whose required producer/mode/lowerer/test evidence is incomplete.
    pub row_evidence_gaps: usize,
    /// Required test-catalog categories whose durable evidence is absent.
    pub catalog_evidence_gaps: usize,
    /// `pass` only when no reachable identity or required evidence remains missing.
    pub gate: &'static str,
    /// Why the gate passes or fails.
    pub gate_reason: String,
    /// Confirms totals were derived from the enumeration, not copied from prose.
    pub stale_literal_counts_rejected: bool,
}

/// Catalog of positive, negative, differential, ownership, and host-test
/// identifiers that evidence the reported capability surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TestCatalog {
    /// Positive execution tests on `wasm32-wasi`.
    pub positive: Vec<&'static str>,
    /// Negative/diagnostic tests asserting unsupported shapes are rejected.
    pub negative: Vec<&'static str>,
    /// Durable differential tests against pinned php-src profiles.
    pub differential: Vec<&'static str>,
    /// Ownership/GC/COW regression tests.
    pub ownership: Vec<&'static str>,
    /// Three-host (Wasmer/Wasmtime/Node) portability test identifiers.
    pub host: Vec<&'static str>,
    /// Explicit missing evidence categories. This prevents placeholders from
    /// being presented as tests that already exist.
    pub missing_categories: Vec<&'static str>,
}

/// The full W0 inventory report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InventoryReport {
    /// Report metadata and normative pins.
    pub metadata: ReportMetadata,
    /// Per-family inventories keyed by family name.
    pub families: BTreeMap<&'static str, FamilyTotals>,
    /// Shape predicates enforced before WAT staging.
    pub shapes: Vec<ShapePredicate>,
    /// Public execution modes that can reach the backend.
    pub execution_modes: Vec<ExecutionMode>,
    /// Positive/negative/differential/ownership/host test identifiers.
    pub tests: TestCatalog,
    /// Aggregate totals derived from every family.
    pub totals: AggregateTotals,
}
