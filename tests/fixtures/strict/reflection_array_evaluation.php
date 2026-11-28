<?php
function reflectionArrayEffect(string $label, int $value): int { echo $label; return $value; }
function reflectionArrayRead(int $value): string { return str_repeat((string)$value, 2); }
function reflectionArraySource(): array { echo 'C'; return ['value' => 3]; }
$function = new ReflectionFunction('reflectionArrayRead');
echo $function->invokeArgs(['value' => reflectionArrayEffect('A', 1), 'value' => reflectionArrayEffect('B', 2)]), '|';
echo $function->invokeArgs(reflectionArraySource()), '|';
class ReflectionArrayTarget {
    public function __construct() { echo 'M'; }
    public function read(int $value): string { return str_repeat((string)$value, 2); }
    public static function shared(int $value): string { return str_repeat((string)$value, 2); }
}
$method = new ReflectionMethod('ReflectionArrayTarget', 'read');
echo $method->invokeArgs(object: new ReflectionArrayTarget(), args: ['value' => reflectionArrayEffect('N', 4)]), '|';
echo $method->invokeArgs(args: ['value' => reflectionArrayEffect('P', 5)], object: new ReflectionArrayTarget()), '|';
$static = new ReflectionMethod('ReflectionArrayTarget', 'shared');
echo $static->invokeArgs(null, ['value' => reflectionArrayEffect('Q', 6)]), '|';
function reflectionArrayWord(string $label, string $value): string { echo $label; return str_repeat($value, 2); }
echo (new ReflectionFunction('strlen'))->invokeArgs(['string' => reflectionArrayWord('U', 'old'), 'string' => reflectionArrayWord('V', 'last')]), '|';
class ReflectionArrayOwned {
    public static int $retired = 0;
    public string $label;
    public function __construct(string $label) { $this->label = $label; }
    public function __destruct() { self::$retired++; gc_collect_cycles(); }
}
function reflectionArrayReturn(ReflectionArrayOwned $value): ReflectionArrayOwned { return $value; }
$returned = (new ReflectionFunction('reflectionArrayReturn'))->invokeArgs(['value' => new ReflectionArrayOwned('Y')]);
gc_collect_cycles();
echo $returned->label;
unset($returned);
gc_collect_cycles();
echo ':', ReflectionArrayOwned::$retired, '|';
function reflectionArrayThrow(): int { echo 'R'; throw new RuntimeException('first'); }
try { $function->invokeArgs(['value' => reflectionArrayThrow(), 'value' => reflectionArrayEffect('X', 9)]); }
catch (RuntimeException $error) { echo 'T|'; unset($error); }
function reflectionArrayFail(ReflectionArrayOwned $value): void { gc_collect_cycles(); throw new RuntimeException('callee'); }
try { (new ReflectionFunction('reflectionArrayFail'))->invokeArgs(['value' => new ReflectionArrayOwned('Z')]); }
catch (RuntimeException $error) { echo 'E'; unset($error); }
gc_collect_cycles();
echo ReflectionArrayOwned::$retired, '|';
