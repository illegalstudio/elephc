<?php
$code=$argc>0?'
function probe_static_write(mixed $next): void {
    static $value=1;
    try{$value=$next;echo "SW";}catch(TypeError $error){echo "ST";}
    echo gettype($value),":",$value,"|";
}
function probe_static_union_source(): int|float {return 1;}
function probe_static_mixed_source(): mixed {return 1;}
function probe_static_union(mixed $next): void {
    static $value=probe_static_union_source();
    try{$value=$next;echo "UW";}catch(TypeError $error){echo "UT";}
    echo gettype($value),"|";
}
function probe_static_mixed(mixed $next): void {
    static $value=probe_static_mixed_source();
    $value=$next;echo "M",gettype($value),"|";
}
function probe_static_reflect(mixed $next): void {
    static $value=1;
    try{$value=$next;echo "RW";}catch(TypeError $error){echo "RT";}
    echo gettype($value),"|";
}
function probe_static_snapshot(): mixed {static $value=1;return $value;}
function probe_static_inside_snapshot(): void {
    static $value=1;
    $reflection=new ReflectionFunction("probe_static_inside_snapshot");
    $snapshot=$reflection->getStaticVariables();$snapshot["value"]=2;
    echo "I",$value,"|";
}
function probe_static_reader(): callable {
    static $value=1;
    return static function() use(&$value): mixed{return $value;};
}
function probe_static_unset(): void {
    static $value=1;
    echo "N",gettype($value),":",$value,"|";
    unset($value);$value=2.5;
}
function probe_static_conditional(bool $bind): mixed {
    $value=$bind?2:9;
    if($bind){static $value=1;}
    return $value;
}
$initAttempts=0;
function probe_static_fail(): int {
    global $initAttempts;$initAttempts++;
    throw new RuntimeException("initializer");
}
function probe_static_throw(): void {
    $value="old";
    try{static $value=probe_static_fail();}catch(Throwable $error){echo "X",$value,"|";}
}
probe_static_write(2.5);probe_static_write(3.5);
probe_static_union(2.5);probe_static_mixed(2.5);
$reflection=new ReflectionFunction("probe_static_reflect");$before=$reflection->getStaticVariables();
probe_static_reflect(2.5);
probe_static_snapshot();$reflection=new ReflectionFunction("probe_static_snapshot");
$snapshot=$reflection->getStaticVariables();$snapshot["value"]=2.5;
echo "P",gettype(probe_static_snapshot()),":",probe_static_snapshot(),"|";
probe_static_inside_snapshot();
$read=probe_static_reader();$reflection=new ReflectionFunction("probe_static_reader");
$snapshot=$reflection->getStaticVariables();$snapshot["value"]=2;
echo "A",$read(),"|";
try{$snapshot["value"]=2.5;echo "AW";}catch(TypeError $error){echo "AT";}
echo gettype($read()),"|";
probe_static_unset();probe_static_unset();
echo "C",probe_static_conditional(false),":",probe_static_conditional(true),"|";
probe_static_throw();probe_static_throw();echo "F",$initAttempts,"|";
$recursiveAttempts=0;
function initialize_recursive_static(): int {
    global $recursiveAttempts;$recursiveAttempts++;
    if($recursiveAttempts===1){return read_recursive_static()+10;}
    return 2;
}
function read_recursive_static(): int {static $value=initialize_recursive_static();return $value;}
echo "Z",read_recursive_static(),":",read_recursive_static(),"|";
function rebind_static_reference(mixed $next): void {
    static $value=1;echo "B",$value,":";
    $local=$next;$value=&$local;$value=3.5;echo $value,"|";
}
rebind_static_reference(2.5);rebind_static_reference(2.5);
$globalStatic=probe_static_mixed_source();
function rebind_static_global(): void {
    static $globalStatic=1;echo "G",$globalStatic,"|";
    global $globalStatic;$globalStatic=2.5;
}
rebind_static_global();rebind_static_global();echo "J",gettype($globalStatic),":",$globalStatic,"|";
class StaticDiscardThrows {
    public function __destruct(){throw new RuntimeException("retired");}
}
function probe_static_retirement(): void {
    $value=new StaticDiscardThrows();
    try{static $value=1;}catch(Throwable $error){echo "D",gettype($value),":",$value,"|";}
}
probe_static_retirement();probe_static_retirement();
':'';
eval($code);
gc_collect_cycles();
