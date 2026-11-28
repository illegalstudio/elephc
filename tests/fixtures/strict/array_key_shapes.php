<?php
function shapeKey(string $key): string { return $key; }
class KeyShapeBox {
    public $initial = ['0' => 3];
    public array $declared = ['1' => 4];
}
$literal = ['0' => 10, '1' => 11, '01' => 12];
echo is_string(array_key_first($literal)) ? 'S' : 'I';
$key = shapeKey($argc > 0 ? '0' : '1');
$written = [];
$written[$key] = 20;
$written['0'] = 21;
echo is_string(array_key_first($written)) ? 'S' : 'I';
$both = [1];
$both['0'] = 8;
echo count($both), is_string(array_key_last($both)) ? 'S' : 'I';
$loop = [];
for ($i = 0; $i < 2; $i++) { $loop['0'] = $i; }
echo is_string(array_key_first($loop)) ? 'S' : 'I';
$holder = new KeyShapeBox();
echo is_string(array_key_first($holder->initial)) ? 'S' : 'I';
echo is_string(array_key_first($holder->declared)) ? 'S' : 'I';
$closure = fn() => ['1' => 40];
$result = $closure();
echo is_string(array_key_first($result)) ? 'S' : 'I';
$flipped = array_flip(['1', '01']);
$filled = array_fill_keys(['1', '01'], 3);
$counts = array_count_values(['1', '01']);
$combined = array_combine(['1', '01'], [7, 8]);
echo is_string(array_key_first($flipped)) ? 'S' : 'I';
echo is_string(array_key_first($filled)) ? 'S' : 'I';
echo is_string(array_key_first($counts)) ? 'S' : 'I';
echo is_string(array_key_first($combined)) ? 'S' : 'I';
$appended = [];
$appended[]['0'] .= 'x';
echo is_string(array_key_first($appended[0])) ? 'S' : 'I';
