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
