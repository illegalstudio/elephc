//! Purpose:
//! Checks delayed eval metadata, its eager fallback and activation-local state.
//!
//! Called from:
//! - Focused `deferred_native_catalog` codegen tests on executable supported targets.
//!
//! Key details:
//! - Every fixture uses opaque runtime source and runs both IR modes with heap diagnostics.
//! - The optional PHP oracle checks real numeric, warning, reference and destructor behavior.
//! - A later metadata-dependent fragment must upgrade the existing context before execution.

use crate::support::*;

#[path = "eval_deferred_catalog/probe.rs"]
mod probe;

/// Removes only one test's temporary source, executable and cache on any exit.
struct FixtureDir(std::path::PathBuf);

impl Drop for FixtureDir {
    /// Removes this fixture even when a semantic or heap assertion fails.
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

/// Checks PHP-visible output and bounds only a separately reproduced baseline retention.
fn check_deferred(source: &str, expected: &str, max_live_bytes: usize) {
    for mode in ["--ir-opt=off", "--ir-opt=on"] {
        let dir = FixtureDir(make_cli_test_dir("elephc_deferred_catalog"));
        let path = dir.0.join("main.php");
        fs::write(&path, source).unwrap();
        let compile = elephc_cli_command(&dir.0).args([mode, "--php-version=8.5", "--heap-debug"])
            .arg(&path).output().expect("compile deferred catalog fixture");
        assert!(compile.status.success(), "{mode}: {}", String::from_utf8_lossy(&compile.stderr));
        let output = run_binary(&dir.0.join("main"), &dir.0);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{mode}: {stderr}");
        assert_eq!(output.stdout, expected.as_bytes(), "{mode}: {stderr}");
        if max_live_bytes == 0 {
            assert!(stderr.contains("HEAP DEBUG: leak summary: clean"), "{mode}: {stderr}");
        } else {
            let bytes: usize = stderr.lines().filter_map(|line| line.strip_prefix("HEAP DEBUG: allocs="))
                .flat_map(str::split_whitespace).find_map(|field| field.strip_prefix("live_bytes="))
                .expect("heap report").parse().unwrap();
            assert!(bytes <= max_live_bytes, "{mode}: {stderr}");
        }
        if std::env::var_os("ELEPHC_PHP_CHECK").is_some() {
            let php = Command::new("php").arg("-n").arg(&path).output().expect("run PHP oracle");
            assert!(php.status.success(), "{}", String::from_utf8_lossy(&php.stderr));
            assert_eq!(output.stdout, php.stdout, "PHP mismatch in {mode}");
        }
    }
}

/// Numeric-only eval keeps new function activations independent.
#[test]
fn test_deferred_native_catalog_numeric_activations() {
    check_deferred(r#"<?php
function run(int $n) {global $code; $sum=0;for($i=0;$i<$n;$i++){eval($code);}return $sum; }
$code=str_repeat('$sum=$sum+3;', $argc);
var_dump(run(1),run(100),run(511),run(512),run(513),run(1000));
"#, r#"int(3)
int(300)
int(1533)
int(1536)
int(1539)
int(3000)
"#, 0);
}

/// A later native call and object construction see the complete catalog.
#[test]
fn test_deferred_native_catalog_upgrades_before_native_calls_and_classes() {
    check_deferred(r#"<?php
class NativeBox { public int $value=7; }
function nativeAdd($value) { return $value+11; }
function run() {global $first,$second;$sum=0;eval($first);eval($second);return $sum;}
$first=str_repeat('$sum+=3;', $argc);
$second=str_repeat('$sum=nativeAdd($sum);$box=new NativeBox();$sum+=$box->value;', $argc);
var_dump(run());
"#, r#"int(21)
"#, 0);
}

/// An existing reference binding keeps its normal scope writeback.
#[test]
fn test_deferred_native_catalog_reference_fallback() {
    check_deferred(r#"<?php
$sum=(int)$argc;$alias=&$sum;$code=str_repeat('$sum+=3;', $argc);eval($code);var_dump($sum,$alias);
"#, r#"int(4)
int(4)
"#, 0);
}

/// Replacing an object requests metadata before any destructor can run.
#[test]
fn test_deferred_native_catalog_destructor_fallback() {
    check_deferred(r#"<?php
class Victim { public function __destruct() {echo "destroyed;";} }
$sum=new Victim();$code=str_repeat('$sum=3;', $argc);eval($code);var_dump($sum);
"#, r#"destroyed;int(3)
"#, 0);
}

/// Numeric overflow remains float across the transition to the complete catalog.
#[test]
fn test_deferred_native_catalog_numeric_overflow() {
    check_deferred(r#"<?php
$sum=PHP_INT_MAX;$code=str_repeat('$sum=$sum+3;', $argc);eval($code);eval($code);
for($i=0;$i<512;$i++){eval($code);}var_dump($sum);
"#, r#"float(9.223372036854776E+18)
"#, 0);
}

/// Undefined reads use the full catalog before the installed warning handler runs.
/// Frozen main0842 retains one 24-byte source string in this unchanged handler path.
#[test]
fn test_deferred_native_catalog_warning_handler_fallback() {
    check_deferred(r#"<?php
function run(){global $first,$second;$sum=0;eval($first);eval($second);restore_error_handler();return $sum;}
$first=str_repeat('$sum+=3;', $argc);
$second=str_repeat('set_error_handler(function($errno,$message){echo "warning;";return true;});$sum=$missing+2;', $argc);
var_dump(run());
"#, r#"warning;int(2)
"#, 24);
}
