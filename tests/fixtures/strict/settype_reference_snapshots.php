<?php
function collectionSnapshotObservations(): void {
    $value = null; $alias =& $value; $cast = settype(...);
    $cast($value, 'array'); echo count($alias), '|';
    $value = 'item'; $cast($value, 'array'); echo $alias[0], '|';
    $value = ['name' => 'kept', 2 => 'number']; $cast($value, 'object');
    echo $alias->name, ':', $alias->{'2'}, '|';
    $same = $value; $name = 'settype'; $name($value, 'OBJECT');
    echo $same === $alias ? 'I|' : 'X|';
    $name($value, 'array'); echo $alias['name'], ':', $alias[2], '|';
    $value = null; $cast($value, 'object'); echo is_object($alias) ? 'N|' : 'X|';
    $value = 42; settype($value, 'object'); echo $alias->scalar, '|';
    $items = ['k' => ['name' => 'kept']]; $copy = $items;
    $cast($items['k'], 'object'); echo $items['k']->name, ':', $copy['k']['name'];
}
collectionSnapshotObservations();
return true;
