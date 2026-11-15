<?php
extern class ExportedReference { public int $tag; public ptr $child; public int $marker; }
extern class ExportedInteger { public int $tag; public int $value; public int $high; }
extern function __elephc_eval_value_static_property_reference(string $class, int $classLength,
    string $property, int $propertyLength, string $scope, int $scopeLength): ptr;
extern function __elephc_eval_value_release_v3(ptr $value, ptr $error): int;
extern function malloc(int $size): ptr;
extern function free(ptr $value): void;
class ExportedStaticPlace {
    public static mixed $value = '7.9';
    public static int|string $choice = '8.9';
    protected static mixed $secret = 'hidden';
    public static int|string $missing;
}
class ExportedStaticChild extends ExportedStaticPlace {}
$error = malloc(8); ptr_set($error, 0);
$class = ExportedStaticChild::class;
$first = __elephc_eval_value_static_property_reference($class, strlen($class), 'value', 5, '', 0);
$second = __elephc_eval_value_static_property_reference($class, strlen($class), 'value', 5, '', 0);
$choice = __elephc_eval_value_static_property_reference($class, strlen($class), 'choice', 6, '', 0);
if ($first && $second && $choice) {
    echo $first === $second ? 'I' : 'X';
    $alias =& ExportedStaticPlace::$value;
    settype(ExportedStaticPlace::$value, 'int');
    settype(ExportedStaticChild::$choice, 'int');
    $reference = ptr_cast<ExportedReference>($first);
    $integer = ptr_cast<ExportedInteger>($reference->child);
    echo ':', $reference->tag, ':', $reference->marker, ':', $integer->value;
    ExportedStaticPlace::$value = 19;
    $integer = ptr_cast<ExportedInteger>($reference->child);
    echo ':', $integer->value, ':', $alias;
    $reference = ptr_cast<ExportedReference>($choice);
    $integer = ptr_cast<ExportedInteger>($reference->child);
    echo ':', $integer->value;
    try { settype(ExportedStaticPlace::$choice, 'array'); echo 'X'; }
    catch (TypeError $failure) { echo ':R', ExportedStaticPlace::$choice; }
} else { echo 'missing'; }
$secret = __elephc_eval_value_static_property_reference($class, strlen($class), 'secret', 6, '', 0);
$missing = __elephc_eval_value_static_property_reference($class, strlen($class), 'missing', 7, '', 0);
echo $secret || $missing ? ':X' : ':guarded';
__elephc_eval_value_release_v3($first, $error);
__elephc_eval_value_release_v3($second, $error);
__elephc_eval_value_release_v3($choice, $error);
free($error);
