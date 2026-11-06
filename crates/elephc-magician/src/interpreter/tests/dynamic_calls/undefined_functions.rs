//! Purpose:
//! Checks catchable undefined-function errors in direct eval calls and callable creation.
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
        ("", "StrLenX()", "StrLenX"),
        ("namespace ReplCheck;", "StrLenX()", "ReplCheck\\StrLenX"),
        ("namespace ReplCheck;", "\\StrLenX()", "StrLenX"),
        ("namespace ReplCheck;", "Other\\StrLenX()", "ReplCheck\\Other\\StrLenX"),
        ("namespace ReplCheck; use function Lib\\StrLenX as Missing;", "mIsSiNg()", "Lib\\StrLenX"),
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

/// Retaining source spelling does not change builtin, user-function, alias, or fallback lookup.
#[test]
fn direct_calls_remain_case_insensitive() {
    for source in [
        "return StRlEn('abc');",
        "namespace ReplCheck; return StRlEn('abc');",
        "function MixedName($v) { return $v; } return MIXEDNAME(3);",
        "namespace ReplCheck; function MixedName($v) { return $v; } return mIxEdNaMe(3);",
        "namespace { function MixedName($v) { return $v; } } namespace ReplCheck { return mIxEdNaMe(3); }",
        "namespace Lib { function MixedName($v) { return $v; } } namespace ReplCheck { use function Lib\\MixedName as Imported; return iMpOrTeD(3); }",
        "namespace ReplCheck; function MixedName($v) { return $v; } return \\REPLCHECK\\MIXEDNAME(3);",
    ] {
        let program = parse_fragment(source.as_bytes()).unwrap();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let returned = execute_program(&program, &mut scope, &mut values).unwrap();
        assert_eq!(values.get(returned), FakeValue::Int(3), "{source}");
    }
}

/// Missing first-class callables report the resolved source name through catchable errors.
#[test]
fn undefined_function_callables_preserve_source_spelling() {
    for (declarations, callable, name) in [
        ("", "StrLenX(...)", "StrLenX"),
        ("namespace ReplCheck;", "StrLenX(...)", "ReplCheck\\StrLenX"),
        ("namespace ReplCheck;", "\\StrLenX(...)", "StrLenX"),
        ("namespace ReplCheck;", "Other\\StrLenX(...)", "ReplCheck\\Other\\StrLenX"),
        ("namespace ReplCheck; use function Lib\\MissingCall as Nope;", "nOpE(...)", "Lib\\MissingCall"),
    ] {
        let source = format!("{declarations} try {{ {callable}; }} catch (\\Error $e) {{ return $e->getMessage(); }}");
        let program = parse_fragment(source.as_bytes()).unwrap();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let returned = execute_program(&program, &mut scope, &mut values).unwrap();
        assert_eq!(values.get(returned), FakeValue::String(format!("Call to undefined function {name}()")), "{source}");
    }
}

/// Source spelling does not alter first-class builtin, eval-function, alias, or fallback lookup.
#[test]
fn function_callables_remain_case_insensitive() {
    for source in [
        "$fn = StRlEn(...); return $fn('abc');",
        "namespace ReplCheck; $fn = StRlEn(...); return $fn('abc');",
        "namespace ReplCheck; $fn = \\StRlEn(...); return $fn('abc');",
        "function MixedName($v) { return $v; } $fn = MIXEDNAME(...); return $fn(3);",
        "namespace ReplCheck; function MixedName($v) { return $v; } $fn = mIxEdNaMe(...); return $fn(3);",
        "namespace { function MixedName($v) { return $v; } } namespace ReplCheck { $fn = mIxEdNaMe(...); return $fn(3); }",
        "namespace Lib { function MixedName($v) { return $v; } } namespace ReplCheck { use function Lib\\MixedName as Imported; $fn = iMpOrTeD(...); return $fn(3); }",
        "namespace ReplCheck; function MixedName($v) { return $v; } $fn = \\REPLCHECK\\MIXEDNAME(...); return $fn(3);",
    ] {
        let program = parse_fragment(source.as_bytes()).unwrap();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let returned = execute_program(&program, &mut scope, &mut values).unwrap();
        assert_eq!(values.get(returned), FakeValue::Int(3), "{source}");
    }
}
