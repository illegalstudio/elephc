<?php
function builtinKeyInputs(mixed $key): void {
    $items = ['0' => 31, 0 => 10, 1 => 20, '' => 30];
    try { echo array_key_exists($key, $items) ? 'd|' : 'm|'; }
    catch (TypeError $error) { echo 'D|'; unset($error); }
    try { echo \ARRAY_KEY_EXISTS(array: $items, key: $key) ? 'n|' : 'm|'; }
    catch (TypeError $error) { echo 'N|'; unset($error); }
    try { echo array_key_exists(...['array' => $items, 'key' => $key]) ? 's|' : 'm|'; }
    catch (TypeError $error) { echo 'S|'; unset($error); }
    $call = array_key_exists(...);
    try { echo $call($key, $items) ? 'c|' : 'm|'; }
    catch (TypeError $error) { echo 'C|'; unset($error); }
    try { echo call_user_func('array_key_exists', $key, $items) ? 'u|' : 'm|'; }
    catch (TypeError $error) { echo 'U|'; unset($error); }
    try { echo isset($items[$key]) ? 'i|' : 'm|'; }
    catch (TypeError $error) { echo 'I|'; unset($error); }
    try { echo empty($items[$key]) ? 'm|' : 'e|'; }
    catch (TypeError $error) { echo 'E|'; unset($error); }
    try { echo $items[$key] ?? 99, '|'; }
    catch (TypeError $error) { echo 'Q|'; unset($error); }
}
function builtinProducedKeys(): void {
    foreach (array_fill_keys(['0', '01', '0'], 7) as $key => $value) { echo gettype($key), ':', $key, ':', $value, '|'; }
    foreach (array_combine(['0', '01'], [11, 22]) as $key => $value) { echo gettype($key), ':', $key, ':', $value, '|'; }
    foreach (array_flip(['0', '01', '0']) as $key => $value) { echo gettype($key), ':', $key, ':', $value, '|'; }
    foreach (array_count_values(['0', '01', '0']) as $key => $value) { echo gettype($key), ':', $key, ':', $value, '|'; }
    $rows = [['0' => 11, 0 => 22], ['0' => 33, 0 => 44]];
    foreach (array_column($rows, '0') as $value) { echo $value, '|'; }
}
