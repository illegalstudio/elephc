use super::*;

#[test]
fn test_stdin_constant() {
    let out = compile_and_run("<?php echo STDIN;");
    assert_eq!(out, "Resource id #1");
}

#[test]
fn test_stdout_constant() {
    let out = compile_and_run("<?php echo STDOUT;");
    assert_eq!(out, "Resource id #2");
}

#[test]
fn test_stderr_constant() {
    let out = compile_and_run("<?php echo STDERR;");
    assert_eq!(out, "Resource id #3");
}

#[test]
fn test_standard_stream_constants_are_resources() {
    let out = compile_and_run(
        r#"<?php
echo gettype(STDIN) . "|";
echo gettype(STDOUT) . "|";
echo gettype(STDERR);
"#,
    );
    assert_eq!(out, "resource|resource|resource");
}

#[test]
fn test_standard_stream_constants_resolve_from_namespace() {
    let out = compile_and_run(
        r#"<?php
namespace App;
echo gettype(STDOUT) . "|";
echo STDOUT;
"#,
    );
    assert_eq!(out, "resource|Resource id #2");
}

#[test]
fn test_fopen_returns_stream_resource() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$f = fopen("resource.txt", "w");
echo gettype($f) . "|";
echo $f;
fclose($f);
unlink("resource.txt");
"#,
    );
    assert!(out.starts_with("resource|Resource id #"), "unexpected output: {out}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_mixed_file_handle_preserves_resource_type() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
function identity(mixed $value): mixed {
    return $value;
}
$f = fopen("mixed-resource.txt", "w");
$m = identity($f);
echo gettype($m) . "|";
echo $m;
fclose($f);
unlink("mixed-resource.txt");
"#,
    );
    assert!(out.starts_with("resource|Resource id #"), "unexpected output: {out}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_resource_concatenation_uses_php_display_string() {
    let out = compile_and_run("<?php echo \"stream=\" . STDOUT;");
    assert_eq!(out, "stream=Resource id #2");
}

#[test]
fn test_resource_truthiness_does_not_use_raw_descriptor_zero() {
    let out = compile_and_run(
        r#"<?php
echo (bool)STDIN ? "truthy" : "falsy";
echo "|";
echo empty(STDIN) ? "empty" : "not-empty";
"#,
    );
    assert_eq!(out, "truthy|not-empty");
}

#[test]
fn test_var_dump_resource_uses_stream_shape() {
    let out = compile_and_run("<?php var_dump(STDOUT);");
    assert_eq!(out, "resource(2) of type (stream)\n");
}

#[test]
fn test_fopen_fwrite_fclose_fread() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$f = fopen("rw.txt", "w");
fwrite($f, "test data");
fclose($f);
$f = fopen("rw.txt", "r");
$content = fread($f, 9);
fclose($f);
echo $content;
unlink("rw.txt");
"#,
    );
    assert_eq!(out, "test data");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_fgets_stdin() {
    let out = compile_and_run_with_stdin(
        r#"<?php
$line = fgets(STDIN);
echo "got: " . $line;
"#,
        "hello\n",
    );
    assert_eq!(out, "got: hello\n");
}

#[test]
fn test_fopen_nonexistent_fgets_no_hang() {
    let out = compile_and_run(
        r#"<?php
$f = fopen("no_such_file.txt", "r");
$line = fgets($f);
echo "done";
"#,
    );
    assert_eq!(out, "done");
}

#[test]
fn test_fseek_ftell() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("seek.txt", "abcdefghij");
$f = fopen("seek.txt", "r");
$result = fseek($f, 5);
echo $result;
echo ftell($f);
$data = fread($f, 5);
echo $data;
fclose($f);
unlink("seek.txt");
"#,
    );
    assert_eq!(out, "05fghij");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_fseek_return_value() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("seek2.txt", "hello world");
$f = fopen("seek2.txt", "r");
$r1 = fseek($f, 0);
echo $r1;
$r2 = fseek($f, 3, 0);
echo $r2;
$r3 = fseek($f, 2, 1);
echo $r3;
echo ftell($f);
fclose($f);
unlink("seek2.txt");
"#,
    );
    assert_eq!(out, "0005");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_fgetcsv() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("data.csv", "alice,30,NY\n");
$f = fopen("data.csv", "r");
$row = fgetcsv($f);
echo $row[0];
fclose($f);
unlink("data.csv");
"#,
    );
    assert_eq!(out, "alice");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_fputcsv() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$f = fopen("out.csv", "w");
$data = ["hello", "world"];
fputcsv($f, $data);
fclose($f);
$content = file_get_contents("out.csv");
echo trim($content);
unlink("out.csv");
"#,
    );
    assert_eq!(out, "hello,world");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_rewind() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("rw.txt", "abcdef");
$f = fopen("rw.txt", "r");
$first = fread($f, 3);
rewind($f);
$again = fread($f, 3);
fclose($f);
echo $first . "|" . $again;
unlink("rw.txt");
"#,
    );
    assert_eq!(out, "abc|abc");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_feof() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("eof.txt", "hi");
$f = fopen("eof.txt", "r");
$data = fread($f, 2);
$data = fread($f, 1);
if (feof($f)) { echo "eof"; }
fclose($f);
unlink("eof.txt");
"#,
    );
    assert_eq!(out, "eof");
    let _ = fs::remove_dir_all(&dir);
}
