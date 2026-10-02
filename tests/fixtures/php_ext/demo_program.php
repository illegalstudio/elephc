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
