<?php
error_reporting(E_DEPRECATED);
class PendingParent { public mixed $value = false; public static mixed $shared = false; }
function pendingKey(mixed $key): mixed { echo 'K|'; return $key; }
function pendingValue(): int { echo 'R|'; return 9; }
function pendingReference(mixed &$parent, mixed $key, int $operation): void {
    if ($operation === 1) { $parent[] = pendingValue(); }
    elseif ($operation === 2) { $parent[pendingKey($key)] += pendingValue(); }
    else { $parent[pendingKey($key)] = pendingValue(); }
}
function pendingCase(int $place, mixed $key, int $operation, mixed $parent = false): void {
    $holder = new PendingParent();
    PendingParent::$shared = false;
    $older = new Exception('older');
    $handler = new Exception('handler', 0, $older);
    set_error_handler(function () use ($handler): bool { echo 'W|'; throw $handler; }, E_DEPRECATED);
    try {
        if ($place === 0) {
            if ($operation === 1) { $parent[] = pendingValue(); }
            elseif ($operation === 2) { $parent[pendingKey($key)] += pendingValue(); }
            else { $parent[pendingKey($key)] = pendingValue(); }
        } elseif ($place === 1) {
            pendingReference($parent, $key, $operation);
        } elseif ($place === 2) {
            if ($operation === 1) { $holder->value[] = pendingValue(); }
            elseif ($operation === 2) { $holder->value[pendingKey($key)] += pendingValue(); }
            else { $holder->value[pendingKey($key)] = pendingValue(); }
        } else {
            if ($operation === 1) { PendingParent::$shared[] = pendingValue(); }
            elseif ($operation === 2) { PendingParent::$shared[pendingKey($key)] += pendingValue(); }
            else { PendingParent::$shared[pendingKey($key)] = pendingValue(); }
        }
        echo 'unexpected|';
    } catch (Throwable $error) {
        echo get_class($error), '|', $error === $handler ? 'same|' : 'new|';
        $previous = $error->getPrevious();
        while ($previous !== null) {
            echo $previous->getMessage(), ':', ($previous === $handler || $previous === $older) ? 'same|' : 'other|';
            $previous = $previous->getPrevious();
        }
        unset($previous, $error);
    }
    restore_error_handler();
    $observed = $place === 2 ? $holder->value : ($place === 3 ? PendingParent::$shared : $parent);
    echo gettype($observed), ':', is_array($observed) ? count($observed) : -1, '|';
    if (is_array($observed)) {
        foreach ($observed as $index => $value) { echo gettype($index), ':', $index, ':', $value, '|'; }
    }
    PendingParent::$shared = null;
}
