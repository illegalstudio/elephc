//! Purpose:
//! Integration or regression tests for optimizer-sensitive codegen coverage of optimizer, dead-code elimination, tries, try inlining pure calls, including dead code elimination inlines non throwing try catch, dead code elimination inlines try with pure builtin call, and dead code elimination inlines try with pure user function call.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled and run so folding, propagation, or pruning stays behavior-preserving.

use super::*;

/// Verifies that a non-throwing try/catch inlines the try body and drops the catch body from
/// assembly. Confirms "7" with `pow` absent.
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

/// Verifies that a try with a pure builtin call inlines the try body, letting the dead catch
/// disappear. Confirms "3" with `pow` absent.
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

/// Verifies that a try with a pure user function inlines the try body, letting the dead catch
/// disappear. Confirms "3" with `pow` absent.
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

/// Verifies that a try with a pure static method call inlines the try body. Confirms "3".
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

/// Verifies that a try with a pure `self::` static relay call inlines correctly. Confirms "3".
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

/// Verifies that a try with a pure private instance method on `$this` inlines correctly.
/// Confirms "3".
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

/// Verifies that a try with a pure closure alias inlines the try body. Confirms "3".
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

/// Verifies that a try prefix with a subsequent throw inlines the non-throwing prefix
/// before the throw, then executes the catch. Confirms "ab" — output before throw
/// is preserved (no reordering across the throw boundary).
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
