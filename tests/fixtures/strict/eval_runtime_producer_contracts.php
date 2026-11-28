<?php
function runtimeBranchProducer(int $input): void {
    $source = $input > 0 ? '$value=$input>0 ? 1 : 2.5; $value=2.5; echo "U";' : '';
    try {eval($source);} catch(TypeError $error) {echo 'T';}
    echo '|';
}
function runtimeNativeProducer(int $input): void {
    $source = $input > 0 ? '$copy=$input; $copy="bad"; echo "W";' : '';
    try {eval($source);} catch(TypeError $error) {echo 'T';}
    echo '|';
}
runtimeBranchProducer($argc);
runtimeNativeProducer($argc);
gc_collect_cycles();

function nativeUnionOrigin(int $input): int|float { return $input; }
function runtimeDeclaredProducer(int $input): void {
    $source = $input > 0 ? '$value=nativeUnionOrigin($input); $value=2.5; echo "R";' : '';
    eval($source);
    echo '|';
}
function runtimeBuiltinProducer(int $input): void {
    $source = $input > 0 ? '$value=strlen("hi"); $value="bad"; echo "W";' : '';
    try {eval($source);} catch(TypeError $error) {echo 'B';}
    echo '|';
}
runtimeDeclaredProducer($argc);
runtimeBuiltinProducer($argc);
gc_collect_cycles();
