<?php
function resourceAliasStore(mixed &$target,mixed $next): void {$target=$next;}
function checkResourceStore(mixed $next,bool $alias): void {
    $stream=fopen('php://temp','w+');
    $owner=$stream;
    try {
        if($alias) {resourceAliasStore($stream,$next);} else {$stream=$next;}
        echo 'W';
    } catch(TypeError $error) {echo 'T';}
    echo gettype($stream),'|';
    fclose($owner);
}
checkResourceStore(1,false);checkResourceStore(false,false);checkResourceStore(true,false);
checkResourceStore(1,true);checkResourceStore(false,true);checkResourceStore(true,true);
function checkResourceExpression(mixed $next): void {
    $stream=fopen('php://temp','w+');$owner=$stream;
    try {$observed=($stream=$next);echo 'W',gettype($observed);}catch(TypeError $error){echo 'T';}
    echo '|';fclose($owner);
}
checkResourceExpression(1);checkResourceExpression(false);
function checkLiteralResourceScope(): void {
    $stream=fopen('php://temp','w+');$owner=$stream;
    try {eval('$stream=1;');echo 'W';}catch(TypeError $error){echo 'T';}
    echo gettype($stream),'|';fclose($owner);
}
checkLiteralResourceScope();
function checkRuntimeResourceScope(int $requests): void {
    $stream=fopen('php://temp','w+');$owner=$stream;
    $code=$requests>0?'$stream=1;':'';
    try {eval($code);echo 'W';}catch(TypeError $error){echo 'T';}
    echo gettype($stream),'|';fclose($owner);
}
checkRuntimeResourceScope($argc);
gc_collect_cycles();

$code=$argc>0?'
function interpretedResourceReplacement(mixed $next): void {
    $stream=fopen("php://temp","w+");$owner=$stream;
    try {$stream=$next;echo "W";}catch(TypeError $error){echo "T";}
    echo gettype($stream),"|";fclose($owner);
}
interpretedResourceReplacement(1);interpretedResourceReplacement(false);interpretedResourceReplacement(true);
':'';
eval($code);
gc_collect_cycles();

function representationAliasStore(mixed &$target,mixed $next): void {$target=$next;}
function replaceRawResource(mixed $next,bool $alias): void {
    $value=stream_context_get_default();
    try {
        if($alias) {representationAliasStore($value,$next);} else {$value=$next;}
        echo 'W';
    } catch(TypeError $error) {echo 'T';}
    echo gettype($value);
    if(is_resource($value)) {echo ':',get_resource_type($value);}
    echo '|';
}
$stream=tmpfile();
$filter=stream_filter_append($stream,'string.toupper',STREAM_FILTER_WRITE);
replaceRawResource($filter,false);replaceRawResource($filter,true);
replaceRawResource(1,false);replaceRawResource(1,true);
stream_filter_remove($filter);fclose($stream);
gc_collect_cycles();

function checkResourceCopy(mixed $next): void {
    $value=tmpfile();$owner=$value;$copy=$value;
    try {$copy=$next;echo 'W';}catch(TypeError $error){echo 'T';}
    echo gettype($copy),':',gettype($value),'|';fclose($owner);
}
function checkFreshResourceCopy(): void {
    $value=tmpfile();$owner=$value;$copy=$value;
    unset($copy);$copy='fresh';
    echo 'N',gettype($copy),':',gettype($value),'|';fclose($owner);
}
checkResourceCopy(1);checkResourceCopy(false);checkFreshResourceCopy();
gc_collect_cycles();
