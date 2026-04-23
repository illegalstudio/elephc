use crate::support::*;

#[test]
fn test_constant_folding_nested_integer_arithmetic_runtime() {
    let out = compile_and_run("<?php echo (2 + 3) * 4;");
    assert_eq!(out, "20");
}

#[test]
fn test_constant_folding_pow_removes_pow_call_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_constant_folding_pow");
    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options("<?php echo 2 ** 3;", &dir, 8_388_608, false, false);

    assert!(
        !user_asm.contains("pow"),
        "constant-folded pow expression should not leave a pow call in user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "8");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_propagation_removes_pow_call_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_constant_propagation_pow");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$x = 2;
$y = 3;
echo $x ** $y;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "constant-propagated pow expression should not leave a pow call in user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "8");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_propagation_merges_identical_if_constants() {
    let dir = make_cli_test_dir("elephc_constant_propagation_if_merge");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
if ($argc > 0) {
    $base = 2;
} else {
    $base = 2;
}

echo $base ** 3;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "merged if constants should let pow disappear from user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "8");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_propagation_merges_identical_switch_constants() {
    let dir = make_cli_test_dir("elephc_constant_propagation_switch_merge");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
switch ($argc) {
    case 1:
        $base = 2;
        break;
    default:
        $base = 2;
}

echo $base ** 3;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "merged switch constants should let pow disappear from user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "8");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_propagation_uses_known_switch_subject_merge() {
    let dir = make_cli_test_dir("elephc_constant_propagation_switch_known_subject");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$mode = 1;
switch ($mode) {
    case 1:
        $base = 2;
        break;
    default:
        $base = 9;
}

echo $base ** 3;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "known switch subject should limit constant propagation merge paths:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "8");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_propagation_merges_identical_try_catch_constants() {
    let dir = make_cli_test_dir("elephc_constant_propagation_try_merge");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
try {
    $base = 2;
} catch (Exception $e) {
    $base = 2;
}

echo $base ** 3;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "merged try/catch constants should let pow disappear from user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "8");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_propagation_ignores_unreachable_catch_constants() {
    let dir = make_cli_test_dir("elephc_constant_propagation_non_throwing_try");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
try {
    $base = 2;
} catch (Exception $e) {
    $base = 9;
}

echo $base ** 3;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "non-throwing try body should keep unreachable catch constants out of the merge:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "8");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_propagation_tracks_uniform_ternary_assignment() {
    let dir = make_cli_test_dir("elephc_constant_propagation_ternary_uniform");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$base = ($argc > 0) ? 2 : 2;
echo $base ** 3;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "uniform ternary assignment should let pow disappear from user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "8");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_propagation_tracks_uniform_match_assignment() {
    let dir = make_cli_test_dir("elephc_constant_propagation_match_uniform");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$base = match ($argc) {
    1 => 2,
    default => 2,
};
echo $base ** 3;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "uniform match assignment should let pow disappear from user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "8");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_propagation_tracks_known_match_assignment() {
    let dir = make_cli_test_dir("elephc_constant_propagation_match_known");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$mode = 1;
$base = match ($mode) {
    1 => 2,
    default => 9,
};
echo $base ** 3;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "known match subject should fold before assignment propagation:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "8");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_propagation_tracks_scalar_list_unpack() {
    let dir = make_cli_test_dir("elephc_constant_propagation_list_unpack");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
[$base, $exp] = [2, 3];
echo $base ** $exp;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "scalar list unpack should let pow disappear from user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "8");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_propagation_preserves_scalar_across_simple_for_loop() {
    let dir = make_cli_test_dir("elephc_constant_propagation_for_loop");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$base = 2;
for ($i = 0; $i < 3; $i++) {
    echo $i;
}
echo $base ** 3;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "simple loop should preserve unrelated scalar constants in user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "0128");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_propagation_preserves_scalar_across_loop_with_switch() {
    let dir = make_cli_test_dir("elephc_constant_propagation_loop_switch");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$base = 2;
for ($i = 0; $i < 3; $i++) {
    switch ($i) {
        case 1:
            echo $i;
            break;
        default:
            echo $i;
    }
}
echo $base ** 3;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "loop-local switch should not kill unrelated scalar constants in user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "0128");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_propagation_preserves_scalar_across_loop_with_try() {
    let dir = make_cli_test_dir("elephc_constant_propagation_loop_try");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$base = 2;
for ($i = 0; $i < 3; $i++) {
    try {
        echo $i;
    } catch (Exception $e) {
        echo 9;
    } finally {
    }
}
echo $base ** 3;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "loop-local try/catch/finally should not kill unrelated scalar constants in user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "0128");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_propagation_preserves_scalar_across_foreach_loop() {
    let dir = make_cli_test_dir("elephc_constant_propagation_foreach");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$base = 2;
foreach ([1, 2, 3] as $k => $value) {
    echo $value;
}
echo $base ** 3;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "simple foreach should preserve unrelated scalar constants in user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "1238");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_propagation_tracks_stable_for_init_assignments() {
    let dir = make_cli_test_dir("elephc_constant_propagation_for_init");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$base = 2;
$i = 0;
for ($exp = 3; $i < 2; $i++) {
    echo $base ** $exp;
}
echo $exp;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "stable for-init assignments should let pow disappear from user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "883");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_propagation_preserves_scalar_across_nested_loops() {
    let dir = make_cli_test_dir("elephc_constant_propagation_nested_loops");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$base = 2;
$i = 0;
for (; $i < 2; $i++) {
    $j = 0;
    while ($j < 2) {
        echo $j;
        $j++;
    }
}
echo $base ** 3;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "nested simple loops should preserve unrelated scalar constants in user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "01018");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_propagation_preserves_scalar_across_loop_local_array_writes() {
    let dir = make_cli_test_dir("elephc_constant_propagation_loop_array_writes");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$base = 2;
$items = [];
$i = 0;
for (; $i < 3; $i++) {
    $items[] = $i;
    $items[0] = $i;
    echo $i;
}
echo $base ** 3;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "local array writes inside the loop should not poison unrelated scalar constants:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "0128");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_propagation_preserves_scalar_across_loop_property_writes() {
    let dir = make_cli_test_dir("elephc_constant_propagation_loop_property_writes");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
class Box {
    public $last = 0;
    public $items = [];
}

$box = new Box();
$base = 2;
$i = 0;
for (; $i < 3; $i++) {
    $box->last = $i;
    $box->items[] = $i;
    $box->items[0] = $i;
    echo $i;
}
echo $base ** 3;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "local property writes inside the loop should not poison unrelated scalar constants:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "0128");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_propagation_preserves_scalar_across_unset_and_loop() {
    let dir = make_cli_test_dir("elephc_constant_propagation_unset");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$base = 2;
$i = 0;
for (; $i < 3; $i++) {
    $tmp = 9;
    unset($tmp);
    echo $i;
}
echo $base ** 3;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "unsetting an unrelated local inside the loop should not poison scalar constants:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "0128");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_folding_string_concat_removes_runtime_concat_call() {
    let dir = make_cli_test_dir("elephc_constant_folding_concat");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php echo "hello " . "world";"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("__rt_concat"),
        "constant-folded concat expression should not call __rt_concat in user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "hello world");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_folding_null_coalesce_removes_runtime_concat_call() {
    let dir = make_cli_test_dir("elephc_constant_folding_null_coalesce");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php echo null ?? ("hello " . "world");"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("__rt_concat"),
        "constant-folded null coalesce should not leave __rt_concat in user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "hello world");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_folding_ternary_removes_pow_call_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_constant_folding_ternary");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php echo (2 < 3) ? (2 ** 3) : (3 ** 4);"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "constant-folded ternary should not leave a pow call in user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "8");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_folding_truthiness_and_spaceship_runtime() {
    let out = compile_and_run(
        r#"<?php
echo !("0");
echo (2 <=> 3) + 2;
"#,
    );
    assert_eq!(out, "11");
}

#[test]
fn test_constant_folding_int_cast_removes_runtime_atoi_call() {
    let dir = make_cli_test_dir("elephc_constant_folding_cast_int");
    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options("<?php echo (int)\"42\";", &dir, 8_388_608, false, false);

    assert!(
        !user_asm.contains("__rt_atoi"),
        "constant-folded int cast should not leave __rt_atoi in user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "42");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_folding_string_cast_removes_runtime_itoa_call() {
    let dir = make_cli_test_dir("elephc_constant_folding_cast_string");
    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options("<?php echo (string)42;", &dir, 8_388_608, false, false);

    assert!(
        !user_asm.contains("__rt_itoa"),
        "constant-folded string cast should not leave __rt_itoa in user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "42");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_folding_prunes_constant_if_branch_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_constant_folding_if_prune");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$n = 8;
if (false) {
    echo 2 ** $n;
} else {
    echo 3;
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "constant false if-branch should be pruned from user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "3");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_folding_prunes_while_false_body_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_constant_folding_while_prune");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$n = 8;
while (false) {
    echo 2 ** $n;
}
echo 3;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "while(false) body should be pruned from user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "3");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_folding_prunes_for_false_body_and_update_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_constant_folding_for_prune");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$n = 8;
for ($i = 1; false; $i = 2 ** $n) {
    echo 2 ** $n;
}
echo $i;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "for(false) body and update should be pruned from user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "1");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_folding_prunes_match_to_selected_arm_in_user_assembly() {
    let dir = make_cli_test_dir("elephc_constant_folding_match_prune");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$n = 8;
echo match (3) {
    1 => 2 ** $n,
    3 => 7,
    default => 9,
};
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "constant match should not leave dead arms in user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "7");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_folding_prunes_switch_leading_cases_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_constant_folding_switch_prune");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$n = 8;
switch (3) {
    case 1:
        echo 2 ** $n;
        break;
    case 3:
        echo 7;
        break;
    default:
        echo 9;
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "constant switch should not leave dead leading cases in user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "7");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_folding_prunes_dead_statements_after_return_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_constant_folding_return_dce");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function answer() {
    return 7;
    echo 2 ** 8;
}
echo answer();
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "dead statements after return should not remain in user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "7");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_folding_prunes_pure_expr_statements_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_constant_folding_pure_expr_stmt_dce");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
strlen(...);
echo 7;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "7");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_dead_code_elimination_prunes_pure_builtin_expr_statement() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_pure_builtin_expr_stmt");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
strlen("abc");
echo 7;
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("strlen()"),
        "pure builtin expr statements should disappear from user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "7");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_folding_prunes_dead_statements_after_break_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_constant_folding_break_dce");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
switch (1) {
    case 1:
        echo 7;
        break;
        echo 2 ** 8;
    default:
        echo 9;
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "dead statements after break should not remain in user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "7");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_folding_prunes_dead_statements_after_exhaustive_if_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_constant_folding_exhaustive_if_dce");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function answer($flag) {
    if ($flag) {
        return 7;
    } else {
        return 8;
    }
    echo 2 ** 8;
}
echo answer(true);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "dead statements after exhaustive if should not remain in user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "7");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_folding_prunes_dead_statements_after_exhaustive_switch_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_constant_folding_exhaustive_switch_dce");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function answer($flag) {
    switch ($flag) {
        case 1:
            return 7;
        default:
            return 8;
    }
    echo 2 ** 8;
}
echo answer(1);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "dead statements after exhaustive switch should not remain in user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "7");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_constant_folding_prunes_unused_pure_ternary_branch_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_constant_folding_ternary_dead_branch");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function answer() {
    return 7;
}
echo true ? answer() : (2 ** 8);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "unused pure ternary branch should not remain in user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "7");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_dead_code_elimination_prunes_after_exhaustive_try_catch() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_try_catch");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function answer() {
    try {
        return 7;
    } catch (Exception $e) {
        return 8;
    }
    echo 2 ** 8;
}
echo answer();
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "dead statements after exhaustive try/catch should not remain in user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "7");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_dead_code_elimination_prunes_after_try_finally_exit() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_try_finally");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function answer() {
    try {
        return 7;
    } finally {
        echo 1;
    }
    echo 2 ** 8;
}
echo answer();
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "dead statements after try/finally exit should not remain in user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "17");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_dead_code_elimination_preserves_effectful_empty_if_condition() {
    let out = compile_and_run(
        r#"<?php
function touch() {
    echo "t";
    return true;
}
if (touch()) {
}
echo "!";
"#,
    );

    assert_eq!(out, "t!");
}

#[test]
fn test_dead_code_elimination_collapses_empty_switch_shell_after_branch_dce() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_empty_switch_shell");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function touch() {
    echo "s";
    return 1;
}

switch (touch()) {
    case 1:
        strlen("abc");
        break;
}

echo "!";
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("switch_end"),
        "empty switch shells should not survive user assembly after DCE:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "s!");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_dead_code_elimination_collapses_empty_try_shell_after_branch_dce() {
    let out = compile_and_run(
        r#"<?php
try {
    strlen("abc");
} catch (Exception $e) {
    strlen("def");
} finally {
    echo "f";
}
echo "!";
"#,
    );

    assert_eq!(out, "f!");
}

#[test]
fn test_dead_code_elimination_drops_unreachable_catch_after_non_throwing_try() {
    let out = compile_and_run(
        r#"<?php
try {
    echo "t";
} catch (Exception $e) {
    echo "c";
}
echo "!";
"#,
    );

    assert_eq!(out, "t!");
}

#[test]
fn test_dead_code_elimination_drops_unreachable_catch_before_finally() {
    let out = compile_and_run(
        r#"<?php
try {
    echo "t";
} catch (Exception $e) {
    echo "c";
} finally {
    echo "f";
}
echo "!";
"#,
    );

    assert_eq!(out, "tf!");
}

#[test]
fn test_dead_code_elimination_reduces_empty_if_chain_to_needed_condition_checks() {
    let out = compile_and_run(
        r#"<?php
function touch() {
    echo "a";
    return false;
}

function tap() {
    echo "b";
    return false;
}

if (touch()) {
    strlen("abc");
} elseif (tap()) {
    strlen("def");
}

echo "!";
"#,
    );

    assert_eq!(out, "ab!");
}

#[test]
fn test_dead_code_elimination_rebuilds_empty_elseif_tail_as_needed_guard() {
    let out = compile_and_run(
        r#"<?php
function touch() {
    echo "a";
    return false;
}

function tap() {
    echo "b";
    return true;
}

if (touch()) {
    echo "x";
} elseif (tap()) {
    strlen("abc");
} else {
    echo "z";
}

echo "!";
"#,
    );

    assert_eq!(out, "ab!");
}

#[test]
fn test_dead_code_elimination_sinks_tail_into_if_fallthrough_branch() {
    let out = compile_and_run(
        r#"<?php
function run(bool $flag) {
    if ($flag) {
        echo "a";
        return;
    } else {
        echo "b";
    }
    echo "c";
}

run(true);
run(false);
echo "!";
"#,
    );

    assert_eq!(out, "abc!");
}

#[test]
fn test_dead_code_elimination_sinks_tail_into_switch_exit_paths() {
    let out = compile_and_run(
        r#"<?php
function run(int $flag) {
    switch ($flag) {
        case 1:
            echo "a";
        case 2:
            echo "b";
        default:
            echo "c";
    }
    echo "!";
}

run(1);
run(2);
run(3);
"#,
    );

    assert_eq!(out, "abc!bc!c!");
}

#[test]
fn test_dead_code_elimination_sinks_tail_into_switch_break_paths() {
    let out = compile_and_run(
        r#"<?php
function run(int $flag) {
    switch ($flag) {
        case 1:
            echo "a";
            break;
        case 2:
            echo "b";
        default:
            echo "c";
    }
    echo "!";
}

run(1);
run(2);
run(3);
"#,
    );

    assert_eq!(out, "a!bc!c!");
}

#[test]
fn test_dead_code_elimination_sinks_tail_into_try_fallthrough_paths() {
    let out = compile_and_run(
        r#"<?php
function run(bool $flag) {
    try {
        if ($flag) {
            throw new Exception("boom");
        }
        echo "a";
    } catch (Exception $e) {
        return;
    }
    echo "b";
}

run(false);
run(true);
echo "!";
"#,
    );

    assert_eq!(out, "ab!");
}

#[test]
fn test_dead_code_elimination_sinks_tail_into_try_catch_only_fallthrough_paths() {
    let out = compile_and_run(
        r#"<?php
function run(bool $flag) {
    try {
        if ($flag) {
            throw new Exception("boom");
        }
        return;
    } catch (Exception $e) {
        echo "a";
    }
    echo "b";
}

run(true);
run(false);
echo "!";
"#,
    );

    assert_eq!(out, "ab!");
}

#[test]
fn test_dead_code_elimination_drops_shadowed_throwable_catch_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_shadowed_throwable_catch");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
try {
    throw new Exception("boom");
} catch (Throwable $t) {
    echo "a";
} catch (Exception $e) {
    echo "shadowed";
}
echo "!";
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("shadowed"),
        "shadowed catch body should not remain in user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "a!");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_dead_code_elimination_drops_shadowed_switch_case_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_shadowed_switch_case");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
switch (1) {
    case 1:
        echo "a";
        break;
    case 1:
        echo "shadowed";
        break;
    default:
        echo "z";
}
echo "!";
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("shadowed"),
        "shadowed switch case body should not remain in user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "a!");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_dead_code_elimination_drops_shadowed_match_arm_from_user_assembly() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_shadowed_match_arm");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function id($value) {
    return $value;
}

echo match (id(1)) {
    1 => "a",
    1 => "shadowed",
    default => "z",
};
echo "!";
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("shadowed"),
        "shadowed match arm should not remain in user assembly:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "a!");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_outer_guard() {
    let out = compile_and_run(
        r#"<?php
function run($flag) {
    if ($flag) {
        if (!$flag) {
            echo "bad";
        } else {
            echo "a";
        }
    } else {
        echo "b";
    }
}

run(true);
run(false);
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_invalidates_outer_guard_after_local_write() {
    let out = compile_and_run(
        r#"<?php
function run($flag) {
    if ($flag) {
        $flag = false;
        if ($flag) {
            echo "bad";
        } else {
            echo "a";
        }
    }
}

run(true);
"#,
    );

    assert_eq!(out, "a");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_outer_strict_bool_guard() {
    let out = compile_and_run(
        r#"<?php
function run($flag) {
    if ($flag === true) {
        if ($flag === false) {
            echo "bad";
        } else {
            echo "a";
        }
    } else {
        echo "b";
    }
}

run(true);
run(false);
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_invalidates_outer_strict_bool_guard_after_local_write() {
    let out = compile_and_run(
        r#"<?php
function run($flag) {
    if ($flag === true) {
        $flag = false;
        if ($flag === true) {
            echo "bad";
        } else {
            echo "a";
        }
    }
}

run(true);
"#,
    );

    assert_eq!(out, "a");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_outer_and_guard() {
    let out = compile_and_run(
        r#"<?php
function run($a, $b) {
    if ($a && $b) {
        if (!$a || !$b) {
            echo "bad";
        } else {
            echo "a";
        }
    } else {
        echo "b";
    }
}

run(true, true);
run(true, false);
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_outer_negated_and_guard() {
    let out = compile_and_run(
        r#"<?php
function run($a, $b) {
    if (!($a && $b)) {
        if ($a && $b) {
            echo "bad";
        } else {
            echo "a";
        }
    } else {
        echo "b";
    }
}

run(true, false);
run(true, true);
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_demorgan_equivalent_guard() {
    let out = compile_and_run(
        r#"<?php
function run($a, $b) {
    if (!($a && $b)) {
        if (!$a || !$b) {
            echo "a";
        } else {
            echo "bad";
        }
    } else {
        echo "b";
    }
}

run(true, false);
run(true, true);
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_loose_comparison_guard() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_loose_comparison_guard");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($value) {
    if ($value == 0) {
        if ($value != 0) {
            echo "dead-loose";
        } else {
            echo "a";
        }
    } else {
        echo "b";
    }
}

run(0);
run(2);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "ab");
    assert!(!user_asm.contains("dead-loose"));
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_relational_guard() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_relational_guard");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($value) {
    if ($value > 10) {
        if ($value <= 10) {
            echo "dead-rel";
        } else {
            echo "a";
        }
    } else {
        echo "b";
    }
}

run(11);
run(10);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "ab");
    assert!(!user_asm.contains("dead-rel"));
}

#[test]
fn test_dead_code_elimination_prunes_nested_elseif_from_composite_guard_refinement() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_composite_guard_refinement");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($a, $b, $c) {
    if (($a && $b) || $c) {
        if (!$c) {
            if ($a && $b) {
                echo "a";
            } elseif (true) {
                echo "dead";
            }
        } else {
            echo "c";
        }
    } else {
        echo "x";
    }
}

run(true, true, false);
run(false, false, true);
run(false, false, false);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "acx");
    assert!(!user_asm.contains("dead"));
}

#[test]
fn test_dead_code_elimination_prunes_nested_subexpr_from_composite_guard_refinement() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_nested_subexpr_guard_refinement");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($a, $b, $c, $d) {
    if ((($a && $b) || $c) && $d) {
        if ($d) {
            if (!$c) {
                if ($a && $b) {
                    echo "a";
                } elseif (true) {
                    echo "dead-ab";
                }
            } else {
                echo "c";
            }
        } else {
            echo "dead-d";
        }
    } else {
        echo "x";
    }
}

run(true, true, false, true);
run(false, false, true, true);
run(false, false, false, false);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "acx");
    assert!(!user_asm.contains("dead-ab"));
    assert!(!user_asm.contains("dead-d"));
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_outer_or_false_branch() {
    let out = compile_and_run(
        r#"<?php
function run($a, $b) {
    if (!$a || $b) {
        echo "b";
    } else {
        if ($a && !$b) {
            echo "a";
        } else {
            echo "bad";
        }
    }
}

run(true, false);
run(false, false);
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_outer_null_guard() {
    let out = compile_and_run(
        r#"<?php
function runNull() {
    $value = null;
    if ($value === null) {
        if ($value !== null) {
            echo "bad";
        } else {
            echo "a";
        }
    } else {
        echo "b";
    }
}

function runInt() {
    $value = 1;
    if ($value === null) {
        echo "bad";
    } else {
        echo "b";
    }
}

runNull();
runInt();
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_outer_zero_guard() {
    let out = compile_and_run(
        r#"<?php
function run($value) {
    if ($value === 0) {
        if ($value) {
            echo "bad";
        } else {
            echo "a";
        }
    } else {
        echo "b";
    }
}

run(0);
run(1);
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_excluded_zero_guard() {
    let out = compile_and_run(
        r#"<?php
function run($value) {
    if ($value === 0) {
        echo "b";
    } else {
        if ($value === 0) {
            echo "bad";
        } else {
            echo "a";
        }
    }
}

run(1);
run(0);
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_excluded_null_guard() {
    let out = compile_and_run(
        r#"<?php
function runNotNull() {
    $value = 1;
    if ($value !== null) {
        if ($value === null) {
            echo "bad";
        } else {
            echo "a";
        }
    } else {
        echo "b";
    }
}

function runNull() {
    $value = null;
    if ($value !== null) {
        echo "bad";
    } else {
        echo "b";
    }
}

runNotNull();
runNull();
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_outer_empty_string_guard() {
    let out = compile_and_run(
        r#"<?php
function run($value) {
    if ($value === "") {
        if ($value) {
            echo "bad";
        } else {
            echo "a";
        }
    } else {
        echo "b";
    }
}

run("");
run("x");
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_excluded_empty_string_guard() {
    let out = compile_and_run(
        r#"<?php
function run($value) {
    if ($value === "") {
        echo "b";
    } else {
        if ($value === "") {
            echo "bad";
        } else {
            echo "a";
        }
    }
}

run("x");
run("");
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_outer_string_zero_guard() {
    let out = compile_and_run(
        r#"<?php
function run($value) {
    if ($value === "0") {
        if ($value) {
            echo "bad";
        } else {
            echo "a";
        }
    } else {
        echo "b";
    }
}

run("0");
run("1");
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_excluded_string_zero_guard() {
    let out = compile_and_run(
        r#"<?php
function run($value) {
    if ($value === "0") {
        echo "b";
    } else {
        if ($value === "0") {
            echo "bad";
        } else {
            echo "a";
        }
    }
}

run("1");
run("0");
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_outer_zero_float_guard() {
    let out = compile_and_run(
        r#"<?php
function run($value) {
    if ($value === 0.0) {
        if ($value) {
            echo "bad";
        } else {
            echo "a";
        }
    } else {
        echo "b";
    }
}

run(0.0);
run(1.5);
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_excluded_float_guard() {
    let out = compile_and_run(
        r#"<?php
function run($value) {
    if ($value === 1.5) {
        echo "b";
    } else {
        if ($value === 1.5) {
            echo "bad";
        } else {
            echo "a";
        }
    }
}

run(2.5);
run(1.5);
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_prunes_nested_if_region_from_switch_bool_guard_case() {
    let out = compile_and_run(
        r#"<?php
function run($flag) {
    switch (true) {
        case $flag === true:
            if ($flag === false) {
                echo "bad";
            } else {
                echo "a";
            }
            break;
        default:
            echo "b";
    }
}

run(true);
run(false);
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_drops_impossible_switch_cases_from_outer_guards() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_guard_prune");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($value, $flag) {
    if ($value === 0) {
        switch ($value) {
            case 1:
                echo "dead-int";
                break;
            case 0:
                echo "a";
                break;
        }
    }

    if ($flag === true) {
        switch (true) {
            case $flag === false:
                echo "dead-bool";
                break;
            case $flag === true:
                echo "b";
                break;
        }
    }
}

run(0, true);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "ab");
    assert!(!user_asm.contains("dead-int"));
    assert!(!user_asm.contains("dead-bool"));
}

#[test]
fn test_dead_code_elimination_drops_unreachable_elseif_suffix_from_cumulative_guards() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_elseif_guard_prune");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function tap($label, $ret) {
    echo $label;
    return $ret;
}

$flag = $argc > 1;
if ($flag) {
    echo "A";
} elseif (!$flag) {
    echo "B";
} elseif (tap("dead-elseif", true)) {
    echo "C";
} else {
    echo "dead-else";
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "B");
    assert!(!user_asm.contains("dead-elseif"));
    assert!(!user_asm.contains("dead-else"));
}

#[test]
fn test_dead_code_elimination_drops_unreachable_elseif_suffix_from_negated_composite_guards() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_negated_elseif_guard_prune");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function tap($label, $ret) {
    echo $label;
    return $ret;
}

function run($a, $b) {
    if ($a || $b) {
        echo "A";
    } elseif (!($a || $b)) {
        echo "B";
    } elseif (tap("dead-elseif", true)) {
        echo "C";
    } else {
        echo "dead-else";
    }
}

run(true, false);
run(false, false);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "AB");
    assert!(!user_asm.contains("dead-elseif"));
    assert!(!user_asm.contains("dead-else"));
}

#[test]
fn test_dead_code_elimination_drops_unreachable_elseif_suffix_from_demorgan_equivalent_guards() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_demorgan_elseif_guard_prune");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($a, $b) {
    if (!($a && $b)) {
        echo "A";
    } elseif (!$a || !$b) {
        echo "dead-equivalent";
    } elseif (true) {
        echo "C";
    } else {
        echo "dead-else";
    }
}

run(true, false);
run(true, true);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "AC");
    assert!(!user_asm.contains("dead-equivalent"));
    assert!(!user_asm.contains("dead-else"));
}

#[test]
fn test_dead_code_elimination_drops_exhaustive_switch_true_default_from_cumulative_guards() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_true_exhaustive");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$flag = $argc > 1;
switch (true) {
    case $flag:
        echo "A";
        break;
    case !$flag:
        echo "B";
        break;
    default:
        echo "dead-default";
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "B");
    assert!(!user_asm.contains("dead-default"));
}

#[test]
fn test_dead_code_elimination_prunes_negated_strict_switch_true_case() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_true_negated_strict");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($value) {
    if ($value !== 1) {
        switch (true) {
            case $value === 1:
                echo "dead-case";
                break;
            case !($value === 1):
                echo "A";
                break;
            default:
                echo "dead-default";
        }
    }
}

run(2);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "A");
    assert!(!user_asm.contains("dead-case"));
    assert!(!user_asm.contains("dead-default"));
}

#[test]
fn test_dead_code_elimination_prunes_exhaustive_negated_and_switch_true_default() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_true_negated_and");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$a = $argc > 1;
$b = $argc > 2;
switch (true) {
    case $a && $b:
        echo "A";
        break;
    case !($a && $b):
        echo "B";
        break;
    default:
        echo "dead-default";
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "B");
    assert!(!user_asm.contains("dead-default"));
}

#[test]
fn test_dead_code_elimination_prunes_exhaustive_negated_or_switch_true_default() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_true_negated_or");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$a = $argc > 1;
$b = $argc > 2;
switch (true) {
    case $a || $b:
        echo "A";
        break;
    case !($a || $b):
        echo "B";
        break;
    default:
        echo "dead-default";
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "B");
    assert!(!user_asm.contains("dead-default"));
}

#[test]
fn test_dead_code_elimination_drops_switch_true_suffix_after_exhaustive_multi_pattern_case() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_true_multi_pattern");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$flag = $argc > 1;
$other = false;
switch (true) {
    case $flag:
    case !$flag:
        echo "A";
        break;
    case $other:
        echo "dead-case";
        break;
    default:
        echo "dead-default";
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "A");
    assert!(!user_asm.contains("dead-case"));
    assert!(!user_asm.contains("dead-default"));
}

#[test]
fn test_dead_code_elimination_uses_cumulative_switch_true_guards_inside_case_body() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_true_cumulative_body");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($a, $b, $c, $d) {
    if ($d) {
        switch (true) {
            case (($a && $b) || $c) && $d:
                echo "A";
                break;
            case !$c:
                if ($a && $b) {
                    echo "dead-ab";
                } else {
                    echo "B";
                }
                break;
            default:
                echo "dead-default";
        }
    }
}

run(true, true, true, true);
run(false, false, false, true);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "AB");
    assert!(!user_asm.contains("dead-ab"));
    assert!(!user_asm.contains("dead-default"));
}

#[test]
fn test_dead_code_elimination_drops_scalar_switch_suffix_after_exhaustive_multi_pattern_case() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_scalar_multi_pattern");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$x = 2;
if ($x === 2) {
    switch ($x) {
        case 1:
        case 2:
            echo "A";
            break;
        case 3:
            echo "dead-case";
            break;
        default:
            echo "dead-default";
    }
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "A");
    assert!(!user_asm.contains("dead-case"));
    assert!(!user_asm.contains("dead-default"));
}

#[test]
fn test_dead_code_elimination_drops_excluded_scalar_switch_case_from_outer_guard() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_excluded_scalar_case");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($value) {
    if ($value !== 1) {
        switch ($value) {
            case 1:
                echo "dead-case";
                break;
            case 2:
                echo "A";
                break;
            default:
                echo "live-default";
        }
    }
}

run(2);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "A");
    assert!(!user_asm.contains("dead-case"));
    assert!(user_asm.contains("live-default"));
}

#[test]
fn test_dead_code_elimination_prunes_truthy_switch_cases_and_default() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_truthy_switch_cases");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($flag) {
    if ($flag) {
        switch ($flag) {
            case false:
                echo "dead-false";
                break;
            case true:
                if ($flag) {
                    echo "A";
                } else {
                    echo "bad";
                }
                break;
            default:
                echo "dead-default";
        }
    }
}

run(true);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "A");
    assert!(!user_asm.contains("dead-false"));
    assert!(!user_asm.contains("dead-default"));
    assert!(!user_asm.contains("bad"));
}

#[test]
fn test_dead_code_elimination_keeps_unknown_truthy_switch_entry_before_matching_case() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_truthy_switch_unknown_entry");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($flag, $other) {
    if ($flag) {
        switch ($flag) {
            case $other:
            case false:
                echo "maybe-first";
                break;
            case true:
                echo "A";
                break;
            default:
                echo "dead-default";
        }
    }
}

run(true, false);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "A");
    assert!(user_asm.contains("maybe-first"));
    assert!(!user_asm.contains("dead-default"));
}

#[test]
fn test_dead_code_elimination_prunes_falsy_scalar_labels_from_truthy_switch_subject() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_truthy_switch_scalar_labels");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($flag, $other) {
    if ($flag) {
        switch ($flag) {
            case 0:
            case "":
                echo "dead-falsy-case";
                break;
            case $other:
            case true:
                echo "A";
                break;
            default:
                echo "dead-default";
        }
    }
}

run(true, false);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "A");
    assert!(!user_asm.contains("dead-falsy-case"));
    assert!(!user_asm.contains("dead-default"));
}

#[test]
fn test_dead_code_elimination_combines_exclusion_and_truthy_switch_guards() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_mixed_truthy_exclusion");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($value) {
    if ($value) {
        if ($value !== 1) {
            switch ($value) {
                case 1:
                case 0:
                    echo "dead-mixed-case";
                    break;
                case 2:
                case true:
                    echo "A";
                    break;
                default:
                    echo "dead-default";
            }
        }
    }
}

run(true);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "A");
    assert!(!user_asm.contains("dead-mixed-case"));
    assert!(!user_asm.contains("dead-default"));
}

#[test]
fn test_dead_code_elimination_prunes_dead_label_inside_live_mixed_switch_case() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_live_case_label_prune");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($value) {
    if ($value) {
        if ($value !== 1) {
            switch ($value) {
                case 0:
                    echo "dead-first-case";
                    break;
                case 1:
                case 2:
                case true:
                    echo "A";
                    break;
                default:
                    echo "dead-default";
            }
        }
    }
}

run(true);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "A");
    assert!(!user_asm.contains("dead-first-case"));
    assert!(!user_asm.contains("dead-default"));
}

#[test]
fn test_dead_code_elimination_invalidates_switch_bool_guard_after_local_write() {
    let out = compile_and_run(
        r#"<?php
function run($flag) {
    switch (true) {
        case $flag === true:
            $flag = false;
            if ($flag === true) {
                echo "bad";
            } else {
                echo "a";
            }
            break;
    }
}

run(true);
"#,
    );

    assert_eq!(out, "a");
}

#[test]
fn test_dead_code_elimination_invalidates_outer_guard_before_catch_body() {
    let out = compile_and_run(
        r#"<?php
function run($flag) {
    if ($flag) {
        try {
            $flag = false;
            throw new Exception("boom");
        } catch (Exception $e) {
            if ($flag) {
                echo "bad";
            } else {
                echo "a";
            }
        }
    }
}

run(true);
"#,
    );

    assert_eq!(out, "a");
}

#[test]
fn test_dead_code_elimination_invalidates_outer_guard_before_catch_body_from_switch_throw_path() {
    let out = compile_and_run(
        r#"<?php
function run($flag, $value) {
    if ($flag) {
        try {
            switch ($value) {
                case 1:
                    $flag = false;
                    throw new Exception("boom");
                default:
                    echo "default";
            }
        } catch (Exception $e) {
            if ($flag) {
                echo "bad";
            } else {
                echo "a";
            }
        }
    }
}

run(true, 1);
"#,
    );

    assert_eq!(out, "a");
}

#[test]
fn test_dead_code_elimination_ignores_unreachable_switch_throw_path_writes_before_catch_body() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_throw_path_cfg_prune");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function run($flag, $value) {
    if ($value === 1) {
        if ($flag) {
            try {
                switch ($value) {
                    case 2:
                        $flag = false;
                        throw new Exception("dead-case");
                    case 1:
                        throw new Exception("boom");
                }
            } catch (Exception $e) {
                if ($flag) {
                    echo "a";
                } else {
                    echo "dead-switch-unreachable";
                }
            }
        }
    }
}

run(true, 1);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "a");
    assert!(!user_asm.contains("dead-switch-unreachable"));
}

#[test]
fn test_dead_code_elimination_invalidates_outer_guard_before_finally_body() {
    let out = compile_and_run(
        r#"<?php
function run($flag) {
    if ($flag) {
        try {
            $flag = false;
        } finally {
            if ($flag) {
                echo "bad";
            } else {
                echo "a";
            }
        }
    }
}

run(true);
"#,
    );

    assert_eq!(out, "a");
}

#[test]
fn test_dead_code_elimination_preserves_outer_guard_for_finally_when_only_other_locals_change() {
    let out = compile_and_run(
        r#"<?php
function run($flag) {
    if ($flag) {
        try {
            $other = 1;
        } finally {
            if ($flag) {
                echo "a";
            } else {
                echo "bad";
            }
        }
    }
}

run(true);
"#,
    );

    assert_eq!(out, "a");
}

#[test]
fn test_dead_code_elimination_preserves_outer_guard_for_catch_when_only_non_throw_path_writes() {
    let out = compile_and_run(
        r#"<?php
function run($flag, $other) {
    if ($flag) {
        try {
            if ($other) {
                $flag = false;
            } else {
                throw new Exception("boom");
            }
        } catch (Exception $e) {
            if ($flag) {
                echo "a";
            } else {
                echo "bad";
            }
        }
    }
}

run(true, false);
"#,
    );

    assert_eq!(out, "a");
}

#[test]
fn test_dead_code_elimination_sinks_tail_into_safe_finally_path() {
    let out = compile_and_run(
        r#"<?php
try {
    echo "a";
} finally {
    echo "b";
}
echo "c";
"#,
    );

    assert_eq!(out, "abc");
}

#[test]
fn test_dead_code_elimination_inlines_empty_try_finally() {
    let out = compile_and_run(
        r#"<?php
try {
} finally {
    echo "f";
}
echo "!";
"#,
    );

    assert_eq!(out, "f!");
}

#[test]
fn test_dead_code_elimination_inverts_single_live_else_branch() {
    let out = compile_and_run(
        r#"<?php
$flag = false;
if ($flag) {
} else {
    echo "e";
}
"#,
    );

    assert_eq!(out, "e");
}

#[test]
fn test_dead_code_elimination_collapses_identical_if_branches() {
    let out = compile_and_run(
        r#"<?php
function step($label, $ret) {
    echo $label;
    return $ret;
}
if (step("c", false)) {
    echo "X";
} else {
    echo "X";
}
"#,
    );

    assert_eq!(out, "cX");
}

#[test]
fn test_dead_code_elimination_inlines_default_only_switch() {
    let out = compile_and_run(
        r#"<?php
$x = 1;
switch ($x) {
    default:
        echo "d";
}
"#,
    );

    assert_eq!(out, "d");
}

#[test]
fn test_dead_code_elimination_preserves_elseif_order_after_empty_head() {
    let out = compile_and_run(
        r#"<?php
function step($label, $ret) {
    echo $label;
    return $ret;
}
if (step("a", false)) {
} elseif (step("b", true)) {
    echo "!";
}
"#,
    );

    assert_eq!(out, "ab!");
}

#[test]
fn test_dead_code_elimination_skips_elseif_when_empty_head_matches() {
    let out = compile_and_run(
        r#"<?php
function step($label, $ret) {
    echo $label;
    return $ret;
}
if (step("a", true)) {
} elseif (step("b", true)) {
    echo "!";
}
echo "?";
"#,
    );

    assert_eq!(out, "a?");
}

#[test]
fn test_dead_code_elimination_preserves_regular_elseif_order_after_normalization() {
    let out = compile_and_run(
        r#"<?php
function step($label, $ret) {
    echo $label;
    return $ret;
}
if (step("a", false)) {
    echo "A";
} elseif (step("b", true)) {
    echo "B";
} else {
    echo "C";
}
"#,
    );

    assert_eq!(out, "abB");
}

#[test]
fn test_dead_code_elimination_merges_identical_if_chain_bodies_with_short_circuit() {
    let out = compile_and_run(
        r#"<?php
function step($label, $ret) {
    echo $label;
    return $ret;
}
if (step("a", true)) {
    echo "X";
} else {
    if (step("b", true)) {
        echo "X";
    } else {
        echo "Y";
    }
}
"#,
    );

    assert_eq!(out, "aX");
}

#[test]
fn test_dead_code_elimination_merges_identical_if_chain_tail_with_short_circuit() {
    let out = compile_and_run(
        r#"<?php
function step($label, $ret) {
    echo $label;
    return $ret;
}
if (step("a", true)) {
    echo "X";
} else {
    if (step("b", true)) {
        echo "Y";
    } else {
        echo "X";
    }
}
echo "|";
if (step("c", false)) {
    echo "X";
} else {
    if (step("d", true)) {
        echo "Y";
    } else {
        echo "X";
    }
}
"#,
    );

    assert_eq!(out, "aX|cdY");
}

#[test]
fn test_dead_code_elimination_recursively_merges_longer_if_chains() {
    let out = compile_and_run(
        r#"<?php
function step($label, $ret) {
    echo $label;
    return $ret;
}
if (step("a", false)) {
    echo "X";
} else {
    if (step("b", false)) {
        echo "X";
    } else {
        if (step("c", true)) {
            echo "X";
        } else {
            echo "Y";
        }
    }
}
echo "|";
if (step("d", false)) {
    echo "X";
} else {
    if (step("e", false)) {
        echo "Y";
    } else {
        if (step("f", true)) {
            echo "Y";
        } else {
            echo "X";
        }
    }
}
"#,
    );

    assert_eq!(out, "abcX|defY");
}

#[test]
fn test_dead_code_elimination_normalizes_single_case_switch_with_effectful_subject() {
    let out = compile_and_run(
        r#"<?php
function step($label, $ret) {
    echo $label;
    return $ret;
}
switch (step("s", 1)) {
    case step("a", 1):
        echo "A";
        break;
    default:
        echo "D";
}
"#,
    );

    assert_eq!(out, "saA");
}

#[test]
fn test_dead_code_elimination_merges_identical_adjacent_switch_cases() {
    let out = compile_and_run(
        r#"<?php
function step($label, $ret) {
    echo $label;
    return $ret;
}
switch (step("s", 2)) {
    case 1:
        echo "A";
        break;
    case 2:
        echo "A";
        break;
    default:
        echo "D";
}
"#,
    );

    assert_eq!(out, "sA");
}

#[test]
fn test_dead_code_elimination_merges_fallthrough_switch_labels_into_next_case() {
    let out = compile_and_run(
        r#"<?php
function step($label, $ret) {
    echo $label;
    return $ret;
}
switch (step("s", 2)) {
    case 1:
    case 2:
    case 3:
        echo "A";
        break;
    default:
        echo "D";
}
"#,
    );

    assert_eq!(out, "sA");
}

#[test]
fn test_dead_code_elimination_merges_identical_adjacent_catches() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_merge_identical_catches");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
class A extends Exception {}
class B extends Exception {}
function boom($flag) {
    if ($flag) {
        throw new A("a");
    }
    throw new B("b");
}
try {
    boom($argc > 1);
} catch (A $e) {
    echo pow($argc, 3);
} catch (B $e) {
    echo pow($argc, 3);
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "1");
}

#[test]
fn test_dead_code_elimination_deduplicates_merged_catch_types() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_dedup_catch_types");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
class A extends Exception {}
class B extends Exception {}
class C extends Exception {}
function boom($flag) {
    if ($flag === 1) {
        throw new A("a");
    }
    if ($flag === 2) {
        throw new B("b");
    }
    throw new C("c");
}
try {
    boom($argc);
} catch (A | B $e) {
    echo pow(2, 3);
} catch (B | C $e) {
    echo pow(2, 3);
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    assert_eq!(out, "8");
}

#[test]
fn test_dead_code_elimination_accepts_sorted_multi_catch_types() {
    let out = compile_and_run(
        r#"<?php
class Alpha extends Exception {}
class Mid extends Exception {}
class Zed extends Exception {}
function boom($flag) {
    if ($flag === 1) {
        throw new Zed("z");
    }
    if ($flag === 2) {
        throw new Alpha("a");
    }
    throw new Mid("m");
}
try {
    boom($argc);
} catch (Zed | Alpha | Mid $e) {
    echo "ok";
}
"#,
    );

    assert_eq!(out, "ok");
}

#[test]
fn test_dead_code_elimination_folds_outer_finally_into_single_inner_try() {
    let out = compile_and_run(
        r#"<?php
class A extends Exception {}
function boom() {
    throw new A("a");
}
try {
    try {
        boom();
    } catch (A $e) {
        echo 7;
    }
} finally {
    echo 9;
}
"#,
    );

    assert_eq!(out, "79");
}

#[test]
fn test_dead_code_elimination_materializes_constant_switch_match() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_match");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
switch (2) {
    case 1:
        echo 2 ** 8;
        break;
    case 2:
        echo 7;
        break;
    default:
        echo 2 ** 9;
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "constant switch match should inline the selected path and drop dead pow calls:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "7");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_dead_code_elimination_materializes_constant_switch_fallthrough() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_fallthrough");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
switch (1) {
    case 1:
    case 2:
        echo 7;
        break;
    default:
        echo 2 ** 9;
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "constant switch fallthrough should inline the selected tail and drop dead pow calls:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "7");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_dead_code_elimination_materializes_constant_switch_default() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_switch_default");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
switch (3) {
    case 1:
        echo 2 ** 8;
        break;
    default:
        echo 7;
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "constant switch default should inline the default path and drop dead pow calls:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "7");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_dead_code_elimination_inlines_non_throwing_try_catch() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_try_catch_inline");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
try {
    echo 7;
} catch (Exception $e) {
    echo 2 ** 8;
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "non-throwing try/catch should inline the try body and drop dead pow calls:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "7");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_dead_code_elimination_inlines_try_with_pure_builtin_call() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_try_pure_builtin");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
try {
    echo strlen("abc");
} catch (Exception $e) {
    echo 2 ** 8;
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "pure non-throwing builtin calls should let dead catch bodies disappear:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "3");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_dead_code_elimination_inlines_try_with_pure_user_function_call() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_try_pure_user_function");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
function len3() {
    return strlen("abc");
}

try {
    echo len3();
} catch (Exception $e) {
    echo 2 ** 8;
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "pure non-throwing user functions should let dead catch bodies disappear:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "3");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_dead_code_elimination_inlines_try_with_pure_static_method_call() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_try_pure_static_method");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
class Util {
    public static function len3() {
        return strlen("abc");
    }
}

try {
    echo Util::len3();
} catch (Exception $e) {
    echo 2 ** 8;
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "pure non-throwing static methods should let dead catch bodies disappear:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "3");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_dead_code_elimination_inlines_try_with_pure_self_static_method_call() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_try_pure_self_static_method");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
class Util {
    public static function len3() {
        return strlen("abc");
    }

    public static function relay() {
        return self::len3();
    }
}

try {
    echo Util::relay();
} catch (Exception $e) {
    echo 2 ** 8;
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "self:: pure static methods should let dead catch bodies disappear:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "3");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_dead_code_elimination_inlines_try_with_pure_private_instance_method_call() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_try_pure_private_instance_method");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
class Util {
    private function len3() {
        return strlen("abc");
    }

    public function relay() {
        try {
            return $this->len3();
        } catch (Exception $e) {
            return 2 ** 8;
        }
    }
}

$util = new Util();
echo $util->relay();
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "pure private instance methods on $this should let dead catch bodies disappear:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "3");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_dead_code_elimination_inlines_try_with_pure_closure_alias() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_try_pure_closure_alias");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$f = function () {
    return strlen("abc");
};

try {
    echo $f();
} catch (Exception $e) {
    echo 2 ** 8;
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "pure closure aliases should let dead catch bodies disappear:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "3");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_dead_code_elimination_inlines_try_with_ternary_callable_alias() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_try_ternary_callable_alias");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$flag = true;
$f = $flag ? strlen(...) : strlen(...);

try {
    echo $f("abc");
} catch (Exception $e) {
    echo 2 ** 8;
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "ternary-selected callable aliases should let dead catch bodies disappear:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "3");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_dead_code_elimination_inlines_try_with_match_callable_alias() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_try_match_callable_alias");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$mode = 1;
$f = match ($mode) {
    1 => strlen(...),
    default => strlen(...),
};

try {
    echo $f("abc");
} catch (Exception $e) {
    echo 2 ** 8;
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "match-selected callable aliases should let dead catch bodies disappear:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "3");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_dead_code_elimination_inlines_try_with_named_first_class_callable_expr_call() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_try_named_first_class_expr_call");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
try {
    echo (strlen(...))("abc");
} catch (Exception $e) {
    echo 2 ** 8;
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "named first-class callable expr calls should let dead catch bodies disappear:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "3");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_dead_code_elimination_inlines_try_with_callable_alias_chain() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_try_callable_alias_chain");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$f = strlen(...);
$g = $f;

try {
    echo $g("abc");
} catch (Exception $e) {
    echo 2 ** 8;
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "callable alias chains should let dead catch bodies disappear:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "3");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_dead_code_elimination_inlines_try_with_callable_alias_if_merge() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_try_callable_alias_if_merge");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
$flag = true;
if ($flag) {
    $g = strlen(...);
} else {
    $g = strlen(...);
}

try {
    echo $g("abc");
} catch (Exception $e) {
    echo 2 ** 8;
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "merged callable aliases across if paths should let dead catch bodies disappear:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "3");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_dead_code_elimination_inlines_try_with_callable_alias_try_merge() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_try_callable_alias_try_merge");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
try {
    $g = strlen(...);
} catch (Exception $e) {
    $g = strlen(...);
} finally {
    strlen("done");
}

try {
    echo $g("abc");
} catch (Exception $e) {
    echo 2 ** 8;
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "merged callable aliases across try/catch/finally should let dead catch bodies disappear:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "3");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_dead_code_elimination_inlines_try_with_callable_alias_switch_merge() {
    let dir = make_cli_test_dir("elephc_dead_code_elimination_try_callable_alias_switch_merge");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
switch ($argc) {
    case 1:
        $g = strlen(...);
        break;
    case 2:
    default:
        $g = strlen(...);
}

try {
    echo $g("abc");
} catch (Exception $e) {
    echo 2 ** 8;
}
"#,
        &dir,
        8_388_608,
        false,
        false,
    );

    assert!(
        !user_asm.contains("pow"),
        "merged callable aliases across switch fallthrough paths should let dead catch bodies disappear:\n{}",
        user_asm
    );

    let out = assemble_and_run(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );
    assert_eq!(out, "3");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_dead_code_elimination_inlines_non_throwing_try_finally_fallthrough() {
    let out = compile_and_run(
        r#"<?php
try {
    echo "a";
} finally {
    echo "b";
}
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_hoists_non_throwing_try_prefix() {
    let out = compile_and_run(
        r#"<?php
try {
    echo "a";
    throw new Exception("boom");
} catch (Exception $e) {
    echo "b";
}
"#,
    );

    assert_eq!(out, "ab");
}

#[test]
fn test_dead_code_elimination_flattens_nested_single_path_ifs() {
    let out = compile_and_run(
        r#"<?php
$a = true;
$b = true;
if ($a) {
    if ($b) {
        echo 7;
    }
}
"#,
    );

    assert_eq!(out, "7");
}
