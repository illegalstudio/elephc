<?php
interface R62Marker {}
class R62Base {}
class R62Both extends R62Base implements R62Marker {}
function r62Marker(R62Marker $value): int { return 7; }
function r62Label(R62Marker $value, int $label): int { echo 'label', $label, "\n"; return $label; }
function r62Int(int $value): int { return $value; }
function r62String(string $value): string { return $value; }
function r62Numbered(?R62Base $value, int $flag): int {
    if ($value === null) { return 0; }
    if ($value instanceof R62Marker) {
        return match ($flag) { 1 => r62Marker($value), default => 0 };
    }
    return 0;
}
function r62Default(?R62Base $value, int $flag): int {
    if ($value === null) { return 0; }
    if ($value instanceof R62Marker) {
        return match ($flag) { 1 => 0, default => r62Marker($value) };
    }
    return 0;
}
function r62Labels(?R62Base $value, int $flag): int {
    if ($value === null) { return 0; }
    if ($value instanceof R62Marker) {
        return match ($flag) {
            r62Label($value, 1), r62Label($value, 2) => r62Marker($value),
            default => r62Marker($value),
        };
    }
    return 0;
}
function r62Sibling(?R62Base $value, int $flag): int {
    if ($value === null) { return 0; }
    if ($value instanceof R62Marker) {
        return match ($flag) { 1 => ($value = new R62Base()) ? 0 : 0, 2 => r62Marker($value), default => 0 };
    }
    return 0;
}
function r62Finally(?R62Base $value, int $flag): int {
    if ($value === null) { return 0; }
    if ($value instanceof R62Marker) {
        try { return 1; }
        finally {
            $result = match ($flag) { 1 => r62Marker($value), default => r62Marker($value) };
            echo 'finally:', $result, "\n";
        }
    }
    return 0;
}
function r62Scalar(string $value, int $flag): string {
    if (is_int($value)) {
        $result = match ($flag) { 1 => r62Int($value), default => r62Int($value) };
        echo $result;
    }
    return r62String($value);
}
var_dump(r62Numbered(new R62Both(), 1), r62Numbered(new R62Both(), 2), r62Numbered(null, 1));
var_dump(r62Default(new R62Both(), 2), r62Default(new R62Both(), 1), r62Default(null, 2));
var_dump(r62Labels(new R62Both(), 1), r62Labels(new R62Both(), 2), r62Labels(new R62Both(), 3));
var_dump(r62Sibling(new R62Both(), 1), r62Sibling(new R62Both(), 2), r62Sibling(null, 1));
var_dump(r62Finally(new R62Both(), 1), r62Finally(new R62Both(), 2), r62Finally(null, 1));
echo r62Scalar('safe', 1), "\n";
