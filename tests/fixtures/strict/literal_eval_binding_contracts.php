<?php
function literalNativeBinding(): void {
    $value = 1;
    try { eval('$value = 2.5;'); echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($value), ':', $value, '|';
}
literalNativeBinding();
function literalAliasedBinding(): void {
    $value = 1; $alias =& $value;
    try { eval('$value = 2.5;'); echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($value), ':', $value, ':', $alias, '|';
    eval('$value = 3;'); echo $value, ':', $alias, '|';
}
literalAliasedBinding();
mixed $open = 1; eval('$open = 2.5;'); echo gettype($open), ':', $open, '|';
int|float $wide = 1; eval('$wide = 2.5;'); echo gettype($wide), ':', $wide, '|';
function literalBooleanBinding(): void {
    $value = true; $alias =& $value;
    try { eval('$value = 1;'); echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($value), ':', $value, ':', $alias, '|';
}
literalBooleanBinding();
function literalStringBinding(): void {
    $value = 'kept'; $alias =& $value;
    try { eval('$value = 1;'); echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($value), ':', $value, ':', $alias, '|';
}
literalStringBinding();
function literalFloatBinding(): void {
    $value = 2.5; $alias =& $value;
    try { eval('$value = 2;'); echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($value), ':', $value, ':', $alias, '|';
}
literalFloatBinding();
function literalNullBinding(): void {
    $value = null; $alias =& $value;
    try { eval('$value = 1;'); echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($value), ':', $value, ':', $alias, '|';
}
literalNullBinding();
