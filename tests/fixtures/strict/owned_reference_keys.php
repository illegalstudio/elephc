<?php
class OwnedKeyPair {
    public function update(mixed &$first, mixed &$second): void {
        $first = 31; $second = 41; echo $first, '/', $second, '|';
    }
}
class OwnedKeyAlias { public function __construct(public mixed &$value) {} }
function ownedKeyObservations(): void {
    $items = ['0' => 1, 0 => 2, '01' => 3, "a\0b" => 4];
    $pair = new OwnedKeyPair();
    $pair->update($items['0'], $items[0]);
    echo $items['0'], '/', $items[0], '|';
    $box = new OwnedKeyAlias($items['01']);
    $box->value = 7; echo $items['01'], '|';
    $items['01'] = 8; echo $box->value, '|';
    $nested = [['0' => 5, 0 => 6]];
    $nestedBox = new OwnedKeyAlias($nested[0]['0']);
    $nestedBox->value = 9; echo $nested[0]['0'], '/', $nested[0][0], '|';
    $nested[0]['0'] = 10; echo $nestedBox->value, '|';
    $pair->update($items["a\0b"], $items['01']);
    echo $items["a\0b"], '/', $box->value, '|';
}
