<?php
function nativeAfterRebinding(mixed $replacement, int $seed): void {
    $value=1;
    $source=$seed>0?'unset($value); $value=2.5;':'';
    eval($source);
    try {$value=$replacement; echo 'W';} catch(TypeError $error) {echo 'T';}
    echo gettype($value), ':', $value, '|';
}
function nativeRuntimeOriginCopy(int $seed): void {
    $source=$seed>0?'$created=2.5;':'';
    eval($source);
    $copy=$created;
    try {$copy=$seed; echo 'W';} catch(TypeError $error) {echo 'T';}
    echo gettype($copy), ':', $copy, '|';
}
nativeAfterRebinding(3.5, $argc);
nativeAfterRebinding(1, $argc);
nativeRuntimeOriginCopy($argc);
gc_collect_cycles();


function copyNativeInt(int $seed): void {
 $source=$seed; eval($seed>0?'$marker=0;':''); $copy=$source;
 try {$copy=3.5; echo 'W';} catch(TypeError $e) {echo 'T';}
 echo gettype($copy),':',$copy,'|';
}
function copyNativeUnion(int|float $source,int $seed): void {
 eval($seed>0?'$marker=0;':''); $copy=$source; $copy=3.5;
 try {$copy='bad'; echo 'W';} catch(TypeError $e) {echo 'T';}
 echo gettype($copy),':',$copy,'|';
}
function copyOpenSource(mixed $source,int $seed): void {
 eval($seed>0?'$marker=0;':''); $copy=$source; $copy=1;
 echo gettype($copy),':',$copy,'|';
}
function copyRuntimeUnion(int $seed): void {
 eval($seed>0?'$source=$seed>0?1:2.5;':''); $copy=$source; $copy=3.5;
 try {$copy='bad'; echo 'W';} catch(TypeError $e) {echo 'T';}
 echo gettype($copy),':',$copy,'|';
}
function copyRuntimeOpen(mixed $input,int $seed): void {
 eval($seed>0?'$source=$input;':''); $copy=$source; $copy=1;
 echo gettype($copy),':',$copy,'|';
}
function copyRuntimeIndependent(int $seed): void {
 eval($seed>0?'$source=2.5;':''); $copy=$source; $copy=3.5;
 $second=$copy;
 eval($seed>0?'unset($source); $source="new";':'');
 try {$second=$seed; echo 'W';} catch(TypeError $e) {echo 'T';}
 echo gettype($copy),':',$copy,':',gettype($second),':',$second,'|';
}
function copyExpression(int $seed): void {
 eval($seed>0?'$source=2.5;':''); $observed=($copy=$source);
 try {$copy=$seed; echo 'W';} catch(TypeError $e) {echo 'T';}
 echo $observed,':',$copy,'|';
}
copyNativeInt($argc); copyNativeUnion(1,$argc); copyOpenSource(2.5,$argc);
copyRuntimeUnion($argc); copyRuntimeOpen(2.5,$argc); copyRuntimeIndependent($argc); copyExpression($argc);
gc_collect_cycles();

function copyNativeDeclaredArray(int $seed): void {
    array<int> $source=[1]; eval($seed>0?'$marker=0;':''); $copy=$source;
    try {$copy=$seed; echo 'W';} catch(TypeError $error) {echo 'T';}
    echo gettype($copy), '|';
}
copyNativeDeclaredArray($argc);
