<?php
function native_late_static(): void {
    $value="local";$alias=&$value;static $value="seed";
    echo $value,":",$alias,"|";$value="retained";
}
function native_conditional_static(bool $bind): int {
    $value=$bind?2:9;if($bind){static $value=1;}return $value;
}
function native_fail_static(): int {throw new RuntimeException("init");}
function native_late_throw(): void {
    $value="old";try{static $value=native_fail_static();}catch(Throwable $error){echo $value,"|";}
}
class NativeStaticDiscardThrows {
    public function __destruct(){throw new RuntimeException("retired");}
}
function native_static_retirement(): void {
    $value=new NativeStaticDiscardThrows();
    try{static $value=1;}catch(Throwable $error){echo gettype($value),":",$value,"|";}
}
function native_static_new_origin(): void {
    $value="old";static $value=1;$value=2;echo $value,"|";
}
native_late_static();native_late_static();
echo native_conditional_static(false),":",native_conditional_static(true),":",native_conditional_static(false),"|";
native_late_throw();native_late_throw();native_static_retirement();native_static_retirement();
native_static_new_origin();native_static_new_origin();

function native_static_dynamic(mixed $next): void {
    static $value=1;
    try{$value=$next;echo "W";}catch(TypeError $error){echo "T";}
    echo gettype($value),":",$value,"|";
}
function native_static_union_seed(int|float $seed,mixed $next): void {
    if(is_int($seed)){
        static $value=$seed;$value=$next;echo gettype($value),":",$value,"|";
    }
}
function native_static_mixed_seed(mixed $seed,mixed $next): void {
    if(is_int($seed)){
        static $value=$seed;$value=$next;echo gettype($value),":",$value,"|";
    }
}
function native_static_ref_parameter(string &$value,bool $bind): void {
    if($bind){static $value=1;}
    echo gettype($value),":",$value,"|";
}
function native_static_array_owner(): void {
    $seed=[1];static $value=$seed;unset($seed);
    $copy=$value;$value[0]=2;echo $copy[0],":",$value[0],"|";
}
class NativeStaticToken {public string $label="kept";}
function native_static_object_owner(): void {
    $seed=new NativeStaticToken();static $value=$seed;unset($seed);
    echo $value->label,"|";
}
function native_static_callable_owner(): void {
    $seed=static function(): int {return 7;};static $value=$seed;unset($seed);
    echo $value(),"|";
}
native_static_dynamic(2.5);native_static_dynamic(3.5);
native_static_union_seed(1,2.5);native_static_mixed_seed(1,"new");
$native_static_parameter="local";
native_static_ref_parameter($native_static_parameter,false);
native_static_ref_parameter($native_static_parameter,true);
echo $native_static_parameter,"|";
native_static_array_owner();native_static_array_owner();
native_static_object_owner();native_static_object_owner();
native_static_callable_owner();native_static_callable_owner();

function native_static_recursive_initializer(): int {
    static $calls=0;$calls=$calls+1;
    return $calls<2?native_static_recursive_value():$calls;
}
function native_static_recursive_value(): int {
    static $value=native_static_recursive_initializer();return $value;
}
echo native_static_recursive_value(),":",native_static_recursive_value(),"|";
