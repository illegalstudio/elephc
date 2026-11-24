//! Purpose:
//! Build-size guard for programs that call a callable the compiler cannot see.
//!
//! Called from:
//! - `cargo test --lib ir_lower::tests::dynamic_call_footprint`.
//!
//! Key details:
//! - A bare `$f()`, `call_user_func` or `array_map` program needs the invalid-callback helper,
//!   and nothing else. The helper once asked classes about their methods by name; in a closed
//!   world that keeps every class alive and grew such a program from 198 to 903 functions (the
//!   Reflection classes). No helper or prelude may pull Reflection or the session surface into a
//!   program that does not use them.

/// Bound on the functions of a program that only calls an unknown callable. A bare program
/// lowers to about 200; the bound leaves room for helper growth, not for a class library.
const FUNCTION_BOUND: usize = 260;

/// Dynamic calls of every route stay free of the Reflection and session class libraries.
#[test]
fn dynamic_call_programs_do_not_pull_reflection_or_session_classes() {
    for source in [
        "<?php function f(int $x): int { return $x; } $f = $argc > 1 ? 'f' : 'g'; echo $f(1);",
        "<?php $f = $argc > 1 ? 'strlen' : 'nope'; echo call_user_func($f, 'a');",
        "<?php $f = $argc > 1 ? 'strlen' : 'nope'; print_r(array_map($f, ['a', 'bb']));",
        "<?php $f = $argc > 1 ? 'strlen' : 'nope'; $a = [3, 1]; usort($a, $f); echo $a[0];",
    ] {
        let module = super::lower_source(source);
        assert!(module.functions.len() <= FUNCTION_BOUND,
            "{} functions (bound {FUNCTION_BOUND}): {source}", module.functions.len());
        let heavy: Vec<_> = module.functions.iter()
            .map(|function| function.name.as_str())
            .filter(|name| name.contains("Reflection") || name.contains("Session"))
            .collect();
        assert!(heavy.is_empty(), "{source}: pulled {heavy:?}");
    }
}
