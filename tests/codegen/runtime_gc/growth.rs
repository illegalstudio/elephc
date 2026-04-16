use crate::support::*;

#[test]
fn test_example_cow_compiles_and_runs() {
    let out = compile_and_run(include_str!("../../../examples/cow/main.php"));
    assert_eq!(
        out,
        "left: 1 2 3 \nright: 99 2 3 4 \nouterA inner: 10 20 \nouterB inner: 10 77 \n"
    );
}

#[test]
fn test_array_literal_spread_grows_past_initial_capacity() {
    let out = compile_and_run(
        r#"<?php
$nums = [...range(1, 10), ...range(11, 20), ...range(21, 30)];
echo count($nums) . "|" . $nums[25];
"#,
    );
    assert_eq!(out, "30|26");
}

#[test]
fn test_array_literal_spread_refcounted_grows_past_initial_capacity() {
    let out = compile_and_run(
        r#"<?php
$inner = [1];
$a = array_fill(0, 10, $inner);
$b = array_fill(0, 10, $inner);
$c = [...$a, ...$b, ...$a];
echo count($c) . "|" . count($c[25]);
"#,
    );
    assert_eq!(out, "30|1");
}
