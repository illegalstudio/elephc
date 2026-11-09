//! Purpose:
//! Pins the size of the `__call()` fallback around calls on an unknown receiver.
//!
//! Called from:
//! - `crate::ir_lower::tests` through the Rust test harness.
//!
//! Key details:
//! - Counts lowered instructions, not time: the fallback spells its arguments in both branches,
//!   so without evaluating them once a chain of nested calls grows exponentially.

use crate::ir_lower::expr::magic_call_fallback::MAGIC_CALL_HOISTED_ARGS;

/// Instructions lowered for a function whose body is `$o->wrap(` nested `depth` times.
fn chain_instructions(depth: usize) -> usize {
    let mut call = String::from("$n");
    for _ in 0..depth {
        call = format!("$o->wrap({call} . 'x')");
    }
    let source = format!(
        "<?php\nclass M {{ public function __call($n, $a) {{ return 'm'; }} }}\n\
         class R {{ public function wrap($x) {{ return \"R($x)\"; }} }}\n\
         function chain($o, $n) {{ return {call}; }}\n\
         echo chain($argc > 1 ? new M : new R, 'a');\n"
    );
    let module = super::lower_source(&source);
    module
        .functions
        .iter()
        .filter(|function| function.name.eq_ignore_ascii_case("chain"))
        .map(|function| function.instructions.len())
        .sum()
}

/// Each nesting level adds a constant amount of code: going from four to eight levels may not
/// even double the body, where lowering every argument in both branches multiplied it by 16.
#[test]
fn nested_unknown_receiver_calls_grow_linearly() {
    MAGIC_CALL_HOISTED_ARGS.with(|count| count.set(0));
    let four = chain_instructions(4);
    let eight = chain_instructions(8);
    let hoisted = MAGIC_CALL_HOISTED_ARGS.with(std::cell::Cell::get);
    eprintln!("MAGIC_CALL_CHAIN four={four} eight={eight} hoisted={hoisted}");
    assert!(hoisted > 0, "the fixture must reach the __call fallback");
    assert!(eight < four * 3, "{four} instructions at depth 4 but {eight} at depth 8");
}
