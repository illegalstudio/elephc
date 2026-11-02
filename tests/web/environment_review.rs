//! Purpose:
//! Regresses executed callback observers that early environment demand must keep reachable.
//!
//! Called from:
//! - The environment module in `tests/web_tests.rs`.
//!
//! Key details:
//! - Every fixture compiles separately so an observer cannot mask another missed demand path.
//! - Computed eval names hide ENV access until later optimization, matching the original bug.
//! - EIR optimization is unrelated to early demand and makes these eval-capable hosts costly.

use super::*;

/// Reads and changes ENV through a later-folded eval without exposing an ENV AST node to demand.
const OBSERVER: &str = r#"
function hidden_compare(int $left, int $right): int {
    $prefix = 'ev';
    $fn = $prefix . 'al';
    $fn('echo $_ENV["ELEPHC_WEB_TEST_ENV"] ?? "missing"; $_ENV["ELEPHC_WEB_TEST_ENV"] = "changed";');
    return $left <=> $right;
}
"#;

/// Compiles one observer path and verifies that consecutive requests see fresh process values.
fn assert_callback_environment(label: &str, declarations: &str, handle: &str) {
    let dir = EnvironmentFixture(make_test_dir(label));
    let source = format!(r#"<?php
{declarations}
class ReviewApplication implements Elephc\Web\WorkerApplication {{
    public function handle(Elephc\Web\Request $request): void {{ {handle} }}
    public function shutdown(): void {{}}
}}
Elephc\Web\Worker::register(new ReviewApplication());
"#);
    let binary = compile_web_with_flags(&dir.0, &source, "app",
        &["--web-lifecycle=application", "--no-ir-opt"]);
    let addr = format!("127.0.0.1:{}", free_port());
    let _server = environment_server(&binary, &addr, false);
    for _ in 0..2 {
        let response = http_get(&addr, "/");
        assert!(response.starts_with("HTTP/1.1 200"), "{label}: {response}");
        assert_eq!(raw_response_body(&response).trim(), "present", "{label}");
    }
}

/// Callback metadata retains a comparator reached through a computed function name.
#[test]
fn web_environment_callback_builtin_retains_population() {
    assert_callback_environment("environment_callback_builtin", OBSERVER,
        "array_udiff([1], [2], 'hidden_' . 'compare');");
}

/// Callable identifiers are rooted using decoded PHP bytes rather than lexer marker characters.
#[test]
fn web_environment_escaped_callback_retains_population() {
    let observer = OBSERVER.replace("hidden_compare", "café");
    assert_callback_environment("environment_escaped_callback", &observer,
        r#"$target = "caf\xc3\xa9"; $target(1, 2);"#);
}

/// Fiber construction keeps its native callback entry reachable before start invokes it.
#[test]
fn web_environment_fiber_callback_retains_population() {
    assert_callback_environment("environment_fiber_callback", OBSERVER,
        "$fiber = new Fiber('hidden_' . 'compare'); $fiber->start(1, 2);");
}

/// A variadic callable array is not proven simple by a global string with the same name.
#[test]
fn web_environment_variadic_callback_retains_population() {
    let declarations = format!(r#"{OBSERVER}
function dispatch_callback(mixed ...$fn): void {{ $fn(); }}
class CallbackOwner {{ public function hidden(): void {{ hidden_compare(1, 2); }} }}
$fn = 'strlen';
"#);
    assert_callback_environment("environment_variadic_callback", &declarations,
        "dispatch_callback(new CallbackOwner(), 'hid' . 'den');");
}

/// Explicit type arguments still enter the generic template's callback body.
#[test]
fn web_environment_generic_callback_retains_population() {
    let declarations = format!(r#"{OBSERVER}
function dispatch_callback<T>(callable $callback, T $left, T $right): void {{ $callback($left, $right); }}
"#);
    assert_callback_environment("environment_generic_callback", &declarations,
        "dispatch_callback<int>('hidden_' . 'compare', 1, 2);");
}

/// Scoped parent construction preserves callbacks that are invoked by native iterator methods.
#[test]
fn web_environment_parent_constructor_callback_retains_population() {
    let declarations = r#"
function filter_hidden(mixed $value, mixed $key, Iterator $iterator): bool {
    $prefix = 'ev'; $fn = $prefix . 'al';
    $fn('echo $_ENV["ELEPHC_WEB_TEST_ENV"] ?? "missing"; $_ENV["ELEPHC_WEB_TEST_ENV"] = "changed";');
    return true;
}
class ReviewFilter extends CallbackFilterIterator {
    public static function make(Iterator $iterator, callable $callback): CallbackFilterIterator {
        return new parent($iterator, $callback);
    }
}
"#;
    assert_callback_environment("environment_parent_constructor", declarations,
        "$filter = ReviewFilter::make(new ArrayIterator([1]), 'filter_' . 'hidden'); $filter->rewind();");
}
