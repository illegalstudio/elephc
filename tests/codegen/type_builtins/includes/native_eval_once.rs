//! Include-once state across compiled and interpreted execution.

use super::*;

fn run_cross_engine_once(main: &str, included: &str) -> String {
    compile_cli_files_and_run(&[("main.php", main), ("module.php", included)], "main.php")
}

#[test]
fn test_native_include_once_is_visible_to_eval() {
    let out = run_cross_engine_once(r#"<?php
require_once __DIR__.'/module.php';
$path = __DIR__.'/module.php';
eval('include_once $path;');
echo 'done';
"#, "<?php echo 'load|';");
    assert_eq!(out, "load|done");
}

#[test]
fn test_eval_include_once_is_visible_to_native() {
    let out = run_cross_engine_once(r#"<?php
$path = __DIR__.'/module.php';
eval('include_once $path;');
require_once __DIR__.'/module.php';
echo 'done';
"#, "<?php echo 'load|';");
    assert_eq!(out, "load|done");
}

#[test]
fn test_native_interface_include_once_is_not_redeclared_by_eval() {
    let out = run_cross_engine_once(r#"<?php
require_once __DIR__.'/module.php';
$path = __DIR__.'/module.php';
eval('include_once $path;');
echo 'done';
"#, "<?php interface CrossEngineProtocol {} echo 'load|';");
    assert_eq!(out, "load|done");
}

#[test]
fn test_skipped_native_interface_include_can_first_execute_in_eval() {
    let out = run_cross_engine_once(r#"<?php
if ($argc < 0) { require_once __DIR__.'/module.php'; }
$path = __DIR__.'/module.php';
eval('include_once $path;');
echo 'done';
"#, "<?php interface InitiallyUnloadedProtocol {} echo 'load|';");
    assert_eq!(out, "load|done");
}
