use crate::support::*;

// --- Phase 11: v0.5 — I/O and file system ---

#[test]
fn test_print_basic() {
    let out = compile_and_run("<?php print \"hello\";");
    assert_eq!(out, "hello");
}

#[test]
fn test_print_int() {
    let out = compile_and_run("<?php print 42;");
    assert_eq!(out, "42");
}

#[test]
fn test_stdin_constant() {
    let out = compile_and_run("<?php echo STDIN;");
    assert_eq!(out, "0");
}

#[test]
fn test_stdout_constant() {
    let out = compile_and_run("<?php echo STDOUT;");
    assert_eq!(out, "1");
}

#[test]
fn test_stderr_constant() {
    let out = compile_and_run("<?php echo STDERR;");
    assert_eq!(out, "2");
}

#[test]
fn test_var_dump_int() {
    let out = compile_and_run("<?php var_dump(42);");
    assert_eq!(out, "int(42)\n");
}

#[test]
fn test_var_dump_string() {
    let out = compile_and_run(r#"<?php var_dump("hello");"#);
    assert_eq!(out, "string(5) \"hello\"\n");
}

#[test]
fn test_var_dump_bool_true() {
    let out = compile_and_run("<?php var_dump(true);");
    assert_eq!(out, "bool(true)\n");
}

#[test]
fn test_var_dump_bool_false() {
    let out = compile_and_run("<?php var_dump(false);");
    assert_eq!(out, "bool(false)\n");
}

#[test]
fn test_var_dump_null() {
    let out = compile_and_run("<?php var_dump(null);");
    assert_eq!(out, "NULL\n");
}

#[test]
fn test_var_dump_float() {
    let out = compile_and_run("<?php var_dump(3.14);");
    assert_eq!(out, "float(3.14)\n");
}

#[test]
fn test_var_dump_mixed_prints_concrete_payload() {
    let out = compile_and_run(
        r#"<?php
class Box {}

$map = [
    "i" => 42,
    "s" => "hello",
    "b" => true,
    "n" => null,
    "a" => [1, 2],
    "o" => new Box(),
];

var_dump($map["i"]);
var_dump($map["s"]);
var_dump($map["b"]);
var_dump($map["n"]);
var_dump($map["a"]);
var_dump($map["o"]);
"#,
    );
    assert_eq!(
        out,
        "int(42)\nstring(5) \"hello\"\nbool(true)\nNULL\narray(2) {\n}\nobject(Box)\n"
    );
}

#[test]
fn test_print_r_int() {
    let out = compile_and_run("<?php print_r(42);");
    assert_eq!(out, "42");
}

#[test]
fn test_print_r_string() {
    let out = compile_and_run(r#"<?php print_r("hello");"#);
    assert_eq!(out, "hello");
}

#[test]
fn test_print_r_bool_true() {
    let out = compile_and_run("<?php print_r(true);");
    assert_eq!(out, "1");
}

#[test]
fn test_print_r_bool_false() {
    let out = compile_and_run("<?php print_r(false);");
    assert_eq!(out, "");
}

#[test]
fn test_print_r_array() {
    let out = compile_and_run("<?php print_r([1, 2, 3]);");
    assert_eq!(out, "Array\n");
}

#[test]
fn test_file_put_get_contents() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("test.txt", "hello world");
echo file_get_contents("test.txt");
"#,
    );
    assert_eq!(out, "hello world");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_file_get_contents_missing_emits_runtime_warning() {
    let out = compile_and_run_capture(
        r#"<?php
echo file_get_contents("missing.txt");
echo "after";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "after");
    assert!(
        out.stderr.contains("Warning: file_get_contents()"),
        "expected runtime warning, got stderr={}",
        out.stderr
    );
}

#[test]
fn test_file_get_contents_missing_is_strict_false() {
    let out = compile_and_run_capture(
        r#"<?php
$value = @file_get_contents("missing.txt");
echo $value === false ? "false" : "string";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "false");
    assert_eq!(out.stderr, "");
}

#[test]
fn test_file_get_contents_success_is_not_false() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("test.txt", "");
$value = file_get_contents("test.txt");
echo $value === false ? "false" : "string";
"#,
    );
    assert_eq!(out, "string");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_error_control_suppresses_runtime_warning() {
    let out = compile_and_run_capture(
        r#"<?php
echo @file_get_contents("missing.txt");
echo "after";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "after");
    assert_eq!(out.stderr, "");
}

#[test]
fn test_file_exists() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("exists.txt", "data");
if (file_exists("exists.txt")) {
    echo "yes";
}
if (!file_exists("nope.txt")) {
    echo "no";
}
"#,
    );
    assert_eq!(out, "yesno");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_filesize() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("size.txt", "12345");
echo filesize("size.txt");
"#,
    );
    assert_eq!(out, "5");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_is_file_is_dir() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("afile.txt", "x");
mkdir("adir");
if (is_file("afile.txt")) { echo "F"; }
if (!is_dir("afile.txt")) { echo "!D"; }
if (is_dir("adir")) { echo "D"; }
if (!is_file("adir")) { echo "!F"; }
rmdir("adir");
"#,
    );
    assert_eq!(out, "F!DD!F");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_mkdir_rmdir() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("testdir");
if (is_dir("testdir")) { echo "made"; }
rmdir("testdir");
if (!is_dir("testdir")) { echo "gone"; }
"#,
    );
    assert_eq!(out, "madegone");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_copy_unlink() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("orig.txt", "content");
copy("orig.txt", "dup.txt");
echo file_get_contents("dup.txt");
unlink("dup.txt");
if (!file_exists("dup.txt")) { echo "|gone"; }
unlink("orig.txt");
"#,
    );
    assert_eq!(out, "content|gone");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_rename_file() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("old.txt", "data");
rename("old.txt", "new.txt");
echo file_get_contents("new.txt");
if (!file_exists("old.txt")) { echo "|moved"; }
unlink("new.txt");
"#,
    );
    assert_eq!(out, "data|moved");
    let _ = fs::remove_dir_all(&dir);
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
fn test_readline() {
    let out = compile_and_run_with_stdin(
        r#"<?php
$line = readline();
echo "read: " . trim($line);
"#,
        "world\n",
    );
    assert_eq!(out, "read: world");
}

#[test]
fn test_file_lines() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("lines.txt", "one\ntwo\nthree\n");
$lines = file("lines.txt");
echo count($lines);
unlink("lines.txt");
"#,
    );
    assert_eq!(out, "3");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_getcwd() {
    let out = compile_and_run(
        r#"<?php
$cwd = getcwd();
if (strlen($cwd) > 0) { echo "ok"; }
"#,
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_sys_get_temp_dir() {
    let out = compile_and_run(
        r#"<?php
$tmp = sys_get_temp_dir();
echo $tmp;
"#,
    );
    assert!(out.contains("tmp") || out.contains("Tmp"));
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
fn test_is_readable_writable() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("perm.txt", "x");
if (is_readable("perm.txt")) { echo "R"; }
if (is_writable("perm.txt")) { echo "W"; }
unlink("perm.txt");
"#,
    );
    assert_eq!(out, "RW");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_chdir_getcwd() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("subdir");
$before = getcwd();
chdir("subdir");
$after = getcwd();
if (strlen($after) > strlen($before)) { echo "changed"; }
chdir("..");
rmdir("subdir");
"#,
    );
    assert_eq!(out, "changed");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_var_dump_multiple() {
    let out = compile_and_run(
        r#"<?php
var_dump(1);
var_dump("hi");
var_dump(true);
"#,
    );
    assert_eq!(out, "int(1)\nstring(2) \"hi\"\nbool(true)\n");
}

// --- File I/O: CSV, timestamps, directory listing, temp files, seek/rewind/eof ---

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
fn test_filemtime() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("ts.txt", "x");
$t = filemtime("ts.txt");
if ($t > 1000000000) { echo "ok"; }
unlink("ts.txt");
"#,
    );
    assert_eq!(out, "ok");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_scandir() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("sd");
file_put_contents("sd/a.txt", "a");
file_put_contents("sd/b.txt", "b");
$files = scandir("sd");
if (
    count($files) == 4 &&
    in_array(".", $files) &&
    in_array("..", $files) &&
    in_array("a.txt", $files) &&
    in_array("b.txt", $files)
) {
    echo "ok";
}
unlink("sd/a.txt");
unlink("sd/b.txt");
rmdir("sd");
"#,
    );
    assert_eq!(out, "ok");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_glob_fn() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
mkdir("gd");
file_put_contents("gd/g1.txt", "a");
file_put_contents("gd/g2.txt", "b");
$matches = glob("gd/*.txt");
if (
    count($matches) == 2 &&
    in_array("gd/g1.txt", $matches) &&
    in_array("gd/g2.txt", $matches)
) {
    echo "ok";
}
unlink("gd/g1.txt");
unlink("gd/g2.txt");
rmdir("gd");
"#,
    );
    assert_eq!(out, "ok");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_tempnam() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$tmp = tempnam(".", "test");
if (file_exists($tmp)) { echo "ok"; }
unlink($tmp);
"#,
    );
    assert_eq!(out, "ok");
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
