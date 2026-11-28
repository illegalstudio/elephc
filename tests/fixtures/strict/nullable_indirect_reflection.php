<?php
declare(strict_types=0);
function nullableIndirectValue(): mixed { echo 'V'; return null; }
$reflector = new ReflectionFunction('strlen');
$method = $argc > 0 ? 'invoke' : 'INVOKE';
try { $reflector->$method(nullableIndirectValue()); echo 'bad'; }
catch (Error $error) { echo str_contains($error->getMessage(), '--strict-nullability') ? 'D|' : 'bad'; }
$reflector = new ReflectionFunction('strlen');
try { call_user_func([$reflector, 'invoke'], nullableIndirectValue()); echo 'bad'; }
catch (Error $error) { echo str_contains($error->getMessage(), '--strict-nullability') ? 'C|' : 'bad'; }
$reflector = new ReflectionFunction('strlen');
$callback = $reflector->invoke(...);
try { $callback(nullableIndirectValue()); echo 'bad'; }
catch (Error $error) { echo str_contains($error->getMessage(), '--strict-nullability') ? 'F|' : 'bad'; }
$reflector = new ReflectionFunction('strlen');
try { call_user_func([$reflector, 'invokeArgs'], [nullableIndirectValue()]); echo 'bad'; }
catch (Error $error) { echo str_contains($error->getMessage(), '--strict-nullability') ? 'A' : 'bad'; }
class NullableIndirectMethodTarget { public function consume(string $value): int { return strlen($value); } }
$target = new NullableIndirectMethodTarget();
$reflector = new ReflectionMethod(NullableIndirectMethodTarget::class, 'consume');
try { $reflector->$method($target, nullableIndirectValue()); echo 'bad'; }
catch (Error $error) { echo str_contains($error->getMessage(), '--strict-nullability') ? '|M' : 'bad'; }
$reflector = new ReflectionMethod(NullableIndirectMethodTarget::class, 'consume');
try { call_user_func([$reflector, 'invokeArgs'], $target, [nullableIndirectValue()]); echo 'bad'; }
catch (Error $error) { echo str_contains($error->getMessage(), '--strict-nullability') ? '|R' : 'bad'; }
