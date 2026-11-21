<?php
class EvalParentUpdateHolder {
    public mixed $value = false;
    public static mixed $shared = false;
}
function evalParentUpdateCase(int $place, mixed $parent, mixed $key): void {
    $holder = new EvalParentUpdateHolder();
    $holder->value = $parent;
    EvalParentUpdateHolder::$shared = $parent;
    $dynamic = new stdClass();
    $dynamic->value = $parent;
    set_error_handler(function (int $severity, string $message): bool {
        if (str_contains($message, 'false to array')) { echo 'D|'; }
        return true;
    }, E_DEPRECATED);
    try {
        if ($place === 0) { $parent[$key] += 1; }
        elseif ($place === 1) { $holder->value[$key] += 1; }
        elseif ($place === 2) { EvalParentUpdateHolder::$shared[$key] += 1; }
        else { $dynamic->value[$key] += 1; }
        echo 'W|';
    } catch (TypeError $error) {
        echo 'E|';
        unset($error);
    }
    restore_error_handler();
    $observed = $place === 0 ? $parent
        : ($place === 1 ? $holder->value
        : ($place === 2 ? EvalParentUpdateHolder::$shared : $dynamic->value));
    echo gettype($observed), '|';
    EvalParentUpdateHolder::$shared = null;
}
function evalParentLeafCase(mixed $child, mixed $outerKey, mixed $leafKey): void {
    $parent = [$child];
    try {
        $parent[$outerKey][$leafKey] = 1;
        echo 'W|';
    } catch (TypeError $error) {
        echo 'E|';
        unset($error);
    }
    echo gettype($parent[0]), '|';
}
function evalParentValidLeafCase(mixed $child): void {
    $parent = [$child];
    $parent[0][1] = 7;
    echo gettype($parent[0]), ':', $parent[0][1], '|';
}
