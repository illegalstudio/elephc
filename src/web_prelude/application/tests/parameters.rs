//! Purpose:
//! Checks callable-parameter proofs used to omit unobservable request environment copies.
//!
//! Called from:
//! - Application prelude library tests.
//!
//! Key details:
//! - Fixtures inspect request preparation independently of bootstrap population.
//! - Unknown entries, mutation and escaped targets must invalidate the proof.

use super::application_request_usage;

/// Literal callable arguments survive ordinary function and static helper boundaries.
#[test]
fn application_environment_parameter_proof_accepts_literal_arguments() {
    for source in [
        "<?php function through(callable $factory): string { return $factory(); } echo through(fn(): string => 'ok');",
        "<?php function through(callable $factory): int { return $factory('ok'); } echo through('strlen');",
        "<?php function through(callable $factory): int { return $factory('ok'); } echo through(strlen(...));",
        "<?php namespace App; function through(callable $factory) { return $factory(); } through(fn() => 'ok');",
        "<?php final class Host { public static function serve(callable $factory) { self::dispatch(fn() => $factory()); } private static function dispatch(callable $factory) { return $factory(); } } Host::serve(fn() => 'ok');",
        "<?php final class Host { public static function dispatch(callable $factory) { return $factory(); } } Host::dispatch(fn() => 'ok'); Host::dispatch(fn() => 'other');",
        "<?php function through(callable $factory = 'strlen') { return $factory('ok'); } through();",
        "<?php function unused_global() { global $callback; $callback = $runtime; } $callback = 'strlen'; $callback('ok');",
        "<?php class Host { public static function safe(): void {} } function through_array(array $factory) { foreach ($factory as $entry) { $entry = $runtime; } $factory(); } through_array(['Host', 'safe']);",

        "<?php class Host { public function through(callable $factory) { return $factory(); } } (new Host())->through(fn() => 'ok');",
        "<?php class JsonBody { public static function decode(string $body, callable $decoder) { return $decoder($body); } } class Query { public static function decode(string $body, int $limit) {} public static function read(string $body, int $limit) { self::decode($body, $limit); } } Query::read('ok', 100); JsonBody::decode('ok', fn(string $body) => $body);",
        "<?php interface Decoder { public function decode(string $input); } class JsonBody { public static function decode(string $body, callable $decoder) { return $decoder($body); } } class TextDecoder implements Decoder { public function decode(string $input) { return $input; } } $decoder = new TextDecoder(); $decoder->decode('ok'); JsonBody::decode('ok', fn(string $body) => $body);",
    ] {
        assert!(!application_request_usage(source, false, false).references("elephc_web_env_count"), "{source}");
    }
}

/// Parameter facts stay local and cannot certify an unrelated runtime-supplied callable.
#[test]
fn application_environment_parameter_proof_isolates_scopes() {
    for source in [
        "<?php function through(callable $factory) { return $factory(); } through(fn() => 'ok'); $factory = $runtime; $factory();",
        "<?php function through(callable $factory) { return $factory(); } function other(callable $factory) { $factory(); } through(fn() => 'ok'); other($runtime);",
        "<?php function through(callable $factory) { $inner = function(callable $factory) { $factory(); }; $inner($runtime); } through(fn() => 'ok');",
        "<?php function through(callable $factory) { global $factory; $factory(); } through(fn() => 'ok');",
    ] {
        assert!(application_request_usage(source, false, false).references("elephc_web_env_count"), "{source}");
    }
}

/// Every possible caller and parameter mutation must preserve a simple callable identity.
#[test]
fn application_environment_parameter_proof_rejects_uncertain_entries() {
    for source in [
        "<?php function through(callable $factory) { $factory(); } through(fn() => 'ok'); through($runtime);",
        "<?php function through(callable $factory) { $factory(); } through(fn() => 'ok'); $escaped = 'through';",
        "<?php function through(callable $factory) { $factory(); } through(fn() => 'ok'); $escaped = through(...);",
        "<?php function through(callable $factory) { $factory(); } through(fn() => 'ok'); $escaped = ['Host', 'through'];",
        "<?php class Host { public static function through(callable $factory) { $factory(); } } Host::through(fn() => 'ok'); $escaped = Host::through(...);",
        "<?php function through(callable &$factory) { $factory(); } through(fn() => 'ok');",
        "<?php function through(callable $factory) { $factory = $runtime; $factory(); } through(fn() => 'ok');",
        "<?php function through(callable $factory) { replace($factory); $factory(); } through(fn() => 'ok');",
        "<?php function through(callable $factory) { $alias =& $factory; $alias = $runtime; $factory(); } through(fn() => 'ok');",
        "<?php function through(callable $factory) { $closure = function() use (&$factory) { $factory = $runtime; }; $closure(); $factory(); } through(fn() => 'ok');",
        "<?php function through(callable $factory) { $factory(); } through(factory: fn() => 'ok');",
        "<?php function through(callable $factory) { $factory(); } through(...$arguments);",
        "<?php function through(callable $factory = 'eval') { $factory(); } through();",
        "<?php function through(callable $factory) { $factory(); } through('ev' . 'al');",
        "<?php function through(callable $factory) { $factory(); } through('eval');",
        "<?php function through(callable $factory) { $factory(); } through(fn() => $_ENV['PATH']);",
        "<?php function through(callable $factory) { $factory(); } through(fn() => get_defined_vars());",
        "<?php interface Host { public function through(callable $factory); } class Impl implements Host { public function through(callable $factory) { $factory(); } } (new Impl())->through(fn() => 'ok');",
        "<?php class Base { public function through(callable $factory) { $factory(); } } class Child extends Base {} (new Child())->through(fn() => 'ok');",
        "<?php class Host { public static function through(callable $factory) { $factory(); } } use Host as Alias; Host::through(fn() => 'ok'); Alias::through($runtime);",
        "<?php function through(callable $factory) { $factory(); } use function through as alias; alias($runtime); through(fn() => 'ok');",
    ] {
        assert!(application_request_usage(source, false, false).references("elephc_web_env_count"), "{source}");
    }
}

/// Captured call targets depend on every enclosing write, including loop back edges.
#[test]
fn application_environment_parameter_proof_keeps_capture_dependencies() {
    for source in [
        "<?php $callback = 'strlen'; for ($i = 0; $i < 2; $i++) { $reader = fn() => $callback('code'); $reader(); $callback = $runtime; }",
        "<?php $callback = 'strlen'; while ($again) { $reader = function() use ($callback) { $callback('code'); }; $reader(); $callback = $runtime; }",
        "<?php $callback = 'strlen'; for ($i = 0; $i < 2; $i++) { $outer = function() use ($callback) { $reader = fn() => $callback('code'); $reader(); }; $outer(); $callback = $runtime; }",
        "<?php $callback = $runtime; $reader = function() use ($callback) { $callback('code'); $callback = 'strlen'; }; $reader();",
    ] {
        assert!(application_request_usage(source, false, false).references("elephc_web_env_count"), "{source}");
    }
}

/// Reachable global mutations invalidate a callable invoked only in the outer scope.
#[test]
fn application_environment_parameter_proof_keeps_global_mutations() {
    for source in [
        "<?php $callback = 'strlen'; function replace_callback() { global $callback; $callback = $runtime; } replace_callback(); $callback('code');",
        "<?php $callback = 'strlen'; replace_callback(); $callback('code'); function replace_callback() { global $callback; $callback = $runtime; }",
        "<?php $callback = 'strlen'; function outer_callback() { inner_callback(); } function inner_callback() { global $callback; $callback = $runtime; } outer_callback(); $callback('code');",
    ] {
        assert!(application_request_usage(source, false, false).references("elephc_web_env_count"), "{source}");
    }
}

/// Dimension, reference and iteration writes invalidate a certified callable array owner.
#[test]
fn application_environment_parameter_proof_keeps_array_mutations() {
    for body in [
        "$factory[1][0] = 'h';",
        "$alias =& $factory[1]; $alias = 'ha' . 'fe';",
        "replace_method($factory[1]);",
        "replace_method($factory[1][0]);",
        "foreach ($factory as $key => &$entry) { if (is_string($entry)) { $entry = 'ha' . 'fe'; } }",
    ] {
        let source = format!(r#"<?php
class Host {{
    public static function safe(): void {{}}
    public static function hafe(): void {{ $prefix = 'ev'; $exec = $prefix . 'al'; $exec('echo $_ENV["PATH"] ?? "";'); }}
}}
function replace_method(string &$name): void {{ $name = 'ha' . 'fe'; }}
function through_array(array $factory): void {{ {body} $factory(); }}
through_array(['Host', 'safe']);
"#);
        assert!(application_request_usage(&source, false, false).references("elephc_web_env_count"), "{body}");
    }
}
