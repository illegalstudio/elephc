//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of types examples, including example functions compiles and runs.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Uses checked-in example PHP fixtures through include_str! in addition to inline native-output assertions.

use super::*;

/// Compiles and runs the checked-in `examples/functions/main.php` fixture and asserts stdout contains
/// the expected function outputs including `my_abs`, `my_max`, `clamp`, `gcd`, power, `describe`,
/// `add_ten`, `profile` (named args), `str_repeat` (named args), etc.
#[test]
fn test_example_functions_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/functions/main.php"));
    assert_eq!(
        out,
        "my_abs(-42) = 42\nmy_max(3, 7) = 7\nclamp(15, 0, 10) = 10\ngcd(48, 18) = 6\n2^10 = 1024\ndescribe(42) = integer:42\ndescribe(null) = NULL:null\nadd_ten() = 20\nprofile(age: 30, name: \"Ada\") = Ada:30\nprofile(..., name: \"Lin\") = Lin:31\nstr_repeat(times: 3, string: \"ha\") = hahaha\nstr_repeat(..., times: 3) = hahaha\n",
    );
}
