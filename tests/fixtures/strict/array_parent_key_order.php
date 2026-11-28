<?php
class ParentKeyOperand {}
function parentKeyOperand(mixed $key): mixed { echo 'K|'; return $key; }
function parentKeyValue(): int { echo 'R|'; return 9; }
function parentKeyAssign(mixed &$parent, mixed $key): void {
    $parent[parentKeyOperand($key)] = parentKeyValue();
}
function parentKeyCopyCheck(): void {
    $parent = [3];
    $snapshot = $parent;
    parentKeyAssign($parent, 0);
    echo $parent[0], ':', $snapshot[0], '|';
}
function parentKeyWrite(mixed $key, bool $falseParent, int $mutation): void {
    global $keyParent, $parentMutation;
    $keyParent = $falseParent ? false : null;
    $parentMutation = $mutation;
    set_error_handler(function (int $severity, string $message): bool {
        global $keyParent, $parentMutation;
        if (str_contains($message, 'false to array')) {
            echo 'F:', gettype($keyParent), ':', is_array($keyParent) ? count($keyParent) : -1, '|';
            if ($parentMutation === 1) { $keyParent = ['h' => 7]; }
            if ($parentMutation === 2) { $keyParent['h'] = 7; }
        }
        return true;
    });
    try {
        if ($mutation === 3) { parentKeyAssign($keyParent, $key); }
        else { $keyParent[parentKeyOperand($key)] = parentKeyValue(); }
        echo 'ok|';
    }
    catch (TypeError $error) { echo 'T|'; unset($error); }
    restore_error_handler();
    echo gettype($keyParent), ':', is_array($keyParent) ? count($keyParent) : -1, '|';
    if (is_array($keyParent)) {
        foreach ($keyParent as $storedKey => $storedValue) {
            echo gettype($storedKey), ':', $storedKey, ':', $storedValue, '|';
        }
    }
    unset($keyParent, $parentMutation);
}
