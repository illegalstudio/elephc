<?php
function extendedGlobalRead(): mixed { global $root; return $root; }
function extendedRootReader(): mixed {
    global $root;
    return function() use (&$root): mixed { return $root; };
}
$root = 'old';
$oldRoot = extendedRootReader();
if ($argc > 0) { unset($root); }
echo isset($root) ? 'P' : 'A', ':', $root ?? 'missing', '|';
try { echo $root; } catch (Error $error) { echo 'I'; }
echo '|';
$root = 'new';
echo $oldRoot(), ':', extendedGlobalRead(), '|';
class ExtendedInitializationFields { public $loose; public int $typed; }
$object = new ExtendedInitializationFields();
$key = $argc > 0 ? 'loose' : 'other';
unset($object->{$key});
try { echo $object->{$key}; } catch (Error $error) { echo 'I'; }
echo '|', isset($object->{$key}) ? 'P' : 'A', empty($object->{$key}) ? 'E' : 'F', ':', $object->{$key} ?? 'missing', '|';
$property = new ReflectionProperty(ExtendedInitializationFields::class, 'loose');
try { echo $property->getValue($object); } catch (Error $error) { echo 'I'; }
echo '|';
try { $object->{$key} += 1; echo 'bad'; } catch (Error $error) {
    echo str_contains($error->getMessage(), 'strict-initialization') ? 'I' : 'other';
}
echo ':', isset($object->{$key}) ? 'P' : 'A', '|';
$missingName = $argc > 0 ? 'missing' : 'loose';
try { echo $object->{$missingName}; } catch (Error $error) { echo 'I'; }
echo '|';
class ExtendedRetiredRoot {
    function __destruct() { throw new Error('retired'); }
}
function extendedRetiredRootTouch(): void { global $retiredRoot; }
$retiredRoot = new ExtendedRetiredRoot(); extendedRetiredRootTouch();
try { if ($argc > 0) { unset($retiredRoot); } } catch (Error $error) { echo 'D'; }
echo ':';
try { echo $retiredRoot; } catch (Error $error) { echo 'I'; }
echo '|';
