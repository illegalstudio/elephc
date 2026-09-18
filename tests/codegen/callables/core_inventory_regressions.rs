//! Purpose:
//! Pins inventory visibility and snapshot ownership across native and opaque eval code.
//!
//! Called from:
//! - `cargo test --test codegen_tests test_core_inventory_regression_`.
//!
//! Key details:
//! - Runtime-dependent eval source prevents compile-time declaration discovery.

use crate::support::*;

/// Eval resource type registration and close events are visible to both inventory consumers.
#[test]
fn test_core_inventory_regression_eval_resource_close_and_filter_type() {
    let out = compile_and_run(r#"<?php
$source = '$stream = fopen("php://memory", "w+");
$filter = stream_filter_append($stream, "string.toupper", STREAM_FILTER_WRITE);
echo count(get_resources("stream filter")), ":";
stream_filter_remove($filter);
fclose($stream);
echo count(get_resources("stream")), ":";' . ' // ' . $argc;
eval($source);
echo count(get_resources('stream')), ':', count(get_resources('stream filter'));
"#);
    assert_eq!(out, "1:3:3:0");
}

/// Flat and categorized constant snapshots preserve nested arrays and independent mutation.
#[test]
fn test_core_inventory_regression_array_constants() {
    let out = compile_and_run(r#"<?php
const INVENTORY_ITEMS = [1, 2];
const INVENTORY_NESTED = ['items' => [3, 'four'], 'nullable' => null, '42' => true];
$flat = get_defined_constants();
$categorized = get_defined_constants(true);
echo $flat['INVENTORY_ITEMS'][1], ':', $categorized['user']['INVENTORY_NESTED']['items'][1];
echo ':', $flat['INVENTORY_NESTED'][42] ? 'true' : 'bad';
$flat['INVENTORY_ITEMS'][1] = 9;
echo ':', $categorized['user']['INVENTORY_ITEMS'][1], ':', INVENTORY_ITEMS[1];
unset($flat);
echo ':', $categorized['user']['INVENTORY_NESTED']['items'][0];
"#);
    assert_eq!(out, "2:four:true:2:2:3");
}

/// Native flat/categorized inventories observe constants and functions declared by eval.
#[test]
fn test_core_inventory_regression_eval_declarations_visible_to_aot() {
    let out = compile_and_run(r#"<?php
$before = get_defined_constants();
$source = 'define("INVENTORY_LIVE", 42); function inventory_live_function(): int { return 7; }' . ' // ' . $argc;
eval($source);
$flat = get_defined_constants();
$categorized = get_defined_constants(true);
$functions = get_defined_functions();
echo isset($before['INVENTORY_LIVE']) ? 'bad' : 'absent';
echo ':', $flat['INVENTORY_LIVE'], ':', $categorized['user']['INVENTORY_LIVE'];
echo ':', in_array('inventory_live_function', $functions['user']) ? 'yes' : 'bad';
unset($flat);
echo ':', $categorized['user']['INVENTORY_LIVE'];
"#);
    assert_eq!(out, "absent:42:42:yes:42");
}

/// Eval sees native streams and a returned inventory alias shares native close state.
#[test]
fn test_core_inventory_regression_resources_shared_with_eval() {
    let out = compile_and_run(r#"<?php
$file = fopen('/dev/null', 'r');
$id = get_resource_id($file);
echo count(get_resources('stream')), ':';
$source = '$resources = get_resources("stream"); echo count($resources), ":";' . ' // ' . $argc;
eval($source);
unset($file);
echo count(get_resources('stream')), ':';
$alias = $resources[$id];
fclose($alias);
echo count(get_resources('stream'));
"#);
    assert_eq!(out, "4:4:4:3");
}

/// Opaque eval fragments read AOT user-declared scalar constants under PHP's name rules.
#[test]
fn test_core_inventory_regression_eval_sees_aot_user_scalar_constants() {
    let out = compile_and_run(r#"<?php
const USER_INT = -7;
const USER_TEXT = 'seeded';
const USER_FLOAT = -1.5;
const USER_NULL = null;
const USER_FLAG = true;
$source = 'echo USER_INT, ":", USER_TEXT, ":", USER_FLOAT, ":";
echo USER_NULL === null ? "null" : "bad", ":";
echo USER_FLAG ? "flag" : "bad", ":";
echo \USER_INT, ":", constant("USER_TEXT"), ":";
echo defined("USER_INT") ? "yes" : "bad", ":";
echo defined("user_int") ? "bad" : "case", ":";
echo defined("USER_MISSING") ? "bad" : "absent";' . ' // ' . $argc;
eval($source);
"#);
    assert_eq!(out, "-7:seeded:-1.5:null:flag:-7:seeded:yes:case:absent");
}

/// Opaque eval fragments read nested AOT user array constants and get an independent copy.
#[test]
fn test_core_inventory_regression_eval_sees_aot_user_array_constants() {
    let out = compile_and_run(r#"<?php
const USER_LIST = [1, 2];
const USER_TABLE = ['items' => [3, 'four'], 'nullable' => null, '42' => true];
$source = 'echo USER_LIST[1], ":", USER_TABLE["items"][1], ":";
echo USER_TABLE[42] ? "true" : "bad", ":";
$copy = USER_LIST;
$copy[1] = 9;
echo $copy[1], ":", USER_LIST[1], ":", count(USER_TABLE);' . ' // ' . $argc;
eval($source);
echo ':', USER_LIST[1];
"#);
    assert_eq!(out, "2:four:true:9:2:3:2");
}

/// Eval reports seeded AOT constants under `user`, keeps `Core` intact, and locks redefinition.
///
/// `PHP_INT_SIZE` and `STDOUT` are the `Core` controls: the second one also pins that a
/// resource-typed predefined constant stays a live resource handle inside eval, and that
/// resource-typed names are reported under `Core`, never under the seeded `user` category.
#[test]
fn test_core_inventory_regression_eval_constant_categories() {
    let out = compile_and_run_capture(r#"<?php
const USER_ONE = 1;
$source = 'define("EVAL_TWO", 2);
$flat = get_defined_constants();
$categorized = get_defined_constants(true);
echo $flat["USER_ONE"], ":", $categorized["user"]["USER_ONE"], ":";
echo isset($categorized["Core"]["USER_ONE"]) ? "bad" : "clean", ":";
echo $categorized["user"]["EVAL_TWO"], ":", $categorized["Core"]["PHP_INT_SIZE"], ":";
echo get_resource_type($categorized["Core"]["STDOUT"]), ":";
echo define("USER_ONE", 5) ? "bad" : "locked";' . ' // ' . $argc;
eval($source);
$after = get_defined_constants(true);
echo ':', $after['user']['USER_ONE'], ':', count($after['user']);
"#);
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "1:1:clean:2:8:stream:locked:1:2");
    assert!(
        out.stderr
            .contains("Warning: define(): Constant already defined"),
        "expected duplicate seeded-constant warning, got stderr={}",
        out.stderr
    );
}

/// Opaque eval fragments resolve namespaced AOT user constants by their canonical name.
///
/// The seeded registry key is the constant's fully qualified name without a leading `\`
/// (`Demo\Values\ANSWER`), so a fragment reaches it through a fully qualified fetch or
/// through `constant()` / `defined()` with the same qualified name. The unqualified short
/// name must stay undefined, because the fragment itself runs in the global namespace.
#[test]
fn test_core_inventory_regression_eval_sees_namespaced_aot_user_constants() {
    let out = compile_and_run(r#"<?php
namespace Demo\Values;

const ANSWER = 42;
const TABLE = ['left' => 'L', 'right' => 'R'];

namespace Demo\App;

$source = 'echo \\Demo\\Values\\ANSWER, ":";
$table = \\Demo\\Values\\TABLE;
echo $table["left"], $table["right"], ":";
echo constant(\'Demo\\Values\\ANSWER\'), ":";
echo defined(\'Demo\\Values\\ANSWER\') ? "yes" : "bad", ":";
echo defined(\'\\Demo\\Values\\ANSWER\') ? "leading" : "bad", ":";
echo defined(\'ANSWER\') ? "bad" : "unqualified";' . ' // ' . $argc;
eval($source);
echo ':', \Demo\Values\ANSWER;
"#);
    assert_eq!(out, "42:LR:42:yes:leading:unqualified:42");
}

/// Repeated seeded-constant reads, copies, and inventories inside eval leave the heap clean.
///
/// Each iteration fetches both seeded array constants, mutates one copy (so copy-on-write
/// has to detach it from the seeded metadata), and builds both the flat and the categorized
/// inventories. `count($user)` stays at the two seeded names in every iteration and after
/// the fragment returns, which is what proves a seeded name is published exactly once
/// rather than re-appended per eval dispatch or per inventory call.
#[test]
fn test_core_inventory_regression_eval_user_constant_reads_are_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
const HEAP_LIST = [1, 2, 3];
const HEAP_TABLE = ['items' => [4, 5], 'name' => 'seeded'];
$source = 'for ($i = 0; $i < 5; $i++) {
    $copy = HEAP_LIST;
    $copy[1] = $i;
    $table = HEAP_TABLE;
    $flat = get_defined_constants();
    $categorized = get_defined_constants(true);
    $user = $categorized["user"];
    echo $copy[1], ":", $copy[2], ":", count($table["items"]), ":";
    echo count($user), ":", $flat["HEAP_TABLE"]["name"], ";";
}' . ' // ' . $argc;
eval($source);
$after = get_defined_constants(true);
$again = get_defined_constants(true);
echo count($after['user']), ':', HEAP_LIST[1], ':', count($again['user']);
"#);
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "0:3:2:2:seeded;1:3:2:2:seeded;2:3:2:2:seeded;3:3:2:2:seeded;4:3:2:2:seeded;2:2:2",
        "stderr: {}",
        out.stderr
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got stderr={}",
        out.stderr
    );
}
