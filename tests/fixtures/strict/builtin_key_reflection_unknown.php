<?php
function reflectionGuardNeverCalled(mixed $value): bool { echo 'CALLEE-RAN|'; return true; }
class UnknownReflectionGuardTarget {
    public static function hit(mixed $value): bool { echo 'CALLEE-RAN|'; return true; }
}
class UnknownOrdinaryInvoker {
    public function invoke(mixed $value): bool { echo gettype($value), ':'; return true; }
}
function invokeUnknownKeyReceiver(mixed $receiver, mixed $value): void {
    try { echo $receiver->invoke($value) ? 'user|' : 'miss|'; }
    catch (Error $error) {
        echo str_contains($error->getMessage(), '--strict-array-keys') ? 'blocked|' : 'wrong|';
        unset($error);
    }
}
$function = new ReflectionFunction('reflectionGuardNeverCalled');
$method = new ReflectionMethod(UnknownReflectionGuardTarget::class, 'hit');
$ordinary = new UnknownOrdinaryInvoker();
invokeUnknownKeyReceiver($function, true);
invokeUnknownKeyReceiver($ordinary, true);
invokeUnknownKeyReceiver($method, true);
invokeUnknownKeyReceiver($ordinary, 0);
