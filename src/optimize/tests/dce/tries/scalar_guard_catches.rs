//! Purpose:
//! Pins that the metadata-aware optimizer keeps a guarded call to a pure callee observable.
//!
//! Called from:
//! - The optimizer DCE unit tests.
//!
//! Key details:
//! - The optimizer is driven the way an embedder would, with no guard sites installed by
//!   hand: `PostTypecheckOptimizer::new_with_type_metadata` takes them from the check result.

use super::*;

/// A `mixed` null bound to a pure callee's `string` parameter throws `TypeError` at run time, so
/// neither the call nor its `catch (TypeError)` may be removed.
#[test]
fn test_metadata_optimizer_keeps_guarded_pure_call_and_its_catch() {
    let source = r#"<?php
function box(mixed $v): mixed { return $v; }
function pure(string $s): void {}
$null = box(null);
try { pure($null); } catch (TypeError $e) { echo 'caught'; }
"#;
    let tokens = crate::lexer::tokenize(source).unwrap();
    let program = crate::parser::parse(&tokens).unwrap();
    let checked = crate::types::check(&program).unwrap();
    assert!(!checked.scalar_guard_arg_sites.is_empty(), "the checker records the guarded argument");
    let keeps_catch = |sites: &std::collections::HashSet<crate::span::Span>| {
        let optimizer = PostTypecheckOptimizer::new_with_type_metadata(
            &program, &checked.functions, &checked.classes, &checked.interfaces, sites,
        );
        let result = optimizer.eliminate_dead_code(program.clone(), checked.local_binding_decision_spans());
        result.iter().any(|stmt| matches!(&stmt.kind,
            StmtKind::Try { catches, .. } if catches.iter().any(|catch|
                catch.exception_types.iter().any(|name| name == "TypeError"))))
    };
    assert!(keeps_catch(&checked.scalar_guard_arg_sites));
    // Without the sites the call is pure and its handler unreachable: this is what a driver that
    // forgot to install them by hand used to get.
    assert!(!keeps_catch(&Default::default()));
}

/// `new self($v)`, `new parent($v)` and `new static($v)` bind a `mixed` null to the selected
/// constructor's `string` parameter through the same runtime check, so the optimizer keeps their
/// `catch (TypeError)` even when the constructor body cannot throw (`new self` lost it: the
/// guarded-argument lookup ignored scoped construction).
#[test]
fn test_metadata_optimizer_keeps_the_catch_of_a_guarded_scoped_construction() {
    for (label, body) in [
        ("self", "try { new self($v); } catch (TypeError $e) { echo 'caught'; }"),
        ("parent", "try { new parent($v); } catch (TypeError $e) { echo 'caught'; }"),
        ("static", "try { new static($v); } catch (TypeError $e) { echo 'caught'; }"),
    ] {
        let source = format!(
            "<?php\nclass P {{ public function __construct(string $s) {{}} }}\n\
             class A extends P {{ public function __construct(string $s) {{}}\n\
             public static function m(mixed $v): void {{ {body} }} }}\nA::m(null);\n"
        );
        let tokens = crate::lexer::tokenize(&source).unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        let checked = crate::types::check(&program).unwrap();
        assert!(!checked.scalar_guard_arg_sites.is_empty(), "{label}: the checker records the guarded argument");
        let optimizer = PostTypecheckOptimizer::new_with_type_metadata(
            &program, &checked.functions, &checked.classes, &checked.interfaces, &checked.scalar_guard_arg_sites,
        );
        let result = optimizer.eliminate_dead_code(program.clone(), checked.local_binding_decision_spans());
        assert!(format!("{result:?}").contains("\"TypeError\""), "{label}: the catch (TypeError) was removed");
    }
}
