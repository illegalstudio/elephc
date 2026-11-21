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
function placeBindAlias(mixed &$value): callable {
    return function () use (&$value): mixed { return $value; };
}
function placeWrite(bool $static, bool $falseParent, mixed $key, int $mutation): void {
    global $placeAlias;
    $holder = new ParentPlace();
    $holder->value = $falseParent ? false : null;
    ParentPlace::$shared = $falseParent ? false : null;
    if ($mutation >= 3) {
        if ($mutation !== 7) {
            if ($static) { $placeAlias = placeBindAlias(ParentPlace::$shared); }
            else { $placeAlias = placeBindAlias($holder->value); }
        }
        $mutation -= 3;
    }
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
            if ($mutation === 3 || $mutation === 4) { unset($holder->value); }
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
    else {
        if (isset($holder->value)) { placeDescribe($holder->value); }
        else { echo 'absent|'; }
    }
    if (isset($placeAlias)) { placeDescribe($placeAlias()); }
    ParentPlace::$shared = null;
    $placeAlias = null;
}
