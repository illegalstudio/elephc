<?php
$attempts=0;
function reflected_initializer(): int {global $attempts;$attempts++;return $attempts;}
class ReflectedConstant {const VALUE=4;}
define('REFLECTED_USER_CONSTANT',3);
function reflected_pending($parameter=5) {
    static $call=reflected_initializer();
    static $parameterValue=$parameter;
    static $number=(1+2)*3;
    static $dead=(1+2<0)?reflected_initializer():9;
    static $items=[2=>'two','01'=>[true],4];
    static $cast=(int)'2';
    static $numeric='2'+1;
    static $core=PHP_INT_MAX;
    static $user=REFLECTED_USER_CONSTANT;
    static $classValue=ReflectedConstant::VALUE;
    static $magic=__FUNCTION__;
    static $bytes="\xff"."\0";
    return $parameterValue;
}
function reflected_broken() {static $value=1/0;return $value;}
function reflected_warning() {static $value='2x'+1;return $value;}
function reflected_fraction() {static $value=1%1.2;return $value;}
$reflection=new ReflectionFunction('reflected_pending');
for($read=0;$read<2;$read++) {
    $snapshot=$reflection->getStaticVariables();
    echo gettype($snapshot['call']),':',gettype($snapshot['parameterValue']),':';
    echo $snapshot['number'],':',$snapshot['dead'],':',count($snapshot['items']),':';
    echo $snapshot['items'][2],':',$snapshot['items'][3],':',$snapshot['cast'],':',$snapshot['numeric'],':';
    echo $snapshot['core']===PHP_INT_MAX?'C':'bad',':',gettype($snapshot['user']),':';
    echo $snapshot['classValue'],':',$snapshot['magic'],':',bin2hex($snapshot['bytes']),':',$attempts,'|';
}
foreach(['reflected_broken','reflected_warning','reflected_fraction'] as $name) {
    $deferred=new ReflectionFunction($name);
    $snapshot=$deferred->getStaticVariables();
    echo gettype($snapshot['value']),'|';
}
echo reflected_pending(7),':';
$snapshot=$reflection->getStaticVariables();
echo $snapshot['call'],':',$snapshot['parameterValue'],':',$attempts,'|';
// Native runtime checks for array operators, IEEE values and warning handlers.
$warnings=0;
set_error_handler(function($level,$message) {global $warnings;$warnings++;return true;});
class ReflectedEffect {public function __construct() {global $attempts;$attempts++;}}
function reflected_more() {
    static $infinity=PHP_FLOAT_MAX*2;
    static $union=[1]+[2];
    static $comparison=[1]==[1];
    static $offset="\xff"[0];
    static $badCast=(int)INF;
    static $badPower=0**-1;
    static $badKey=[1.2=>1];
    static $object=new ReflectedEffect();
    static $builtin=ReflectionMethod::IS_PUBLIC;
    static $ordered=['a'=>[1],'b'=>2]==['b'=>2,'a'=>[1]];
    static $identity=['b'=>2,'a'=>[1]]===['a'=>[1],'b'=>2];
    static $nested=[false]==[[]];
}
$moreReflection=new ReflectionFunction('reflected_more');
$more=$moreReflection->getStaticVariables();
echo 'E',is_infinite($more['infinity']),':',$more['union'][0],':',$more['comparison'],':',bin2hex($more['offset']),':';
echo gettype($more['badCast']),':',gettype($more['badPower']),':',gettype($more['badKey']),':',gettype($more['object']),':';
echo $warnings,':',$attempts,':',$more['builtin'],':',$more['ordered'],':',$more['identity'],':',$more['nested'],'|';
restore_error_handler();
