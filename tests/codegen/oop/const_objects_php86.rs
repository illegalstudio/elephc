//! Purpose:
//! PHP 8.6 "mutating objects stored in constants": an object built by `new` in a constant is
//! created once and every read shares that handle, so property writes through the constant are
//! visible to later reads and identity comparisons hold.
//!
//! Called from:
//! - `cargo test` through the `codegen_tests` binary (`tests/codegen/oop.rs`).
//!
//! Key details:
//! - Covers a class constant, a top-level `const`, and a `define()`d value, plus arrays held by
//!   the constant object and a scalar constant that must keep its old folding.
//! - The expected output is PHP 8.6's for the same source; PHP 8.5 cannot compile the `new`
//!   constant initializers at all.

use crate::support::compile_and_run;

/// A class constant object keeps one identity, and writes through it stick.
#[test]
fn test_class_constant_object_accepts_mutating_writes() {
    let out = compile_and_run(
        r#"<?php
final class Defaults {
    public int $timeout = 10;
    public array $tags = ['a'];
}
class Config { public const DEFAULTS = new Defaults(); }
var_dump(Config::DEFAULTS === Config::DEFAULTS);
Config::DEFAULTS->timeout = 30;
Config::DEFAULTS->tags[] = 'b';
var_dump(Config::DEFAULTS->timeout, count(Config::DEFAULTS->tags));
"#,
    );
    assert_eq!(out, "bool(true)\nint(30)\nint(2)\n");
}

/// Top-level `const` and `define()` objects share identity across reads too.
#[test]
fn test_global_constant_objects_accept_mutating_writes() {
    let out = compile_and_run(
        r#"<?php
final class Defaults { public int $timeout = 10; }
const TOP = new Defaults();
TOP->timeout = 40;
var_dump(TOP === TOP, TOP->timeout);
define('DEFINED', new Defaults());
DEFINED->timeout = 50;
var_dump(DEFINED === DEFINED, DEFINED->timeout);
const SCALAR = 5;
var_dump(SCALAR);
"#,
    );
    assert_eq!(
        out,
        "bool(true)\nint(40)\nbool(true)\nint(50)\nint(5)\n"
    );
}
