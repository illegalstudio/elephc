//! Purpose:
//! PHP 8.6 Phar fixtures: `Phar::mungServer()` validation and `Phar::addEmptyDir()`'s
//! magic `.phar` path rules.
//!
//! Called from:
//! - `cargo test --test codegen_tests phar_php86`.
//!
//! Key details:
//! - The fixtures catch the exact exception class and echo its message, so the
//!   validation row is asserted without relying on stderr formatting.
//! - A reference element is built with `$ref = &$refs[0]`, since array literals
//!   cannot spell reference elements yet.
//! - Each case is a direct `try`/`catch` block rather than a closure-taking helper,
//!   keeping the fixture independent of the closure string-context lowering.

use elephc::php_version::PhpVersion;

use crate::support::*;

/// Compiles and runs one fixture with the PHP 8.6 profile.
fn run86(source: &str) -> String {
    compile_and_run_with_php_version(source, PhpVersion::Php86)
}

/// `addEmptyDir()` rejects the magic `.phar` directory forms and accepts ordinary names.
#[test]
fn test_phar_add_empty_dir_magic_paths_php86() {
    let out = run86(
        r#"<?php
$p = new Phar("rules.phar");
try {
    $p->addEmptyDir(".phar");
    echo "ok\n";
} catch (Throwable $e) {
    echo get_class($e), ":", $e->getMessage(), "\n";
}
try {
    $p->addEmptyDir("/.phar");
    echo "ok\n";
} catch (Throwable $e) {
    echo get_class($e), ":", $e->getMessage(), "\n";
}
try {
    $p->addEmptyDir(".phar/sub");
    echo "ok\n";
} catch (Throwable $e) {
    echo get_class($e), ":", $e->getMessage(), "\n";
}
try {
    $p->addEmptyDir(".pharma");
    echo "ok\n";
} catch (Throwable $e) {
    echo get_class($e), ":", $e->getMessage(), "\n";
}
try {
    $p->addEmptyDir("dir");
    echo "ok\n";
} catch (Throwable $e) {
    echo get_class($e), ":", $e->getMessage(), "\n";
}
$q = new Phar("rules.phar");
foreach ($q as $name => $info) {
    echo $name, "|";
}
"#,
    );
    assert_eq!(
        out,
        "BadMethodCallException:Cannot create a directory in magic \".phar\" directory\nBadMethodCallException:Cannot create a directory in magic \".phar\" directory\nBadMethodCallException:Cannot create a directory in magic \".phar\" directory\nok\nok\n.pharma/|dir/|"
    );
}

/// The four `mungServer()` validation failures and the accepted names, including a
/// reference element (which PHP dereferences before validating).
#[test]
fn test_phar_mung_server_validation_php86() {
    let out = run86(
        r#"<?php
try {
    Phar::mungServer([]);
    echo "ok\n";
} catch (Throwable $e) {
    echo get_class($e), ":", $e->getMessage(), "\n";
}
try {
    Phar::mungServer(["PHP_SELF", "REQUEST_URI", "SCRIPT_NAME", "SCRIPT_FILENAME", "PHP_SELF"]);
    echo "ok\n";
} catch (Throwable $e) {
    echo get_class($e), ":", $e->getMessage(), "\n";
}
try {
    Phar::mungServer([42]);
    echo "ok\n";
} catch (Throwable $e) {
    echo get_class($e), ":", $e->getMessage(), "\n";
}
try {
    Phar::mungServer(["NOPE"]);
    echo "ok\n";
} catch (Throwable $e) {
    echo get_class($e), ":", $e->getMessage(), "\n";
}
try {
    Phar::mungServer(["PHP_SELF", "SCRIPT_FILENAME"]);
    echo "ok\n";
} catch (Throwable $e) {
    echo get_class($e), ":", $e->getMessage(), "\n";
}
$refs = ["REQUEST_URI"];
$ref = &$refs[0];
try {
    Phar::mungServer($refs);
    echo "ok\n";
} catch (Throwable $e) {
    echo get_class($e), ":", $e->getMessage(), "\n";
}
"#,
    );
    let suffix = "passed to Phar::mungServer(), expecting an array of any of these strings: PHP_SELF, REQUEST_URI, SCRIPT_FILENAME, SCRIPT_NAME";
    assert_eq!(
        out,
        format!(
            "PharException:No values {suffix}\nPharException:Too many values {suffix}\nPharException:Non-string value {suffix}\nPharException:Invalid value {suffix}\nok\nok\n"
        )
    );
}
