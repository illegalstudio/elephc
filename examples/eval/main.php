<?php
function compiled_add($left, $right) { return $left + $right; }

$x = 1;
$profile = ["name" => "Ada"];
$result = eval('$x = $x + 2; $created = "dynamic"; return $x + 4;');
eval('$profile["name"] = "Grace";');
eval('if ($x >= 3) { echo "x>=3\n"; }');
eval('if ($x < 0) { echo "negative\n"; } elseif ($x == 3) { echo "x==3\n"; }');
eval('foreach ([1, 2] as $n) { echo "n=" . $n . "\n"; }');
$meta = eval('return ["source" => "eval"];');
$meta_count = eval('return count($meta);');
eval('function plus_one($value) { return $value + 1; }');
$dynamic_call = eval('return plus_one(4);');
$eval_native_call = eval('return compiled_add(2, 8);');
eval('function native_add($left, $right) { return $left + $right; }');
eval('function native_double($value) { return $value * 2; }');

echo "x=" . $x . "\n";
echo "created=" . $created . "\n";
echo "name=" . $profile["name"] . "\n";
echo "source=" . $meta["source"] . "\n";
echo "meta-count=" . $meta_count . "\n";
echo "dynamic-call=" . $dynamic_call . "\n";
echo "eval-native-call=" . $eval_native_call . "\n";
echo "native-dynamic-call=" . native_add(40, 2) . "\n";
echo "call-user-func=" . call_user_func('native_double', 6) . "\n";
echo "function-exists=" . (function_exists('native_double') ? "yes" : "no") . "\n";
echo "result=" . $result . "\n";
