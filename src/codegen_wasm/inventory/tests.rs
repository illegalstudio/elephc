//! Purpose:
//! Schema and disposition regression tests for the WASM capability inventory.
//!
//! Called from:
//! - `cargo test` through the `inventory` module's `#[cfg(test)]` harness.
//!
//! Key details:
//! - Every identity must carry exactly one disposition; totals are derived
//!   from the enumeration; missing reachable identities fail the gate;
//!   excluded rows carry a complete contract and matching diagnostic.
#![allow(dead_code)]

use super::*;
use crate::codegen_wasm::capability::{op_is_supported, runtime_function_is_supported};
use crate::ir::{IrHeapKind, IrType};
use std::collections::HashSet;

/// Extracts top-level variant names from a repo-owned enum declaration.
///
/// The parser is deliberately narrow: the audited enums use Rust identifiers
/// and optional tuple/struct payloads. It is a CI drift tripwire, not a general
/// Rust parser.
fn declared_enum_variants(source: &str, declaration: &str) -> Vec<String> {
    let declaration_start = source
        .find(declaration)
        .unwrap_or_else(|| panic!("missing enum declaration {declaration:?}"));
    let body_start = source[declaration_start..]
        .find('{')
        .map(|offset| declaration_start + offset + 1)
        .expect("enum declaration has a body");
    let mut variants = Vec::new();
    let mut payload_depth = 0usize;
    for raw_line in source[body_start..].lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with("///") || line.starts_with("#[") {
            continue;
        }
        if payload_depth == 0 && line.starts_with('}') {
            break;
        }
        if payload_depth == 0 {
            let name = line
                .split(|character: char| {
                    matches!(character, ',' | '(' | '{' | '=') || character.is_whitespace()
                })
                .next()
                .unwrap_or_default();
            if name
                .bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_uppercase())
            {
                variants.push(name.to_string());
            }
        }
        if !line.starts_with("//") {
            payload_depth += line.bytes().filter(|byte| *byte == b'{').count();
            payload_depth -= line.bytes().filter(|byte| *byte == b'}').count();
        }
    }
    variants
}

/// Extracts every capability predicate whose function name ends in `_issue`.
fn declared_shape_predicates(source: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in source.lines().map(str::trim_start) {
        let Some(fn_offset) = line.find("fn ") else {
            continue;
        };
        let tail = &line[fn_offset + 3..];
        let name = tail
            .split(|character: char| character == '(' || character.is_whitespace())
            .next()
            .unwrap_or_default();
        if name.ends_with("_issue") {
            names.push(name.to_string());
        }
    }
    names.sort();
    names.dedup();
    names
}

/// Resolves one report test identifier to its exact checked-in Rust module.
fn test_source(identifier: &str) -> &'static str {
    if identifier.starts_with("codegen_wasm::tests::") {
        include_str!("../mod.rs")
    } else if identifier.starts_with("codegen_wasm::capability::tests::") {
        include_str!("../capability.rs")
    } else if identifier.starts_with("codegen_wasm::closures::tests::") {
        include_str!("../closures.rs")
    } else if identifier.starts_with("codegen_wasm::strict::tests::") {
        include_str!("../strict.rs")
    } else if identifier.starts_with("codegen_wasm::function::tests::") {
        include_str!("../function.rs")
    } else if identifier.starts_with("codegen_wasm::gc::tests::") {
        include_str!("../gc.rs")
    } else if identifier.starts_with("codegen_wasm::statics::tests::") {
        include_str!("../statics.rs")
    } else if identifier.starts_with("codegen_wasm::builtins::tests::") {
        include_str!("../builtins.rs")
    } else if identifier.starts_with("codegen::cli::") {
        include_str!("../../../tests/codegen/cli.rs")
    } else if identifier.starts_with("ir_lower::tests::ownership::") {
        include_str!("../../ir_lower/tests/ownership.rs")
    } else {
        panic!("inventory references an unaudited Rust test module {identifier:?}");
    }
}

/// Extracts one Rust function item using balanced braces.
fn function_source<'a>(source: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("fn {name}(");
    let start = source.find(&needle)?;
    let open = source[start..].find('{')? + start;
    let mut depth = 0usize;
    for (offset, byte) in source.as_bytes()[open..].iter().copied().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&source[start..=open + offset]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Extracts simple function-call identifiers from one Rust function body.
fn called_function_names(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut names = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index].is_ascii_alphabetic() || bytes[index] == b'_' {
            let start = index;
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            let mut next = index;
            while next < bytes.len() && bytes[next].is_ascii_whitespace() {
                next += 1;
            }
            if next < bytes.len() && bytes[next] == b'(' {
                names.push(source[start..index].to_string());
            }
        } else {
            index += 1;
        }
    }
    names
}

/// Follows same-module helper calls to prove a test or one of its helpers
/// contains the claimed opcode marker.
fn function_exercises_marker(
    source: &str,
    function: &str,
    marker: &str,
    depth: usize,
    seen: &mut HashSet<String>,
) -> bool {
    if depth == 0 || !seen.insert(function.to_string()) {
        return false;
    }
    let Some(body) = function_source(source, function) else {
        return false;
    };
    if body.contains(marker) {
        return true;
    }
    called_function_names(body)
        .into_iter()
        .any(|called| function_exercises_marker(source, &called, marker, depth - 1, seen))
}

/// Returns the canonical Rust source markers that construct one opcode.
fn op_source_markers(op: Op) -> Vec<String> {
    let mut markers = vec![format!("Op::{op:?}")];
    let builder_helper = match op {
        Op::ConstI64 => Some("emit_const_i64("),
        Op::ConstF64 => Some("emit_const_f64("),
        Op::ConstStr => Some("emit_const_str("),
        Op::ConstNull => Some("emit_const_null("),
        Op::ConstBool => Some("emit_const_bool("),
        Op::LoadLocal => Some("emit_load_local("),
        Op::StoreLocal => Some("emit_store_local("),
        _ => None,
    };
    if let Some(marker) = builder_helper {
        markers.push(marker.to_string());
    }
    markers
}

/// Verifies every enumerated identity carries exactly one disposition and
/// the report validates against the W0 schema.
#[test]
fn every_identity_has_exactly_one_disposition() {
    let report = build_report();
    let errors = validate_report(&report);
    assert!(errors.is_empty(), "schema/disposition errors:\n{}", errors.join("\n"));
    for family in report.families.values() {
        for row in &family.rows {
            let payloads = row.supported.is_some() as usize
                + row.excluded.is_some() as usize
                + row.missing.is_some() as usize;
            assert_eq!(
                payloads, 1,
                "row {:?} ({:?}) carries {} payload fields",
                row.name, row.disposition, payloads
            );
        }
    }
}

/// Verifies the canonical enumerators and the exhaustive capability
/// classifiers agree on the supported/missing split, so the report cannot
/// drift from `codegen_wasm::capability`.
#[test]
fn inventory_matches_capability_classifiers() {
    for op in Op::all() {
        let row = op_row(*op);
        if op_exclusion(*op).is_some() {
            assert_eq!(row.disposition, Disposition::Excluded, "op {:?}", op.name());
        } else if op_is_supported(*op) {
            assert_eq!(row.disposition, Disposition::Supported, "op {:?}", op.name());
        } else {
            assert_eq!(row.disposition, Disposition::Missing, "op {:?}", op.name());
        }
    }
    for id in RuntimeFnId::all() {
        let row = runtime_fn_row(*id);
        if runtime_fn_exclusion(*id).is_some() {
            assert_eq!(row.disposition, Disposition::Excluded, "runtime_fn {:?}", id.as_eir());
        } else if runtime_function_is_supported(*id) {
            assert_eq!(row.disposition, Disposition::Supported, "runtime_fn {:?}", id.as_eir());
        } else {
            assert_eq!(row.disposition, Disposition::Missing, "runtime_fn {:?}", id.as_eir());
        }
    }
    for ir_type in ir_type_representatives() {
        let row = ir_type_row(ir_type);
        let expected = if ir_type == IrType::Heap(IrHeapKind::Buffer) {
            Disposition::Excluded
        } else {
            Disposition::Supported
        };
        assert_eq!(row.disposition, expected, "ir_type {}", ir_type.as_eir());
    }
}

/// Verifies no `Op::all()` variant is duplicated and the enumeration covers
/// every variant the exhaustive `Op::name` classifier knows.
#[test]
fn op_enumeration_has_no_duplicates() {
    let mut seen = HashSet::new();
    for op in Op::all() {
        assert!(seen.insert(op.name()), "duplicate Op name {:?}", op.name());
    }
    let mut rt_seen = HashSet::new();
    for id in RuntimeFnId::all() {
        assert!(rt_seen.insert(id.as_eir()), "duplicate RuntimeFnId name {:?}", id.as_eir());
    }
    let mut un_seen = HashSet::new();
    for u in UnaryStringRuntime::all() {
        assert!(un_seen.insert(u.as_eir()), "duplicate UnaryStringRuntime name {:?}", u.as_eir());
    }
}

/// Verifies the three `::all()` enumerators exactly match their enum declarations.
#[test]
fn enum_enumerators_match_source_declarations() {
    let op_declared =
        declared_enum_variants(include_str!("../../ir/instr.rs"), "pub enum Op");
    let op_enumerated: Vec<String> = Op::all().iter().map(|op| format!("{op:?}")).collect();
    assert_eq!(op_enumerated, op_declared, "Op::all() drifted");

    let runtime_declared = declared_enum_variants(
        include_str!("../../ir/runtime_fn.rs"),
        "pub enum RuntimeFnId",
    );
    let runtime_enumerated: Vec<String> = RuntimeFnId::all()
        .iter()
        .map(|id| format!("{id:?}"))
        .collect();
    assert_eq!(
        runtime_enumerated, runtime_declared,
        "RuntimeFnId::all() drifted"
    );

    let unary_declared = declared_enum_variants(
        include_str!("../../ir/runtime_call.rs"),
        "pub enum UnaryStringRuntime",
    );
    let unary_enumerated: Vec<String> = UnaryStringRuntime::all()
        .iter()
        .map(|target| format!("{target:?}"))
        .collect();
    assert_eq!(
        unary_enumerated, unary_declared,
        "UnaryStringRuntime::all() drifted"
    );

    let ir_type_variants =
        declared_enum_variants(include_str!("../../ir/types.rs"), "pub enum IrType");
    assert_eq!(
        ir_type_variants,
        ["I64", "F64", "Str", "TaggedScalar", "Heap", "Void"]
    );
    let heap_variants =
        declared_enum_variants(include_str!("../../ir/types.rs"), "pub enum IrHeapKind");
    assert_eq!(
        heap_variants,
        ["Array", "Hash", "Object", "Mixed", "Iterable", "Union", "Buffer"]
    );
    assert_eq!(
        ir_type_representatives().len(),
        ir_type_variants.len() - 1 + heap_variants.len(),
        "IrType concrete-form inventory drifted"
    );
}

/// Verifies payload enum forms and capability predicates cannot outgrow the inventory.
#[test]
fn payload_forms_and_shape_predicates_match_source_declarations() {
    let runtime_targets = declared_enum_variants(
        include_str!("../../ir/runtime_call.rs"),
        "pub enum RuntimeCallTarget",
    );
    assert_eq!(
        runtime_targets,
        ["ArrayFetchForWrite", "UnaryString", "Function", "ProfiledFunction"]
    );
    assert_eq!(
        runtime_call_target_rows().len(),
        runtime_targets.len(),
        "RuntimeCallTarget inventory drifted"
    );

    let terminators =
        declared_enum_variants(include_str!("../../ir/block.rs"), "pub enum Terminator");
    assert_eq!(
        terminators,
        [
            "Br",
            "CondBr",
            "Switch",
            "Return",
            "Throw",
            "Fatal",
            "GeneratorSuspend",
            "Unreachable",
        ]
    );
    assert_eq!(
        terminator_representatives().len(),
        terminators.len(),
        "Terminator inventory drifted"
    );

    let declared =
        declared_shape_predicates(include_str!("../capability.rs"));
    let mut inventoried: Vec<String> = shape_predicates()
        .iter()
        .map(|predicate| predicate.name.to_string())
        .collect();
    inventoried.sort();
    inventoried.dedup();
    assert_eq!(inventoried, declared, "shape predicate inventory drifted");
}

/// Verifies every Rust test identifier in the report names a checked-in test function.
#[test]
fn rust_test_identifiers_resolve_to_checked_in_tests() {
    let report = build_report();
    let mut identifiers = Vec::new();
    identifiers.extend(report.tests.positive.iter().copied());
    identifiers.extend(report.tests.negative.iter().copied());
    identifiers.extend(report.tests.differential.iter().copied());
    identifiers.extend(report.tests.ownership.iter().copied());
    for family in report.families.values() {
        for row in &family.rows {
            if let Some(evidence) = &row.supported {
                identifiers.extend(evidence.tests.iter().copied());
            }
        }
    }

    for identifier in identifiers {
        let source = test_source(identifier);
        let test_name = identifier
            .rsplit("::")
            .next()
            .expect("test identifier has a final segment");
        assert!(
            source.contains(&format!("fn {test_name}(")),
            "inventory references missing Rust test {identifier:?}"
        );
    }

    assert!(include_str!("../../../scripts/test-wasm-hosts.sh").contains("#!/"));
    assert!(
        include_str!("../../../.github/workflows/ci.yml").contains("wasm-host-portability:")
    );
}

/// Verifies every claimed supported-op test reaches the exact opcode directly
/// or through same-module helpers.
#[test]
fn supported_op_test_claims_reach_the_claimed_opcode() {
    for op in Op::all() {
        let row = op_row(*op);
        let Some(evidence) = row.supported else {
            continue;
        };
        if evidence.tests.is_empty() {
            continue;
        }
        let markers = op_source_markers(*op);
        assert!(
            evidence.tests.iter().any(|identifier| {
                let source = test_source(identifier);
                let function = identifier
                    .rsplit("::")
                    .next()
                    .expect("test identifier has a final segment");
                markers.iter().any(|marker| {
                    function_exercises_marker(
                        source,
                        function,
                        marker,
                        8,
                        &mut HashSet::new(),
                    )
                })
            }),
            "supported op {:?} cites tests that do not reach any of {:?}",
            op.name(),
            markers
        );
    }
}

/// Verifies supported runtime-function tests reach the exact typed target.
#[test]
fn supported_runtime_fn_test_claims_reach_the_claimed_target() {
    for id in RuntimeFnId::all() {
        let row = runtime_fn_row(*id);
        let Some(evidence) = row.supported else {
            continue;
        };
        let marker = format!("RuntimeFnId::{id:?}");
        assert!(
            evidence.tests.iter().any(|identifier| {
                let source = test_source(identifier);
                let function = identifier
                    .rsplit("::")
                    .next()
                    .expect("test identifier has a final segment");
                function_exercises_marker(
                    source,
                    function,
                    &marker,
                    8,
                    &mut HashSet::new(),
                )
            }),
            "supported runtime function {:?} cites tests that do not reach {marker}",
            id.as_eir()
        );
    }
}

/// Verifies supported runtime-call-form tests reach the exact payload variant.
#[test]
fn supported_runtime_call_form_tests_reach_the_claimed_variant() {
    for row in runtime_call_target_rows()
        .into_iter()
        .filter(|row| row.disposition == Disposition::Supported)
    {
        let marker = match row.name.as_str() {
            "function" => "RuntimeCallTarget::Function",
            "profiled_function" => "RuntimeCallTarget::ProfiledFunction",
            other => panic!("unexpected supported runtime-call form {other:?}"),
        };
        let evidence = row.supported.expect("supported form has evidence");
        assert!(
            evidence.tests.iter().any(|identifier| {
                let source = test_source(identifier);
                let function = identifier
                    .rsplit("::")
                    .next()
                    .expect("test identifier has a final segment");
                function_exercises_marker(
                    source,
                    function,
                    marker,
                    8,
                    &mut HashSet::new(),
                )
            }),
            "runtime-call form {:?} cites tests that do not reach {marker}",
            row.name
        );
    }
}

/// Verifies the report rejects stale literal historical counts by deriving
/// totals from the enumeration rather than copying prose figures.
#[test]
fn totals_are_derived_not_copied_from_prose() {
    let report = build_report();
    assert!(report.totals.stale_literal_counts_rejected);
    let op = &report.families["op"];
    assert_eq!(op.total, Op::all().len());
    assert_eq!(op.total, op.supported + op.excluded + op.missing);
    let rt = &report.families["runtime_fn"];
    assert_eq!(rt.total, RuntimeFnId::all().len());
    let un = &report.families["unary_string"];
    assert_eq!(un.total, UnaryStringRuntime::all().len());
    let ir_type = &report.families["ir_type"];
    assert_eq!(ir_type.total, ir_type_representatives().len());
    // The supported count is derived from the capability classifier, not
    // copied from the spec prose's historical "90 of 236" figure.
    let derived_supported = Op::all()
        .iter()
        .copied()
        .filter(|o| op_is_supported(*o) && op_exclusion(*o).is_none())
        .count();
    assert_eq!(op.supported, derived_supported);
    let derived_excluded = Op::all()
        .iter()
        .copied()
        .filter(|o| op_exclusion(*o).is_some())
        .count();
    assert_eq!(op.excluded, derived_excluded);
    let _ = (rt.supported, un.missing);
}

/// Verifies the gate fails while any missing identity remains reachable,
/// matching the W0 rule that missing/reachable entries fail the gate.
#[test]
fn gate_fails_when_missing_reachable() {
    let report = build_report();
    assert!(report.totals.missing > 0, "current revision must still have missing identities");
    assert_eq!(report.totals.gate, "fail");
    assert!(
        report.totals.gate_reason.contains("missing"),
        "gate_reason should explain the missing count: {}",
        report.totals.gate_reason
    );
}

/// Verifies a dispatched lowerer is not reported as supported when every PHP shape is rejected.
#[test]
fn float_to_int_remains_missing_until_a_php_shape_is_admitted() {
    assert!(!op_is_supported(Op::FToI));
    let row = op_row(Op::FToI);
    assert_eq!(row.disposition, Disposition::Missing);
    assert!(row.supported.is_none());
}

/// Verifies supported rows never reuse a merely existing but unrelated test
/// identifier as proof of lowering coverage.
#[test]
fn supported_test_evidence_is_identity_specific() {
    let integer_or = op_row(Op::IBitOr);
    assert!(
        integer_or
            .supported
            .as_ref()
            .expect("integer bitwise-or remains supported")
            .tests
            .is_empty(),
        "IBitOr must remain an explicit evidence gap until a lowering test exercises it"
    );
    let object_new = op_row(Op::ObjectNew);
    assert_eq!(
        object_new
            .supported
            .as_ref()
            .expect("object construction remains supported")
            .tests,
        ["codegen_wasm::tests::object_prop_set_overwrites"]
    );
}

/// Verifies every supported-row lowerer path resolves to a function in the
/// backend module named by the path.
#[test]
fn supported_lowerer_paths_resolve_to_backend_functions() {
    let report = build_report();
    for row in report
        .families
        .values()
        .flat_map(|family| &family.rows)
        .filter(|row| row.disposition == Disposition::Supported)
    {
        let evidence = row.supported.as_ref().expect("supported row has evidence");
        let lowerer = evidence.lowerer;
        let source = if lowerer.starts_with("codegen_wasm::inst_hash::") {
            include_str!("../inst_hash.rs")
        } else if lowerer.starts_with("codegen_wasm::inst::") {
            include_str!("../inst.rs")
        } else if lowerer.starts_with("codegen_wasm::objects::") {
            include_str!("../objects.rs")
        } else if lowerer.starts_with("codegen_wasm::methods::") {
            include_str!("../methods.rs")
        } else if lowerer.starts_with("codegen_wasm::classes::") {
            include_str!("../classes.rs")
        } else if lowerer.starts_with("codegen_wasm::closures::") {
            include_str!("../closures.rs")
        } else if lowerer.starts_with("codegen_wasm::refcell::") {
            include_str!("../refcell.rs")
        } else if lowerer.starts_with("codegen_wasm::function::") {
            include_str!("../function.rs")
        } else if lowerer.starts_with("codegen_wasm::values::") {
            include_str!("../values.rs")
        } else if lowerer.starts_with("codegen_wasm::strict::") {
            include_str!("../strict.rs")
        } else if lowerer.starts_with("codegen_wasm::builtins::") {
            include_str!("../builtins.rs")
        } else {
            panic!(
                "supported row {:?} references unaudited lowerer path {lowerer:?}",
                row.name
            );
        };
        let function = lowerer
            .rsplit("::")
            .next()
            .expect("lowerer path has a final segment")
            .split('(')
            .next()
            .expect("lowerer function has a stable name");
        assert!(
            source.contains(&format!("fn {function}(")),
            "supported row {:?} references missing lowerer {lowerer:?}",
            row.name
        );
    }
}

/// Verifies producer and execution-mode evidence is carried by rows rather
/// than being hidden inside the supported-only payload.
///
/// Both dispositions are checked, because that is the whole claim: a row that is still a gap
/// names the PHP that reaches it just as a lowered one does. Using only a missing row would let
/// the evidence quietly move into the supported payload, and using only a supported row would
/// let it disappear from gaps — which is where it matters most, since a gap with no producer is
/// a gap nobody can act on.
#[test]
fn row_level_producers_and_execution_modes_are_revision_honest() {
    let array_map = runtime_fn_row(RuntimeFnId::ArrayMap);
    assert_eq!(array_map.producers, ["array_map(...)"]);
    assert_eq!(array_map.execution_modes, ["command", "npm"]);

    let md5 = runtime_fn_row(RuntimeFnId::Md5);
    assert_eq!(md5.disposition, Disposition::Supported);
    assert_eq!(md5.producers, ["md5(...)"]);
    assert_eq!(md5.execution_modes, ["command", "npm"]);

    let gzcompress = runtime_fn_row(RuntimeFnId::Gzcompress);
    assert_eq!(gzcompress.disposition, Disposition::Missing);
    assert_eq!(gzcompress.producers, ["gzcompress(...)"]);
    assert_eq!(gzcompress.execution_modes, ["command", "npm"]);

    for row in ir_type_representatives().into_iter().map(ir_type_row) {
        assert!(!row.producers.is_empty(), "{} lacks a producer", row.name);
        assert_eq!(row.execution_modes, ["command", "npm"]);
    }
}

/// Verifies excluded rows carry a complete contract and a matching target
/// diagnostic, so exclusions are never silently "unsupported".
#[test]
fn excluded_rows_carry_complete_contracts() {
    let report = build_report();
    let mut excluded = 0usize;
    for family in report.families.values() {
        for row in &family.rows {
            if row.disposition == Disposition::Excluded {
                excluded += 1;
                let exclusion = row.excluded.as_ref().expect("excluded row has contract");
                assert!(!exclusion.category.is_empty());
                assert!(!exclusion.reason.is_empty());
                assert!(!exclusion.owner.is_empty());
                assert!(!exclusion.removal_gate.is_empty());
                assert!(
                    !exclusion.diagnostic.is_empty(),
                    "excluded row {:?} lacks a matching diagnostic",
                    row.name
                );
                let expected = match row.family {
                    "op" => format!("unsupported op {}", row.name),
                    "runtime_fn" => {
                        format!("unsupported runtime function {}", row.name)
                    }
                    "ir_type" => format!("unsupported storage type {}", row.name),
                    other => panic!("unexpected excluded family {other:?}"),
                };
                assert_eq!(
                    exclusion.diagnostic, expected,
                    "excluded row {:?} diagnostic drifted",
                    row.name
                );
            }
        }
    }
    assert!(excluded > 0, "expected native-only exclusions to be recorded");
}

/// Verifies native implementation requirements never exclude ordinary PHP builtins.
///
/// The rule under test is about EXCLUSION, which is reserved for Elephc-only extensions and the
/// web SAPI: an ordinary PHP builtin with no WASM lowerer is `missing`, a reachable gap someone
/// can close. So these may be `supported` once they ARE lowered — `sha1` now is — and what must
/// never happen is any of them turning into `excluded`.
#[test]
fn ordinary_php_bridge_and_system_builtins_remain_reachable_gaps() {
    for id in [
        RuntimeFnId::Md5,
        RuntimeFnId::Hash,
        RuntimeFnId::Sha1,
        RuntimeFnId::MbStrlen,
        RuntimeFnId::Gzcompress,
    ] {
        assert!(
            runtime_fn_exclusion(id).is_none(),
            "ordinary PHP runtime {} must not be excluded",
            id.as_eir()
        );
        assert!(
            matches!(
                runtime_fn_row(id).disposition,
                Disposition::Missing | Disposition::Supported
            ),
            "ordinary PHP runtime {} is a reachable gap or lowered, never excluded",
            id.as_eir()
        );
    }
    for id in [
        RuntimeFnId::Ptr,
        RuntimeFnId::BufferLen,
        RuntimeFnId::ZvalPack,
        RuntimeFnId::ClassAttributeNames,
        RuntimeFnId::Header,
    ] {
        assert_eq!(
            runtime_fn_row(id).disposition,
            Disposition::Excluded,
            "{} is an explicit Elephc/web exclusion",
            id.as_eir()
        );
    }
}

/// Verifies the report serializes to a well-formed JSON object whose
/// derived totals and per-family counts match the in-memory report, so the
/// committed JSON artifact is a faithful encoding.
#[test]
fn report_serializes_to_faithful_json() {
    let report = build_report();
    let json = serde_json::to_string(&report).expect("serialize report");
    let value: serde_json::Value =
        serde_json::from_str(&json).expect("report JSON parses");
    let obj = value.as_object().expect("report is a JSON object");
    assert_eq!(obj["metadata"]["schema"], SCHEMA_ID);
    assert_eq!(
        obj["metadata"]["pins"]["wasm_compliance_sha256"],
        FROZEN_SPEC_SHA256
    );
    assert_eq!(
        obj["metadata"]["pins"]["wasm_core_3_0"]["commit"],
        "9d36019973201a19f9c9ebb0f10828b2fe2374aa"
    );
    assert_eq!(obj["metadata"]["pins"]["php_src"].as_array().unwrap().len(), 4);
    assert_eq!(obj["metadata"]["pins"]["toolchain"]["wasmparser"], "0.252.0");
    let totals = &obj["totals"];
    assert_eq!(totals["total"], report.totals.total);
    assert_eq!(totals["supported"], report.totals.supported);
    assert_eq!(totals["excluded"], report.totals.excluded);
    assert_eq!(totals["missing"], report.totals.missing);
    assert_eq!(totals["evidence_gaps"], report.totals.evidence_gaps);
    assert_eq!(
        totals["row_evidence_gaps"],
        report.totals.row_evidence_gaps
    );
    assert_eq!(
        totals["catalog_evidence_gaps"],
        report.totals.catalog_evidence_gaps
    );
    assert_eq!(totals["gate"], report.totals.gate);
    assert_eq!(totals["stale_literal_counts_rejected"], true);
    for (name, family) in &report.families {
        let f = &obj["families"][name];
        assert_eq!(f["total"], family.total);
        assert_eq!(f["supported"], family.supported);
        assert_eq!(f["excluded"], family.excluded);
        assert_eq!(f["missing"], family.missing);
        assert_eq!(
            f["rows"].as_array().unwrap().len(),
            family.rows.len()
        );
    }
    assert!(validate_report(&report).is_empty());
}

/// Verifies revision metadata is either absent for the baseline or a full paired record.
#[test]
fn revision_metadata_rejects_partial_or_short_git_identity() {
    let mut report = build_report();
    report.metadata.commit = Some("abc".to_string());
    let errors = validate_report(&report);
    assert!(
        errors
            .iter()
            .any(|error| error.contains("must either both be present")),
        "{errors:?}"
    );

    report.metadata.dirty = Some(false);
    let errors = validate_report(&report);
    assert!(
        errors
            .iter()
            .any(|error| error.contains("full 40-hex Git commit")),
        "{errors:?}"
    );

    report.metadata.commit =
        Some("0123456789abcdef0123456789abcdef01234567".to_string());
    assert!(validate_report(&report).is_empty());
}

/// Verifies the human summary is non-empty and names the derived totals.
#[test]
fn human_summary_names_derived_totals() {
    let report = build_report();
    let summary = human_summary(&report);
    assert!(summary.contains("wasm32-wasi"));
    assert!(summary.contains("derived"));
    assert!(summary.contains(&report.totals.total.to_string()));
}
