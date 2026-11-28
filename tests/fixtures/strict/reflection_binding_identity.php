<?php
class ReflectionBindingTarget0 { public static string $text = "old"; }
class ReflectionBindingText0 {
    public function __toString(): string { echo "S"; return "new"; }
    public function __destruct() {
        global $secondBinding0, $argc;
        echo "D", ReflectionBindingTarget0::$text;
        $rebind = $argc > 0 ? 'ReflectionBindingTarget0::$text =& $secondBinding0;' : '';
        eval($rebind);
        echo ":", ReflectionBindingTarget0::$text;
    }
}
$firstBinding0 = "old"; $secondBinding0 = "second";
class ReflectionBindingTarget1 { public static string $text = "old"; }
class ReflectionBindingText1 {
    public function __toString(): string { echo "S"; return "new"; }
    public function __destruct() {
        global $secondBinding1, $argc;
        echo "D", ReflectionBindingTarget1::$text;
        $rebind = $argc > 0 ? 'ReflectionBindingTarget1::$text =& $secondBinding1;' : '';
        eval($rebind);
        echo ":", ReflectionBindingTarget1::$text;
    }
}
$firstBinding1 = "old"; $secondBinding1 = "second";
$firstBinding1 =& ReflectionBindingTarget1::$text;
class ReflectionBindingTarget2 { public static string $text = "old"; }
class ReflectionBindingText2 {
    public function __toString(): string { echo "S"; return "new"; }
    public function __destruct() {
        global $secondBinding2, $argc;
        echo "D", ReflectionBindingTarget2::$text;
        $rebind = $argc > 0 ? 'ReflectionBindingTarget2::$text =& $secondBinding2;' : '';
        eval($rebind);
        echo ":", ReflectionBindingTarget2::$text;
        throw new ValueError("retirement");
    }
}
$firstBinding2 = "old"; $secondBinding2 = "second";
class ReflectionBindingTarget3 { public static string $text = "old"; }
class ReflectionBindingText3 {
    public function __toString(): string { echo "S"; return "new"; }
    public function __destruct() {
        global $secondBinding3, $argc;
        echo "D", ReflectionBindingTarget3::$text;
        $rebind = $argc > 0 ? 'ReflectionBindingTarget3::$text =& $secondBinding3;' : '';
        eval($rebind);
        echo ":", ReflectionBindingTarget3::$text;
        throw new ValueError("retirement");
    }
}
$firstBinding3 = "old"; $secondBinding3 = "second";
$firstBinding3 =& ReflectionBindingTarget3::$text;
// invoke cases
$reflector0 = new ReflectionClass(ReflectionBindingTarget0::class);
try { $reflector0->setStaticPropertyValue("text", new ReflectionBindingText0()); }
catch (ValueError $retirementError0) { echo "T"; unset($retirementError0); }
echo "|", $firstBinding0, ":", $secondBinding0, ":", ReflectionBindingTarget0::$text, "\n";
$reflector1 = new ReflectionClass(ReflectionBindingTarget1::class);
try { $reflector1->setStaticPropertyValue("text", new ReflectionBindingText1()); }
catch (ValueError $retirementError1) { echo "T"; unset($retirementError1); }
echo "|", $firstBinding1, ":", $secondBinding1, ":", ReflectionBindingTarget1::$text, "\n";
$reflector2 = new ReflectionClass(ReflectionBindingTarget2::class);
try { $reflector2->setStaticPropertyValue("text", new ReflectionBindingText2()); }
catch (ValueError $retirementError2) { echo "T"; unset($retirementError2); }
echo "|", $firstBinding2, ":", $secondBinding2, ":", ReflectionBindingTarget2::$text, "\n";
$reflector3 = new ReflectionClass(ReflectionBindingTarget3::class);
try { $reflector3->setStaticPropertyValue("text", new ReflectionBindingText3()); }
catch (ValueError $retirementError3) { echo "T"; unset($retirementError3); }
echo "|", $firstBinding3, ":", $secondBinding3, ":", ReflectionBindingTarget3::$text, "\n";
gc_collect_cycles();
