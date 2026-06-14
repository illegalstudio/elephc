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
$positives = array_filter($numbers, "is_positive", ARRAY_FILTER_USE_VALUE);
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

class CapturedFormatter {
    public function __construct(private string $prefix) {}

    public function label(string $value, string $suffix = "!"): string {
        return $this->prefix . $value . $suffix;
    }
}

$captured_formatter = new CapturedFormatter("old:");
$captured_label = $captured_formatter->label(...);
$captured_formatter = new CapturedFormatter("new:");
echo "method callable captured receiver: " . $captured_label(value: "Ada") . "\n";
$captured_formatter_map = new CapturedFormatter("map-old:");
$captured_label_map = $captured_formatter_map->label(...);
$captured_formatter_map = new CapturedFormatter("map-new:");
$captured_mapped = array_map($captured_label_map, ["Ada", "Bob"]);
echo "method callable array_map captured receiver: " . $captured_mapped[0] . " " . $captured_mapped[1] . "\n";

$named_callbacks = [
    function($left, $right) { return ($left * 10) + $right; },
    function($right, $left) { return ($right * 100) + $left; }
];
$named_choice = 0;
$named_callback = $named_callbacks[$named_choice];
$named_args = ["right" => 2, "left" => 1];
echo "dynamic named call_user_func_array: " . call_user_func_array($named_callback, $named_args) . "\n";

$prefix = "captured:";
$captured_callbacks = [
    function(string $name) use ($prefix): string { return $prefix . $name; }
];
$prefix = "changed:";
echo "captured closure call_user_func_array: " . call_user_func_array($captured_callbacks[0], ["name" => "Ada"]) . "\n";
$map_prefix = "map:";
$map_callback = function(int $value) use ($map_prefix): string { return $map_prefix . $value; };
$map_prefix = "changed:";
$mapped_capture = array_map($map_callback, [1, 2]);
echo "captured closure array_map: " . $mapped_capture[0] . " " . $mapped_capture[1] . "\n";

function dynamic_string_sum(int $left, int $right): int {
    return $left + $right;
}
function callback_name_passthrough(string $name): string {
    return $name;
}
$string_callback = "DYNAMIC_STRING_SUM";
echo "dynamic string call_user_func: " . call_user_func($string_callback, 4, 5) . "\n";
$direct_string_callback = "DYNAMIC_STRING_SUM";
echo "dynamic string direct call: " . $direct_string_callback(6, 7) . "\n";

$builtin_callback = "STRLEN";
echo "dynamic builtin call_user_func: " . call_user_func($builtin_callback, "hello") . "\n";

class DynamicFormatter {
    public static function tag(string $prefix, int $value): string {
        return $prefix . ":" . $value;
    }

    public function wrap(string $value, string $suffix = ""): string {
        return "<" . $value . $suffix . ">";
    }
}

$static_string_callback = "DynamicFormatter::tag";
$static_string_args = ["value" => 7, "prefix" => "id"];
echo "dynamic static string call_user_func_array: " . call_user_func_array($static_string_callback, $static_string_args) . "\n";

function collect_labels($head = 5, ...$rest) {
    echo $head . ":" . count($rest);
}

$dynamic_collect = "COLLECT_LABELS";
echo "dynamic string defaults/variadic: ";
call_user_func_array($dynamic_collect, []);
echo "\n";

$dynamic_formatter = new DynamicFormatter();
$method_array_callback = [$dynamic_formatter, "wrap"];
echo "method array call_user_func: " . call_user_func($method_array_callback, "ok") . "\n";
echo "method array direct call: " . $method_array_callback(value: "direct") . "\n";
$dynamic_method_name = callback_name_passthrough("wrap");
$dynamic_method_array_callback = [$dynamic_formatter, $dynamic_method_name];
echo "method array runtime direct call: " . $dynamic_method_array_callback(value: "runtime") . "\n";
echo "method array runtime literal direct call: " . ([$dynamic_formatter, $dynamic_method_name])(value: "literal runtime") . "\n";
echo "literal method array direct call: " . ([$dynamic_formatter, "wrap"])(value: "literal direct") . "\n";
echo "method array literal call_user_func_array: " . call_user_func_array([$dynamic_formatter, "wrap"], ["value" => "lit"]) . "\n";
$dynamic_method_args = ["dyn"];
echo "method array dynamic call_user_func_array: " . call_user_func_array($method_array_callback, $dynamic_method_args) . "\n";
$dynamic_method_named_args = ["value" => "named"];
echo "method array dynamic assoc call_user_func_array: " . call_user_func_array($method_array_callback, $dynamic_method_named_args) . "\n";
$method_spread_args = ["spread"];
echo "method array spread call_user_func: " . call_user_func($method_array_callback, ...$method_spread_args) . "\n";
$method_spread_tail = [" tail"];
echo "method array positional spread call_user_func: " . call_user_func($method_array_callback, "lead", ...$method_spread_tail) . "\n";
$static_array_callback = [DynamicFormatter::class, "tag"];
echo "static method array direct call: " . $static_array_callback(value: 8, prefix: "direct") . "\n";
$dynamic_static_class = callback_name_passthrough(DynamicFormatter::class);
$dynamic_static_method = callback_name_passthrough("tag");
$dynamic_static_array_callback = [$dynamic_static_class, $dynamic_static_method];
echo "static method array runtime direct call: " . ($dynamic_static_array_callback)(value: 10, prefix: "runtime") . "\n";
echo "static method array runtime literal direct call: " . ([$dynamic_static_class, $dynamic_static_method])(value: 11, prefix: "literal runtime") . "\n";
echo "literal static method array direct call: " . ([DynamicFormatter::class, "tag"])(value: 9, prefix: "literal") . "\n";

function passthrough_args(mixed $value): mixed {
    return $value;
}

$opaque_method_args = passthrough_args(["opaque"]);
echo "method array mixed call_user_func_array: " . call_user_func_array($method_array_callback, $opaque_method_args) . "\n";
$opaque_method_named_args = passthrough_args(["value" => "opaque named"]);
echo "method array mixed assoc call_user_func_array: " . call_user_func_array($method_array_callback, $opaque_method_named_args) . "\n";

class InvokeFormatter {
    public function __invoke(string $value, string $suffix = ""): string {
        return "{" . $value . $suffix . "}";
    }
}

echo "invokable call_user_func: " . call_user_func(new InvokeFormatter(), "go") . "\n";
$direct_invokable = new InvokeFormatter();
echo "invokable direct call: " . $direct_invokable(value: "go", suffix: "?") . "\n";
echo "invokable expression direct call: " . (new InvokeFormatter())(value: "expr", suffix: "!") . "\n";
$invoke_spread_args = ["wide"];
echo "invokable spread call_user_func: " . call_user_func(new InvokeFormatter(), ...$invoke_spread_args) . "\n";
$invoke_spread_tail = ["!"];
echo "invokable positional spread call_user_func: " . call_user_func(new InvokeFormatter(), "wide", ...$invoke_spread_tail) . "\n";

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

class SelectedOffsetCallbacks {
    public $map_bonus;
    public $reduce_bonus;
    public $walk_bonus;

    public function __construct($map_bonus, $reduce_bonus, $walk_bonus) {
        $this->map_bonus = $map_bonus;
        $this->reduce_bonus = $reduce_bonus;
        $this->walk_bonus = $walk_bonus;
    }

    public function map_selected($item) {
        return $item + $this->map_bonus;
    }

    public function add_selected($carry, $item) {
        return $carry + $item + $this->reduce_bonus;
    }

    public function show_selected($item) {
        echo ($item + $this->walk_bonus) . " ";
    }
}

$small_offsets = new SelectedOffsetCallbacks(5, 1, 10);
$large_offsets = new SelectedOffsetCallbacks(20, 10, 20);
$use_small_offsets = false;
$selected_map = array_map($use_small_offsets ? $small_offsets->map_selected(...) : $large_offsets->map_selected(...), [1, 2]);
echo "selected callable array_map: " . $selected_map[0] . " " . $selected_map[1] . "\n";
echo "selected callable array_reduce: " . array_reduce([1, 2], $use_small_offsets ? $small_offsets->add_selected(...) : $large_offsets->add_selected(...), 0) . "\n";
echo "selected callable array_walk: ";
array_walk([1, 2], $use_small_offsets ? $small_offsets->show_selected(...) : $large_offsets->show_selected(...));
echo "\n";

class SelectedCalculator {
    public $base;

    public function __construct($base) {
        $this->base = $base;
    }

    public function scale($value = 1, $factor = 1) {
        return $this->base + ($value * $factor);
    }
}

$small_calc = new SelectedCalculator(10);
$large_calc = new SelectedCalculator(100);
$use_small_calc = false;
$scale_args = [3];
echo "selected callable named spread: " . ($use_small_calc ? $small_calc->scale(...) : $large_calc->scale(...))(...$scale_args, factor: 7) . "\n";
$stored_scale = $use_small_calc ? $small_calc->scale(...) : $large_calc->scale(...);
echo "stored selected callable named direct: " . $stored_scale(value: 2, factor: 4) . "\n";
$stored_scale_args = [2];
echo "stored selected callable named spread: " . $stored_scale(...$stored_scale_args, factor: 4) . "\n";

class SelectedBumper {
    public $step;

    public function __construct($step) {
        $this->step = $step;
    }

    public function bump(&$value) {
        $value = $value + $this->step;
    }
}

$small_bumper = new SelectedBumper(3);
$large_bumper = new SelectedBumper(7);
$stored_bump = $use_small_calc ? $small_bumper->bump(...) : $large_bumper->bump(...);
$stored_value = 5;
$stored_bump($stored_value);
echo "stored selected callable by-ref: " . $stored_value . "\n";

function run_named_bump(callable $cb) {
    $named_value = 5;
    $cb(value: $named_value);
    echo "callable param named by-ref: " . $named_value . "\n";
}

run_named_bump($stored_bump);

function run_spread_bump(callable $cb) {
    $spread_value = 5;
    $tail = [];
    $cb($spread_value, ...$tail);
    echo "callable param spread by-ref: " . $spread_value . "\n";
}

run_spread_bump($stored_bump);

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

class SelectedSorter {
    private bool $descending;

    public function __construct(bool $descending) {
        $this->descending = $descending;
    }

    public function compare($a, $b) {
        if ($this->descending) {
            return $b - $a;
        }
        return $a - $b;
    }
}

$ascending_sorter = new SelectedSorter(false);
$descending_sorter = new SelectedSorter(true);
$use_descending_sorter = true;
$selected_sorted = [1, 3, 2];
usort($selected_sorted, $use_descending_sorter ? $descending_sorter->compare(...) : $ascending_sorter->compare(...));
echo "selected callable usort: ";
foreach ($selected_sorted as $v) { echo $v . " "; }
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

// array_map over a heterogeneous (mixed) array: each element keeps its own runtime type.
// A closure that returns its parameter infers a `mixed` return type, so the string element
// survives instead of being coerced to an integer.
$mixed_values = [1, "two", 3.5, true];
$identity = array_map(function (mixed $value) { return $value; }, $mixed_values);
echo "mixed array_map identity: ";
foreach ($identity as $value) { echo $value . " "; }
echo "\n";
$mixed_types = array_map(fn($value) => gettype($value), $mixed_values);
echo "mixed array_map gettype: " . implode(", ", $mixed_types) . "\n";
