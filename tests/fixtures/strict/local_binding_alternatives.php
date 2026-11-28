<?php
function bindingCandidate(mixed $value): mixed { return $value; }
function separateBindings(bool $pick): void {
    if ($pick) { $value = 1; } else { $value = 's'; }
    echo gettype($value), '|';
}
function replacedBinding(bool $pick): void {
    $value = 1;
    if ($pick) { unset($value); $value = 's'; }
    echo gettype($value), '|';
}
function joinedBindings(bool $pick): void {
    if ($pick) { $value = 1; } else { $value = 's'; }
    try { $value = bindingCandidate(2); echo 'W'; }
    catch (TypeError $error) { echo 'T'; unset($error); }
    echo gettype($value), '|';
}
function optionalBinding(bool $pick): void {
    if ($pick) { $value = 1; }
    try { $value = bindingCandidate('s'); echo 'W'; }
    catch (TypeError $error) { echo 'T'; unset($error); }
    echo gettype($value), '|';
}
function copiedBinding(bool $pick): void {
    if ($pick) { $value = 1; } else { $value = 's'; }
    $copy = $value;
    unset($value);
    try { $copy = bindingCandidate(2); echo 'W'; }
    catch (TypeError $error) { echo 'T'; unset($error); }
    echo gettype($copy), '|';
}
function repeatedBindings(): void {
    for ($index = 0; $index < 2; $index++) {
        $value = 1; unset($value); $value = 's';
        echo gettype($value), '|'; unset($value);
    }
}
function openBindings(bool $pick): void {
    $value = bindingCandidate(1);
    if ($pick) { $value = 'open'; }
    echo gettype($value), '|';
}
separateBindings(true); separateBindings(false);
replacedBinding(true); replacedBinding(false);
joinedBindings(true); joinedBindings(false);
optionalBinding(true); optionalBinding(false);
copiedBinding(true); copiedBinding(false);
repeatedBindings(); openBindings(true); openBindings(false);

function numericFirstBinding(bool $pick): void {
    if ($pick) { $value = 1; } else { $value = 2.5; }
    try { $value = bindingCandidate(2); echo 'W'; }
    catch (TypeError $error) { echo 'T'; unset($error); }
    echo gettype($value), '|';
}
function nullableFirstBinding(bool $pick): void {
    if ($pick) { $value = null; } else { $value = 's'; }
    try { $value = bindingCandidate('new'); echo 'W'; }
    catch (TypeError $error) { echo 'T'; unset($error); }
    echo gettype($value), '|';
}
numericFirstBinding(true); numericFirstBinding(false);
nullableFirstBinding(true); nullableFirstBinding(false);

function freshAfterUnset(bool $reset, mixed $replacement): void {
    $value = 1;
    if ($reset) { unset($value); $value = 's'; }
    try { $value = $replacement; echo 'W'; }
    catch (TypeError $error) { echo 'T'; unset($error); }
    echo gettype($value), '|';
}
function absentBeforeUnset(bool $born, mixed $replacement): void {
    if ($born) { $value = 1; }
    unset($value); $value = 's';
    try { $value = $replacement; echo 'W'; }
    catch (TypeError $error) { echo 'T'; unset($error); }
    echo gettype($value), '|';
}
freshAfterUnset(true, 2); freshAfterUnset(false, 2);
absentBeforeUnset(true, 2); absentBeforeUnset(false, 2);
