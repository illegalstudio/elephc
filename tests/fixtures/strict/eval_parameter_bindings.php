<?php
$code=$argc>0?'
function runtimeBindingInt(int $value,mixed $next): void {
    try {$value=$next;echo "W";}catch(TypeError $error){echo "T";}
    echo gettype($value),"|";
}
function runtimeBindingFloat(float $value,mixed $next): void {
    try {$value=$next;echo "W";}catch(TypeError $error){echo "T";}
    echo gettype($value),"|";
}
function runtimeBindingNullable(?int $value,mixed $next): void {
    try {$value=$next;echo "W";}catch(TypeError $error){echo "T";}
    echo gettype($value),"|";
}
function runtimeBindingUnion(int|float $value,mixed $next): void {
    try {$value=$next;echo "W";}catch(TypeError $error){echo "T";}
    echo gettype($value),"|";
}
function runtimeBindingMixed(mixed $value,mixed $next): void {$value=$next;echo gettype($value),"|";}
function runtimeBindingUntyped($value,$next): void {$value=$next;echo gettype($value),"|";}
function runtimeBindingCallable(callable $value,mixed $next): void {
    try {$value=$next;echo "W";}catch(TypeError $error){echo "T";}
    echo gettype($value),"|";
}
function runtimeBindingIterable(iterable $value,mixed $next): void {
    try {$value=$next;echo "W";}catch(TypeError $error){echo "T";}
    echo gettype($value),"|";
}
function runtimeBindingVariadic(mixed $next,int ...$values): void {
    try {$values=$next;echo "W";}catch(TypeError $error){echo "T";}
    echo gettype($values),"|";
}
function runtimeBindingCallback(int $value): int {return $value+10;}
function runtimeBindingInferred(mixed $next): void {
    $value=static function(int $number): int {return $number+1;};
    try {$value=$next;echo "W";}catch(TypeError $error){echo "T";}
    echo gettype($value);if(is_callable($value)){echo $value(1);}echo "|";
}
function runtimeBindingRefStore(mixed &$target,mixed $next): void {$target=$next;}
function runtimeBindingAlias(int $value,mixed $next): void {
    try {runtimeBindingRefStore($value,$next);echo "W";}catch(TypeError $error){echo "T";}
    echo gettype($value),"|";
}
function runtimeBindingFresh(int $value): void {unset($value);$value="fresh";echo gettype($value),"|";}
class RuntimeBindingMethods {
    function instance(int $value,mixed $next): void {
        try {$value=$next;echo "W";}catch(TypeError $error){echo "T";}
        echo gettype($value),"|";
    }
    static function entry(int $value,mixed $next): void {
        try {$value=$next;echo "W";}catch(TypeError $error){echo "T";}
        echo gettype($value),"|";
    }
    static function nominal(self $value,mixed $next): void {
        try {$value=$next;echo "W";}catch(TypeError $error){echo "T";}
        echo get_class($value),"|";
    }
}
class RuntimeBindingChildMethods extends RuntimeBindingMethods {
    static function nominalParent(parent $value,mixed $next): void {
        try {$value=$next;echo "W";}catch(TypeError $error){echo "T";}
        echo get_class($value),"|";
    }
}
runtimeBindingInt(1,2.5);runtimeBindingInt("2",3);
runtimeBindingFloat(1,2);runtimeBindingFloat(1,2.5);
runtimeBindingNullable(null,2.5);runtimeBindingNullable(null,2);
runtimeBindingUnion(1,2.5);runtimeBindingMixed(1,2.5);runtimeBindingUntyped(1,2.5);
runtimeBindingCallable("runtimeBindingCallback",1);
runtimeBindingCallable("runtimeBindingCallback","missingRuntimeBindingCallback");
runtimeBindingCallable("runtimeBindingCallback","RUNTIMEBINDINGCALLBACK");
runtimeBindingIterable([1],1);runtimeBindingIterable([1],[2]);
runtimeBindingVariadic(1,2);runtimeBindingVariadic([2.5],2);
runtimeBindingInferred("runtimeBindingCallback");runtimeBindingInferred("missingRuntimeBindingCallback");
runtimeBindingAlias(1,2.5);runtimeBindingFresh(1);
$object=new RuntimeBindingMethods();$object->instance(1,2.5);RuntimeBindingMethods::entry(1,2.5);
$closure=static function(int $value,mixed $next): void {
    try {$value=$next;echo "W";}catch(TypeError $error){echo "T";}
    echo gettype($value),"|";
};
$closure(1,2.5);
RuntimeBindingMethods::nominal($object,new stdClass());
RuntimeBindingChildMethods::nominalParent($object,new stdClass());
':'';
eval($code);
gc_collect_cycles();
