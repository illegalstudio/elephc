<?php
// PHP binds the top-level functions and the simple classes of a fragment when it compiles it,
// before the first statement runs. A fragment may therefore use them above the line that
// declares them. The fragment is selected at run time, so the eval interpreter runs it.
$fragment = <<<'CODE'
echo greet("early binding"), "\n";
echo class_exists("Shape") ? "Shape is bound\n" : "Shape is not bound yet\n";

// A class that implements an interface is linked only when its statement runs.
echo class_exists("Circle") ? "Circle is bound\n" : "Circle is not bound yet\n";

// Binding a class evaluates none of its members: this constant is read at its first use,
// after the statement that defines UNIT has run.
define("UNIT", "cm");
echo Shape::UNIT_NAME, "\n";

function greet(string $name): string { return "hello, $name"; }
class Shape { const UNIT_NAME = UNIT; }
interface HasArea {}
class Circle extends Shape implements HasArea {}

echo class_exists("Circle") ? "Circle is bound\n" : "Circle is not bound yet\n";
CODE;

eval($argc > 1 ? $argv[1] : $fragment);
