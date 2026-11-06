//! Purpose:
//! PHP, snapshot and ownership coverage for consuming geometric string append.
//!
//! Called from:
//! - `cargo test --test codegen_tests consuming_string_append`.
//!
//! Key details:
//! - Each fixture runs in both optimizer modes with heap diagnostics and an optional PHP oracle.
//! - Runtime inputs preserve append and snapshot operations through AST optimization.

use crate::support::*;

/// Checks a complete program's output and balanced heap in both optimizer modes.
fn check_append(source: &str, expected: &str) {
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        let dir = make_cli_test_dir("elephc_consuming_string_append");
        let php_path = dir.join("main.php");
        fs::write(&php_path, source).expect("write append fixture");
        let compile = elephc_cli_command(&dir).args([mode, "--heap-debug"])
            .arg(&php_path).output().expect("compile append fixture");
        if !compile.status.success() {
            fs::remove_dir_all(&dir).expect("remove failed append fixture");
            panic!("{mode}: {}", String::from_utf8_lossy(&compile.stderr));
        }
        let output = run_binary(&dir.join("main"), &dir);
        let oracle = std::env::var_os("ELEPHC_PHP_CHECK").map(|_| {
            Command::new("php").arg("-n").arg(&php_path).output().expect("run PHP oracle")
        });
        fs::remove_dir_all(&dir).expect("remove append fixture");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{mode}: {stderr}");
        assert_eq!(output.stdout, expected.as_bytes(), "{mode}: {stderr}");
        assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{mode}: {stderr}");
        if let Some(php) = oracle {
            assert!(php.status.success(), "{}", String::from_utf8_lossy(&php.stderr));
            assert_eq!(output.stdout, php.stdout, "PHP mismatch with {mode}");
        }
    }
}

/// Geometric growth crosses the concat scratch limit while keeping the result byte-exact.
#[test]
fn test_consuming_string_append_large_growth() {
    check_append("<?php $s='';for($i=0;$i<70000+$argc;$i++){$s.='x';}echo strlen($s),':',substr($s,0,2),':',substr($s,-2);",
        "70001:xx:xx");
}

/// A writable capture discovered after append keeps the ordinary concat owner protocol.
#[test]
fn test_consuming_string_append_before_writable_reference_capture() {
    for source in [
        "<?php $s=(string)$argc;$s.='x';$f=function()use(&$s){$s='z';};$f();echo$s;",
        "<?php function build(int $n):void{$s=(string)$n;$s.='x';$f=function()use(&$s){$s='z';};$f();echo$s;}build($argc);",
        "<?php $s=(string)$argc;for($i=0;$i<$argc;$i++){$s.='x';}$f=function()use(&$s){$s='z';};$f();echo$s;",
    ] {
        check_append(source, "z");
    }
}

/// Ordinary concat snapshots and by-value assignments survive later source mutation.
#[test]
fn test_consuming_string_append_preserves_snapshots_and_aliases() {
    let source = r#"<?php
$s = str_repeat('a', $argc + 8);
$t = $s . 'x';
$b = $s;
$s .= 'y';
echo $t, '|', $b, '|', $s, '|';
$r =& $s;
$s .= '!';
echo $r;
"#;
    check_append(source, "aaaaaaaaax|aaaaaaaaa|aaaaaaaaay|aaaaaaaaay!");
}

/// Empty, NUL and short tails preserve lengths; self append copies before retiring its source.
#[test]
fn test_consuming_string_append_self_overlap_nul_and_small_lengths() {
    let source = r#"<?php
$s = str_repeat('a', $argc);
for ($i = 0; $i < 5; $i++) { $s .= $s; }
echo strlen($s), '|';
$s .= '';
$part = substr($s, 1, 3);
$s .= $part;
echo strlen($s), '|';
$nul = "\0z";
$s .= $nul;
echo strlen($s), '|', ord(substr($s, -2, 1)), '|', substr($s, -1), '|';
foreach ([1, 2, 4, 8, 9] as $length) {
    $piece = str_repeat('x', $length + $argc - 1);
    $piece .= '!';
    echo strlen($piece), ',';
}
"#;
    check_append(source, "32|35|37|0|z|2,3,5,9,10,");
}

/// Calls, borrowed parameters and capture mutation stay on the existing assignment path.
#[test]
fn test_consuming_string_append_fallback_order_and_borrowed_inputs() {
    let source = r#"<?php
function appendParameter(string $value): string { $value .= '!'; return $value; }
function failPart(): string { throw new RuntimeException('stop'); }
$original = str_repeat('a', $argc);
echo appendParameter($original), '|', $original, '|';
$s = $original;
$f = function() use ($s) { $s .= '!'; return $s; };
echo $f(), '|', $f(), '|', $s, '|';
try { $s .= failPart(); }
catch (RuntimeException $error) { echo $s; }
"#;
    check_append(source, "a!|a|a!|a!|a|a");
}

/// A local builder may borrow its RHS parameter while preserving the caller's input.
#[test]
fn test_consuming_string_append_borrows_rhs_parameter() {
    let source = "<?php function printRepeatPart(string $part,int $n):void{$result='';for($i=0;$i<$n;$i++){$result.=$part;}echo$result;}$part=str_repeat('x',$argc);printRepeatPart($part,3);echo'|',$part;";
    check_append(source, "xxx|x");
}

/// Returning an owned local builder transfers its result through the ordinary call boundary.
#[test]
fn test_consuming_string_append_returns_local_owner() {
    check_append("<?php function build(int $n):string{$s='';for($i=0;$i<$n;$i++){$s.='x';}return$s;}echo build($argc+3);", "xxxx");
}

/// INI materialization and complete copies keep their old identity across in-place mutation.
#[test]
fn test_consuming_string_append_preserves_ini_origin_snapshots() {
    let source = "<?php error_reporting(0);$s=$argc>0?'UTF-8':'ASCII';ini_set('mbstring.internal_encoding',$s);$copy=$s;for($i=0;$i<20;$i++){$s.='x';}echo ini_get('mbstring.internal_encoding'),'|',$copy,'|',mb_strlen($s),'|';$s.='';echo ini_get('mbstring.internal_encoding');";
    check_append(source, "UTF-8|UTF-8|25|UTF-8");
}

/// Recursive builders keep each frame's destination owner independent.
#[test]
fn test_consuming_string_append_recursive_frames() {
    let source = "<?php function buildDepth(int $depth):string{$s='';if($depth>0){$piece=buildDepth($depth-1);$s.=$piece;}$s.='x';return$s;}echo buildDepth($argc+2);";
    check_append(source, "xxxx");
}
