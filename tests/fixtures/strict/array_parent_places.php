<?php
class ParentPlace { public mixed $value = null; public static mixed $shared = null; }
function placeKey(mixed $key): mixed { echo 'K|'; return $key; }
function placeValue(): int { echo 'R|'; return 9; }
function placeDescribe(mixed $parent): void {
    echo gettype($parent), ':', is_array($parent) ? count($parent) : -1, '|';
    if (is_array($parent)) {
        foreach ($parent as $key => $value) { echo gettype($key), ':', $key, ':', $value, '|'; }
    }
}
function placeWrite(bool $static, bool $falseParent, mixed $key, int $mutation): void {
    $holder = new ParentPlace();
    $holder->value = $falseParent ? false : null;
    ParentPlace::$shared = $falseParent ? false : null;
    set_error_handler(function (int $severity, string $message) use ($static, $holder, $mutation): bool {
        if (str_contains($message, 'false to array')) {
            echo 'F|';
            if ($static) { placeDescribe(ParentPlace::$shared); }
            else { placeDescribe($holder->value); }
            if ($mutation === 1) {
                if ($static) { ParentPlace::$shared = ['h' => 7]; }
                else { $holder->value = ['h' => 7]; }
            }
            if ($mutation === 2) {
                if ($static) { ParentPlace::$shared['h'] = 7; }
                else { $holder->value['h'] = 7; }
            }
        }
        return true;
    });
    try {
        if ($static) { ParentPlace::$shared[placeKey($key)] = placeValue(); }
        else { $holder->value[placeKey($key)] = placeValue(); }
        echo 'ok|';
    } catch (TypeError $error) { echo 'T|'; unset($error); }
    restore_error_handler();
    if ($static) { placeDescribe(ParentPlace::$shared); }
    else { placeDescribe($holder->value); }
    ParentPlace::$shared = null;
}
