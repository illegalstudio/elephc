<?php
// Loop-tail guard views preserve entry reads and all actual stores.
function r59NeedString(string &$value): void { echo $value, "\n"; }
function r59ForInt(string $value): void {
    for ($i = 0; $i < 1; $i++) { if (is_int($value)) { continue; } }
    r59NeedString($value);
}
function r59DoInt(string $value): void {
    $i = 0;
    do {
        if (is_int($value)) { continue; }
        $i++;
    } while ($i < 1);
    r59NeedString($value);
}
function r59ForArray(string $value): void {
    for ($i = 0; $i < 1; $i++) { if (is_array($value)) { continue; } }
    r59NeedString($value);
}
class R59Widget {}
class R59Missing {}
class R59Other {}
function r59NeedWidget(R59Widget $value): void {
    echo ($value instanceof R59Widget) ? "widget\n" : "bad\n";
}
function r59Object(R59Widget $value): void {
    for ($i = 0; $i < 1; $i++) {
        if ($value instanceof R59Missing || $value instanceof R59Other) { continue; }
    }
    r59NeedWidget($value);
}
function r59Single(string $value): void {
    for ($i = 0; $i < 1; $i++) {
        if (is_int($value)) { continue; }
        break;
    }
    r59NeedString($value);
}
class R59Root {}
class R59Child extends R59Root {}
function r59NeedChild(R59Child $value): void {
    echo ($value instanceof R59Child) ? "child\n" : "bad\n";
}
function r59Nested(R59Root $value): void {
    if ($value instanceof R59Child) {
        for ($i = 0; $i < 1; $i++) {
            if ($value instanceof R59Missing) { continue; }
        }
        r59NeedChild($value);
    }
}
function r59NestedFallback(string $value): void {
    for ($i = 0; $i < 1; $i++) {
        if (is_int($value)) {
            if (is_array($value)) { continue; }
        }
    }
    r59NeedString($value);
}
function r59StoredBefore(?int $value): ?int {
    $value ??= 3;
    for ($i = 0; $i < 1; $i++) {
        $value = null;
        if (is_null($value)) { continue; }
    }
    return $value;
}
function r59StoredInside(?int $value): ?int {
    for ($i = 0; $i < 1; $i++) {
        if (is_int($value)) { $value = null; continue; }
        $value = 5;
    }
    return $value;
}
function r59StoredBreak(?int $value): ?int {
    $value ??= 3;
    for ($i = 0; $i < 1; $i++) {
        $value = null;
        if (is_null($value)) { break; }
    }
    return $value;
}
function r59Refined(?int $value): ?int {
    do {
        if (is_int($value)) { continue; }
        $value = 5;
    } while (false);
    return $value;
}
r59ForInt('for-int');
r59DoInt('do-int');
r59ForArray('for-array');
r59Object(new R59Widget());
r59Single('single');
r59Nested(new R59Child());
r59NestedFallback('nested-fallback');
var_dump(r59StoredBefore(2), r59StoredBefore(null));
var_dump(r59StoredInside(2), r59StoredInside(null));
var_dump(r59StoredBreak(2), r59StoredBreak(null));
var_dump(r59Refined(2), r59Refined(null));
