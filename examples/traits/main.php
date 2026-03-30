<?php
// Traits — compile-time composition of methods and properties

trait HasName {
    public $name = "elephc";

    public function target() {
        return $this->name;
    }
}

trait Greets {
    public function greet() {
        return "Hello";
    }

    public static function version() {
        return "traits";
    }
}

trait CasualGreets {
    public function greet() {
        return "Hi";
    }
}

class Demo {
    use HasName, Greets, CasualGreets {
        Greets::greet insteadof CasualGreets;
        Greets::greet as protected baseGreet;
    }

    public function greetAll() {
        return $this->baseGreet() . ", " . $this->target();
    }
}

$demo = new Demo();
echo $demo->greetAll() . "\n";
echo Demo::version() . "\n";
