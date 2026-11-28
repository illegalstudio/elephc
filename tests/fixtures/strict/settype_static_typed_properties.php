<?php
class TypedStaticCells {
    public static int $number = 7;
    public static float $fraction = 8.9;
    public static bool $ready = true;
    public static string $text = '19.9';
    public static ?int $optional = 11;
    public static stdClass $object;
    public static iterable $items;
    public static string $missing;
}
class TypedStaticChild extends TypedStaticCells {}
$number =& TypedStaticChild::$number;
$fraction =& TypedStaticCells::$fraction;
$text =& TypedStaticCells::$text;
$optional =& TypedStaticCells::$optional;
settype(TypedStaticChild::$number, 'string');
echo gettype($number), ':', $number;
try { settype(TypedStaticCells::$number, 'array'); echo 'X'; }
catch (TypeError $error) { echo '|A', $number; }
$convert = settype(...);
$convert(TypedStaticCells::$fraction, 'int');
echo '|', gettype($fraction), ':', $fraction;
$convert(TypedStaticCells::$text, 'int');
echo '|', gettype($text), ':', $text;
try { $convert(TypedStaticCells::$ready, 'array'); echo 'X'; }
catch (TypeError $error) { echo '|B'; }
settype(TypedStaticCells::$optional, 'null');
echo $optional === null ? '|N' : '|X';
TypedStaticCells::$optional = 12;
echo '|', $optional;
TypedStaticCells::$object = new stdClass();
$object =& TypedStaticCells::$object;
try { settype(TypedStaticCells::$object, 'array'); echo 'X'; }
catch (TypeError $error) { echo '|O', get_class($object); }
TypedStaticCells::$items = [2, 3];
$items =& TypedStaticCells::$items;
settype(TypedStaticCells::$items, 'array');
echo '|', implode(',', (array)$items);
TypedStaticCells::$items = new ArrayIterator([4]);
echo '|I';
foreach ($items as $entry) { echo $entry; }
function replaceTypedStaticValue(mixed &$value, mixed $replacement): void {
    echo 'W'; $value = $replacement; echo 'P';
}
$replace = substr('replaceTypedStaticValue', 0, strlen($argv[0]) + 1024);
$replace(TypedStaticCells::$number, '25');
echo '|', $number;
try { $replace(TypedStaticCells::$number, []); echo 'X'; }
catch (TypeError $error) { echo '|R', $number; }
$reflection = new ReflectionClass(TypedStaticCells::class);
$reflection->setStaticPropertyValue('number', 31);
echo '|', $reflection->getStaticPropertyValue('number'), ':', $number;
TypedStaticCells::$text = TypedStaticCells::$text;
echo '|', $text;
try { settype(TypedStaticCells::$missing, 'int'); echo 'X'; }
catch (Error $error) { echo '|U'; }
gc_collect_cycles();
