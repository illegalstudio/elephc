//! Purpose:
//! Runs independent SimpleXML review programs without combining AOT and opaque eval graphs.
//!
//! Called from:
//! - Managed-libxml2 codegen CI on every executable target.
//!
//! Key details:
//! - Each fixture keeps its native helper declarations before the marked PHP body.
//! - Native and eval modes preserve all eighteen original outputs and strict heap checks.

use crate::support::*;

/// Compiles one program, retaining separate process-level heap checks for filter call shapes.
fn run_review_program(fixture: &str, expected: &[&str], opaque: bool) {
    let (declarations, body) = fixture.split_once("// REVIEW_BODY\n").unwrap();
    let source = if opaque {
        let body = body.replace('\\', "\\\\").replace('\'', "\\'");
        // Opaque source needs an explicit native declaration to retain the XML capability.
        format!("{declarations}\nclass ReviewProgramXmlCapability extends SimpleXMLElement {{}}\n\
            $source = $argc > 0 ? '{body}' : ''; eval($source);")
    } else { format!("{declarations}{body}") };
    let args: Vec<Vec<&str>> = if expected.len() == 2 { vec![vec![], vec!["constructor"]] }
        else { vec![vec![]] };
    let argvs: Vec<_> = args.iter().map(Vec::as_slice).collect();
    let (runs, assembly) = compile_and_run_with_heap_debug_and_asm_per_argv(&source, &argvs);
    preserve_stdout_mismatches(&source, &assembly, &runs, expected);
    drop(assembly);
    assert_eq!(runs.len(), expected.len());
    for (mode, (output, expected)) in runs.iter().zip(expected).enumerate() {
        assert!(output.success, "mode={mode} stdout={:?} stderr={}", output.stdout, output.stderr);
        assert_eq!(output.stdout.as_str(), *expected, "mode={mode}: {}", output.stderr);
        assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "mode={mode}: {}", output.stderr);
        assert!(!output.stderr.contains("Warning:"), "mode={mode}: {}", output.stderr);
    }
}

/// Preserves the names review assertions in an independent native program.
#[test]
fn test_simplexml_review_names_native_bundle() {
    if skip_without_xml_native("test_simplexml_review_names_native_bundle") { return; }
    run_review_program(include_str!("../../fixtures/simplexml/review_names.php"),
        &[include_str!("../../fixtures/simplexml/review_names.stdout")], false);
}

/// Preserves the names review assertions in an independent eval program.
#[test]
fn test_simplexml_review_names_eval_bundle() {
    if skip_without_xml_native("test_simplexml_review_names_eval_bundle") { return; }
    run_review_program(include_str!("../../fixtures/simplexml/review_names.php"),
        &[include_str!("../../fixtures/simplexml/review_names.stdout")], true);
}

/// Preserves the filters review assertions in an independent native program.
#[test]
fn test_simplexml_review_filters_native_bundle() {
    if skip_without_xml_native("test_simplexml_review_filters_native_bundle") { return; }
    run_review_program(include_str!("../../fixtures/simplexml/review_filters.php"),
        &[include_str!("../../fixtures/simplexml/review_filters.stdout"), include_str!("../../fixtures/simplexml/review_filter_constructor.stdout")], false);
}

/// Preserves the filters review assertions in an independent eval program.
#[test]
fn test_simplexml_review_filters_eval_bundle() {
    if skip_without_xml_native("test_simplexml_review_filters_eval_bundle") { return; }
    run_review_program(include_str!("../../fixtures/simplexml/review_filters.php"),
        &[include_str!("../../fixtures/simplexml/review_filters.stdout"), include_str!("../../fixtures/simplexml/review_filter_constructor.stdout")], true);
}

/// Preserves the semantics review assertions in an independent native program.
#[test]
fn test_simplexml_review_semantics_native_bundle() {
    if skip_without_xml_native("test_simplexml_review_semantics_native_bundle") { return; }
    run_review_program(include_str!("../../fixtures/simplexml/review_semantics.php"),
        &[include_str!("../../fixtures/simplexml/review_semantics.stdout")], false);
}

/// Preserves the semantics review assertions in an independent eval program.
#[test]
fn test_simplexml_review_semantics_eval_bundle() {
    if skip_without_xml_native("test_simplexml_review_semantics_eval_bundle") { return; }
    run_review_program(include_str!("../../fixtures/simplexml/review_semantics.php"),
        &[include_str!("../../fixtures/simplexml/review_semantics.stdout")], true);
}

/// Preserves the updates review assertions in an independent native program.
#[test]
fn test_simplexml_review_updates_native_bundle() {
    if skip_without_xml_native("test_simplexml_review_updates_native_bundle") { return; }
    run_review_program(include_str!("../../fixtures/simplexml/review_updates_native.php"),
        &[include_str!("../../fixtures/simplexml/review_updates_native.stdout")], false);
}

/// Preserves the updates review assertions in an independent eval program.
#[test]
fn test_simplexml_review_updates_eval_bundle() {
    if skip_without_xml_native("test_simplexml_review_updates_eval_bundle") { return; }
    run_review_program(include_str!("../../fixtures/simplexml/review_updates_eval.php"),
        &[include_str!("../../fixtures/simplexml/review_updates_eval.stdout")], true);
}

/// Preserves the quiet review assertions in an independent native program.
#[test]
fn test_simplexml_review_quiet_native_bundle() {
    if skip_without_xml_native("test_simplexml_review_quiet_native_bundle") { return; }
    run_review_program(include_str!("../../fixtures/simplexml/review_quiet.php"),
        &[include_str!("../../fixtures/simplexml/review_quiet.stdout")], false);
}

/// Preserves the quiet review assertions in an independent eval program.
#[test]
fn test_simplexml_review_quiet_eval_bundle() {
    if skip_without_xml_native("test_simplexml_review_quiet_eval_bundle") { return; }
    run_review_program(include_str!("../../fixtures/simplexml/review_quiet.php"),
        &[include_str!("../../fixtures/simplexml/review_quiet.stdout")], true);
}

/// Preserves the projections review assertions in an independent native program.
#[test]
fn test_simplexml_review_projections_native_bundle() {
    if skip_without_xml_native("test_simplexml_review_projections_native_bundle") { return; }
    run_review_program(include_str!("../../fixtures/simplexml/review_projections.php"),
        &[include_str!("../../fixtures/simplexml/review_projections.stdout")], false);
}

/// Preserves the projections review assertions in an independent eval program.
#[test]
fn test_simplexml_review_projections_eval_bundle() {
    if skip_without_xml_native("test_simplexml_review_projections_eval_bundle") { return; }
    run_review_program(include_str!("../../fixtures/simplexml/review_projections.php"),
        &[include_str!("../../fixtures/simplexml/review_projections.stdout")], true);
}

/// Preserves the dom review assertions in an independent native program.
#[test]
fn test_simplexml_review_dom_native_bundle() {
    if skip_without_xml_native("test_simplexml_review_dom_native_bundle") { return; }
    run_review_program(include_str!("../../fixtures/simplexml/review_dom.php"),
        &[include_str!("../../fixtures/simplexml/review_dom.stdout")], false);
}

/// Preserves the dom review assertions in an independent eval program.
#[test]
fn test_simplexml_review_dom_eval_bundle() {
    if skip_without_xml_native("test_simplexml_review_dom_eval_bundle") { return; }
    run_review_program(include_str!("../../fixtures/simplexml/review_dom.php"),
        &[include_str!("../../fixtures/simplexml/review_dom.stdout")], true);
}

/// Preserves the contract review assertions in an independent native program.
#[test]
fn test_simplexml_review_contract_native_bundle() {
    if skip_without_xml_native("test_simplexml_review_contract_native_bundle") { return; }
    run_review_program(include_str!("../../fixtures/simplexml/review_contract.php"),
        &[include_str!("../../fixtures/simplexml/review_contract.stdout")], false);
}

/// Preserves the contract review assertions in an independent eval program.
#[test]
fn test_simplexml_review_contract_eval_bundle() {
    if skip_without_xml_native("test_simplexml_review_contract_eval_bundle") { return; }
    run_review_program(include_str!("../../fixtures/simplexml/review_contract.php"),
        &[include_str!("../../fixtures/simplexml/review_contract.stdout")], true);
}
