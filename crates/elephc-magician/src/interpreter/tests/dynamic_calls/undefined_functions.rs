//! Purpose:
//! Checks catchable undefined-function errors in direct eval calls.
//!
//! Called from:
//! - The Magician dynamic-call interpreter unit tests.
//!
//! Key details:
//! - Lookup failures precede argument evaluation and preserve namespace diagnostics.
//! - Missing capabilities keep their existing unsupported status.

use super::super::super::*;
use super::super::support::*;

/// Undefined direct calls throw Error before evaluating any positional or named operands.
#[test]
fn undefined_functions_are_catchable_before_argument_effects() {
    for arguments in ["bump()", "value: bump()", "...[bump()]"] {
        let source = format!(r#"
$n = 0;
function bump() {{ global $n; $n++; return $n; }}
try {{ isseet({arguments}); }} catch (Error $error) {{ echo $error->getMessage(); }}
return $n;
"#);
        let program = parse_fragment(source.as_bytes()).unwrap();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let returned = execute_program(&program, &mut scope, &mut values).unwrap();
        assert_eq!(values.output, "Call to undefined function isseet()");
        assert_eq!(values.get(returned), FakeValue::Int(0), "{arguments}");
    }
}

/// Both qualified calls and failed namespace fallback report the unresolved PHP name.
#[test]
fn undefined_functions_preserve_namespace_resolution() {
    for (declarations, call, name) in [
        ("namespace replcheck;", "isseet()", "replcheck\\isseet"),
        ("namespace replcheck;", "\\isseet()", "isseet"),
        ("namespace replcheck;", "other\\isseet()", "replcheck\\other\\isseet"),
        ("namespace replcheck; use function elsewhere\\isseet as missing;", "missing()", "elsewhere\\isseet"),
    ] {
        let source = format!("{declarations} try {{ {call}; }} catch (\\Error $e) {{ return $e->getMessage(); }}");
        let program = parse_fragment(source.as_bytes()).unwrap();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let returned = execute_program(&program, &mut scope, &mut values).unwrap();
        assert_eq!(values.get(returned), FakeValue::String(format!("Call to undefined function {name}()")));
    }
}

/// A catalogued prelude function without its native implementation remains unsupported.
#[test]
fn undefined_functions_do_not_relabel_missing_capabilities() {
    let program = parse_fragment(b"ini_get('precision');").unwrap();
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    assert_eq!(execute_program(&program, &mut scope, &mut values), Err(EvalStatus::UnsupportedConstruct));
}
