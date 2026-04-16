use crate::support::*;

#[test]
fn test_math_trig_basic() {
    let out = compile_and_run(
        r#"<?php
echo round(sin(0.0), 4) . "|" . round(cos(0.0), 4) . "|" . round(tan(0.0), 4);
"#,
    );
    assert_eq!(out, "0|1|0");
}

#[test]
fn test_math_trig_pi() {
    let out = compile_and_run(
        r#"<?php
echo round(sin(M_PI_2), 4) . "|" . round(cos(M_PI), 1) . "|" . round(tan(M_PI_4), 4);
"#,
    );
    assert_eq!(out, "1|-1|1");
}

#[test]
fn test_math_inverse_trig() {
    let out = compile_and_run(
        r#"<?php
echo round(asin(1.0), 4) . "|" . round(acos(0.0), 4) . "|" . round(atan(1.0), 4);
"#,
    );
    assert_eq!(out, "1.5708|1.5708|0.7854");
}

#[test]
fn test_math_atan2() {
    let out = compile_and_run(
        r#"<?php
echo round(atan2(1.0, 0.0), 4);
"#,
    );
    assert_eq!(out, "1.5708");
}

#[test]
fn test_math_hyperbolic() {
    let out = compile_and_run(
        r#"<?php
echo round(sinh(0.0), 4) . "|" . round(cosh(0.0), 4) . "|" . round(tanh(0.0), 4);
"#,
    );
    assert_eq!(out, "0|1|0");
}

#[test]
fn test_math_log_exp() {
    let out = compile_and_run(
        r#"<?php
echo round(log(M_E), 4) . "|" . log2(8.0) . "|" . log10(1000.0) . "|" . exp(0.0);
"#,
    );
    assert_eq!(out, "1|3|3|1");
}

#[test]
fn test_math_hypot() {
    let out = compile_and_run(
        r#"<?php
echo hypot(3.0, 4.0);
"#,
    );
    assert_eq!(out, "5");
}

#[test]
fn test_math_deg_rad() {
    let out = compile_and_run(
        r#"<?php
echo round(deg2rad(180.0), 4) . "|" . round(rad2deg(M_PI), 1);
"#,
    );
    assert_eq!(out, "3.1416|180");
}

#[test]
fn test_math_pi_function() {
    let out = compile_and_run(
        r#"<?php
echo round(pi(), 4);
"#,
    );
    assert_eq!(out, "3.1416");
}

#[test]
fn test_math_constants() {
    let out = compile_and_run(
        r#"<?php
echo round(M_E, 4) . "|" . round(M_SQRT2, 4) . "|" . round(M_PI_2, 4) . "|" . round(M_PI_4, 4);
"#,
    );
    assert_eq!(out, "2.7183|1.4142|1.5708|0.7854");
}

#[test]
fn test_math_int_coercion() {
    let out = compile_and_run(
        r#"<?php
echo sin(0) . "|" . cos(0) . "|" . log(1) . "|" . exp(0);
"#,
    );
    assert_eq!(out, "0|1|0|1");
}

#[test]
fn test_math_distance_calculation() {
    let out = compile_and_run(
        r#"<?php
$x1 = 1.0; $y1 = 2.0;
$x2 = 4.0; $y2 = 6.0;
$dist = hypot($x2 - $x1, $y2 - $y1);
echo round($dist, 4);
"#,
    );
    assert_eq!(out, "5");
}

#[test]
fn test_log_natural() {
    let out = compile_and_run(
        r#"<?php
echo round(log(M_E), 4);
"#,
    );
    assert_eq!(out, "1");
}

#[test]
fn test_log_base_10() {
    let out = compile_and_run(
        r#"<?php
echo log(1000, 10);
"#,
    );
    assert_eq!(out, "3");
}

#[test]
fn test_log_base_2() {
    let out = compile_and_run(
        r#"<?php
echo log(256, 2);
"#,
    );
    assert_eq!(out, "8");
}

#[test]
fn test_log_base_custom() {
    let out = compile_and_run(
        r#"<?php
echo round(log(27, 3), 4);
"#,
    );
    assert_eq!(out, "3");
}
