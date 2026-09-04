//! Purpose:
//! Regression tests for registry-backed stream-context identity, per-handle state,
//! copy-on-write ownership, and exact cleanup of retained children.
//!
//! Called from:
//! - `cargo test --test codegen_tests test_stream_context_registry_` through Rust's test harness.
//!
//! Key details:
//! - Expected output is pinned to the local PHP 8.5.6 CLI oracle.
//! - Context options, params, and notification callbacks must never flow through
//!   another context merely because a legacy runtime scratch slot was reused.

use crate::support::*;

/// Verifies independent contexts retain and mutate only their own option trees.
#[test]
fn test_stream_context_registry_options_are_isolated_by_handle() {
    let out = compile_and_run(
        r#"<?php
$a = stream_context_create([
    "http" => ["method" => "POST", "header" => "X-A: 1"],
]);
$b = stream_context_create([
    "ssl" => ["peer_name" => "example.test"],
]);

stream_context_set_option($a, "http", "method", "PUT");
$aOptions = stream_context_get_options($a);
$bOptions = stream_context_get_options($b);

echo $aOptions["http"]["method"], "|";
echo $aOptions["http"]["header"], "|";
echo $bOptions["ssl"]["peer_name"], "|";
echo isset($bOptions["http"]) ? "leak" : "isolated";
"#,
    );
    assert_eq!(out, "PUT|X-A: 1|example.test|isolated");
}

/// Verifies a newly created empty context does not inherit the previous context's options.
#[test]
fn test_stream_context_registry_empty_context_does_not_inherit_options() {
    let out = compile_and_run(
        r#"<?php
$configured = stream_context_create([
    "http" => ["method" => "POST"],
]);
$empty = stream_context_create();

echo count(stream_context_get_options($configured)), "|";
echo count(stream_context_get_options($empty));
"#,
    );
    assert_eq!(out, "1|0");
}

/// Verifies `get_params` returns each handle's exact notifier and options entries.
#[test]
fn test_stream_context_registry_get_params_is_exact_and_handle_local() {
    let out = compile_and_run(
        r#"<?php
$notifyA = function ($code) { return "A" . $code; };
$notifyB = function ($code) { return "B" . $code; };
$a = stream_context_create(
    ["http" => ["method" => "POST"]],
    ["notification" => $notifyA],
);
$b = stream_context_create(
    [],
    ["notification" => $notifyB],
);

$aParams = stream_context_get_params($a);
$bParams = stream_context_get_params($b);

echo implode(",", array_keys($aParams)), "|";
echo $aParams["options"]["http"]["method"], "|";
echo $aParams["notification"] === $notifyA ? "A" : "wrong", "|";
echo $bParams["notification"] === $notifyB ? "B" : "wrong", "|";
echo $aParams["notification"] === $bParams["notification"] ? "leak" : "isolated";
"#,
    );
    assert_eq!(out, "notification,options|POST|A|B|isolated");
}

/// Verifies `set_params` replaces params only on the explicitly addressed context.
#[test]
fn test_stream_context_registry_set_params_updates_only_target_handle() {
    let out = compile_and_run(
        r#"<?php
$notifyA = function ($code) { return "A" . $code; };
$a = stream_context_create();
$b = stream_context_create();

stream_context_set_params($a, ["notification" => $notifyA]);
$aParams = stream_context_get_params($a);
$bParams = stream_context_get_params($b);

echo isset($aParams["notification"]) ? "set" : "missing", "|";
echo $aParams["notification"] === $notifyA ? "same" : "wrong", "|";
echo isset($bParams["notification"]) ? "leak" : "isolated", "|";
echo implode(",", array_keys($aParams)), "|";
echo implode(",", array_keys($bParams));
"#,
    );
    assert_eq!(out, "set|same|isolated|notification,options|options");
}

/// Verifies input and returned option hashes stay copy-on-write independent from context state.
#[test]
fn test_stream_context_registry_options_preserve_copy_on_write_boundaries() {
    let out = compile_and_run(
        r#"<?php
$options = ["http" => ["method" => "POST"]];
$context = stream_context_create($options);

$options["http"] = ["method" => "PATCH"];
$snapshot = stream_context_get_options($context);
$snapshot["http"] = ["method" => "LOCAL"];
stream_context_set_option($context, "http", "header", "X-Ctx: 1");
$current = stream_context_get_options($context);

echo $options["http"]["method"], "|";
echo $current["http"]["method"], "|";
echo $snapshot["http"]["method"], "|";
echo $current["http"]["header"];
"#,
    );
    assert_eq!(out, "PATCH|POST|LOCAL|X-Ctx: 1");
}

/// Verifies four-argument updates do not mutate an earlier options snapshot.
///
/// PHP 8.5.6 oracle: `A|B`.
#[test]
fn test_stream_context_set_option_four_args_preserves_snapshot() {
    let out = compile_and_run(
        r#"<?php
$context = stream_context_create(["http" => ["header" => "A"]]);
$snapshot = stream_context_get_options($context);
stream_context_set_option($context, "http", "header", "B");
$current = stream_context_get_options($context);

echo $snapshot["http"]["header"], "|", $current["http"]["header"];
"#,
    );
    assert_eq!(out, "A|B");
}

/// Verifies PHP's two-argument option form merges wrappers and their option maps.
///
/// PHP 8.5.6 oracle: `POST|B|ssl|ftp`. The two-argument spelling emits a
/// deprecation on PHP 8.5, but its merge semantics remain normative.
#[test]
fn test_stream_context_set_option_array_merges_existing_options() {
    let out = compile_and_run(
        r#"<?php
$context = stream_context_create([
    "http" => ["method" => "POST", "header" => "A"],
    "ssl" => ["verify_peer" => true],
]);
$patch = [
    "http" => ["header" => "B"],
    "ftp" => ["overwrite" => true],
];
stream_context_set_option($context, $patch);
$patch["http"]["header"] = "LOCAL";
$options = stream_context_get_options($context);

echo $options["http"]["method"], "|";
echo $options["http"]["header"], "|";
echo $options["ssl"]["verify_peer"] ? "ssl" : "no", "|";
echo $options["ftp"]["overwrite"] ? "ftp" : "no";
"#,
    );
    assert_eq!(out, "POST|B|ssl|ftp");
}

/// Verifies the four-argument form preserves an arbitrary Mixed option value.
///
/// PHP 8.5.6 oracle: `array|42|true`.
#[test]
fn test_stream_context_set_option_four_args_preserves_mixed_value() {
    let out = compile_and_run(
        r#"<?php
function stream_option_value($value): mixed {
    return $value;
}

$context = stream_context_create();
$payload = ["answer" => 42, "flag" => true];
stream_context_set_option(
    $context,
    "custom",
    "payload",
    stream_option_value($payload),
);
$payload["answer"] = 9;
$payload["flag"] = false;
$options = stream_context_get_options($context);
$actual = $options["custom"]["payload"];

echo is_array($actual) ? "array" : "wrong", "|";
echo $actual["answer"], "|";
echo $actual["flag"] ? "true" : "false";
"#,
    );
    assert_eq!(out, "array|42|true");
}

/// Verifies `set_params` merges only `options` and preserves an absent notifier.
///
/// PHP 8.5.6 oracle: `kept|POST|B|false|replaced`.
#[test]
fn test_stream_context_set_params_merges_options_and_conditionally_replaces_notification() {
    let out = compile_and_run(
        r#"<?php
function option_params($options) {
    return ["options" => $options];
}
function notification_params($notification) {
    return ["notification" => $notification];
}

$first = function ($code) { return $code; };
$second = function ($code) { return $code + 1; };
$context = stream_context_create(
    ["http" => ["method" => "POST", "header" => "A"]],
    ["notification" => $first],
);
$patch = [
    "http" => ["header" => "B"],
    "ssl" => ["verify_peer" => false],
];
stream_context_set_params($context, option_params($patch));
$patch["http"]["header"] = "LOCAL";
$params = stream_context_get_params($context);

echo $params["notification"] === $first ? "kept" : "lost", "|";
echo $params["options"]["http"]["method"], "|";
echo $params["options"]["http"]["header"], "|";
echo $params["options"]["ssl"]["verify_peer"] ? "true" : "false";

stream_context_set_params($context, notification_params($second));
$params = stream_context_get_params($context);
echo "|", $params["notification"] === $second ? "replaced" : "wrong";
"#,
    );
    assert_eq!(out, "kept|POST|B|false|replaced");
}

/// Verifies construction applies direct options before the params options patch.
///
/// PHP 8.5.6 oracle: `PATCH|A|ssl|ftp`.
#[test]
fn test_stream_context_create_applies_params_options_after_direct_options() {
    let out = compile_and_run(
        r#"<?php
function create_params($options) {
    return ["options" => $options];
}

$context = stream_context_create(
    [
        "http" => ["method" => "POST", "header" => "A"],
        "ssl" => ["verify_peer" => true],
    ],
    create_params(
        [
            "http" => ["method" => "PATCH"],
            "ftp" => ["overwrite" => true],
        ],
    ),
);
$options = stream_context_get_options($context);

echo $options["http"]["method"], "|";
echo $options["http"]["header"], "|";
echo $options["ssl"]["verify_peer"] ? "ssl" : "no", "|";
echo $options["ftp"]["overwrite"] ? "ftp" : "no";
"#,
    );
    assert_eq!(out, "PATCH|A|ssl|ftp");
}

/// Verifies function-return and local-alias resource owners retain the same live context.
#[test]
fn test_stream_context_resource_return_and_alias_transfer_ownership() {
    let out = compile_and_run(
        r#"<?php
function make_context() {
    return stream_context_create(["http" => ["method" => "POST"]]);
}

$context = make_context();
$alias = $context;
stream_context_set_option($alias, "http", "header", 42);
$options = stream_context_get_options($context);
echo $options["http"]["method"], "|", $options["http"]["header"];
"#,
    );
    assert_eq!(out, "POST|42");
}

/// Verifies repeated context replacement releases options, params, and notifier ownership.
#[test]
fn test_stream_context_registry_releases_owned_children_at_scope_exit() {
    let baseline = compile_and_run_with_gc_stats("<?php echo \"base\";");
    assert!(baseline.success, "baseline failed: {}", baseline.stderr);

    let exercised = compile_and_run_with_gc_stats(
        r#"<?php
function runtime_context_params($options): array {
    return ["options" => $options];
}

function exercise_context(int $index): void {
    $context = stream_context_create(
        ["http" => ["method" => "POST", "header" => "X-Test: " . $index]],
        ["notification" => function ($code) use ($index) { echo ""; }],
    );
    stream_context_set_params(
        $context,
        ["notification" => function ($code) use ($index) { echo ""; }],
    );
    stream_context_set_option(
        $context,
        "http",
        "timeout",
        $index,
    );
    stream_context_set_option(
        $context,
        "custom",
        "payload",
        ["index" => $index, "enabled" => true],
    );
    stream_context_set_option(
        $context,
        ["http" => ["header" => "X-Merged: " . $index]],
    );
    stream_context_set_params(
        $context,
        runtime_context_params(
            [
                "http" => ["protocol_version" => 1],
                "ssl" => ["verify_peer" => true],
            ],
        ),
    );
    $options = stream_context_get_options($context);
    $params = stream_context_get_params($context);
}

for ($index = 0; $index < 32; $index++) {
    exercise_context($index);
}
echo "done";
"#,
    );
    assert!(exercised.success, "program failed: {}", exercised.stderr);
    assert_eq!(exercised.stdout, "done");

    let (baseline_allocs, baseline_frees) = parse_gc_stats(&baseline.stderr);
    let (context_allocs, context_frees) = parse_gc_stats(&exercised.stderr);
    assert_eq!(
        context_allocs.saturating_sub(context_frees),
        baseline_allocs.saturating_sub(baseline_frees),
        "context-owned options, params, and notifiers must not add persistent live allocations"
    );
}

/// Verifies stream attachment retains a temporary context and `fclose` releases that owner.
#[test]
fn test_stream_context_registry_fopen_attachment_releases_on_close() {
    let baseline = compile_and_run_with_gc_stats(
        r#"<?php
stream_context_get_default();
function exercise_unattached_context(int $index): void {
    $context = stream_context_create(
        ["http" => ["method" => "POST", "header" => "X-Test: " . $index]],
        ["notification" => function ($code) use ($index) { echo ""; }],
    );
    unset($context);
    $stream = fopen("php://memory", "r+");
    fwrite($stream, "x");
    fclose($stream);
    unset($stream);
}

for ($index = 0; $index < 32; $index++) {
    exercise_unattached_context($index);
}
echo "done";
"#,
    );
    assert!(baseline.success, "baseline failed: {}", baseline.stderr);
    assert_eq!(baseline.stdout, "done");

    let exercised = compile_and_run_with_gc_stats(
        r#"<?php
stream_context_get_default();
function exercise_attached_context(int $index): void {
    $context = stream_context_create(
        ["http" => ["method" => "POST", "header" => "X-Test: " . $index]],
        ["notification" => function ($code) use ($index) { echo ""; }],
    );
    $stream = fopen("php://memory", "r+", false, $context);
    unset($context);
    fwrite($stream, "x");
    fclose($stream);
    unset($stream);
}

for ($index = 0; $index < 32; $index++) {
    exercise_attached_context($index);
}
echo "done";
"#,
    );
    assert!(exercised.success, "program failed: {}", exercised.stderr);
    assert_eq!(exercised.stdout, "done");

    let (baseline_allocs, baseline_frees) = parse_gc_stats(&baseline.stderr);
    let (context_allocs, context_frees) = parse_gc_stats(&exercised.stderr);
    assert_eq!(
        context_allocs.saturating_sub(context_frees),
        baseline_allocs.saturating_sub(baseline_frees),
        "destroying the stream must release its attached context without adding persistent live children"
    );
}
