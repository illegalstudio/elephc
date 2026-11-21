<?php
interface ArgumentReceiver { public function fill(mixed &$target): void; }
class ArgumentBox implements ArgumentReceiver {
    public function fill(mixed &$target): void { $target = 'method'; }
    public static function fillStatic(mixed &$target): void { $target = 'static'; }
    public function rest(mixed &...$targets): void { $targets[0] = 'first'; $targets['tail'] = 'last'; }
    public function inspect(mixed &$target, string $observed): void {
        echo $observed, ':', gettype($target), '|';
        $target = 'seen';
    }
}
$box = new ArgumentBox();
$box->fill($method); echo $method, '|';
ArgumentBox::fillStatic(target: $static); echo $static, '|';
$box->inspect(target: $ordered, observed: array_key_exists('ordered', get_defined_vars()) ? 'present' : 'missing');
echo $ordered, '|';
class ConstructBinding {
    public function __construct(mixed &$target) { $target = 'constructor'; }
    public static function create(): void { $copy = new self($created); echo $created, '|'; }
}
$constructed = new ConstructBinding($constructor); echo $constructor, '|';
(function(mixed &$target): void { $target = 'immediate'; })($immediate);
echo $immediate, '|';

ConstructBinding::create();
class InheritedArguments extends ArgumentBox {}
$child = new InheritedArguments(); $child->FILL(target: $inherited); echo $inherited, '|';
function throughInterface(ArgumentReceiver $receiver): void {
    $receiver->fill($interfaceBinding); echo $interfaceBinding, '|';
}
throughInterface($box);
function optionalArgument(?ArgumentBox $receiver): void {
    $receiver?->fill($optional);
    echo array_key_exists('optional', get_defined_vars()) ? 'present|' : 'absent|';
}
optionalArgument(null);
optionalArgument($box);
$box->rest($first, tail: $last); echo $first, ':', $last, '|';
(function(mixed &$target): void {})($emptyClosure);
echo array_key_exists('emptyClosure', get_defined_vars()) ? 'formed|' : 'lost|';
function rewriteArgument(mixed &$target): string { $target = 'later'; return 'changed'; }
$box->inspect(target: $changed, observed: rewriteArgument($changed));
echo $changed, '|';
function conditionalScope(bool $enabled): void {
    $knownNull = null;
    if ($enabled) { $capture = function() use (&$branch) {}; }
    $names = get_defined_vars();
    echo array_key_exists('knownNull', $names) ? 'null:' : 'lost-null:';
    echo array_key_exists('branch', $names) ? 'present|' : 'absent|';
    $future = 'after';
}
conditionalScope(false);
conditionalScope(true);
$fixed = 1;
try { $box->fill($fixed); echo 'W'; } catch (TypeError $error) { echo 'T'; }
echo gettype($fixed), '|';
