<?php
function scalarBindingValue(mixed $value): mixed { return $value; }
function replaceScalarBinding(&$target, mixed $value): void { $target = $value; }

function stringBinding(): void {
    $value = 'before';
    try { $value = scalarBindingValue(2); echo 'W'; }
    catch (TypeError $error) { echo 'T'; }
    echo gettype($value), '|', $value, '|';
}

function booleanBinding(): void {
    $value = true;
    try { $value = scalarBindingValue(2); echo 'W'; }
    catch (TypeError $error) { echo 'T'; }
    echo gettype($value), '|', $value, '|';
}

function nullBinding(): void {
    $value = null;
    try { $value = scalarBindingValue(2); echo 'W'; }
    catch (TypeError $error) { echo 'T'; }
    echo gettype($value), '|', $value, '|';
}

class OriginalScalarBinding {}
class OtherScalarBinding {}
function nominalBinding(): void {
    $value = new OriginalScalarBinding();
    try { $value = scalarBindingValue(new OtherScalarBinding()); echo 'W'; }
    catch (TypeError $error) { echo 'T'; }
    echo get_class($value), '|';
}

function arrayKindBinding(): void {
    $value = [1];
    try { $value = scalarBindingValue(2); echo 'W'; }
    catch (TypeError $error) { echo 'T'; }
    echo gettype($value), '|';
}

function arrayElementOpenness(): void {
    $value = [1];
    $value = scalarBindingValue(['next']);
    echo gettype($value), '|', $value[0], '|';
}

function referenceBooleanBinding(): void {
    $value = true;
    try { replaceScalarBinding($value, 2); echo 'W'; }
    catch (TypeError $error) { echo 'T'; }
    echo gettype($value), '|', $value, '|';
}

function explicitOpenBinding(): void {
    mixed $value = true;
    $value = 2;
    echo gettype($value), '|', $value, '|';
}

stringBinding();
booleanBinding();
nullBinding();
nominalBinding();
arrayKindBinding();
arrayElementOpenness();
referenceBooleanBinding();
explicitOpenBinding();
gc_collect_cycles();
