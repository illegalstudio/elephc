use crate::support::*;

fn compile_and_run_expect_runtime_error(source: &str) -> String {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_test_{}_{:?}_{}", pid, tid, id));
    fs::create_dir_all(&dir).unwrap();

    let (user_asm, _runtime_asm, required_libraries) =
        compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    let stderr = assemble_and_run_expect_failure(
        &user_asm,
        get_runtime_obj(),
        &dir,
        &required_libraries,
        &default_link_paths(),
        &[],
    );

    let _ = fs::remove_dir_all(&dir);
    stderr
}

// --- Constants (const / define) ---

#[test]
fn test_const_int() {
    let out = compile_and_run("<?php\nconst MAX = 100;\necho MAX;\n");
    assert_eq!(out, "100");
}

#[test]
fn test_const_string() {
    let out = compile_and_run("<?php\nconst GREETING = \"hello\";\necho GREETING;\n");
    assert_eq!(out, "hello");
}

#[test]
fn test_const_float() {
    let out = compile_and_run("<?php\nconst PI = 3.14;\necho PI;\n");
    assert_eq!(out, "3.14");
}

#[test]
fn test_const_bool() {
    let out = compile_and_run("<?php\nconst DEBUG = true;\necho DEBUG;\n");
    assert_eq!(out, "1");
}

#[test]
fn test_define_int() {
    let out = compile_and_run("<?php\ndefine(\"MAX_SIZE\", 256);\necho MAX_SIZE;\n");
    assert_eq!(out, "256");
}

#[test]
fn test_define_string() {
    let out = compile_and_run("<?php\ndefine(\"APP_NAME\", \"elephc\");\necho APP_NAME;\n");
    assert_eq!(out, "elephc");
}

#[test]
fn test_const_in_expression() {
    let out = compile_and_run("<?php\nconst X = 10;\nconst Y = 20;\necho X + Y;\n");
    assert_eq!(out, "30");
}

#[test]
fn test_const_in_function() {
    let out =
        compile_and_run("<?php\nconst LIMIT = 42;\nfunction test() { echo LIMIT; }\ntest();\n");
    assert_eq!(out, "42");
}

#[test]
fn test_define_in_function() {
    let out =
        compile_and_run("<?php\ndefine(\"RATE\", 100);\nfunction show() { echo RATE; }\nshow();\n");
    assert_eq!(out, "100");
}

#[test]
fn test_const_concat() {
    let out = compile_and_run("<?php\nconst PREFIX = \"hello\";\necho PREFIX . \" world\";\n");
    assert_eq!(out, "hello world");
}

// --- List unpacking ---

#[test]
fn test_list_unpack_int() {
    let out = compile_and_run(
        "<?php\n[$a, $b, $c] = [10, 20, 30];\necho $a . \" \" . $b . \" \" . $c;\n",
    );
    assert_eq!(out, "10 20 30");
}

#[test]
fn test_list_unpack_string() {
    let out = compile_and_run("<?php\n[$x, $y] = [\"hello\", \"world\"];\necho $x . \" \" . $y;\n");
    assert_eq!(out, "hello world");
}

#[test]
fn test_list_unpack_from_variable() {
    let out = compile_and_run(
        "<?php\n$arr = [1, 2, 3];\n[$a, $b, $c] = $arr;\necho $a . \" \" . $b . \" \" . $c;\n",
    );
    assert_eq!(out, "1 2 3");
}

#[test]
fn test_list_unpack_two_vars() {
    let out = compile_and_run("<?php\n[$first, $second] = [42, 99];\necho $first + $second;\n");
    assert_eq!(out, "141");
}

// --- call_user_func_array ---

#[test]
fn test_call_user_func_array_basic() {
    let out = compile_and_run("<?php\nfunction add($a, $b) { return $a + $b; }\necho call_user_func_array(\"add\", [3, 4]);\n");
    assert_eq!(out, "7");
}

#[test]
fn test_call_user_func_array_single_arg() {
    let out = compile_and_run("<?php\nfunction double($n) { return $n * 2; }\necho call_user_func_array(\"double\", [21]);\n");
    assert_eq!(out, "42");
}

#[test]
fn test_call_user_func_array_string_return() {
    let out = compile_and_run("<?php\nfunction greet($name) { return \"Hello \" . $name; }\necho call_user_func_array(\"greet\", [\"World\"]);\n");
    assert_eq!(out, "Hello World");
}

#[test]
fn test_call_user_func_array_variadic_callback() {
    let out = compile_and_run(
        "<?php
        function summarize($head = 1, ...$rest) {
            echo $head;
            echo \":\";
            echo count($rest);
        }
        call_user_func_array(summarize(...), [7, 8, 9]);
        ",
    );
    assert_eq!(out, "7:2");
}

// -- v0.8 constants --

#[test]
fn test_php_eol() {
    let out = compile_and_run("<?php echo \"a\" . PHP_EOL . \"b\";");
    assert_eq!(out, "a\nb");
}

#[test]
fn test_php_os() {
    let out = compile_and_run("<?php echo PHP_OS;");
    assert_eq!(out, target().platform.php_os_name());
}

#[test]
fn test_directory_separator() {
    let out = compile_and_run("<?php echo DIRECTORY_SEPARATOR;");
    assert_eq!(out, "/");
}

// -- v0.8 time / microtime --

#[test]
fn test_time() {
    let out = compile_and_run("<?php $t = time(); if ($t > 1000000000) { echo \"ok\"; }");
    assert_eq!(out, "ok");
}

#[test]
fn test_microtime() {
    let out = compile_and_run("<?php $t = microtime(true); if ($t > 1000000000) { echo \"ok\"; }");
    assert_eq!(out, "ok");
}

// -- v0.8 sleep / usleep --

#[test]
fn test_sleep_zero() {
    let out = compile_and_run("<?php sleep(0); echo \"ok\";");
    assert_eq!(out, "ok");
}

#[test]
fn test_usleep_zero() {
    let out = compile_and_run("<?php usleep(0); echo \"ok\";");
    assert_eq!(out, "ok");
}

// -- v0.8 getenv --

#[test]
fn test_getenv_home() {
    let out =
        compile_and_run("<?php $home = getenv(\"HOME\"); if (strlen($home) > 0) { echo \"ok\"; }");
    assert_eq!(out, "ok");
}

#[test]
fn test_getenv_nonexistent() {
    let out = compile_and_run(
        "<?php $missing = getenv(\"ELEPHC_NONEXISTENT_VAR_XYZ\"); echo strlen($missing);",
    );
    assert_eq!(out, "0");
}

#[test]
fn test_putenv() {
    let out = compile_and_run(
        r#"<?php
putenv("ELEPHC_TEST_VAR=hello");
echo getenv("ELEPHC_TEST_VAR");
"#,
    );
    assert_eq!(out, "hello");
}

// -- v0.8 phpversion / php_uname --

#[test]
fn test_phpversion() {
    let out = compile_and_run("<?php echo phpversion();");
    assert_eq!(out, "0.7.1");
}

#[test]
fn test_php_uname() {
    let out = compile_and_run(
        r#"<?php
$default = php_uname();
$explicit = php_uname("a");
echo $default === $explicit ? "same" : "different";
"#,
    );
    assert_eq!(out, "same");
}

#[test]
fn test_php_uname_modes() {
    let out = compile_and_run(
        r#"<?php
$sys = php_uname("s");
$node = php_uname("n");
$release = php_uname("r");
$version = php_uname("v");
$machine = php_uname("m");
$all = php_uname("a");
echo $sys . "\n";
if (
    strlen($node) > 0 &&
    strlen($release) > 0 &&
    strlen($version) > 0 &&
    strlen($machine) > 0 &&
    str_contains($all, $sys) &&
    str_contains($all, $node) &&
    str_contains($all, $release) &&
    str_contains($all, $version) &&
    str_contains($all, $machine)
) {
    echo "ok";
} else {
    echo "bad";
}
"#,
    );
    assert_eq!(out, format!("{}\nok", target().platform.php_os_name()));
}

#[test]
fn test_php_uname_rejects_invalid_mode_length_at_runtime() {
    let err = compile_and_run_expect_runtime_error(r#"<?php $mode = "sn"; echo php_uname($mode);"#);
    assert!(err.contains("php_uname(): Argument #1 ($mode) must be a single character"));
}

#[test]
fn test_php_uname_rejects_invalid_mode_value_at_runtime() {
    let err = compile_and_run_expect_runtime_error(r#"<?php $mode = "x"; echo php_uname($mode);"#);
    assert!(err.contains("php_uname(): Argument #1 ($mode) must be one of"));
}

// -- v0.8 exec / shell_exec / system / passthru --

#[test]
fn test_shell_exec() {
    let out = compile_and_run("<?php $out = shell_exec(\"echo hello\"); echo trim($out);");
    assert_eq!(out, "hello");
}

#[test]
fn test_exec() {
    let out = compile_and_run("<?php $out = exec(\"echo test\"); echo trim($out);");
    assert_eq!(out, "test");
}

#[test]
fn test_system() {
    let out = compile_and_run("<?php system(\"echo hi\");");
    assert_eq!(out, "hi\n");
}

#[test]
fn test_passthru() {
    let out = compile_and_run("<?php passthru(\"echo bye\");");
    assert_eq!(out, "bye\n");
}
