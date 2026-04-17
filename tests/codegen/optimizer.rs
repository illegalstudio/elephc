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
