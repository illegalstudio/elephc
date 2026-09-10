//! Purpose:
//! Verifies numeric array aggregation through boxed PHP arrays and callable boundaries.
//!
//! Called from:
//! - The runtime GC codegen suite on every executable target.
//!
//! Key details:
//! - Results retain PHP integer or float tags, including empty arrays and integer overflow.
//! - Warning callbacks must not invalidate source snapshots or leak partial numeric results.

use crate::support::*;

/// Declared arrays, heterogeneous hashes and empty float arrays preserve numeric result tags.
#[test]
fn test_core_boxed_array_aggregates_layouts_and_empty_types() {
    assert_clean_aggregate(r#"<?php
function sumNumbers(array $items): mixed { return \ArRaY_SuM($items); }
function productNumbers(array $items): mixed { return array_product($items); }
function emptyFloats(): array { $items = [1.5]; array_pop($items); return $items; }
function integerTotal(int ...$items): int { return array_sum($items); }
function floatTotal(array $items): float { return array_sum($items); }
$items = ["first" => 1.5, "removed" => 99, "last" => "2.25"];
unset($items["removed"]);
echo sumNumbers($items), ":", productNumbers($items), "|";
echo array_sum([1.5, 2.25]), ":", array_product([1.5, 2.0]), "|";
echo array_sum([1, true, null]), ":", array_product([2, true, null]), "|";
$sum = array_sum(emptyFloats());
$product = array_product(emptyFloats());
echo gettype($sum), ":", $sum, ":", gettype($product), ":", $product, "|";
echo integerTotal(1, 2, 3), ":", floatTotal([1.5, 2.25]);
unset($items, $sum, $product);
"#, "3.75:3.375|3.75:3|2:0|integer:0:integer:1|6:3.75");
}

/// Exact decimal integers avoid double rounding and overflowing arithmetic promotes to float.
#[test]
fn test_core_boxed_array_aggregates_numeric_strings_and_overflow() {
    assert_clean_aggregate(r#"<?php
function numericStrings(array $values): mixed { return array_sum($values); }
echo numericStrings(["9007199254740993"]), "|";
echo numericStrings(["9223372036854775807"]), "|";
echo numericStrings(["-9223372036854775808"]), "|";
echo numericStrings([" +1.5 ", "2e1", ".25"]), "|";
echo array_product(["1.5", "2e1"]), "|";
echo is_float(array_sum([PHP_INT_MAX, 1])) ? "sum-float|" : "bad|";
echo is_float(array_product([PHP_INT_MAX, 2])) ? "product-float|" : "bad|";
echo is_float(numericStrings(["9223372036854775808"])) ? "string-float|" : "bad|";
$long = str_repeat("0", 5000) . "1.5";
echo numericStrings([$long]);
unset($long);
"#, "9007199254740993|9223372036854775807|-9223372036854775808|21.75|30|sum-float|product-float|string-float|1.5");
}

/// Direct, named, spread, first-class and runtime-selected callables share boxed results.
#[test]
fn test_core_boxed_array_aggregates_callable_matrix() {
    assert_clean_aggregate(r#"<?php
function aggregateItems(): array { return ["one" => 1.5, "two" => "2"]; }
function selectAggregate(bool $product): string { return $product ? "array_product" : "array_sum"; }
$sum = array_sum(...);
$product = array_product(...);
echo $sum(aggregateItems()), ":", $product(aggregateItems()), "|";
echo call_user_func("array_sum", aggregateItems()), ":", call_user_func("array_product", aggregateItems()), "|";
echo array_sum(...["array" => aggregateItems()]), ":", array_product(array: aggregateItems()), "|";
$callback = selectAggregate($argc === 1);
echo $callback(aggregateItems()), "|";
$callback = selectAggregate($argc !== 1);
echo $callback(aggregateItems());
unset($callback, $sum, $product);
"#, "3.5:3|3.5:3|3.5:3|3|3.5");
}

/// Unsupported entries warn once, numeric prefixes still contribute, and objects use class names.
#[test]
fn test_core_boxed_array_aggregates_warning_types_and_numeric_prefixes() {
    assert_clean_aggregate(r#"<?php
class AggregateUnsupported {}
set_error_handler(function(int $level, string $message): bool { echo $level, ":", $message, "|"; return true; });
echo array_sum([1, "bad", [2], new AggregateUnsupported(), "2.5suffix", "3\0tail"]), "|";
echo array_product([2, "bad", 4]), "|";
restore_error_handler();
echo "done";
"#, "2:array_sum(): Addition is not supported on type string|2:array_sum(): Addition is not supported on type array|2:array_sum(): Addition is not supported on type AggregateUnsupported|2:A non-numeric value encountered|2:A non-numeric value encountered|6.5|2:array_product(): Multiplication is not supported on type string|0|done");
}

/// Resource fallback uses the PHP resource id and never consumes or closes its borrowed stream.
#[test]
fn test_core_boxed_array_aggregates_resource_identity() {
    assert_clean_aggregate(r#"<?php
set_error_handler(function(int $level, string $message): bool { echo str_contains($message, "resource") ? "resource|" : "bad|"; return true; });
$stream = fopen("php://memory", "w+");
$sum = array_sum([$stream]);
$product = array_product([2, $stream]);
restore_error_handler();
echo $sum === get_resource_id($stream) ? "sum|" : "bad|";
echo $product === 2 * get_resource_id($stream) ? "product|" : "bad|";
fwrite($stream, "live");
rewind($stream);
echo fread($stream, 4);
fclose($stream);
unset($stream, $sum, $product);
"#, "resource|resource|sum|product|live");
}

/// Source replacement and nested numeric parsing cannot invalidate the outer iteration snapshot.
#[test]
fn test_core_boxed_array_aggregates_warning_mutation_keeps_snapshot() {
    assert_clean_aggregate(r#"<?php
function aggregateSnapshot(): array { return ["first" => 1.5, "bad" => "invalid", "last" => "2.25"]; }
$items = aggregateSnapshot();
set_error_handler(function(int $level, string $message) use (&$items): bool {
    $items = [100];
    echo array_product(["1.5", "2"]), "|";
    return true;
});
$sum = array_sum($items);
restore_error_handler();
echo $sum, ":", $items[0];
unset($sum, $items);
"#, "3|3.75:100");
}

/// Throwing warning handlers retire the partial carry and the temporary source for both operations.
#[test]
fn test_core_boxed_array_aggregates_warning_throw_retires_owners() {
    assert_clean_aggregate(r#"<?php
function aggregateThrowSource(): array { return [str_repeat("1", 3), "bad", 2.5]; }
set_error_handler(function(int $level, string $message): bool { throw new RuntimeException("aggregate"); });
try { array_sum(aggregateThrowSource()); }
catch (RuntimeException $error) { echo $error->getMessage(), "|"; unset($error); }
try { array_product(aggregateThrowSource()); }
catch (RuntimeException $error) { echo $error->getMessage(), "|"; unset($error); }
restore_error_handler();
echo "done";
"#, "aggregate|aggregate|done");
}

/// Discarded sums and products keep warning effects and reject non-array Mixed arguments.
#[test]
fn test_core_boxed_array_aggregates_discarded_calls_and_invalid_sources() {
    let source = r#"<?php
function aggregateInvalid(mixed $source): void {
    try { array_sum($source); echo "missed|"; }
    catch (TypeError $error) { echo "sum-error|"; unset($error); }
    try { array_product($source); echo "missed|"; }
    catch (TypeError $error) { echo "product-error|"; unset($error); }
}
$warnings = 0;
set_error_handler(function(int $level, string $message) use (&$warnings): bool { $warnings++; return true; });
array_sum(["invalid"]);
array_product([[]]);
restore_error_handler();
echo $warnings, "|";
aggregateInvalid(null);
aggregateInvalid(42);
$scalar = match($argc) { 1 => 1, default => "invalid" };
try { array_sum($scalar); echo "missed|"; }
catch (TypeError $error) { echo "match-error|"; unset($error); }
echo "done";
"#;
    assert_eq!(compile_and_run(source), "2|sum-error|product-error|sum-error|product-error|match-error|done");
}

/// Pre-8.3 profiles keep legacy conversions without introducing newer warning callbacks.
#[test]
fn test_core_boxed_array_aggregates_php82_has_no_new_warnings() {
    let source = r#"<?php
set_error_handler(function(int $level, string $message): bool { echo "unexpected|"; return true; });
echo array_sum([1, "bad", [], "2.5suffix"]), ":", array_product([2, "bad"]);
restore_error_handler();
"#;
    assert_eq!(compile_and_run_with_php_version(source, elephc::php_version::PhpVersion::Php82), "3.5:0");
}

/// Checks native and tagged representations while requiring clean numeric and source ownership.
fn assert_clean_aggregate(source: &str, expected: &str) {
    let (out, assembly) = compile_and_run_with_heap_debug_and_asm(source);
    assert!(out.success, "stdout={:?}\nstderr={}\n{assembly}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}\n{assembly}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}\n{assembly}", out.stderr);
    assert_eq!(compile_and_run_tagged(source), expected);
}
