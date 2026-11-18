<?php
class EvalExactPair {
    public function update(mixed &$first, mixed &$second): void {
        $first = 31; $second = 41; echo $first, '/', $second, '|';
    }
}
class EvalExactHolder { public mixed $items = ['0' => 1, 0 => 2, '01' => 3]; }
function evalExactMetadataObservations(): void {
    $automatic = ['0' => 10, 20]; $automatic[] = 30;
    foreach ($automatic as $key => $value) { echo gettype($key), ':', $key, ':', $value, '|'; }
    $left = 10; $right = 20; $references = ['0' => &$left, 1 => &$right, '01' => 30];
    $references[0] = 20; $references['0'] = 12; echo $left, '/', $right, '|';
    $probe = new EvalExactPair(); $probe->update($references['0'], $references[0]);
    echo $left, '/', $right, '/', $references['0'], '/', $references[0], '|';
    $owner = new EvalExactHolder(); $owner->items['0'] = 9; unset($owner->items[0]);
    foreach ($owner->items as $key => $value) { echo gettype($key), ':', $key, ':', $value, '|'; }
    $nested = [['0' => 1, 0 => 2]]; $nested[0]['0'] = 3;
    echo $nested[0]['0'], '/', $nested[0][0], '|';
}
