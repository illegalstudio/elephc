<?php
function gatherUnpackKeys(...$values): mixed { return $values; }
function dumpUnpackKeys(mixed $values): void {
    foreach ($values as $key => $value) { echo gettype($key), ':', $key, '=', $value, ','; }
    echo '|';
}
class NumericUnpackConstructor {
    public mixed $values;
    public function __construct(...$values) { $this->values = $values; }
}
dumpUnpackKeys(gatherUnpackKeys(...['0' => 10]));
dumpUnpackKeys(gatherUnpackKeys(...[0 => 1, '0' => 2]));
$values = ['0' => 10, '01' => 20];
dumpUnpackKeys(gatherUnpackKeys(...$values));
$callback = gatherUnpackKeys(...);
dumpUnpackKeys($callback(...['0' => 30]));
$object = new NumericUnpackConstructor(...['0' => 40]);
dumpUnpackKeys($object->values);
$reflection = new ReflectionClass('NumericUnpackConstructor');
$reflected = $reflection->newInstanceArgs(['0' => 50]);
dumpUnpackKeys($reflected->values);
dumpUnpackKeys(gatherUnpackKeys(...['0' => 10], ...[20]));
dumpUnpackKeys(gatherUnpackKeys(...[20], ...['0' => 10]));
dumpUnpackKeys(gatherUnpackKeys(...["a\x00b" => 60]));
class RegularUnpackConstructor {
    public float $value;
    public function __construct(float $value) { $this->value = $value; }
}
$regularReflection = new ReflectionClass('RegularUnpackConstructor');
$regular = $regularReflection->newInstanceArgs(['value' => 2]);
echo gettype($regular->value), ':', $regular->value, '|';
