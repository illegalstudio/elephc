<?php
declare(strict_types=1);
class ConstructorArrayTarget {
    public function __construct(int $value = 7) { echo 'K', $value, '|'; }
}
class ConstructorArrayOther {
    public function __construct(int $value = 5) { echo 'W', $value, '|'; }
}
$reflection = new ReflectionClass('ConstructorArrayTarget');
$reflection->newInstance(value: '12');
$reflection->newInstanceArgs(['value' => '13']);
try { $reflection->newInstanceArgs(new ArrayIterator([8])); }
catch (TypeError $error) { echo 'T|'; unset($error); }
$reflection->newInstanceArgs();
$arguments = ['value' => 8];
$arguments['value'] = 9;
$reflection->newInstanceArgs(args: $arguments);
(new ReflectionClass('ConstructorArrayTarget'))->newInstanceArgs([10]);
$reflectedObject = new ReflectionObject(new ConstructorArrayTarget());
$reflectedObject->newInstanceArgs(['value' => 11]);
function constructorArrayRebind(ReflectionClass &$reflection): array {
    echo 'R';
    $reflection = new ReflectionClass('ConstructorArrayOther');
    gc_collect_cycles();
    return ['value' => 4];
}
$reflection->newInstanceArgs(constructorArrayRebind($reflection));
echo $reflection->getName(), '|';
function constructorArrayThrow(): array { echo 'A'; throw new RuntimeException('source'); }
try { (new ReflectionClass('ConstructorArrayTarget'))->newInstanceArgs(constructorArrayThrow()); }
catch (RuntimeException $error) { echo 'T|'; unset($error); }
class ConstructorArrayOwned {
    public static int $retired = 0;
    public string $label;
    public function __construct(string $label) { $this->label = $label; }
    public function __destruct() { self::$retired++; gc_collect_cycles(); }
}
class ConstructorArrayFail {
    public function __construct(ConstructorArrayOwned $value) {
        echo $value->label;
        gc_collect_cycles();
        throw new RuntimeException('constructor');
    }
}
try { (new ReflectionClass('ConstructorArrayFail'))->newInstanceArgs(['value' => new ConstructorArrayOwned('E')]); }
catch (RuntimeException $error) { echo 'T|'; unset($error); }
gc_collect_cycles();
echo ConstructorArrayOwned::$retired, '|';
class ConstructorArrayKeep {
    public ConstructorArrayOwned $value;
    public function __construct(ConstructorArrayOwned $value) { $this->value = $value; }
}
$kept = (new ReflectionClass('ConstructorArrayKeep'))->newInstanceArgs(['value' => new ConstructorArrayOwned('Y')]);
gc_collect_cycles();
echo $kept->value->label;
unset($kept);
gc_collect_cycles();
echo ':', ConstructorArrayOwned::$retired, '|';
