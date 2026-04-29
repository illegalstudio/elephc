use super::*;

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
