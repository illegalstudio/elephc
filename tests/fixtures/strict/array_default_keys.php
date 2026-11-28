<?php
const EXACT_DEFAULT_KEYS = [1 => 10, '1' => 20, '01' => 30];
const EXACT_DEFAULT_SPREAD = ['2' => 20, ...['1' => 10], 30];
function dumpDefaultKeys(mixed $values): void {
    foreach ($values as $key => $value) { echo gettype($key), ':', $key, '=', $value, ','; }
    echo '|';
}
function inlineKeyDefault(array $values = ['2' => 20, 30]): array { return $values; }
function constantKeyDefault(array $values = EXACT_DEFAULT_KEYS): array { return $values; }
class ExactKeyDefaults {
    public array $values = [1 => 10, '1' => 20, '01' => 30];
    public static array $shared = ['2' => 20, 30];
    public const KEYS = ['2' => 20, ...['1' => 10], 30];
}
dumpDefaultKeys(constantKeyDefault());
dumpDefaultKeys(inlineKeyDefault());
$call = inlineKeyDefault(...);
dumpDefaultKeys($call());
$reflection = new ReflectionFunction('inlineKeyDefault');
dumpDefaultKeys($reflection->invoke());
$parameter = new ReflectionParameter('inlineKeyDefault', 'values');
dumpDefaultKeys($parameter->getDefaultValue());
$object = new ExactKeyDefaults();
dumpDefaultKeys($object->values);
dumpDefaultKeys(ExactKeyDefaults::$shared);
$class = new ReflectionClass('ExactKeyDefaults');
dumpDefaultKeys($class->getConstant('KEYS'));
dumpDefaultKeys(EXACT_DEFAULT_SPREAD);
