<?php
function currentNativeStore(mixed $replacement, int $seed): void {
    $value=1;
    $source=$seed>0?'unset($value); $value=2.5;':'';
    eval($source);
    try {$value=$replacement; echo 'W';} catch(TypeError $error) {echo 'T';}
    echo gettype($value), ':', $value, '|';
}
function currentNativeExpression(mixed $replacement, int $seed): void {
    $value=1;
    $source=$seed>0?'unset($value); $value=2.5;':'';
    eval($source);
    try {$observed=($value=$replacement); echo 'W',gettype($observed),':',$observed;}
    catch(TypeError $error) {echo 'T';}
    echo ':',gettype($value),':',$value,'|';
}
function currentOwnedText(string $text): mixed {echo 'O'; return $text.'!';}
function currentNativeOwnedString(int $seed): void {
    $value=1;
    $source=$seed>0?'unset($value); $value="before";':'';
    eval($source);
    try {$value=currentOwnedText('after'); echo 'W';} catch(TypeError $error) {echo 'T';}
    echo gettype($value), ':', $value, '|';
}
function currentNativeOwnedRejection(int $seed): void {
    $value=1;
    $source=$seed>0?'unset($value); $value=2.5;':'';
    eval($source);
    try {$value=currentOwnedText('after'); echo 'W';} catch(TypeError $error) {echo 'T';}
    echo gettype($value), ':', $value, '|';
}
function currentNativeArray(mixed $replacement, int $seed): void {
    $value=1;
    $source=$seed>0?'unset($value); $value=[1];':'';
    eval($source);
    try {$value=$replacement; echo 'W';} catch(TypeError $error) {echo 'T';}
    echo gettype($value), '|';
}
function currentNativeNull(mixed $replacement, int $seed): void {
    $value=1;
    $source=$seed>0?'unset($value); $value=null;':'';
    eval($source);
    try {$value=$replacement; echo 'W';} catch(TypeError $error) {echo 'T';}
    echo gettype($value), ':', $value, '|';
}
function currentNativeOpen(mixed $input, mixed $replacement, int $seed): void {
    $value=1;
    $source=$seed>0?'unset($value); $value=$input;':'';
    eval($source);
    $value=$replacement;
    echo gettype($value), ':', $value, '|';
}
function currentNativeKnownFloat(int $seed): void {
    $value=1;
    eval($seed>0?'unset($value); $value=2.5;':'');
    try {$value=3.5; $value=4.5; echo 'K';} catch(TypeError $error) {echo 'T';}
    echo gettype($value), ':', $value, '|';
}
currentNativeStore(3.5, $argc);
currentNativeStore(1, $argc);
currentNativeExpression(3.5, $argc);
currentNativeExpression(1, $argc);
currentNativeOwnedString($argc);
currentNativeOwnedRejection($argc);
currentNativeArray(['ok'], $argc);
currentNativeArray(1, $argc);
currentNativeNull(null, $argc);
currentNativeNull(1, $argc);
currentNativeOpen(2.5, 1, $argc);
currentNativeKnownFloat($argc);
currentNativeKnownFloat(0);
gc_collect_cycles();
