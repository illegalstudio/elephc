<?php
function literalConditionalStore(bool $bound, mixed $replacement): void {
    if($bound) {$value=1;}
    try {eval('$value=2.5;'); echo 'A';} catch(TypeError $error) {echo 'E';}
    try {$value=$replacement; echo 'W';} catch(TypeError $error) {echo 'T';}
    echo gettype($value), ':', $value, '|';
}
literalConditionalStore(false, 3.5);
literalConditionalStore(false, 1);
literalConditionalStore(true, 1);
literalConditionalStore(true, 3.5);
function literalKnownNativeStore(bool $bound): void {
    if($bound) {$value=1;}
    try {eval('$value=2.5;');} catch(TypeError $error) {echo 'E';}
    try {$value=3.5; $value=4.5; echo 'K';} catch(TypeError $error) {echo 'T';}
    echo gettype($value), ':', $value, '|';
}
literalKnownNativeStore(false);
literalKnownNativeStore(true);
gc_collect_cycles();
