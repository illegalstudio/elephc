<?php
class ExactReferenceValues {
    public static float $number = 1.5;
    public static int|string $choice = '7';
    public float $value = 2.5;
    public int|string $union = '8';
}
$staticAlias =& ExactReferenceValues::$number;
$choiceAlias =& ExactReferenceValues::$choice;
$object = new ExactReferenceValues();
$instanceAlias =& $object->value;
$unionAlias =& $object->union;
try { settype(ExactReferenceValues::$number, 'int'); echo 'X'; }
catch (TypeError $error) { echo 'S', gettype($staticAlias), ':', $staticAlias; }
try { settype($object->value, 'int'); echo 'X'; }
catch (TypeError $error) { echo '|I', gettype($instanceAlias), ':', $instanceAlias; }
$convert = settype(...);
try { $convert($staticAlias, 'string'); echo 'X'; }
catch (TypeError $error) { echo '|A', $staticAlias; }
$convert(ExactReferenceValues::$choice, 'int');
echo '|', gettype($choiceAlias), ':', $choiceAlias;
$convert($object->union, 'int');
echo '|', gettype($unionAlias), ':', $unionAlias;
ExactReferenceValues::$number = 3.5;
$object->value = 4.5;
echo '|', $staticAlias, ':', $instanceAlias;
echo '|', (int)'junk';
unset($error);
gc_collect_cycles();
