<?php
// Exercises the elephc_demo fixture through every value path. Its output is
// compared line for line with real PHP running the same extension.

echo demo_add(2), " ", demo_add(2, 40), "\n";
echo demo_greet("Ada"), " | ", demo_greet("Ada", "Hi"), "\n";
echo implode("/", demo_split("a,b,,c")), " ", count(demo_split("x")), " ", implode("+", demo_split("1--2--3", "--")), "\n";

$stats = demo_stats([3, 1, 2]);
echo $stats["count"], " ", $stats["sum"], " ", $stats["min"], " ", $stats["max"], " ", $stats["mean"], "\n";
$empty = demo_stats([]);
var_dump($empty["mean"]);

// Initialized as a bool: Elephc types a by-reference variable by its storage,
// and a write through a reference into a variable that has only ever held null
// is not yet carried back (see docs/beyond-php/php-extensions.md).
$flag = false;
echo demo_inc(41, $flag), " ", var_export($flag, true), "\n";
echo demo_inc(PHP_INT_MAX, $flag), " ", var_export($flag, true), "\n";
echo demo_inc(1), "\n";

try {
    demo_parse("12x");
} catch (DemoException $e) {
    echo get_class($e), " ", $e->getCode(), " ", $e->getMessage(), "\n";
}
echo demo_parse("-17"), "\n";
try {
    demo_fail();
} catch (OutOfRangeException $e) {
    echo get_class($e), " ", $e->getCode(), " ", $e->getMessage(), "\n";
}
$left = 10;
echo demo_consume(4, $left), " ", $left, "\n";
try {
    demo_consume(9, $left);
} catch (UnderflowException $e) {
    echo get_class($e), " ", $e->getCode(), " ", $e->getMessage(), " left=", $left, "\n";
}
try {
    demo_split("abc", "");
} catch (ValueError $e) {
    echo get_class($e), ": ", $e->getMessage(), "\n";
}

echo json_encode(demo_echo(["a" => [1, 2.5, "x"], "b" => null, 3 => true])), "\n";
var_dump(demo_echo(1.5), demo_echo("str"), demo_echo(false), demo_echo(null), demo_echo(-9));
// The largest integer key: its table has no next free key.
var_dump(demo_echo([PHP_INT_MAX => 1]));

$nested = demo_nested();
echo json_encode($nested), "\n";
echo $nested["object"]->label, " ", $nested["object"]->items[0], " ", $nested[7], "\n";

$object = demo_object("elephc");
echo $object->name, " ", $object->length, "\n";

$big = str_repeat("k", 1000);
for ($i = 0; $i < 3; $i++) {
    demo_keep(["i" => $i, "s" => $big . $i]);
}
$kept = demo_kept();
echo count($kept), " ", $kept[2]["i"], " ", strlen($kept[1]["s"]), " ", substr($kept[0]["s"], -2), "\n";

echo demo_calls(), "\n";
echo DEMO_ANSWER, " ", DEMO_NAME, " ", DEMO_RATIO, "\n";
var_dump(extension_loaded("elephc_demo"));

// An optional by-reference parameter followed by a by-value one: a literal
// may follow the variable, and either may be named.
$positions = [];
echo demo_count("abcabcab", "ab"), " ", demo_count("abcabcab", "ab", $positions, 2), " ", implode(",", $positions), "\n";
$named = [];
echo demo_count("aaaa", "a", offset: 1), " ", demo_count("aaaa", "aa", positions: $named), " ", implode(",", $named), "\n";

// DemoCounter: a class whose objects live in the engine, held by Elephc
// wrappers.
$counter = new DemoCounter(3);
$same = $counter->add(4)->add(1);
var_dump($same === $counter, $counter->value(), $counter instanceof DemoCounter);
$parsed = DemoCounter::fromString("40");
echo get_class($parsed), " ", $parsed->value(), " ", $counter->merge($parsed)->value(), "\n";
[$low, $high] = $counter->split();
echo $low->value(), " ", $high->value(), " ", get_class($high), "\n";
echo $counter->label, " ", DemoCounter::STEP, "\n";
$counter->label = "renamed";
echo $counter->label, " ", var_export(isset($counter->label), true), "\n";
try {
    $counter->add(-1);
} catch (ValueError $e) {
    echo get_class($e), ": ", $e->getMessage(), "\n";
}

class LoudCounter extends DemoCounter {
    public function shout(): string {
        return strtoupper($this->label) . "=" . $this->value();
    }
}
$loud = new LoudCounter(2);
echo $loud->add(5)->value(), " ", $loud->shout(), " ", get_class($loud), "\n";

function total(DemoCounter ...$counters): int {
    $sum = 0;
    foreach ($counters as $c) {
        $sum += $c->value();
    }
    return $sum;
}
echo total($counter, $parsed, $loud), "\n";
// A private constructor: only the factory makes one, and its state stays.
$token = DemoToken::issue("t-42");
echo get_class($token), " ", $token->id(), " ", $token->id, "\n";

// An exception code beyond 32 bits keeps its value.
try {
    demo_throw_code(4294967296);
} catch (DemoException $e) {
    echo $e->getCode(), " ", $e->getMessage(), "\n";
}

echo demo_late(), "\n";

// Callables the extension calls back: a closure with a capture, a function
// named by a string, and an exception that crosses the extension intact.
$prefix = "#";
var_dump(demo_apply(fn ($value, $key) => $prefix . $key . "=" . $value, ["a" => 1, "b" => 2]));
function tag(mixed $value, mixed $key): string {
    return "<$key:$value>";
}
var_dump(demo_apply('tag', [5, 6]));
$marker = new LogicException("from the callable", 42);
try {
    demo_apply(function ($value) use ($marker) {
        if ($value > 1) {
            throw $marker;
        }
        return $value;
    }, [1, 2, 3]);
} catch (LogicException $e) {
    var_dump($e === $marker, $e->getMessage(), $e->getCode());
}

// Optional parameters whose defaults only the C code knows, given ints.
echo implode(",", demo_slice([1, 2, 3, 4, 5])), " ", implode(",", demo_slice([1, 2, 3, 4, 5], 1)), " ",
    implode(",", demo_slice([1, 2, 3, 4, 5], 1, 2)), " ", implode(",", demo_slice([1, 2, 3], length: null, offset: 2)), "\n";

// foreach through the extension's own iterator.
$range = new DemoRange(3);
foreach ($range as $key => $value) {
    echo "$key=$value ";
}
echo "\n";
foreach ($range as $value) {
    if ($value === 10) {
        break;
    }
    echo "$value ";
}
echo "\n", demo_iterators_freed(), "\n";
var_dump(iterator_to_array(new DemoRange(2)), $range instanceof Traversable, iterator_count($range));
try {
    foreach (new DemoRange(-1) as $value) {
        echo "never\n";
    }
} catch (RuntimeException $e) {
    echo get_class($e), " ", $e->getCode(), " ", $e->getMessage(), "\n";
}
echo demo_iterators_freed(), "\n";

// clone goes through the class's clone handler; the copy is independent.
$original = new DemoCounter(4);
$original->label = "orig";
$copy = clone $original;
$copy->add(3);
$copy->label = "copy";
echo $original->value(), " ", $copy->value(), " ", $original->label, " ", $copy->label, "\n";
$copyOfCopy = clone $copy;
echo $copyOfCopy->add(1)->value(), " ", $copy->value(), "\n";
$token = DemoToken::issue("t1");
$tokenCopy = clone $token;
$tokenCopy->id = "t2";
echo $token->id, " ", $tokenCopy->id, "\n";
try {
    $range2 = clone $range;
} catch (Error $e) {
    echo get_class($e), ": ", $e->getMessage(), "\n";
}
$before = demo_counters_freed();
unset($original, $copy, $copyOfCopy);
echo demo_counters_freed() - $before, "\n";

// Scalars passed and read back without boxing, at their limits.
var_dump(demo_add(PHP_INT_MAX - 1), demo_add(PHP_INT_MIN, 0), demo_add(-5, -3), demo_add("7", true));
var_dump(demo_scale(1.25), demo_scale(-0.5, true), demo_scale(3, 1), demo_scale(1e308), demo_scale(negate: true, x: 2.5));
var_dump(demo_is_even(4), demo_is_even(-3), demo_is_even(PHP_INT_MIN));

$before = demo_counters_freed();
unset($low, $high);
echo demo_counters_freed() - $before, "\n";
