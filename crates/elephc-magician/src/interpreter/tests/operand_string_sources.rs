//! Purpose:
//! Checks operand ownership in direct string-length calls and nested eval source parsing.
//!
//! Called from:
//! - The Magician interpreter unit-test harness.
//!
//! Key details:
//! - FakeOps records source-cell owners; native heap-debug fixtures cover actual allocations.
//! - Parse failures must retire source temporaries just like successful nested execution.

use super::super::*;
use super::support::*;

/// Direct strlen consumes temporaries without stealing the owner of a borrowed local string.
#[test]
fn direct_strlen_retires_temporary_and_borrowed_string_leases() {
    for source in [
        "return strlen('value');",
        "return strlen(str_repeat('x', 48));",
        "$value = 'kept'; return strlen($value);",
    ] {
        let mut values = FakeOps::default();
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let program = parse_fragment(source.as_bytes()).unwrap();
        let returned = execute_program_with_context(&mut context, &program, &mut scope, &mut values).unwrap();
        values.release(returned).unwrap();
        for cell in scope.drain_owned_cells() { values.release(cell).unwrap(); }
        for (id, value) in &values.values {
            if matches!(value, FakeValue::String(_)) {
                assert_eq!(values.cell_owners[id], 0, "{source}: {value:?}");
            }
        }
    }
}

/// Nested eval releases copied source cells before either execution or a parse failure.
#[test]
fn nested_eval_retires_owned_and_borrowed_sources_even_when_parsing_fails() {
    for (source, fails) in [
        (r#"return eval('return 17;');"#, false),
        (r#"$source = 'return 17;'; return eval($source);"#, false),
        (r#"$source = '$source = ""; return 17;'; return eval($source);"#, false),
        (r#"return eval('return (');"#, true),
    ] {
        let mut values = FakeOps::default();
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let program = parse_fragment(source.as_bytes()).unwrap();
        let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values);
        assert_eq!(result.is_err(), fails, "{source}");
        if let Ok(returned) = result { values.release(returned).unwrap(); }
        for cell in scope.drain_owned_cells() { values.release(cell).unwrap(); }
        for (id, value) in &values.values {
            if matches!(value, FakeValue::String(_)) {
                assert_eq!(values.cell_owners[id], 0, "{source}: {value:?}");
            }
        }
    }
}
