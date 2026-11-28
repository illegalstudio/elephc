<?php
function booleanOffsetWrite(mixed $parent, mixed $key): void {
    set_error_handler(function (int $level, string $message) use (&$parent): bool {
        if (str_contains($message, 'Automatic conversion of false')) { echo 'W:', $level, ':', gettype($parent), ':', json_encode($parent), '|'; }
        return true;
    });
    try { $parent[$key] = 9; }
    catch (Throwable $error) { echo 'E:', get_class($error), '|'; unset($error); }
    restore_error_handler();
    echo gettype($parent), ':', json_encode($parent), '|';
}
booleanOffsetWrite(false, 'put');
booleanOffsetWrite(false, 0);
booleanOffsetWrite(true, 'put');
booleanOffsetWrite(42, 'put');
booleanOffsetWrite(2.5, 'put');
booleanOffsetWrite(false, []);
booleanOffsetWrite(false, 0.5);
booleanOffsetWrite(false, true);
booleanOffsetWrite(false, null);
booleanOffsetWrite(null, 0);
function replacedFalseParent(mixed $parent, string $mode): void {
    set_error_handler(function (int $level, string $message) use (&$parent, $mode): bool {
        echo 'R:', $level, ':', json_encode($parent), '|';
        if ($mode === 'array') { $parent = ['seen' => 2]; }
        if ($mode === 'string') { $parent = 'ab'; }
        if ($mode === 'true') { $parent = true; }
        gc_collect_cycles();
        return true;
    });
    $parent['put'] = 9;
    restore_error_handler();
    echo json_encode($parent), '|';
}
replacedFalseParent(false, 'array');
replacedFalseParent(false, 'string');
replacedFalseParent(false, 'true');
class BooleanWritePayload {
    public function __destruct() { echo 'D'; gc_collect_cycles(); }
}
function throwingFalseParent(mixed $parent, mixed $key, mixed $value): void {
    set_error_handler(function (int $level, string $message): bool {
        echo 'T'; gc_collect_cycles(); throw new RuntimeException('deprecation');
    });
    try { $parent[$key] = $value; }
    catch (Throwable $error) { echo get_class($error), ':', json_encode($parent), ':', ($error->getPrevious() === null ? '-' : get_class($error->getPrevious())), '|'; unset($error); }
    restore_error_handler();
}
throwingFalseParent(false, 'put', new BooleanWritePayload());
throwingFalseParent(false, true, new BooleanWritePayload());
throwingFalseParent(false, 0.0, new BooleanWritePayload());
throwingFalseParent(false, null, new BooleanWritePayload());
throwingFalseParent(false, [], new BooleanWritePayload());
echo '|';
