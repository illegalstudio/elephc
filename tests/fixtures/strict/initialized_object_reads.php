<?php
$plain = new stdClass();
$plain->value = null;
echo gettype($plain->value), '|';
if ($argc > 0) { unset($plain->value); }
try { echo $plain->value; echo 'bad'; } catch (Error $error) { echo 'I'; }
echo '|';
$name = $argc > 0 ? 'value' : 'other';
try { echo $plain->$name; echo 'bad'; } catch (Error $error) { echo 'I'; }
echo '|', isset($plain->$name) ? 'P' : 'A', empty($plain->$name) ? 'E' : 'F', ':', $plain->$name ?? 'missing', '|';

class RuntimeInitializedReflection { public $value; }
$reflected = new RuntimeInitializedReflection();
unset($reflected->value);
$property = new ReflectionProperty(RuntimeInitializedReflection::class, 'value');
try { echo $property->getValue($reflected); echo 'bad'; } catch (Error $error) { echo 'I'; }
echo '|';

$updated = new stdClass();
$updated->value = null;
unset($updated->value);
try { $updated->$name += 1; echo 'bad'; } catch (Error $error) { echo 'I'; }
echo ':', isset($updated->$name) ? 'P' : 'A', '|';

function freshInitializedObject(): stdClass {
    $object = new stdClass();
    $object->keep = str_repeat('r', 4);
    return $object;
}
try { echo freshInitializedObject()->missing; echo 'bad'; } catch (Error $error) { echo 'I'; }
echo '|';

try { echo freshInitializedObject()->{str_repeat('x', 2)}; echo 'bad'; } catch (Error $error) { echo 'I'; }
echo '|';
function initializedObjectNameThrows(): string { throw new Error('name'); }
try { echo freshInitializedObject()->{initializedObjectNameThrows()}; } catch (Error $error) { echo 'N'; }
echo '|';
