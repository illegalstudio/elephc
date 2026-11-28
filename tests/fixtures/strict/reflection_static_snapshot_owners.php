<?php
$code=$argc>0?'
function owned_static_value(): int {static $value=7;return $value;}
function make_owned_capture(int $value): callable {
    return static function() use($value): int {return $value;};
}
$callback=make_owned_capture(11);
$staticReflection=new ReflectionFunction("owned_static_value");
$captureReflection=new ReflectionFunction($callback);
for($index=0;$index<3;$index++){
    $staticSnapshot=$staticReflection->getStaticVariables();
    $captureSnapshot=$captureReflection->getClosureUsedVariables();
    echo $staticSnapshot["value"],":",$captureSnapshot["value"],"|";
}
':'';
eval($code);
gc_collect_cycles();
