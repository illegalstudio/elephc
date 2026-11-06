//! Purpose:
//! Registry tests for the PHP arity eval builtins enforce before dispatch: the
//! `ArgumentCountError` wording and the table of builtins whose eval signature
//! understates PHP's parameter list.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Expected messages are PHP 8.5.10 output for the same calls.

use super::*;

/// Returns PHP's refusal for `name` called with `supplied` arguments, via the live registry.
fn refusal(name: &str, supplied: usize) -> Option<String> {
    let (canonical, arity) = eval_builtin_call_arity(name).expect("an arity-checked builtin");
    arity.refusal(canonical, supplied)
}

/// The ordinary range picks `exactly`, `at least` or `at most`, and pluralises the bound.
#[test]
fn ranged_refusals_use_php_wording() {
    assert_eq!(refusal("strlen", 0).as_deref(), Some("strlen() expects exactly 1 argument, 0 given"));
    assert_eq!(refusal("strlen", 2).as_deref(), Some("strlen() expects exactly 1 argument, 2 given"));
    assert_eq!(refusal("strlen", 1), None);
    assert_eq!(refusal("str_pad", 1).as_deref(), Some("str_pad() expects at least 2 arguments, 1 given"));
    assert_eq!(refusal("str_pad", 5).as_deref(), Some("str_pad() expects at most 4 arguments, 5 given"));
    assert_eq!(refusal("max", 0).as_deref(), Some("max() expects at least 1 argument, 0 given"));
    assert_eq!(refusal("max", 40), None, "a variadic builtin has no upper bound");
    assert_eq!(refusal("tmpfile", 1).as_deref(), Some("tmpfile() expects exactly 0 arguments, 1 given"));
    assert_eq!(refusal("STRLEN", 0).as_deref(), Some("strlen() expects exactly 1 argument, 0 given"));
}

/// PHP's own irregular parsers: `strtr` and `pcntl_setqos_class` say `exactly` for either
/// bound, and `rand` takes zero arguments or exactly two.
#[test]
fn irregular_builtins_keep_their_php_wording() {
    assert_eq!(refusal("strtr", 1).as_deref(), Some("strtr() expects exactly 2 arguments, 1 given"));
    assert_eq!(refusal("strtr", 4).as_deref(), Some("strtr() expects exactly 3 arguments, 4 given"));
    assert_eq!(refusal("strtr", 3), None);
    assert_eq!(refusal("mt_rand", 0), None);
    assert_eq!(refusal("mt_rand", 2), None);
    assert_eq!(refusal("mt_rand", 1).as_deref(), Some("mt_rand() expects exactly 2 arguments, 1 given"));
    assert_eq!(refusal("rand", 3).as_deref(), Some("rand() expects exactly 2 arguments, 3 given"));
    let arity = eval_raw_declared_builtin_spec("pcntl_setqos_class").unwrap().php_arity();
    let canonical = "pcntl_setqos_class";
    assert_eq!(arity.refusal(canonical, 0), None);
    assert_eq!(
        arity.refusal(canonical, 2).as_deref(),
        Some("pcntl_setqos_class() expects exactly 1 argument, 2 given")
    );
}

/// The OPcache handlers carry their own table, and language constructs are not arity-checked.
#[test]
fn opcache_names_are_checked_and_constructs_are_not() {
    assert_eq!(
        refusal("opcache_get_status", 2).as_deref(),
        Some("opcache_get_status() expects at most 1 argument, 2 given")
    );
    assert_eq!(
        refusal("opcache_reset", 1).as_deref(),
        Some("opcache_reset() expects exactly 0 arguments, 1 given")
    );
    for construct in ["isset", "empty", "unset", "exit", "die"] {
        assert!(eval_builtin_call_arity(construct).is_none(), "{construct}");
    }
    assert!(eval_builtin_call_arity("no_such_function").is_none());
}

/// Every override names a registered builtin and actually widens its signature: a stale entry
/// (the signature caught up, or the builtin left the registry) fails here instead of rotting.
#[test]
fn every_php_arity_override_is_live_and_needed() {
    for (name, arity) in super::super::arity::PHP_ARITY_OVERRIDES {
        let spec = eval_raw_declared_builtin_spec(name)
            .unwrap_or_else(|| panic!("override {name} is not an eval registry builtin"));
        let from_signature = EvalBuiltinArity::ranged(
            spec.required_param_count(),
            spec.variadic.is_none().then_some(spec.params.len()),
        );
        assert_ne!(*arity, from_signature, "override {name} repeats its eval signature");
        assert_eq!(spec.php_arity(), *arity, "{name}");
    }
}
