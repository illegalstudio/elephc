<?php
class ArrayConversionPlace {
    public array $values = [1, 2];
    public array $named = ['n' => 3];
    public ?array $optional = null;
}
$box = new ArrayConversionPlace();
$values =& $box->values;
$named =& $box->named;
$optional =& $box->optional;
$snapshot = $box->values;
$cast = settype(...);
settype($box->values, 'array');
echo implode(',', $values);
try { $cast($box->values, 'object'); echo 'X'; }
catch (TypeError $error) { echo '|A', $values[0]; }
$cast($box->named, 'array');
echo '|', $named['n'];
settype($box->optional, 'array');
echo '|', count($optional);
function replaceArrayConversionValue(mixed &$value, mixed $replacement): void {
    echo 'W';
    $value = $replacement;
}
$replace = substr('replaceArrayConversionValue', 0, strlen($argv[0]) + 1024);
$replace($box->values, ['text']);
echo is_string($values[0]) ? '|S' : '|X';
echo ':', $values[0];
$replace($box->named, ['other' => 'kept']);
echo '|', $named['other'];
echo '|P', $box->values[0], ':', $box->named['other'];
echo '|C', $snapshot[0];
$valuesBeforeMutation = $box->values;
$namedBeforeMutation = $box->named;
$projection = (array)$box;
array_push($box->values, 'tail');
$box->named['other'] = 'changed';
echo '|M', implode(',', $values), ':', $named['other'];
echo '|V', implode(',', $projection['values']);
echo '|B', implode(',', $valuesBeforeMutation), ':', $namedBeforeMutation['other'];
echo '|J', json_encode($box);
try { $replace($box->values, new stdClass()); echo 'X'; }
catch (TypeError $error) { echo '|R', $values[0]; }
