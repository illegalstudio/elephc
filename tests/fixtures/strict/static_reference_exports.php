<?php
extern class ExportedReference { public int $tag; public ptr $child; public int $marker; }
extern class ExportedInteger { public int $tag; public int $value; public int $high; }
extern class ExportedPointer { public ptr $cell; }
extern function __elephc_eval_value_static_property_reference(string $class, int $classLength,
    string $property, int $propertyLength, string $scope, int $scopeLength): ptr;
extern function __elephc_eval_value_release_v3(ptr $value, ptr $error): int;
extern function __elephc_eval_value_reference_replace_v2(ptr $reference, ptr $value,
    int $flags, ptr $previous, ptr $error): int;
extern function __elephc_eval_value_int(int $value): ptr;
extern function __elephc_eval_value_array_new(int $capacity): ptr;
extern function malloc(int $size): ptr;
extern function free(ptr $value): void;
class ExportedStaticPlace {
    public static mixed $value = '7.9';
    public static int|string $choice = '8.9';
    protected static mixed $secret = 'hidden';
    public static int|string $missing;
}
class ExportedStaticChild extends ExportedStaticPlace {}
class ExportedRetiredValue { public function __destruct() { echo 'D'; } }
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
$previous = malloc(8);
$previousCell = ptr_cast<ExportedPointer>($previous);
$errorCell = ptr_cast<ExportedPointer>($error);
$input = __elephc_eval_value_int(23);
echo ':', __elephc_eval_value_reference_replace_v2($first, $input, 0, $previous, $error);
echo ':', ExportedStaticPlace::$value, ptr_get($previous) ? ':P' : ':X';
__elephc_eval_value_release_v3($previousCell->cell, $error);
__elephc_eval_value_release_v3($input, $error);
ExportedStaticPlace::$value = new ExportedRetiredValue();
$input = __elephc_eval_value_int(29);
echo ':', __elephc_eval_value_reference_replace_v2($first, $input, 0, $previous, $error);
echo 'A';
__elephc_eval_value_release_v3($previousCell->cell, $error);
echo 'B';
__elephc_eval_value_release_v3($input, $error);
$input = __elephc_eval_value_array_new(0);
echo ':', __elephc_eval_value_reference_replace_v2($choice, $input, 0, $previous, $error);
echo ':', ptr_get($previous), ':', ExportedStaticPlace::$choice;
__elephc_eval_value_release_v3($errorCell->cell, $previous);
ptr_set($error, 0);
__elephc_eval_value_release_v3($input, $error);
free($previous);
__elephc_eval_value_release_v3($first, $error);
__elephc_eval_value_release_v3($second, $error);
__elephc_eval_value_release_v3($choice, $error);
free($error);
