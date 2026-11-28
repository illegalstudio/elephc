<?php
function initializedBoxedNamed(mixed $object): mixed { return $object->value; }
function initializedBoxedDynamic(mixed $object, string $name): mixed { return $object->$name; }
function initializedBoxedProbe(mixed $object, string $name): void {
    echo isset($object->$name) ? 'P' : 'A', empty($object->$name) ? 'E' : 'F', ':', $object->$name ?? 'missing', '|';
}
function initializedBoxedFresh(): mixed {
    $object = new stdClass();
    $object->keep = str_repeat('b', 4);
    return $object;
}
class InitializedBoxedAlternative { public $value; public int $typed; }
function initializedBoxedUnion(stdClass|InitializedBoxedAlternative $object): mixed { return $object->value; }
$boxed = new stdClass();
$boxed->value = null;
echo gettype(initializedBoxedNamed($boxed)), ':', gettype(initializedBoxedDynamic($boxed, 'value')), '|';
if ($argc > 0) { unset($boxed->value); }
try { echo initializedBoxedNamed($boxed); echo 'bad'; } catch (Error $error) { echo 'I'; }
echo '|';
try { echo initializedBoxedDynamic($boxed, 'value'); echo 'bad'; } catch (Error $error) { echo 'I'; }
echo '|';
initializedBoxedProbe($boxed, 'value');
initializedBoxedProbe(null, 'value');
try { echo initializedBoxedNamed(initializedBoxedFresh()); echo 'bad'; } catch (Error $error) { echo 'I'; }
echo '|';
try { echo initializedBoxedDynamic(initializedBoxedFresh(), 'value'); echo 'bad'; } catch (Error $error) { echo 'I'; }
echo '|';
try { echo initializedBoxedUnion($boxed); echo 'bad'; } catch (Error $error) { echo 'I'; }
echo '|';
$alternative = new InitializedBoxedAlternative();
echo gettype(initializedBoxedUnion($alternative)), '|';
unset($alternative->value);
try { echo initializedBoxedUnion($alternative); echo 'bad'; } catch (Error $error) { echo 'I'; }
echo '|';

function initializedBoxedDynamicAgain(mixed $object, string $name): mixed { return $object->$name; }
function initializedBoxedProbeAgain(mixed $object, string $name): void {
    echo isset($object->$name) ? 'P' : 'A', empty($object->$name) ? 'E' : 'F', ':', $object->$name ?? 'missing', '|';
}
initializedBoxedProbeAgain($alternative, 'typed');
try { echo initializedBoxedDynamicAgain($boxed, 'value'); echo 'bad'; } catch (Error $error) { echo 'I'; }
echo '|';
function initializedBoxedUnlisted(mixed $object): mixed { return $object->unlistedInitializedName; }
try { echo initializedBoxedUnlisted(initializedBoxedFresh()); echo 'bad'; } catch (Error $error) { echo 'I'; }
echo '|';
