//! Purpose:
//! Regressions for PHP-compatible ErrorException constructor state preservation.
//!
//! Called from:
//! - The Core class codegen integration module.
//!
//! Key details:
//! - Native and opaque eval execute the same source against PHP-oracle expectations.
//! - Heap debugging covers retained previous objects and replacement argument temporaries.

use crate::support::*;

/// Nonmatching planned classes preserve named keys and unpacked values in dynamic construction.
#[test]
fn test_core_review_dynamic_constructor_fallback_preserves_arguments() {
    let source = r#"<?php
class FallbackOpt { public function __construct($a = 1, $b = 2) {} }
class FallbackExact { public function __construct(public int $x) {} }
class FallbackVariadic {
    public function __construct(int ...$values) { echo count($values), ':', $values[0], ':', $values[1], ';'; }
}
function fallbackNamed(string $name): mixed { return new $name(b: 5); }
function fallbackSpread(string $name, $values): mixed { return new $name(...$values); }
foreach ([FallbackExact::class, Exception::class, ErrorException::class] as $name) {
    try { $object = fallbackNamed($name); echo 'accepted;'; unset($object); }
    catch (Error $error) { echo $error->getMessage(), ';'; unset($error); }
}
$object = fallbackSpread(Exception::class, [4, 5]);
echo $object->getMessage(), ':', $object->getCode(), ';';
unset($object);
$object = fallbackSpread(ErrorException::class, [6, 7]);
echo $object->getMessage(), ':', $object->getCode(), ';';
unset($object);
$object = fallbackSpread(FallbackVariadic::class, [8, 9]);
unset($object, $name);
"#;
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, "Unknown named parameter $b;".repeat(3) + "4:5;6:7;2:8:9;");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

const SOURCE: &str = r#"
function coreState(ErrorException $error, Exception $previous): void {
    echo $error->getMessage(), ':', $error->getCode(), ':',
        $error->getPrevious() === $previous ? 'same:' : 'bad:',
        $error->getFile(), ':', $error->getLine(), ':', $error->getSeverity(), ';';
}
function coreStateExtra(): int { echo 'extra;'; return 7; }
$previous = new Exception('old');
$error = new ErrorException('kept', 7, E_WARNING, 'a.php', 3, $previous);
$error->__construct();
coreState($error, $previous);
$error->__construct(filename: null, line: null);
coreState($error, $previous);
$error->__construct(message: 'again', code: 0, previous: null);
coreState($error, $previous);
$replacement = new Exception('new');
$error->__construct(message: 'changed', code: 9, previous: $replacement);
coreState($error, $replacement);
$error->__construct(message: 'alias', previous: $error->getPrevious());
coreState($error, $replacement);
try {
    new ErrorException('too many', 0, E_ERROR, null, null, null, coreStateExtra());
} catch (ArgumentCountError $failure) {
    echo $failure->getMessage();
}
unset($error, $previous, $replacement);
"#;

const EXPECTED: &str = "kept:7:same:a.php:3:1;:7:same:a.php:3:1;again:7:same:a.php:3:1;changed:9:same:a.php:3:1;alias:9:same:a.php:3:1;extra;ErrorException::__construct() expects at most 6 arguments, 7 given";

const ASSOC_SOURCE: &str = r#"
function coreAssocArguments() { echo 'args;'; return ['message' => 'factory', 'code' => 8]; }
$values = ['message' => 'fresh', 'code' => 4];
$error = new ErrorException(...$values);
echo $error->getMessage(), ':', $error->getCode(), ';';
$values = ['filename' => 'named.php'];
$error->__construct(...$values);
echo $error->getMessage(), ':', $error->getCode(), ':', $error->getFile(), ':', $error->getLine(), ';';
$error->__construct(message: 'kept');
unset($values['filename']);
$error->__construct(...$values);
echo $error->getMessage(), ':', $error->getCode(), ';';
$previous = new Exception('cause');
$previousValues = ['previous' => $previous];
$error->__construct(...$previousValues, message: 'named', code: 0, severity: 1, filename: 'later.php', line: 9);
echo $error->getMessage(), ':', $error->getCode(), ':', $error->getPrevious() === $previous ? 'same;' : 'bad;';
$factory = new ErrorException(...coreAssocArguments());
echo $factory->getMessage(), ':', $factory->getCode();
unset($error, $factory, $values, $previousValues, $previous);
"#;

const ASSOC_EXPECTED: &str = "fresh:4;:4:named.php:0;kept:4;named:4:same;args;factory:8";

/// Native constructor calls distinguish omitted messages and preserve zero/null field arguments.
#[test]
fn test_core_review_error_exception_reinitialization_preserves_state() {
    let out = compile_and_run_with_heap_debug(&format!("<?php\n{SOURCE}"));
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, EXPECTED);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Opaque eval retains the same defaults, previous identity, and constructor arity contract.
#[test]
fn test_core_review_error_exception_reinitialization_preserves_state_eval() {
    let encoded = SOURCE.replace('\\', "\\\\").replace('\'', "\\'");
    let out = compile_and_run_with_heap_debug(&format!("<?php $source = '{encoded}' . ' // ' . $argc; eval($source);"));
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, EXPECTED);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Dynamic associative unpacks bind by key and count supplied optional positions once.
#[test]
fn test_core_review_error_exception_assoc_spread_count() {
    let out = compile_and_run_with_heap_debug(&format!("<?php\n{ASSOC_SOURCE}"));
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, ASSOC_EXPECTED);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Opaque eval agrees on associative constructor unpacks, omitted messages, and empty maps.
#[test]
fn test_core_review_error_exception_assoc_spread_count_eval() {
    let encoded = ASSOC_SOURCE.replace('\\', "\\\\").replace('\'', "\\'");
    let out = compile_and_run_with_heap_debug(&format!("<?php $source = '{encoded}' . ' // ' . $argc; eval($source);"));
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, ASSOC_EXPECTED);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

const STREAM_SOURCE: &str = r#"
class StreamError extends ErrorException {
 public static function make(array $values): static { return new static(...$values); }
 public function reset(array $values): void { parent::__construct(...$values); }
}
function streamFailure(array $values): void {
 try { new ErrorException(...$values); echo 'accepted;'; }
 catch (Error $failure) { echo get_class($failure), ':', $failure->getMessage(), ';'; }
}
$values = [9 => 'fresh', 'filename' => 'named.php'];
$error = new ErrorException(...$values);
echo $error->getMessage(), ':', $error->getFile(), ':', $error->getLine(), ';';
$reset = [12 => 'again', 8 => 7];
$error->__construct(...$reset);
echo $error->getMessage(), ':', $error->getCode(), ';';
$child = StreamError::make($values);
echo get_class($child), ':', $child->getMessage(), ';';
$child->reset($reset);
echo $child->getMessage(), ':', $child->getCode(), ';';
$first = [4 => 'two']; $second = [7 => 9];
$pair = new ErrorException(...$first, ...$second);
echo $pair->getMessage(), ':', $pair->getCode(), ';';
$previous = new Exception('cause');
$details = [9 => 'details', 8 => 3, 7 => E_WARNING, 6 => 'spread.php', 5 => 27, 4 => $previous];
$class = ErrorException::class;
$dynamic = new $class(...$details);
echo $dynamic->getMessage(), ':', $dynamic->getCode(), ':', $dynamic->getSeverity(), ':',
    $dynamic->getFile(), ':', $dynamic->getLine(), ':',
    $dynamic->getPrevious() === $previous ? 'same;' : 'bad;';
streamFailure(['unknown' => 1]);
streamFailure(['message' => 'bad', 9 => 1]);
streamFailure([9 => 'bad', 'message' => 'again']);
streamFailure([9 => 'm', 2 => 0, 3 => 1, 4 => null, 5 => null, 6 => null, 7 => 'extra']);
unset($error, $child, $pair, $values, $reset, $first, $second, $previous, $details, $dynamic);
"#;

const STREAM_EXPECTED: &str = "fresh:named.php:0;again:7;StreamError:fresh;again:7;two:9;details:3:2:spread.php:27:same;Error:Unknown named parameter $unknown;Error:Cannot use positional argument after named argument during unpacking;Error:Named parameter $message overwrites previous argument;ArgumentCountError:ErrorException::__construct() expects at most 6 arguments, 7 given;";

/// Runtime unpack streams retain insertion order, lexical parents, and constructor failure cleanup.
#[test]
fn test_core_review_error_exception_unpack_stream() {
    let out = compile_and_run_with_heap_debug(&format!("<?php\n{STREAM_SOURCE}"));
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, STREAM_EXPECTED);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Opaque eval shares the numeric, named, inherited, and excess-argument unpack contract.
#[test]
fn test_core_review_error_exception_unpack_stream_eval() {
    let encoded = STREAM_SOURCE.replace('\\', "\\\\").replace('\'', "\\'");
    let out = compile_and_run_with_heap_debug(&format!("<?php $source = '{encoded}' . ' // ' . $argc; eval($source);"));
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, STREAM_EXPECTED);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}


const CALLABLE_SOURCE: &str = r#"
function coreInvokeConstructor(callable $callback, array $values): void { $callback(...$values); }
function coreConstructorOrderArray(): array { return ['message' => 'bad', 9 => 1]; }
$previous = new Exception('cause');
$error = new ErrorException('kept', 7, E_WARNING, 'base.php', 5, $previous);
$constructor = $error->__construct(...);
coreInvokeConstructor($constructor, [9 => 'fresh', 'line' => 27]);
echo $error->getMessage(), ':', $error->getCode(), ':', $error->getFile(), ':', $error->getLine(), ';';
coreInvokeConstructor($constructor, []);
echo $error->getMessage(), ':', $error->getLine(), ';';
coreInvokeConstructor($constructor, ['filename' => 'named.php', 'line' => null]);
echo $error->getMessage(), ':', $error->getCode(), ':', $error->getFile(), ':', $error->getLine(), ':',
    $error->getPrevious() === $previous ? 'same;' : 'bad;';
try { coreInvokeConstructor($constructor, ['unknown' => 1]); }
catch (Error $failure) { echo $failure->getMessage(), ';'; }
try { coreInvokeConstructor($constructor, [9 => 'bad', 'message' => 'again']); }
catch (Error $failure) { echo $failure->getMessage(), ';'; }
try { coreInvokeConstructor($constructor, ['message' => 'bad', 9 => 1]); }
catch (Error $failure) { echo $failure->getMessage(), ';'; }
try { call_user_func_array($constructor, coreConstructorOrderArray()); }
catch (Error $failure) { echo $failure->getMessage(), ';'; }
try { coreInvokeConstructor($constructor, [9 => 'm', 2 => 0, 3 => 1, 4 => null, 5 => null, 6 => null, 7 => 'extra']); }
catch (ArgumentCountError $failure) { echo $failure->getMessage(); }
unset($error, $previous, $constructor);
"#;

const CALLABLE_EXPECTED: &str = "fresh:7:base.php:27;fresh:27;:7:named.php:0:same;Unknown named parameter $unknown;Named parameter $message overwrites previous argument;Cannot use positional argument after named argument during unpacking;Cannot use positional argument after named argument;ErrorException::__construct() expects at most 6 arguments, 7 given";

/// Descriptor calls preserve the nullable line's physical width before the captured receiver.
#[test]
fn test_core_review_error_exception_callable_constructor_args() {
    let out = compile_and_run_with_heap_debug(&format!("<?php\n{CALLABLE_SOURCE}"));
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, CALLABLE_EXPECTED);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// Eval constructor callables retain the same defaults, named failures, and previous-object owner.
#[test]
fn test_core_review_error_exception_callable_constructor_args_eval() {
    let encoded = CALLABLE_SOURCE.replace('\\', "\\\\").replace('\'', "\\'");
    let out = compile_and_run_with_heap_debug(&format!("<?php $source = '{encoded}' . ' // ' . $argc; eval($source);"));
    assert!(out.success, "stdout: {}\nstderr: {}", out.stdout, out.stderr);
    assert_eq!(out.stdout, CALLABLE_EXPECTED);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
