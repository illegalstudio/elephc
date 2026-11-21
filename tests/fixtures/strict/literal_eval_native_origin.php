<?php
function observedFreshOrigin(): void {
    eval('$created = 1;'); echo gettype($created), ':';
    try { eval('$created = "bad";'); echo 'W'; }
    catch (TypeError $error) { echo 'T'; }
    echo gettype($created), ':', $created, '|';
}
function conditionalOpenOrigin(bool $bound): void {
    if ($bound) { mixed $value = 0; }
    try { eval('$value = 2.5; $value = "changed"; echo $value;'); echo 'W|'; }
    catch (TypeError $error) { echo 'T|'; }
}
observedFreshOrigin(); conditionalOpenOrigin(true); conditionalOpenOrigin(false);
