<?php
class CheckedSettypePlace { public mixed $value = '7.9'; public static mixed $shared = '8.9'; }
$cell = new CheckedSettypePlace(); $cast = settype(...);
$cast($cell->value, 'int'); echo $cell->value, '|';
settype($cell->value, 'string'); echo is_string($cell->value) ? 'S|' : 'X|';
$target = $argc > 0 ? 'integer' : 'int'; $cast(CheckedSettypePlace::$shared, $target);
echo CheckedSettypePlace::$shared, '|';
mixed $items = ['k' => '9.9']; $copy = $items;
$name = $argc > 0 ? 'settype' : 'SETTYPE'; $name($items['k'], $target);
echo $items['k'], ':', $copy['k'], '|';
mixed $nested = ['inner' => ['value' => 'junk']];
try { $cast($nested['inner']['value'], 'float'); echo 'X'; }
catch (ValueError $error) { echo $nested['inner']['value'], '|'; }
$alias =& $cell->value; $cell->value = 'junk';
try { $cast($cell->value, 'float'); echo 'X'; }
catch (ValueError $error) { echo $alias, ':', $cell->value; }

function mutateSelectedPlace(mixed &$items): int { $items['k'] = 'later'; return 0; }
function readSelectedPlace(mixed $value, int $ignored): void { echo $value; }
mixed $readItems = ['k' => 'before']; $reader = $argc > 0 ? 'readSelectedPlace' : 'READSELECTEDPLACE';
echo '|'; $reader($readItems['k'], mutateSelectedPlace($readItems)); echo ':', $readItems['k'];
$cell->value = ['original'];
set_error_handler(function (int $level, string $message) use ($cell): bool { $cell->value = 'callback'; echo 'W'; return true; });
$cast($cell->value, 'string'); echo '|', $cell->value; restore_error_handler();
$cell->value = ['original']; $peer = $cell->value;
set_error_handler(function () use ($cell): bool { $cell->value = 'callback'; throw new ValueError('stop'); });
try { $cast($cell->value, 'string'); echo 'X'; } catch (ValueError $error) { echo '|V', $cell->value, ':', $peer[0]; }
restore_error_handler();
