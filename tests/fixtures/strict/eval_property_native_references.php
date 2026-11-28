<?php
function exactEvalNativeWrite(mixed &$value, mixed $next): void {
    echo "B"; $value = $next; echo "A";
}
function exactEvalNativeCatch(mixed &$value): void {
    echo "B";
    try { $value = 7; echo "X"; } catch (TypeError $error) { echo "C"; }
    echo "E";
}
function exactEvalNativeNested(mixed &$value): void {
    $source = is_float($value) ? '$value = 7; echo "X";' : '';
    eval($source);
    echo "A";
}
$source = $argc > 0 ? '
class ExactEvalNativeSources {
    public float $value = 1.5;
    public int|float $number = 1.5;
    public static float $shared = 1.5;
}
$object = new ExactEvalNativeSources(); $alias = 1.5;
$object->value =& $alias; $object->number =& $alias;
try { exactEvalNativeWrite($alias, 7); } catch (TypeError $error) { echo "T"; }
echo $alias, ":"; exactEvalNativeCatch($alias); echo $alias, ":";
try { exactEvalNativeNested($alias); } catch (TypeError $error) { echo "T"; }
echo $alias, ":";
exactEvalNativeWrite($alias, 2.5); echo $object->value, ":", $object->number, ":";
unset($object->value); exactEvalNativeWrite($alias, 7); echo gettype($alias), ":", $object->number, "|";
$static = 1.5; ExactEvalNativeSources::$shared =& $static;
try { exactEvalNativeWrite($static, 7); } catch (TypeError $error) { echo "T"; }
echo $static, ":", ExactEvalNativeSources::$shared, "|";
$orphan = 1.5; $temporary = new ExactEvalNativeSources(); $temporary->value =& $orphan;
unset($temporary); exactEvalNativeWrite($orphan, 7); echo gettype($orphan), ":", $orphan;
' : '';
eval($source);
