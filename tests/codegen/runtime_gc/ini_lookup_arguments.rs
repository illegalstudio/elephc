//! Purpose:
//! Heap-debug regressions for the key strings passed to the INI lookup wrappers.
//!
//! Called from:
//! - `tests/codegen/runtime_gc.rs` in the codegen integration test suite.
//!
//! Key details:
//! - A caller pins a copy of every string argument whose callee might return it. The
//!   `ini_get()` / `ini_set()` helpers return copies, and they have to be provably so,
//!   or every lookup keeps its key alive for the rest of the program.
//! - Compiled through the CLI, as the mbstring INI tests are: the wrappers are prelude
//!   functions the CLI injects, which the in-process helper does not.

use crate::support::{elephc_cli_command_with_managed_pcre2, make_cli_test_dir};
use std::fs;
use std::process::Command;

/// Looking up and setting directives by variable keys releases every key copy.
#[test]
fn test_ini_lookups_by_variable_key_do_not_leak() {
    let directory = make_cli_test_dir("ini_lookup_arguments");
    let php = directory.join("main.php");
    fs::write(
        &php,
        r#"<?php
function probe(array $names): int {
    $n = 0;
    foreach ($names as $name) {
        $v = ini_get($name);
        if ($v !== false) { $n += strlen($v) + 1; }
    }
    return $n;
}
function run(): void {
    $total = 0;
    for ($i = 0; $i < 30; $i++) {
        $total += probe(["opcache.enable", "mbstring.language", "nope." . $i, "opcache.revalidate_freq", "opcache.file_cache"]);
        ini_set("opcache.revalidate_freq", (string)($i % 3));
    }
    echo $total, "\n";
}
run();
"#,
    )
    .unwrap();
    let compiled = elephc_cli_command_with_managed_pcre2(&directory)
        .arg("--heap-debug")
        .arg(&php)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "compile failed: {}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let output = Command::new(php.with_extension("")).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    fs::remove_dir_all(&directory).unwrap();
    assert_eq!(stdout, "390\n", "stderr: {stderr}");
    assert!(
        stderr.contains("HEAP DEBUG: leak summary: clean"),
        "INI lookup keys leaked: {stderr}"
    );
}
