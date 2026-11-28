<?php
function conditionalRuntimeOrigin(bool $bound, int $seed): void {
    if ($bound) { $value = 1; }
    $source = $seed > 0 ? '$value=2.5; echo "W";' : '';
    try {eval($source);} catch(TypeError $error) {echo 'T';}
    echo gettype($value), ':', $value, '|';
}
function conditionalLiteralOrigin(bool $bound): void {
    if ($bound) { $value = 1; }
    try {eval('$value=2.5;'); echo 'W';} catch(TypeError $error) {echo 'T';}
    echo gettype($value), ':', $value, '|';
}
conditionalRuntimeOrigin(true, $argc);
conditionalRuntimeOrigin(false, $argc);
conditionalLiteralOrigin(true);
conditionalLiteralOrigin(false);
gc_collect_cycles();

function conditionalRepeatedOrigin(bool $bound, int $seed): void {
    if ($bound) {$value=1;}
    $source=$seed>0?'$value=2.5; echo "W";':'';
    for($index=0;$index<2;$index++) {
        try {eval($source);} catch(TypeError $error) {echo 'T';}
        echo gettype($value), ':', $value, '|';
    }
}
conditionalRepeatedOrigin(true, $argc);
conditionalRepeatedOrigin(false, $argc);
gc_collect_cycles();
function conditionalRepeatedLiteralOrigin(bool $bound): void {
    if ($bound) {$value=1;}
    for($index=0;$index<2;$index++) {
        try {eval('$value=2.5;'); echo 'W';} catch(TypeError $error) {echo 'T';}
        echo gettype($value), ':', $value, '|';
    }
}
function repeatedNativeProducer(int $seed): void {
    $source=$seed>0?'$seen=1;':'';
    eval($source);
    $source=$seed>0?'$copy=$seed; $copy="bad"; echo "W";':'';
    try {eval($source);} catch(TypeError $error) {echo 'P';}
    echo '|';
}
function detachedNativeOrigin(int $seed): void {
    $value=1;
    $source=$seed>0?'unset($value); $value=2.5;':'';
    eval($source);
    $source=$seed>0?'$value=3.5; echo "D";':'';
    eval($source);
    echo gettype($value), ':', $value, '|';
}
conditionalRepeatedLiteralOrigin(true);
conditionalRepeatedLiteralOrigin(false);
repeatedNativeProducer($argc);
detachedNativeOrigin($argc);
gc_collect_cycles();
