<?php
function nativeLiteralOriginCopy(int $seed): void {
    eval('$created=2.5;');
    $copy=$created;
    try {$copy=$seed; echo 'W';} catch(TypeError $error) {echo 'T';}
    echo gettype($copy), ':', $copy, '|';
}
nativeLiteralOriginCopy($argc);
gc_collect_cycles();

function nativeLiteralCopyChain(int $seed): void {
    eval('$created=2.5;'); $copy=$created; $copy=3.5; $second=$copy;
    try {$second=$seed; echo 'W';} catch(TypeError $error) {echo 'T';}
    echo $created, ':', $copy, ':', $second, '|';
}
function nativeLiteralKindCopies(mixed $replacement): void {
    eval('$text="before"; $nil=null; $rows=[1];');
    $copyText=$text; $copyNull=$nil; $copyRows=$rows;
    try {$copyText=$replacement; echo 'W';} catch(TypeError $error) {echo 'T';}
    echo gettype($copyText), ':', $copyText, '|';
    try {$copyNull=$replacement; echo 'W';} catch(TypeError $error) {echo 'T';}
    echo gettype($copyNull), '|';
    try {$copyRows=$replacement; echo 'W';} catch(TypeError $error) {echo 'T';}
    $copyRows=['ok']; echo gettype($copyRows), ':', $rows[0], ':', $copyRows[0], '|';
}
function nativeLiteralOpenCopy(int $seed): void {
    mixed $copy='before'; eval('$created=2.5;'); $copy=$created; $copy=$seed;
    echo gettype($copy), ':', $copy, '|';
}
function nativeLiteralAliasCopy(int $seed): void {
    eval('$created=2.5;'); $copy=$created; $alias=&$copy;
    try {$alias=$seed; echo 'W';} catch(TypeError $error) {echo 'T';}
    echo $created, ':', $copy, ':', $alias, '|';
}
nativeLiteralCopyChain($argc); nativeLiteralKindCopies(1);
nativeLiteralOpenCopy($argc); nativeLiteralAliasCopy($argc);
gc_collect_cycles();


function copiedConditionalEval(bool $run, int $seed): void {
 if($run) {eval('$created=2.5;');}
 $copy=$seed; $second=$copy;
 try {$second=3.5; echo 'W';} catch(TypeError $error) {echo 'T';}
 echo gettype($second),':',$second,'|';
}
copiedConditionalEval(false,$argc); copiedConditionalEval(true,$argc);
