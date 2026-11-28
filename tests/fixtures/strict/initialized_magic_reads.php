<?php
$name = $argc > 0 ? 'value' : 'other';
class RuntimeInitializedMagic {
    public $value;
    public int $reads = 0;
    public function __get(string $name): mixed { $this->reads += 1; return 'ready'; }
    public function __isset(string $name): bool { return true; }
}
$magic = new RuntimeInitializedMagic();
unset($magic->value);
try { echo $magic->$name; } catch (Error $error) { echo 'I'; }
echo ':', $magic->reads, '|';
echo isset($magic->$name) ? 'P' : 'A', empty($magic->$name) ? 'E' : 'F', ':', $magic->$name ?? 'missing', ':', $magic->reads, '|';

function initializedMagicBoxedRead(mixed $object, string $name): mixed { return $object->$name; }
function initializedMagicBoxedProbe(mixed $object, string $name): void {
    echo isset($object->$name) ? 'P' : 'A', empty($object->$name) ? 'E' : 'F', ':', $object->$name ?? 'missing', '|';
}
$boxedMagic = new RuntimeInitializedMagic();
unset($boxedMagic->value);
try { echo initializedMagicBoxedRead($boxedMagic, $name); } catch (Error $error) { echo 'I'; }
echo ':', $boxedMagic->reads, '|';
initializedMagicBoxedProbe($boxedMagic, $name);
echo $boxedMagic->reads, '|';

$presentMagic = new RuntimeInitializedMagic();
var_dump($presentMagic->$name);
echo $presentMagic->reads, '|';
echo initializedMagicBoxedRead($boxedMagic, $argc > 0 ? 'unknown' : 'other'), ':', $boxedMagic->reads, '|';
class ThrowingInitializedMagic {
    public $value;
    public function __get(string $name): mixed { throw new Error('getter'); }
}
function freshThrowingInitializedMagic(): ThrowingInitializedMagic {
    $object = new ThrowingInitializedMagic();
    unset($object->value);
    return $object;
}
try { echo initializedMagicBoxedRead(freshThrowingInitializedMagic(), $name); }
catch (Error $error) { echo 'T|'; }

function initializedMagicNullable(?RuntimeInitializedMagic $object, string $name): void {
    echo $object?->$name ?? 'null', ':', $object?->value ?? 'null', '|';
}
initializedMagicNullable(null, $name);
initializedMagicNullable($boxedMagic, $name);
echo $boxedMagic->reads, '|';
class InitializedMagicWithoutIsset {
    public $value;
    public function __get(string $name): int { echo 'G'; return 7; }
}
$withoutIsset = new InitializedMagicWithoutIsset();
unset($withoutIsset->value);
echo $withoutIsset->$name, '|';
echo isset($withoutIsset->$name) ? 'P' : 'A', empty($withoutIsset->$name) ? 'E' : 'F', ':', $withoutIsset->$name ?? 'missing', '|';
class InitializedMagicChild extends RuntimeInitializedMagic {
    public function __get(string $name): mixed { $this->reads += 1; return 'child'; }
}
$childMagic = new InitializedMagicChild();
unset($childMagic->value);
echo initializedMagicBoxedRead($childMagic, $name), ':', $childMagic->reads, '|';
