<?php
class IterableConversionPlace {
    public iterable $values = [1, 2];
    public ?iterable $optional = null;
    public iterable $missing;
}
$box = new IterableConversionPlace();
echo json_encode($box), '|';
$initialProjection = (array)$box;
echo json_encode($initialProjection), '|';
$roundTrip = unserialize(serialize($box));
echo json_encode($roundTrip), '|';
$values =& $box->values;
$optional =& $box->optional;
$cast = settype(...);
settype($box->values, 'array');
echo implode(',', (array)$values);
try { $cast($box->values, 'object'); echo 'X'; }
catch (TypeError $error) { echo '|T', implode(',', (array)$values); }
function replaceIterableConversionValue(mixed &$value, mixed $replacement): void {
    $value = $replacement;
}
$replace = substr('replaceIterableConversionValue', 0, strlen($argv[0]) + 1024);
$replace($box->values, new ArrayIterator([3]));
echo '|I';
foreach ($values as $entry) { echo $entry; }
$projection = (array)$box;
echo '|', get_class($projection['values']);
$replace($box->values, ['next']);
echo '|', implode(',', (array)$values);
echo '|', implode(',', $projection['values']);
$box->values = [5];
echo '|', implode(',', (array)$values);
settype($box->optional, 'array');
echo '|', count((array)$optional);
try { $replace($box->values, new stdClass()); echo 'X'; }
catch (TypeError $error) { echo '|R', implode(',', (array)$values); }
