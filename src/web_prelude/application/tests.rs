//! Purpose:
//! Checks application-prelude demand gating and injection from a small embedder thread.
//!
//! Called from:
//! - Focused library tests of the application prelude entry point.
//!
//! Key details:
//! - A supported-depth AST is constructed without recursive parsing on the small stack.
//! - Demand checks inspect request preparation separately from the complete bootstrap copy.

use super::*;

mod parameters;

/// Collects only the generated request helper, excluding the adapter's bootstrap observers.
fn application_request_usage(
    source: &str,
    autoload_observes_environment: bool,
) -> crate::prelude_prune::usage::Usage {
    let tokens = crate::lexer::tokenize(source).expect("fixture tokenizes");
    let program = crate::parser::parse(&tokens)
        .unwrap_or_else(|error| panic!("fixture parses: {error:?}\n{source}"));
    let injected = inject_with_autoload_usage(
        program, PhpVersion::Php85, &[], &mut Default::default(), None,
        autoload_observes_environment,
    );
    let body = injected.iter().find_map(|statement| match &statement.kind {
        StmtKind::FunctionDecl { name, body, .. } if name == "__elephc_web_prepare_request" => Some(body),
        _ => None,
    }).expect("application request helper");
    crate::prelude_prune::usage::collect(body)
}

/// Unobserved request environments skip enumeration while retaining the empty session scratch array.
#[test]
fn application_environment_demand_omits_unobserved_values() {
    for source in [
        "<?php echo 'ok';",
        "<?php echo getenv('PATH');",
        "<?php echo $_SERVER['PATH'];",
        "<?php $fn = 'strlen'; echo $fn('ok');",
        "<?php $fn = fn() => 'ok'; echo $fn();",
        "<?php session_set_cookie_params(['path' => '/']); session_start();",
    ] {
        let usage = application_request_usage(source, false);
        assert!(!usage.references("elephc_web_env_count"), "{source}");
        assert!(usage.storage_variables.contains("_ENV"), "{source}");
    }
}

/// User observers retain population before the generated adapter can influence the demand scan.
#[test]
fn application_environment_demand_keeps_observable_values() {
    for source in [
        "<?php echo $_ENV['PATH'];",
        "<?php $_ENV['local'] = 'value';",
        "<?php echo $GLOBALS['_ENV']['PATH'];",
        "<?php $fn = fn() => $_ENV['PATH']; echo $fn();",
        "<?php echo count(get_defined_vars());",
        "<?php echo count(compact('_ENV'));",
        "<?php eval($code);",
        "<?php function invoke(string $fn) { $fn(); } invoke($callback);",
    ] {
        assert!(application_request_usage(source, false).references("elephc_web_env_count"), "{source}");
    }
}

/// Unreferenced callback helpers do not force request copies, including Phex-style map methods.
#[test]
fn application_environment_demand_ignores_unreferenced_callbacks() {
    for source in [
        "<?php function unused(callable $callback) { return $callback('ok'); } echo 'ok';",
        "<?php final class Environment { public function map(callable $callback) { return $callback('ok'); } } echo 'ok';",
        "<?php namespace App; function unused(callable $callback) { $callback(); } echo 'ok';",
        "<?php function unused(string $fn) {} $fn = 'strlen'; echo $fn('ok');",
        "<?php function unused(callable $callback) { $callback(); } $fn = 'strlen'; echo $fn('ok');",
        "<?php interface Decoder { public function decode(): void; } class JsonBody { public static function decode(callable $decoder) { $decoder(); } } class Query { public static function decode() {} public static function read() { self::decode(); } } Query::read();",
        "<?php function unused() { helper($fn); } function helper(callable $callback) { $callback(); } echo 'ok';",
        "<?php function unused() { other($fn); } function other(callable $callback) { unused(); $callback(); } echo 'ok';",
        "<?php interface Resource { public function close(): void; } class Host implements Resource { public function close(): void {} public function unused() { self::dispatch($fn); } private static function dispatch(callable $callback) { $callback(); } } echo 'ok';",
    ] {
        assert!(!application_request_usage(source, false).references("elephc_web_env_count"), "{source}");
    }
}

/// Calls, escaped names, lifecycle methods and unknown lookup retain opaque callback demand.
#[test]
fn application_environment_demand_preserves_referenced_callbacks() {
    for source in [
        "<?php function helper(callable $callback) { $callback(); } helper($fn);",
        "<?php function helper(callable $callback) { $callback(); } $escaped = 'helper';",
        "<?php function helper(callable $callback) { $callback(); } $escaped = helper(...);",
        "<?php namespace App; function helper(callable $callback) { $callback(); } helper($fn);",
        "<?php namespace App { function helper(callable $callback) { $callback(); } } namespace { use function App\\helper as run; run($fn); }",
        "<?php class Config { public function map(callable $callback) { $callback(); } } $config->map($fn);",
        "<?php function enter() { helper($fn); } function helper(callable $callback) { $callback(); } enter();",
        "<?php interface Resource { public function close(): void; } class Host implements Resource { public function close(): void { $callback(); } }",
        "<?php interface UserCount extends Countable {} class Counter implements UserCount { public function count(): int { $callback(); return 1; } }",
        "<?php class Config { public static function map(callable $callback) { $callback(); } } Config::map($fn);",
        "<?php class Config { public static function map(callable $callback) { $callback(); } } use Config as C; C::map($fn);",
        "<?php class Config { public static function map(callable $callback) { $callback(); } } class C extends Config {} C::map($fn);",
        "<?php class Config { public function map(callable $callback) { $callback(); } } $escaped = 'Config::map';",
        "<?php class Config { public function map(callable $callback) { $callback(); } } $escaped = [$config, 'map'];",
        "<?php class Config { public function __destruct() { $callback(); } }",
        "<?php class Application implements Elephc\\Web\\WorkerApplication { public function handle($request): void { $callback(); } }",
        "<?php class Config extends Base { public function map(callable $callback) { $callback(); } }",
        "<?php class Base { public function handle($request): void { $callback(); } } class App extends Base implements Elephc\\Web\\WorkerApplication {}",
        "<?php trait Config { public function map(callable $callback) { $callback(); } }",
        "<?php #[Export] function helper(callable $callback) { $callback(); }",
        "<?php trait Config { public function map(callable $callback) { $callback(); } } class C { use Config { map as run; } } $c->run($fn);",
        "<?php class Config { public string $path { get { $callback(); return ''; } } }",
        "<?php function helper(callable $callback) { $callback(); } $fn = 'hel' . 'per'; $fn($callback);",
        "<?php function helper(callable $callback) { $callback(); } get_defined_functions();",
        "<?php function helper(callable $callback) { $callback(); } new ReflectionFunction($name);",
        "<?php function helper(callable $callback) { $callback(); } $reflection->invoke($callback);",
        "<?php class Config { public function map(callable $callback) { $callback(); } } $fn = [$config, $method]; $fn($callback);",
        "<?php function helper(callable $callback) { $callback(); } use function array_map as apply; apply($target, $values);",
        "<?php function helper(callable $callback) { $callback(); } use Closure as C; $escaped = C::fromCallable($target);",
        "<?php class Config { public function map(callable $callback) { $callback(); } } $config?->$method($callback);",
        "<?php extern function invoke_native(callable $callback): void; function helper(callable $callback) { $callback(); } invoke_native($target);",
    ] {
        assert!(application_request_usage(source, false).references("elephc_web_env_count"), "{source}");
    }
}

/// Include-selected bodies remain callable through their public dispatch group.
#[test]
fn application_environment_demand_keeps_include_variants() {
    let tokens = crate::lexer::tokenize(
        "<?php function helper_variant(callable $callback) { $callback(); } helper($fn);",
    ).unwrap();
    let mut program = crate::parser::parse(&tokens).unwrap();
    program.push(Stmt::new(StmtKind::FunctionVariantGroup {
        name: "helper".to_owned(), variants: vec!["helper_variant".to_owned()],
    }, crate::span::Span::synthetic()));
    let usage = super::super::usage::collect_environment(&program);
    assert!(super::super::environment::is_observable(&program, &usage));
}

/// Shared callback roles, XML hooks and callable aliases retain computed observer targets.
#[test]
fn application_environment_demand_keeps_callback_boundaries() {
    for call in [
        "array_udiff([1], [2], 'hel' . 'per')",
        "array_uintersect([1], [2], 'hel' . 'per')",
        "array_find([1], 'hel' . 'per')",
        "array_any([1], 'hel' . 'per')",
        "array_all([1], 'hel' . 'per')",
        "mb_ereg_replace_callback('x', 'hel' . 'per', 'x')",
        "pcntl_signal(10, 'hel' . 'per')",
        "xml_set_element_handler($parser, 'hel' . 'per', null)",
        "use function array_udiff as dispatch; dispatch([1], [2], 'hel' . 'per')",
        "$dispatch = 'array_udiff'; $dispatch([1], [2], 'hel' . 'per')",
        "$dispatch = array_udiff(...); $dispatch([1], [2], 'hel' . 'per')",
        "$dispatch = 'call_user_func'; $dispatch('hel' . 'per')",
        "__elephc_normalize_callable('hel' . 'per')",
    ] {
        let source = format!(r#"<?php
function helper(mixed ...$values): int {{
    $prefix = 'ev'; $fn = $prefix . 'al'; $fn('echo $_ENV["PATH"];'); return 0;
}}
{call};"#);
        assert!(application_request_usage(&source, false).references("elephc_web_env_count"), "{call}");
    }
    assert!(application_request_usage(
        "<?php $dispatch = 'compact'; $dispatch('_ENV');", false,
    ).references("elephc_web_env_count"));
}

/// Escaped PHP bytes match UTF-8 identifiers without decoding identifier text a second time.
#[test]
fn application_environment_demand_keeps_escaped_callable_names() {
    for source in [
        r#"<?php function café(callable $callback) { $callback(); } $fn = "caf\xc3\xa9"; $fn($source);"#,
        r#"<?php class C { public static function méthode(callable $callback) { $callback(); } } $fn = "C::m\xc3\xa9thode"; $fn($source);"#,
        r#"<?php class C { public function méthode(callable $callback) { $callback(); } } $fn = [$object, "m\xc3\xa9thode"]; $fn($source);"#,
    ] {
        assert!(application_request_usage(source, false).references("elephc_web_env_count"), "{source}");
    }
    let source = format!(r#"<?php function target{suffix}(callable $callback) {{ $callback(); }}
$fn = "target\u{{e000}}"; $fn($source);"#, suffix = '\u{e000}');
    assert!(application_request_usage(&source, false).references("elephc_web_env_count"));
}

/// Native callback constructors follow aliases and keep unread string/array targets conservative.
#[test]
fn application_environment_demand_keeps_native_callback_constructors() {
    for source in [
        "<?php function helper(callable $callback) { $callback(); } new Fiber('hel' . 'per');",
        "<?php use Fiber as Task; function helper(callable $callback) { $callback(); } new Task('hel' . 'per');",
        "<?php function helper(callable $callback) { $callback(); } new CallbackFilterIterator($iterator, 'hel' . 'per');",
        "<?php use RecursiveCallbackFilterIterator as Filter; function helper(callable $callback) { $callback(); } new Filter($iterator, callback: 'hel' . 'per');",
        "<?php class Handler { public function test() { $prefix = 'ev'; $fn = $prefix . 'al'; $fn($source); } } new CallbackFilterIterator($iterator, [new Handler(), 'te' . 'st']);",
    ] {
        assert!(application_request_usage(source, false).references("elephc_web_env_count"), "{source}");
    }
    for source in [
        "<?php function unused(callable $callback) { $callback(); } new Fiber('strlen');",
        "<?php use Fiber as Task; function unused(callable $callback) { $callback(); } new Task('strlen');",
        "<?php function unused(callable $callback) { $callback(); } $fn = 'array_any'; echo 'ok';",
        "<?php function unused(callable $callback) { $callback(); } $fn = 'strlen'; $fn('array_any');",
    ] {
        assert!(!application_request_usage(source, false).references("elephc_web_env_count"), "{source}");
    }
}

/// Every constructor spelling preserves native callback escapes, including overridden signatures.
#[test]
fn application_environment_demand_keeps_native_constructor_paths() {
    for construction in [
        "class Filter extends CallbackFilterIterator { public function __construct(callable $callback, string $mode) { parent::__construct($iterator, $callback); } } new Filter('hel' . 'per', 'all');",
        "class Filter extends CallbackFilterIterator { public static function make($iterator, $callback) { return new static($iterator, $callback); } } Filter::make($iterator, 'hel' . 'per');",
        "class Filter extends CallbackFilterIterator { public static function make($iterator, $callback) { return new self($iterator, $callback); } } Filter::make($iterator, 'hel' . 'per');",
        "class Filter extends CallbackFilterIterator { public static function make($iterator, $callback) { return new parent($iterator, $callback); } } Filter::make($iterator, 'hel' . 'per');",
        "class Filter<T> extends CallbackFilterIterator {} new Filter<int>($iterator, 'hel' . 'per');",
        "class Filter extends CallbackFilterIterator { public static function make() { return new static(callback: 'hel' . 'per', iterator: 'all'); } } Filter::make();",
        "class Filter extends CallbackFilterIterator { public static function make() { return new self(...$args); } } Filter::make();",
        "CallbackFilterIterator::__construct($iterator, 'hel' . 'per');",
        "$filter = new CallbackFilterIterator($iterator, 'strlen'); $filter->__construct($iterator, 'hel' . 'per');",
        "class Filter extends CallbackFilterIterator { public function __construct(callable $callback, string $mode) {} } use Filter as Alias; new Alias('hel' . 'per', 'all');",
        "namespace A { class Filter extends \\CallbackFilterIterator { public function __construct() {} } } namespace B { use CallbackFilterIterator as Filter; new Filter($iterator, 'hel' . 'per'); }",
    ] {
        let helper = "function helper(callable $callback) { $callback(); }";
        let source = if construction.starts_with("namespace") {
            format!("<?php namespace {{ {helper} }} {construction}")
        } else { format!("<?php {helper} {construction}") };
        assert!(application_request_usage(&source, false).references("elephc_web_env_count"), "{construction}");
    }
    for construction in [
        "new CallbackFilterIterator($iterator, 'strlen');",
        "use CallbackFilterIterator as Filter; new Filter($iterator, 'strlen');",
        "class Filter extends CallbackFilterIterator {} new Filter($iterator, 'strlen');",
        "class Filter<T> extends CallbackFilterIterator {} new Filter<int>($iterator, 'strlen');",
        "class Filter extends CallbackFilterIterator { public static function make($iterator) { return new self($iterator, 'strlen'); } } Filter::make($iterator);",
        "class Filter extends CallbackFilterIterator { public static function make($iterator) { return new parent($iterator, 'strlen'); } } Filter::make($iterator);",
    ] {
        let source = format!("<?php function unused(callable $callback) {{ $callback(); }} {construction}");
        assert!(!application_request_usage(&source, false).references("elephc_web_env_count"), "{construction}");
    }
    for source in [
        "<?php new Fiber($callback);",
        "<?php use Fiber as Task; new Task($callback);",
        "<?php new CallbackFilterIterator($iterator, $callback);",
        "<?php new $class($callback);",
    ] {
        assert!(application_request_usage(source, false).references("elephc_web_env_count"), "{source}");
    }
    assert!(!application_request_usage(
        "<?php class Data { public function __construct(string $label) {} } new Data('ok'); function_exists($name);", false,
    ).references("elephc_web_env_count"));
}

/// Variadic locals cannot inherit a global binding with the same name.
#[test]
fn application_environment_demand_keeps_variadic_bindings() {
    for dispatch in [
        "function dispatch(mixed ...$fn): void { $fn(); } dispatch(new Handler(), 'te' . 'st');",
        "class Dispatch { public function run(mixed ...$fn): void { $fn(); } } $dispatch = new Dispatch(); $dispatch->run(new Handler(), 'te' . 'st');",
        "$dispatch = function(mixed ...$fn): void { $fn(); }; $dispatch(new Handler(), 'te' . 'st');",
    ] {
        let source = format!(r#"<?php $fn = 'strlen';
class Handler {{ public function test(): void {{
    $prefix = 'ev'; $exec = $prefix . 'al'; $exec('echo $_ENV["PATH"];');
}} }}
{dispatch}"#);
        assert!(application_request_usage(&source, false).references("elephc_web_env_count"), "{dispatch}");
    }
    assert!(!application_request_usage(
        "<?php function unused(mixed ...$fn) {} $fn = 'strlen'; $fn('ok');", false,
    ).references("elephc_web_env_count"));
}

/// Explicit function/method type arguments name their template before monomorphization.
#[test]
fn application_environment_demand_keeps_explicit_generic_calls() {
    for source in [
        "<?php function dispatch<T>(callable $callback, T $value) { $callback($value); } dispatch<int>($source, 1);",
        "<?php namespace App; function dispatch<T>(callable $callback, T $value) { $callback($value); } dispatch<App\\Value>($source, $value);",
        "<?php class C { public function dispatch<T>(callable $callback, T $value) { $callback($value); } } $object = new C(); $object->dispatch<int>($source, 1);",
    ] {
        assert!(application_request_usage(source, false).references("elephc_web_env_count"), "{source}");
    }
}

/// Autoload probe demand retains population even when the entry program has no observer.
#[test]
fn application_environment_demand_keeps_autoloaded_values() {
    assert!(application_request_usage("<?php echo 'ok';", true).references("elephc_web_env_count"));
}

/// A 2 MiB embedder thread can inject a deeply nested program and return it to its owner.
#[test]
fn application_injection_supports_small_embedder_stacks() {
    use crate::parser::ast::{Expr, ExprKind};
    let output = std::thread::Builder::new().stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let span = crate::span::Span::synthetic();
            let mut expression = crate::synthetic_class::e_call("session_set_save_handler", vec![]);
            for _ in 0..1000 {
                expression = Expr::new(ExprKind::ArrayLiteral(vec![expression]), span);
            }
            let program = vec![s_expr(expression)];
            let mut inventory = crate::optimize::reachability::PreludeInventory::new();
            let output = inject(program, PhpVersion::Php85, &[], &mut inventory, None);
            let count = output.len();
            crate::compiler_stack::with_compiler_stack(|| drop(output));
            count
        }).unwrap().join().unwrap();
    assert!(output > 0);
}
