<?php
function native_detach_static(mixed $next): void {
    static $value=1;$alias=&$value;
    echo gettype($value),":",$value,"|";
    unset($value);$value=2.5;
    try{$value=$next;echo "W";}catch(TypeError $error){echo "T";}
    echo gettype($value),":",$value,":",$alias,"|";
}
function native_write_static_reference(mixed &$value,mixed $next): void {$value=$next;}
function native_mutate_static_reference(mixed $next): void {
    static $value=1;
    try{native_write_static_reference($value,$next);echo "W";}catch(TypeError $error){echo "T";}
    echo gettype($value),":",$value,"|";
}
function &native_escaped_static_reference(): mixed {static $value=1;return $value;}
function native_mutate_escaped_static(mixed $next): void {
    $alias=&native_escaped_static_reference();
    try{$alias=$next;echo "W";}catch(TypeError $error){echo "T";}
    echo gettype($alias),":",$alias,"|";
}
function native_static_string_update(): void {
    static $value="9";
    try{$value++;echo "W";}catch(TypeError $error){echo "T";}
    echo gettype($value),":",$value,"|";
}
$native_static_from_global=9;
function native_static_after_global(): void {
    global $native_static_from_global;
    static $native_static_from_global="seed";
    echo $native_static_from_global,"|";
}
native_detach_static(3.5);native_detach_static("fresh");
native_mutate_static_reference(2.5);native_mutate_static_reference(3.5);
native_mutate_escaped_static(2.5);native_mutate_escaped_static(3.5);
native_static_string_update();native_static_string_update();
native_static_after_global();echo $native_static_from_global,"|";
