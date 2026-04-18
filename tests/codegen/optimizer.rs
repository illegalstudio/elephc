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
