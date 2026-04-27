<?php

namespace App\Demo;

// __DIR__ in include paths — the idiomatic PHP way to load a sibling file
// regardless of where the script was launched from.
require __DIR__ . '/lib/helper.php';

// File-related magic constants — useful for finding sibling files relative to
// the currently-executing source file.
echo "__FILE__ = " . __FILE__ . "\n";
echo "__DIR__  = " . __DIR__ . "\n";
echo "__LINE__ = " . __LINE__ . "\n";
echo "__NAMESPACE__ = " . __NAMESPACE__ . "\n";

// Inside a free function, __FUNCTION__ is namespace-qualified.
function greet() {
    echo "  in greet():  __FUNCTION__ = " . __FUNCTION__ . "\n";
    echo "  in greet():  __METHOD__   = " . __METHOD__ . "\n";
}
greet();

// Inside a method, __CLASS__ is the FQN class, __METHOD__ is "Class::method".
class Greeter {
    public function hello() {
        echo "  in Greeter::hello():\n";
        echo "    __CLASS__    = " . __CLASS__ . "\n";
        echo "    __METHOD__   = " . __METHOD__ . "\n";
        echo "    __FUNCTION__ = " . __FUNCTION__ . "\n";
    }
}
$g = new Greeter();
$g->hello();

// Inside a trait method, __TRAIT__ is the trait's FQN.
trait Reportable {
    public function report() {
        echo "  in Reportable::report():\n";
        echo "    __TRAIT__    = " . __TRAIT__ . "\n";
    }
}
class Service {
    use Reportable;
}
$s = new Service();
$s->report();

// Inside a closure, __FUNCTION__ becomes "{closure}".
$f = function() {
    echo "  inside closure: __FUNCTION__ = " . __FUNCTION__ . "\n";
};
$f();
