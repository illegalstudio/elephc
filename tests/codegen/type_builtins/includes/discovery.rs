//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of type-related builtins, includes discovery, including include declaration discovery inside function, include graph declaration discovery inside function, class aliases from includes, and discovered function body resolves nested include.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Multi-file fixtures exercise include/require resolution, temporary project layout, and native binary output.

use super::*;

/// Verifies `require` inside a function triggers declaration discovery before the function is called.
#[test]
fn test_include_declaration_discovery_inside_function() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
function load_lib() {
    require 'lib.php';
}

echo "before";
load_lib();
echo later();
"#,
            ),
            (
                "lib.php",
                r#"<?php
echo "load";

function later() {
    return "after";
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "beforeloadafter");
}

/// Verifies a chain of requires (main → a.php → b.php) allows calling a function declared in the leaf file.
#[test]
fn test_include_graph_declaration_discovery_inside_function() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
function load_graph() {
    require 'a.php';
}

load_graph();
echo deep();
"#,
            ),
            ("a.php", "<?php require 'b.php';"),
            ("b.php", "<?php function deep() { return \"deep\"; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "deep");
}

/// Verifies a function loaded via require can itself contain a nested require; the inner function is callable.
#[test]
fn test_discovered_function_body_resolves_nested_include() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
function load_lib() {
    require 'lib.php';
}

load_lib();
echo from_lib();
"#,
            ),
            (
                "lib.php",
                r#"<?php
function from_lib() {
    require 'inner.php';
    return inner_value();
}
"#,
            ),
            ("inner.php", "<?php function inner_value() { return \"inner\"; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "inner");
}

/// Verifies `require` inside a function discovers interface, trait, and class declarations; polymorphism via trait and interface works.
#[test]
fn test_include_declaration_discovery_for_class_interface_and_trait() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
function load_types() {
    require 'types.php';
}

load_types();
$box = new Box();
echo $box->label();
"#,
            ),
            (
                "types.php",
                r#"<?php
interface Labelled {
    public function label(): string;
}

trait LabelTrait {
    public function label(): string {
        return "boxed";
    }
}

class Box implements Labelled {
    use LabelTrait;
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "boxed");
}

/// Verifies `require_once` triggers class alias registration; `class_exists()` and `new` resolve the alias.
#[test]
fn test_require_once_discovers_top_level_class_alias() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
require_once __DIR__ . "/class_alias_lib.php";
echo class_exists("AliasInsideInclude") ? "yes" : "no";
echo "\n";
$x = new AliasInsideInclude();
echo $x->ok();
echo "\n";
"#,
            ),
            (
                "class_alias_lib.php",
                r#"<?php
class OriginalInsideInclude {
    public function ok() {
        return "ok";
    }
}

class_alias("OriginalInsideInclude", "AliasInsideInclude");
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "yes\nok\n");
}

/// Verifies declarations inside a namespace block loaded via require do not leak into the caller's namespace.
#[test]
fn test_discovered_namespaced_declarations_do_not_leak_to_caller() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
function load_lib() {
    require 'lib.php';
}

load_lib();

class Root {}

echo Root::class;
echo '|';
echo \Lib\label();
"#,
            ),
            (
                "lib.php",
                r#"<?php
namespace Lib;

function label() {
    return 'lib';
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "Root|lib");
}

/// Verifies `use` imports inside a file loaded via require do not leak into the caller's scope.
#[test]
fn test_discovered_use_imports_do_not_leak_to_caller() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
function load_lib() {
    require 'lib.php';
}

load_lib();

class Alias {}

echo Alias::class;
echo '|';
echo imported_alias_name();
"#,
            ),
            (
                "lib.php",
                r#"<?php
use Vendor\Thing as Alias;

function imported_alias_name() {
    return Alias::class;
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "Alias|Vendor\\Thing");
}

/// Verifies two separate include loads maintain independent namespace scopes; no cross-contamination.
#[test]
fn test_discovered_namespaces_do_not_leak_between_included_files() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
function load_a() {
    require 'a.php';
}

function load_b() {
    require 'b.php';
}

load_a();
load_b();

echo \Lib\a();
echo '|';
echo b();
"#,
            ),
            (
                "a.php",
                r#"<?php
namespace Lib;

function a() {
    return 'a';
}
"#,
            ),
            (
                "b.php",
                r#"<?php
function b() {
    return 'b';
}
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "a|b");
}

/// Verifies re-including the same file via regular `include` reports a duplicate declaration error.
#[test]
fn test_regular_reinclude_still_reports_duplicate_declaration() {
    assert!(compile_files_fails(
        &[
            (
                "main.php",
                r#"<?php
include 'lib.php';
include 'lib.php';
"#,
            ),
            ("lib.php", "<?php function duplicated() { return 1; }"),
        ],
        "main.php",
    ));
}

/// Verifies mutually exclusive includes (runtime branch) discover declarations once (deterministic branch coverage).
#[test]
fn test_regular_include_same_file_in_exclusive_branches_discovers_once() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$pick = time() > 0;
if ($pick) {
    include 'lib.php';
} else {
    include 'lib.php';
}
echo branch_value();
"#,
            ),
            ("lib.php", "<?php function branch_value() { return 'ok'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}
