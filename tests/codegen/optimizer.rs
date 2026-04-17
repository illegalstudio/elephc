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
