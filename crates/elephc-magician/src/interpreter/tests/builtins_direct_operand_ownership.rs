//! Purpose:
//! Interpreter tests for the operand cells that direct-dispatch core builtins evaluate.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::builtins_direct_operand_ownership`.
//!
//! Key details:
//! - A call whose operands are literals owns every cell it evaluates; once the statement ends the
//!   fake runtime must hold as many live cells as it does after `strlen("abc");`, a builtin that
//!   releases its operand (the program result itself stays live in the harness).

use super::super::*;
use super::support::*;

/// Runs one fragment and counts the fake cells that still hold a reference afterwards.
fn live_cells_after(fragment: &[u8]) -> usize {
    let program = parse_fragment(fragment).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    values.refcounts.values().filter(|count| **count > 0).count()
}

/// Verifies `serialize()`, `unserialize()`, `var_export()` and `error_log()` give back their operands.
///
/// Each statement discards its result, so a surviving reference is an operand the builtin
/// evaluated and never released.
#[test]
fn direct_core_builtins_release_their_operand_cells() {
    let baseline = live_cells_after(br#"strlen("abc");"#);
    assert_eq!(baseline, live_cells_after(br#"return true;"#), "the baseline leaks");
    for fragment in [
        &br#"serialize("abc");"#[..],
        &br#"unserialize("i:7;");"#[..],
        &br#"unserialize("i:7;", ["allowed_classes" => false]);"#[..],
        &br#"var_export("abc", true);"#[..],
        &br#"var_export("abc");"#[..],
        &br#"error_log("message");"#[..],
    ] {
        assert_eq!(
            live_cells_after(fragment),
            baseline,
            "{} kept a cell alive",
            String::from_utf8_lossy(fragment)
        );
    }
}
