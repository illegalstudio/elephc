use crate::support::*;

#[test]
fn test_buffer_int_direct_read_write() {
    let out = compile_and_run(
        "<?php buffer<int> $values = buffer_new<int>(4); for ($i = 0; $i < buffer_len($values); $i = $i + 1) { $values[$i] = $i + 1; } echo $values[0] + $values[1] + $values[2] + $values[3];",
    );
    assert_eq!(out, "10");
}

#[test]
fn test_buffer_float_direct_read_write() {
    let out = compile_and_run(
        "<?php buffer<float> $values = buffer_new<float>(2); $values[0] = 1.25; $values[1] = 2.75; echo (int) ($values[0] + $values[1]);",
    );
    assert_eq!(out, "4");
}

#[test]
fn test_buffer_bool_direct_read_write() {
    let out = compile_and_run(
        "<?php buffer<bool> $flags = buffer_new<bool>(2); $flags[0] = true; $flags[1] = false; echo $flags[0]; echo $flags[1];",
    );
    assert_eq!(out, "1");
}

#[test]
fn test_buffer_ptr_direct_read_write() {
    let out = compile_and_run(
        "<?php buffer<ptr> $ptrs = buffer_new<ptr>(1); $ptrs[0] = ptr_null(); echo ptr_is_null($ptrs[0]);",
    );
    assert_eq!(out, "1");
}

#[test]
fn test_buffer_packed_field_access() {
    let out = compile_and_run(
        "<?php packed class Vec2 { public float $x; public float $y; } buffer<Vec2> $points = buffer_new<Vec2>(2); $points[0]->x = 1.5; $points[0]->y = 2.5; $points[1]->x = 3.0; $points[1]->y = 4.0; echo (int) ($points[0]->x + $points[0]->y + $points[1]->x + $points[1]->y);",
    );
    assert_eq!(out, "11");
}

#[test]
fn test_buffer_len_returns_declared_length() {
    let out = compile_and_run(
        "<?php buffer<int> $values = buffer_new<int>(7); echo buffer_len($values);",
    );
    assert_eq!(out, "7");
}

#[test]
fn test_buffer_scalar_elements_are_zero_initialized() {
    let out = compile_and_run("<?php buffer<int> $values = buffer_new<int>(3); echo $values[0]; echo $values[1]; echo $values[2];");
    assert_eq!(out, "000");
}

#[test]
fn test_buffer_packed_fields_are_zero_initialized() {
    let out = compile_and_run(
        "<?php packed class Vec2 { public float $x; public float $y; } buffer<Vec2> $points = buffer_new<Vec2>(1); echo (int) $points[0]->x; echo (int) $points[0]->y;",
    );
    assert_eq!(out, "00");
}

#[test]
fn test_buffer_bounds_check_traps() {
    let err = compile_and_run_expect_failure(
        "<?php buffer<int> $values = buffer_new<int>(1); echo $values[1];",
    );
    assert!(err.contains("buffer index out of bounds"), "{}", err);
}

#[test]
fn test_buffer_free_releases_memory() {
    let out = compile_and_run(
        r#"<?php
buffer<int> $buf = buffer_new<int>(10);
$buf[0] = 42;
echo $buf[0] . " ";
buffer_free($buf);
echo "ok";
"#,
    );
    assert_eq!(out, "42 ok");
}

#[test]
fn test_buffer_free_in_loop_does_not_exhaust_heap() {
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 100; $i++) {
    buffer<int> $tmp = buffer_new<int>(1000);
    $tmp[0] = $i;
    buffer_free($tmp);
}
echo "survived";
"#,
    );
    assert_eq!(out, "survived");
}

#[test]
fn test_buffer_use_after_free_read_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
buffer<int> $buf = buffer_new<int>(5);
buffer_free($buf);
echo $buf[0];
"#,
    );
    assert!(err.contains("use of buffer after buffer_free()"), "{}", err);
}

#[test]
fn test_buffer_use_after_free_write_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
buffer<int> $buf = buffer_new<int>(5);
buffer_free($buf);
$buf[0] = 1;
"#,
    );
    assert!(err.contains("use of buffer after buffer_free()"), "{}", err);
}

#[test]
fn test_buffer_len_after_free_is_fatal() {
    let err = compile_and_run_expect_failure(
        r#"<?php
buffer<int> $buf = buffer_new<int>(5);
buffer_free($buf);
echo buffer_len($buf);
"#,
    );
    assert!(err.contains("use of buffer after buffer_free()"), "{}", err);
}
