<?php
// Guard-only views keep binding storage; actual writes keep their effects.
class R58Widget {}
class R58Missing {}
class R58Other {}
function r58NeedWidget(R58Widget $value): void {
    echo ($value instanceof R58Widget) ? "widget\n" : "bad\n";
}
function r58Object(R58Widget $value): void {
    echo ($value instanceof R58Missing || $value instanceof R58Other) ? "bad" : "or:";
    r58NeedWidget($value);
}
function r58NeedString(string &$value): void { echo $value, "\n"; }
function r58StringInt(string $value): void {
    echo is_int($value) ? "bad" : "s-int:";
    r58NeedString($value);
}
function r58StringArray(string $value): void {
    echo is_array($value) ? "bad" : "s-array:";
    r58NeedString($value);
}
function r58StringIf(string $value): void {
    if (is_array($value)) { echo "bad"; }
    echo "s-if:";
    r58NeedString($value);
}
function r58NeedFloat(float &$value): void { var_dump($value); }
function r58FloatInt(float $value): void {
    echo is_int($value) ? "bad" : "f-int:";
    r58NeedFloat($value);
}
function r58FloatArray(float $value): void {
    echo is_array($value) ? "bad" : "f-array:";
    r58NeedFloat($value);
}
function r58NeedNullable(?int &$value): void { var_dump($value); }
function r58NullableArray(?int $value): void {
    echo is_array($value) ? "bad" : "n-array:";
    r58NeedNullable($value);
}
function r58NullableIf(?int $value): void {
    if (is_array($value)) { echo "bad"; }
    echo "n-if:";
    r58NeedNullable($value);
}
function r58Refined(?int $value): int {
    return is_int($value) ? $value : 9;
}
function r58Stored(?int $value): ?int {
    is_int($value) ? ($value = null) : ($value = 5);
    return $value;
}
interface R58Flag {}
class R58Base {}
class R58Both extends R58Base implements R58Flag {}
function r58Reset(mixed &$value): void { $value = null; }
function r58Callback(R58Base $value): void {
    $value instanceof R58Flag ? r58Reset($value) : null;
    var_dump($value);
}
r58Object(new R58Widget());
r58StringInt('value');
r58StringArray('value');
r58StringIf('value');
r58FloatInt(1.5);
r58FloatArray(1.5);
r58NullableArray(2);
r58NullableArray(null);
r58NullableIf(2);
r58NullableIf(null);
var_dump(r58Refined(2), r58Refined(null));
var_dump(r58Stored(2), r58Stored(null));
r58Callback(new R58Both());

