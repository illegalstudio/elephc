<?php
$boolean = true;
try { settype($boolean, 'int'); echo 'W'; } catch (TypeError $error) { echo 'T'; }
echo gettype($boolean), ':', $boolean, '|';
$integer = $argc;
try { settype($integer, 'string'); echo 'W'; } catch (TypeError $error) { echo 'T'; }
echo gettype($integer), ':', $integer, '|';
function mutateSettypeBinding(string $target): void {
    $dynamic = true;
    try { settype($dynamic, $target); echo 'W'; } catch (TypeError $error) { echo 'T'; }
    echo gettype($dynamic), ':', $dynamic, '|';
}
mutateSettypeBinding($argc > 0 ? 'integer' : 'boolean');
mutateSettypeBinding('BOOLEAN');
$first = true; $change = settype(...);
try { $change($first, 'int'); echo 'W'; } catch (TypeError $error) { echo 'T'; }
echo gettype($first), ':', $first, '|';
$selected = true;
$chosen = $argc > 0 ? settype(...) : settype(...);
try { $chosen($selected, 'int'); echo 'W'; } catch (TypeError $error) { echo 'T'; }
echo gettype($selected), ':', $selected, '|';
$copy = true;
try { call_user_func('settype', $copy, 'int'); echo 'W'; } catch (TypeError $error) { echo 'T'; }
echo gettype($copy), ':', $copy, '|';
function settypeTargetAfter(bool &$value): string { $value = false; echo 'S'; return 'int'; }
$ordered = true;
try { settype(type: settypeTargetAfter($ordered), var: $ordered); echo 'W'; }
catch (TypeError $error) { echo 'T'; }
echo gettype($ordered), ':', $ordered, '|';
mixed $openSettype = true;
settype($openSettype, 'int'); echo gettype($openSettype), ':', $openSettype, '|';
$nullSettype = null;
settype($nullSettype, 'null');
try { settype($nullSettype, 'bool'); echo 'W'; } catch (TypeError $error) { echo 'T'; }
echo gettype($nullSettype), ':', $nullSettype, '|';
$arraySettype = [1]; settype($arraySettype, 'array');
try { settype($arraySettype, 'object'); echo 'W'; } catch (TypeError $error) { echo 'T'; }
echo gettype($arraySettype), '|';
