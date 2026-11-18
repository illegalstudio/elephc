<?php
class OrdinaryKeyInvoker {
    public function invoke(mixed $value): bool { return true; }
    public function invokeArgs(array $values): bool { return true; }
}
class ReflectedKeyProbe {
    public static function check(mixed $key, array $items): bool {
        return array_key_exists($key, $items);
    }
}
function reflectedKeyInputs(mixed $key): void {
    $items = ['0' => 31, 0 => 10, 1 => 20, '' => 30];
    $function = new ReflectionFunction('array_key_exists');
    try { echo $function->invoke($key, $items) ? 'r|' : 'm|'; }
    catch (TypeError $error) { echo 'R|'; unset($error); }
    $function = new ReflectionFunction('array_key_exists');
    try { echo $function->invokeArgs(['array' => $items, 'key' => $key]) ? 'a|' : 'm|'; }
    catch (TypeError $error) { echo 'A|'; unset($error); }
    try { echo call_user_func_array('array_key_exists', ['array' => $items, 'key' => $key]) ? 'c|' : 'm|'; }
    catch (TypeError $error) { echo 'C|'; unset($error); }
    $method = new ReflectionMethod(ReflectedKeyProbe::class, 'check');
    try { echo $method->invoke(null, $key, $items) ? 't|' : 'm|'; }
    catch (TypeError $error) { echo 'T|'; unset($error); }
    $method = new ReflectionMethod(ReflectedKeyProbe::class, 'check');
    try { echo $method->invokeArgs(null, ['items' => $items, 'key' => $key]) ? 'v|' : 'm|'; }
    catch (TypeError $error) { echo 'V|'; unset($error); }
    echo count($items), '|';
    $ordinary = new OrdinaryKeyInvoker();
    echo $ordinary->invoke($key) ? 'p|' : 'm|';
    echo $ordinary->invokeArgs([$key]) ? 'q|' : 'm|';
}
