//! Purpose:
//! Pins constructor-prefix exception summaries with authoritative default metadata.
//!
//! Called from:
//! - The optimizer DCE unit tests.
//!
//! Key details:
//! - The executable fixture harness uses an AST-only compatibility analysis.
//! - These tests exercise the CLI's metadata-aware optimizer and supplied-default controls.

use super::*;

/// Checks that selecting a default retains Exception catches while supplied arrays bypass it.
fn assert_default_prefix_catch(call: &str, expected: bool) {
    assert_default_prefix_catch_with_default(call, "[new Value(), parent::A]", expected);
}

/// Exercises both retained scope errors and valid trees after a lazy error branch was pruned.
fn assert_default_prefix_catch_with_default(call: &str, default: &str, expected: bool) {
    let source = format!(r#"<?php
class Value {{ public function __construct() {{ throw new Exception('prefix'); }} }}
class Probe {{
    public static function value(array $items = {default}): void {{}}
    public function instance(array $items = {default}): void {{}}
}}
class Maker {{ public function __construct(array $items = {default}) {{}} }}
function free_default(array $items = {default}): void {{}}
try {{ {call}; }} catch (Exception $error) {{ echo 'caught'; }}
"#);
    let tokens = crate::lexer::tokenize(&source).unwrap();
    let program = crate::parser::parse(&tokens).unwrap();
    let checked = crate::types::check(&program).unwrap();
    let optimizer = PostTypecheckOptimizer::new_with_type_metadata(
        &program, &checked.functions, &checked.classes, &checked.interfaces,
    );
    let result = optimizer.eliminate_dead_code(program, checked.local_binding_decision_spans());
    let retained = result.iter().any(|stmt| matches!(&stmt.kind,
        StmtKind::Try { catches, .. } if catches.iter().any(|catch|
            catch.exception_types.iter().any(|name| name == "Exception"))));
    assert_eq!(retained, expected, "{call}: {result:?}");
}

/// A static method's omitted array default may throw Exception before Error.
#[test]
fn test_nested_default_review_boundaries_static_prefix_summary() {
    assert_default_prefix_catch("Probe::value()", true);
    assert_default_prefix_catch("Probe::value([])", false);
}

/// An instance method's omitted array default keeps its constructor-prefix catch.
#[test]
fn test_nested_default_review_boundaries_instance_prefix_summary() {
    assert_default_prefix_catch("(new Probe())->instance()", true);
    assert_default_prefix_catch("(new Probe())->instance([])", false);
}

/// Constructor defaults have the same prefix exception contract as other methods.
#[test]
fn test_nested_default_review_boundaries_constructor_prefix_summary() {
    assert_default_prefix_catch("new Maker()", true);
    assert_default_prefix_catch("new Maker([])", false);
}

/// A pruned scope-error branch must not erase an earlier throwing default constructor.
#[test]
fn test_nested_default_review_boundaries_prefix_summary_without_scope_error() {
    for default in ["[new Value()]", "[new Value(), true ? 'ok' : parent::A]"] {
        for call in ["Probe::value()", "(new Probe())->instance()", "new Maker()", "free_default()"] {
            assert_default_prefix_catch_with_default(call, default, true);
        }
    }
}
