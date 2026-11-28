<?php
function duplicateValueEffect(string $label, int $value): int { echo $label; return $value; }
function duplicateValueRead(int $value): void { echo $value, '|'; }
function duplicateValueThrow(): int { echo 'T'; throw new RuntimeException('first'); }
duplicateValueRead(...['value' => duplicateValueEffect('A', 1), 'value' => duplicateValueEffect('B', 2)]);
try { duplicateValueRead(...['value' => duplicateValueThrow(), 'value' => duplicateValueEffect('X', 9)]); }
catch (RuntimeException $error) { echo 'C|'; }
class DuplicateValueConstructor {
    public int $value;
    public function __construct(int $value) { echo 'K'; $this->value = $value; }
}
$reflection = new ReflectionClass('DuplicateValueConstructor');
$object = $reflection->newInstanceArgs(['value' => duplicateValueEffect('D', 3), 'value' => duplicateValueEffect('E', 4)]);
echo $object->value, '|';
class DuplicateOwnedValue {
    public static int $retired = 0;
    public string $label;
    public function __construct(string $label) { $this->label = $label; }
    public function __destruct() { self::$retired++; gc_collect_cycles(); }
}
function duplicateOwnedEffect(string $label): DuplicateOwnedValue { echo $label; return new DuplicateOwnedValue($label); }
function duplicateOwnedRead(DuplicateOwnedValue $value): void { gc_collect_cycles(); echo $value->label, ':'; }
duplicateOwnedRead(...['value' => duplicateOwnedEffect('F'), 'value' => duplicateOwnedEffect('G')]);
gc_collect_cycles();
echo DuplicateOwnedValue::$retired, '|';
function duplicateOrderRead(int $first, int $named): void { echo $first, ':', $named, '|'; }
duplicateOrderRead(...[0 => duplicateValueEffect('I', 1), 'named' => duplicateValueEffect('J', 2), 0 => duplicateValueEffect('K', 3)]);
