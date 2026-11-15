<?php
class StaticWriteTarget {
    public static float $number = 1.0;
    public static string $late;
    public static string $failure;
    public static string $nested;
}
class StaticWriteText {
    public function __toString(): string {
        echo isset(StaticWriteTarget::$late) ? 'X' : 'U';
        return 'text';
    }
    public function __destruct() { echo 'D', StaticWriteTarget::$late; }
}
class StaticWriteFailure {
    public function __toString(): string { throw new ValueError('refused'); }
    public function __destruct() { echo isset(StaticWriteTarget::$failure) ? 'X' : 'F'; }
}
class StaticWriteReentry {
    public function __toString(): string {
        echo isset(StaticWriteTarget::$nested) ? 'X' : 'N';
        StaticWriteTarget::$nested = 'inner';
        echo ':', StaticWriteTarget::$nested;
        return 'outer';
    }
    public function __destruct() { echo 'O', StaticWriteTarget::$nested; }
}
$reflection = new ReflectionClass(StaticWriteTarget::class);
$reflection->setStaticPropertyValue('number', 7);
echo gettype(StaticWriteTarget::$number), ':', StaticWriteTarget::$number;
$rejectReflection = new ReflectionClass(StaticWriteTarget::class);
try { $rejectReflection->setStaticPropertyValue('number', []); echo 'X'; }
catch (TypeError $error) { echo '|T', StaticWriteTarget::$number; }
echo '|';
$textSource = new StaticWriteText();
$lateReflection = new ReflectionClass(StaticWriteTarget::class);
$lateReflection->setStaticPropertyValue('late', $textSource);
echo '|', StaticWriteTarget::$late;
unset($textSource);
$failureSource = new StaticWriteFailure();
$failureReflection = new ReflectionClass(StaticWriteTarget::class);
try { $failureReflection->setStaticPropertyValue('failure', $failureSource); echo 'X'; }
catch (ValueError $error) { echo isset(StaticWriteTarget::$failure) ? '|X' : '|E'; }
unset($error);
unset($failureSource);
echo '|';
$nestedSource = new StaticWriteReentry();
$nestedReflection = new ReflectionClass(StaticWriteTarget::class);
$nestedReflection->setStaticPropertyValue('nested', $nestedSource);
echo '|', StaticWriteTarget::$nested;
unset($nestedSource);
gc_collect_cycles();
