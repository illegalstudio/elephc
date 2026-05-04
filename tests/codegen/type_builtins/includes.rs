use super::*;

#[test]
fn test_include_basic() {
    let out = compile_and_run_files(
        &[
            ("main.php", "<?php include 'helper.php'; echo greet();"),
            ("helper.php", "<?php function greet() { return \"hello\"; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "hello");
}

#[test]
fn test_require_basic() {
    let out = compile_and_run_files(
        &[
            ("main.php", "<?php require 'math.php'; echo add(3, 4);"),
            ("math.php", "<?php function add($a, $b) { return $a + $b; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "7");
}

#[test]
fn test_include_with_parens() {
    let out = compile_and_run_files(
        &[
            ("main.php", "<?php include('helper.php'); echo greet();"),
            ("helper.php", "<?php function greet() { return \"hi\"; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "hi");
}

#[test]
fn test_include_top_level_code() {
    // Top-level code in included file executes at the include point
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php echo \"before\"; include 'mid.php'; echo \"after\";",
            ),
            ("mid.php", "<?php echo \"middle\";"),
        ],
        "main.php",
    );
    assert_eq!(out, "beforemiddleafter");
}

#[test]
fn test_include_once() {
    // include_once should only include the file once
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
include_once 'counter.php';
include_once 'counter.php';
echo $x;
"#,
            ),
            ("counter.php", "<?php $x = 42;"),
        ],
        "main.php",
    );
    assert_eq!(out, "42");
}

#[test]
fn test_require_once() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
require_once 'lib.php';
require_once 'lib.php';
echo double(5);
"#,
            ),
            ("lib.php", "<?php function double($n) { return $n * 2; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "10");
}

#[test]
fn test_include_once_skipped_branch_does_not_claim_file() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
if (false) {
    include_once 'piece.php';
}
include_once 'piece.php';
"#,
            ),
            ("piece.php", "<?php echo \"piece\";"),
        ],
        "main.php",
    );
    assert_eq!(out, "piece");
}

#[test]
fn test_include_once_in_loop_executes_file_once() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$i = 0;
while ($i < 3) {
    include_once 'tick.php';
    $i = $i + 1;
}
"#,
            ),
            ("tick.php", "<?php echo \"tick\";"),
        ],
        "main.php",
    );
    assert_eq!(out, "tick");
}

#[test]
fn test_require_once_in_function_is_global_once() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
function load_piece() {
    require_once 'piece.php';
}
load_piece();
load_piece();
"#,
            ),
            ("piece.php", "<?php echo \"piece\";"),
        ],
        "main.php",
    );
    assert_eq!(out, "piece");
}

#[test]
fn test_require_once_in_method_is_global_once() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
class Loader {
    public function load() {
        require_once 'piece.php';
    }
}
$loader = new Loader();
$loader->load();
$loader->load();
"#,
            ),
            ("piece.php", "<?php echo \"piece\";"),
        ],
        "main.php",
    );
    assert_eq!(out, "piece");
}

#[test]
fn test_require_once_in_closure_is_global_once() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$load = function() {
    require_once 'piece.php';
};
$load();
$load();
"#,
            ),
            ("piece.php", "<?php echo \"piece\";"),
        ],
        "main.php",
    );
    assert_eq!(out, "piece");
}

#[test]
fn test_regular_include_in_closure_marks_later_include_once() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$load = function() {
    include 'piece.php';
};
$load();
include_once 'piece.php';
"#,
            ),
            ("piece.php", "<?php echo \"piece\";"),
        ],
        "main.php",
    );
    assert_eq!(out, "piece");
}

#[test]
fn test_regular_include_marks_later_include_once_declarations() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
include 'lib.php';
include_once 'lib.php';
echo seven();
"#,
            ),
            ("lib.php", "<?php function seven() { return 7; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "7");
}

#[test]
fn test_skipped_regular_include_does_not_make_include_once_skip() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
if (false) {
    include 'piece.php';
}
include_once 'piece.php';
"#,
            ),
            ("piece.php", "<?php echo \"piece\";"),
        ],
        "main.php",
    );
    assert_eq!(out, "piece");
}

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

#[test]
fn test_regular_include_in_constant_false_branch_does_not_duplicate_later_include() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
if (false) {
    include 'lib.php';
}
include 'lib.php';
echo false_branch_value();
"#,
            ),
            ("lib.php", "<?php function false_branch_value() { return 'ok'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_regular_include_in_constant_false_elseif_chain_does_not_duplicate_later_include() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
if (false) {
    include 'lib.php';
} elseif (false) {
    include 'lib.php';
}
include 'lib.php';
echo false_elseif_value();
"#,
            ),
            ("lib.php", "<?php function false_elseif_value() { return 'ok'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_regular_include_possible_branch_then_later_include_still_reports_duplicate() {
    assert!(compile_files_fails(
        &[
            (
                "main.php",
                r#"<?php
if (time() > 0) {
    include 'lib.php';
}
include 'lib.php';
"#,
            ),
            ("lib.php", "<?php function maybe_duplicated() { return 1; }"),
        ],
        "main.php",
    ));
}

#[test]
fn test_regular_include_declaration_in_loop_reports_duplicate() {
    assert!(compile_files_fails(
        &[
            (
                "main.php",
                r#"<?php
$i = 0;
while ($i < 2) {
    include 'lib.php';
    $i = $i + 1;
}
"#,
            ),
            ("lib.php", "<?php function loop_duplicated() { return 1; }"),
        ],
        "main.php",
    ));
}

#[test]
fn test_include_once_in_loop_with_nested_regular_include_discovers_once() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$i = 0;
while ($i < 2) {
    include_once 'outer.php';
    $i = $i + 1;
}
echo nested_once_value();
"#,
            ),
            ("outer.php", "<?php include 'inner.php';"),
            ("inner.php", "<?php function nested_once_value() { return 'ok'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_include_once_possible_branch_then_later_include_once_discovers_once() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
if (time() < 0) {
    include_once 'lib.php';
}
include_once 'lib.php';
echo once_later_value();
"#,
            ),
            ("lib.php", "<?php function once_later_value() { return 'ok'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "ok");
}

#[test]
fn test_include_once_exclusive_branches_scan_context_sensitive_nested_includes() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$pick = time() < 0;
if ($pick) {
    define('TARGET_FILE', 'a.php');
    include_once 'outer.php';
    echo branch_a_value();
} else {
    define('TARGET_FILE', 'b.php');
    include_once 'outer.php';
    echo branch_b_value();
}
"#,
            ),
            ("outer.php", "<?php include TARGET_FILE;"),
            ("a.php", "<?php function branch_a_value() { return 'a'; }"),
            ("b.php", "<?php function branch_b_value() { return 'b'; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "b");
}

#[test]
fn test_include_nested() {
    // a.php includes b.php which includes c.php
    let out = compile_and_run_files(
        &[
            ("main.php", "<?php include 'a.php'; echo c_func();"),
            ("a.php", "<?php include 'b.php';"),
            ("b.php", "<?php include 'c.php';"),
            ("c.php", "<?php function c_func() { return \"deep\"; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "deep");
}

#[test]
fn test_include_subdirectory() {
    let out = compile_and_run_files(
        &[
            ("main.php", "<?php include 'lib/utils.php'; echo greet();"),
            (
                "lib/utils.php",
                "<?php function greet() { return \"from lib\"; }",
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "from lib");
}

#[test]
fn test_include_variables_shared_scope() {
    // Variables from included file are in the same scope
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
$prefix = "Hello";
include 'greet.php';
"#,
            ),
            ("greet.php", "<?php echo $prefix . \" World\";"),
        ],
        "main.php",
    );
    assert_eq!(out, "Hello World");
}

#[test]
fn test_include_multiple_files() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                r#"<?php
include 'a.php';
include 'b.php';
echo add(1, 2) . " " . mul(3, 4);
"#,
            ),
            ("a.php", "<?php function add($x, $y) { return $x + $y; }"),
            ("b.php", "<?php function mul($x, $y) { return $x * $y; }"),
        ],
        "main.php",
    );
    assert_eq!(out, "3 12");
}

#[test]
fn test_circular_include_error() {
    assert!(compile_files_fails(
        &[
            ("main.php", "<?php include 'a.php';"),
            ("a.php", "<?php include 'b.php';"),
            ("b.php", "<?php include 'a.php';"),
        ],
        "main.php"
    ));
}

#[test]
fn test_require_missing_file_error() {
    assert!(compile_files_fails(
        &[("main.php", "<?php require 'nonexistent.php';"),],
        "main.php"
    ));
}

// --- Division returns float ---
