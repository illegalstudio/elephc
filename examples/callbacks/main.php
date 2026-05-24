<?php

// Callback-based array functions demo

function double($x) { return $x * 2; }
function is_positive($x) { return $x > 0; }
function sum($carry, $item) { return $carry + $item; }
function compare($a, $b) { return $a - $b; }
function show($x) { echo "  " . $x . "\n"; }

$numbers = [3, -1, 4, -5, 2, -3, 1];

// array_map: transform each element
$doubled = array_map("double", $numbers);
echo "Doubled: ";
foreach ($doubled as $v) { echo $v . " "; }
echo "\n";

// array_filter: keep only matching elements
$positives = array_filter($numbers, "is_positive");
echo "Positives: ";
foreach ($positives as $v) { echo $v . " "; }
echo "\n";

// array_reduce: fold into a single value
$total = array_reduce($numbers, "sum", 0);
echo "Sum: " . $total . "\n";

// usort: sort with custom comparator
$sorted = [5, 2, 8, 1, 9];
usort($sorted, "compare");
echo "Sorted: ";
foreach ($sorted as $v) { echo $v . " "; }
echo "\n";

// array_walk: apply side-effect to each element
echo "Walk:\n";
$items = [10, 20, 30];
array_walk($items, "show");

// call_user_func: indirect function call
$result = call_user_func("double", 21);
echo "call_user_func(double, 21) = " . $result . "\n";

class Formatter {
    public function bracket(string $value): string {
        return "[" . $value . "]";
    }
}

$formatter = new Formatter();
$format = $formatter->bracket(...);
echo "method callable: " . $format("ok") . "\n";
$formatted = array_map($format, ["a", "b"]);
echo "method callable array_map: ";
foreach ($formatted as $v) { echo $v . " "; }
echo "\n";
$format_args = ["value" => "cb"];
echo "method callable call_user_func_array: " . call_user_func_array($format, $format_args) . "\n";

$named_callbacks = [
    function($left, $right) { return ($left * 10) + $right; },
    function($right, $left) { return ($right * 100) + $left; }
];
$named_choice = 0;
$named_callback = $named_callbacks[$named_choice];
$named_args = ["right" => 2, "left" => 1];
echo "dynamic named call_user_func_array: " . call_user_func_array($named_callback, $named_args) . "\n";

function dynamic_string_sum(int $left, int $right): int {
    return $left + $right;
}
$string_callback = "DYNAMIC_STRING_SUM";
echo "dynamic string call_user_func: " . call_user_func($string_callback, 4, 5) . "\n";

function bump(&$value) {
    $value = $value + 1;
}

$bump = bump(...);
$counter_value = 10;
call_user_func_array($bump, [$counter_value]);
echo "call_user_func_array by-ref: " . $counter_value . "\n";

$trim = trim(...);
echo "builtin callable trim: " . $trim("  ready  ") . "\n";

class OffsetCallbacks {
    public function add_offset($carry, $item) {
        return $carry + $item + 10;
    }

    public function show_shifted($item) {
        echo ($item + 5) . " ";
    }

    public function descending($a, $b) {
        return $b - $a;
    }
}

$offsets = new OffsetCallbacks();
echo "method callable array_reduce: " . array_reduce([1, 2], $offsets->add_offset(...), 0) . "\n";
echo "method callable array_walk: ";
array_walk([1, 2], $offsets->show_shifted(...));
echo "\n";
$method_sorted = [1, 3, 2];
usort($method_sorted, $offsets->descending(...));
echo "method callable usort: ";
foreach ($method_sorted as $v) { echo $v . " "; }
echo "\n";
$method_key_sorted = [1, 3, 2];
uksort($method_key_sorted, $offsets->descending(...));
echo "method callable uksort: ";
foreach ($method_key_sorted as $v) { echo $v . " "; }
echo "\n";
$method_value_sorted = [1, 3, 2];
uasort($method_value_sorted, $offsets->descending(...));
echo "method callable uasort: ";
foreach ($method_value_sorted as $v) { echo $v . " "; }
echo "\n";

class Labeler {
    public static function current() {
        $label = static::name(...);
        return $label();
    }

    public static function name() {
        return "base";
    }
}

class LoudLabeler extends Labeler {
    public static function name() {
        return "loud";
    }
}

echo "static callable: " . Labeler::current() . "/" . LoudLabeler::current() . "\n";

// Function string lookups are case-insensitive, like PHP.
if (function_exists("DOUBLE")) {
    echo "function 'double' exists\n";
}
if (is_callable("DoUbLe")) {
    echo "function 'double' is callable\n";
}
if (!function_exists("nonexistent")) {
    echo "function 'nonexistent' does not exist\n";
}

// is_callable: dynamic strings, method arrays, static method arrays, and invokable objects
class Runner {
    public function run() {
        return "running";
    }
}

class InvokableRunner {
    public function __invoke() {
        return "invoked";
    }
}

class StaticRunner {
    public static function run() {
        return "static";
    }
}

$callback_name = "double";
$static_callback_name = "StaticRunner::run";
$runner = new Runner();
$method_callback = [$runner, "run"];
$static_method_callback = [StaticRunner::class, "run"];
$invokable_runner = new InvokableRunner();

echo "is_callable dynamic string: " . (is_callable($callback_name) ? "yes" : "no") . "\n";
echo "is_callable static string: " . (is_callable($static_callback_name) ? "yes" : "no") . "\n";
echo "is_callable method array: " . (is_callable($method_callback) ? "yes" : "no") . "\n";
echo "is_callable static method array: " . (is_callable($static_method_callback) ? "yes" : "no") . "\n";
echo "is_callable invokable object: " . (is_callable($invokable_runner) ? "yes" : "no") . "\n";
