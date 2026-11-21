<?php
function eagerIntegerBinding(): void {
    $value = 1;
    try { eval('$value = "bad"; echo "S"; $value = 2;'); echo 'W'; }
    catch (TypeError $error) { echo 'T'; }
    echo gettype($value), ':', $value, '|';
}
function eagerOpenBinding(): void {
    mixed $value = 1;
    try { eval('$value = "bad"; echo "S"; $value = 2;'); echo 'W'; }
    catch (TypeError $error) { echo 'T'; }
    echo gettype($value), ':', $value, '|';
}
function eagerCommittedBinding(): void {
    $value = 1;
    try { eval('$value = 2; echo "A"; $value = "bad"; echo "S";'); echo 'W'; }
    catch (TypeError $error) { echo 'T'; }
    echo gettype($value), ':', $value, '|';
}
function buildScopeOwnedText(string $text): string { echo 'O'; return $text . '!'; }
function eagerOwnedBinding(): void {
    $value = 1;
    try { eval('$value = buildScopeOwnedText("owned"); echo "S"; $value = 2;'); echo 'W'; }
    catch (TypeError $error) { echo 'T'; }
    echo gettype($value), ':', $value, '|';
}
function eagerArrayBinding(): void {
    $value = [1];
    try { eval('$value = ["next"]; echo "A"; $value = 3; echo "S";'); echo 'W'; }
    catch (TypeError $error) { echo 'T'; }
    echo gettype($value), ':', is_array($value) ? $value[0] : $value, '|';
}
eagerIntegerBinding(); eagerOpenBinding(); eagerCommittedBinding(); eagerOwnedBinding(); eagerArrayBinding();
