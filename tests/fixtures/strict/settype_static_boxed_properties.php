<?php
class StaticConversionPlace {
    public static mixed $value = '7.9';
    public static int|string $choice = '8.9';
    public static array $values = [1];
}
class StaticConversionChild extends StaticConversionPlace {}
$alias =& StaticConversionPlace::$value;
$choice =& StaticConversionPlace::$choice;
$values =& StaticConversionPlace::$values;
settype(StaticConversionPlace::$value, 'int');
echo $alias;
function staticConversionTarget(): string {
    StaticConversionPlace::$value = '18.9';
    return 'int';
}
settype(StaticConversionPlace::$value, staticConversionTarget());
echo '|', $alias;
$cast = settype(...);
$cast(StaticConversionChild::$choice, 'int');
echo '|', gettype($choice), ':', $choice;
try { $cast(StaticConversionPlace::$choice, 'array'); echo 'X'; }
catch (TypeError $error) { echo '|A', $choice; }
function replaceStaticConversionValue(mixed &$value, mixed $replacement): void {
    echo 'W';
    $value = $replacement;
    echo 'P';
}
$replace = substr('replaceStaticConversionValue', 0, strlen($argv[0]) + 1024);
$replace(StaticConversionPlace::$choice, 'text');
echo '|', $choice;
try { $replace(StaticConversionPlace::$choice, []); echo 'X'; }
catch (TypeError $error) { echo '|R', $choice; }
$replace(StaticConversionPlace::$values, ['next']);
echo '|', implode(',', $values);
try { $replace(StaticConversionPlace::$values, new stdClass()); echo 'X'; }
catch (TypeError $error) { echo '|T', implode(',', $values); }
gc_collect_cycles();
class StaticConversionRetiredValue {
    public function __destruct() { echo 'D'; }
}
function staticConversionRetargetWithDestructor(): string {
    StaticConversionPlace::$value = '27.9';
    echo 'B';
    return 'int';
}
StaticConversionPlace::$value = new StaticConversionRetiredValue();
settype(StaticConversionPlace::$value, staticConversionRetargetWithDestructor());
echo '|S', $alias;
function localConversionTarget(mixed &$value): string {
    $value = '31.9';
    echo 'B';
    return 'int';
}
$local = new StaticConversionRetiredValue();
settype($local, localConversionTarget($local));
echo '|L', $local;
gc_collect_cycles();
