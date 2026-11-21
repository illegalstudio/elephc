<?php
$conditionalBindingGlobal = 1;
function writeConditionalGlobal(bool $reset, mixed $next): void {
    global $conditionalBindingGlobal;
    $old = function() use (&$conditionalBindingGlobal): mixed { return $conditionalBindingGlobal; };
    if ($reset) { unset($conditionalBindingGlobal); }
    try { $conditionalBindingGlobal = $next; echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($conditionalBindingGlobal), ':', $conditionalBindingGlobal, ':', $old(), '|';
}
writeConditionalGlobal(true, 'new'); echo $conditionalBindingGlobal, '|';
writeConditionalGlobal(false, 'bad'); echo $conditionalBindingGlobal, '|';
writeConditionalGlobal(false, 1.5); echo $conditionalBindingGlobal, '|';
writeConditionalGlobal(false, 2); echo $conditionalBindingGlobal, '|';
function redeclareConditionalGlobal(bool $reset): void {
    global $conditionalBindingGlobal;
    if ($reset) { unset($conditionalBindingGlobal); $conditionalBindingGlobal = 'local'; }
    global $conditionalBindingGlobal;
    echo gettype($conditionalBindingGlobal), ':', $conditionalBindingGlobal, '|';
}
redeclareConditionalGlobal(true); redeclareConditionalGlobal(false);

function checkFreshGlobalLocal(bool $reset, mixed $next): void {
    global $conditionalBindingGlobal;
    if ($reset) { unset($conditionalBindingGlobal); $conditionalBindingGlobal = 'local'; }
    try { $conditionalBindingGlobal = $next; echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($conditionalBindingGlobal), ':', $conditionalBindingGlobal, '|';
}
checkFreshGlobalLocal(true, 3); echo $conditionalBindingGlobal, '|';
checkFreshGlobalLocal(false, 3); echo $conditionalBindingGlobal, '|';
