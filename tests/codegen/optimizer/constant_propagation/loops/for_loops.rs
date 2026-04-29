use super::*;

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
fn test_constant_propagation_tracks_for_infinite_break_assignment() {
    let dir = make_cli_test_dir("elephc_constant_propagation_for_infinite_break");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
for (;;) {
    $base = 2;
    break;
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
        "for(;;) break assignment should feed the post-loop env:\n{}",
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
fn test_constant_propagation_preserves_for_init_when_condition_is_false() {
    let dir = make_cli_test_dir("elephc_constant_propagation_for_false_init");
    let (user_asm, _runtime_asm, required_libraries) = compile_source_to_asm_with_options(
        r#"<?php
for ($base = 2; false; $base = 9) {
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
        "for condition false should preserve init env and ignore body/update writes:\n{}",
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
