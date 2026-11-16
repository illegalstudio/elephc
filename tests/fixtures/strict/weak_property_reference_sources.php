<?php
function writeWeakPropertyReference(mixed &$target, mixed $value): void {
    $target = $value;
}

$fragment = $argc > 0 ? '
class WeakSourceText { public string $text = "old"; }
class WeakSourceFormatter {
    public function __toString(): string {
        global $firstSource;
        echo "S";
        unset($firstSource->text);
        return "new";
    }
}
$firstSource = new WeakSourceText();
$secondSource = new WeakSourceText();
$source = "old";
$firstSource->text =& $source;
$secondSource->text =& $source;
writeWeakPropertyReference($source, new WeakSourceFormatter());
echo ":", $source, ":", $secondSource->text, ";";

class WeakSourceInteger { public int $number = 1; }
$integerSource = new WeakSourceInteger();
$integerAlias = 1;
$integerSource->number =& $integerAlias;
set_error_handler(function(int $kind, string $message): bool {
    echo $kind == E_DEPRECATED ? "P" : "X";
    throw new RuntimeException("stop");
});
try { writeWeakPropertyReference($integerAlias, 7.5); }
catch (Throwable $precisionError) {
    echo get_class($precisionError), ":", get_class($precisionError->getPrevious());
    unset($precisionError);
}
restore_error_handler();
echo ":", $integerAlias, ":", $integerSource->number;

class WeakSourceMixed { public mixed $value = "42.9"; }
$copiedOwner = new WeakSourceMixed();
$copiedAlias = "42.9";
$copiedOwner->value =& $copiedAlias;
$copiedObject = clone $copiedOwner;
unset($copiedOwner->value);
$copiedOwner->value = "7.9";
settype($copiedAlias, "int");
echo "|", $copiedAlias, ":", $copiedObject->value, ":", $copiedOwner->value;

class WeakSourceRetirement {
    public function __destruct() {
        global $replacementOwner;
        echo "D";
        $replacementOwner->value = "replaced";
    }
}
$replacementOwner = new WeakSourceMixed();
$replacementOwner->value = new WeakSourceRetirement();
unset($replacementOwner->value);
echo ":", $replacementOwner->value;
' : '';
eval($fragment);
gc_collect_cycles();
