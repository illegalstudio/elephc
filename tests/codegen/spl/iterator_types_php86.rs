//! Purpose:
//! Runs PHP 8.6's SPL iterator tentative-return-type fixtures.
//!
//! Called from:
//! - `cargo test --test codegen_tests iterator_types_php86`.
//!
//! Key details:
//! - PHP 8.6 narrows `DirectoryIterator::key()` from `mixed` to `int|string`,
//!   widens `current()` to `string|SplFileInfo|static`, and aligns
//!   `FilesystemIterator` and `SplFileObject` with their php-src stubs.
//! - The union rendering follows PHP's internal member order, so the expected
//!   strings are the canonical reflection order, not the declared one.

use crate::support::compile_and_run;

/// Verifies the 8.6 SPL iterator method return types through reflection.
#[test]
fn test_spl_iterator_return_types_php86() {
    let out = compile_and_run(
        r#"<?php
echo (string) (new ReflectionMethod('DirectoryIterator', 'key'))->getReturnType(), "\n";
echo (string) (new ReflectionMethod('DirectoryIterator', 'current'))->getReturnType(), "\n";
echo (string) (new ReflectionMethod('FilesystemIterator', 'key'))->getReturnType(), "\n";
echo (string) (new ReflectionMethod('FilesystemIterator', 'current'))->getReturnType(), "\n";
echo (string) (new ReflectionMethod('SplFileObject', 'current'))->getReturnType(), "\n";
echo (string) (new ReflectionMethod('SplFileObject', 'key'))->getReturnType(), "\n";
"#,
    );
    assert_eq!(
        out,
        "string|int\nSplFileInfo|static|string\nstring\nSplFileInfo|static|string\narray|string|false\nint\n"
    );
}
