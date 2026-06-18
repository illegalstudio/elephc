<?php
// Cloning — the `clone` expression and `__clone()` magic method.

class Engine {
    public int $power;

    public function __construct(int $power) {
        $this->power = $power;
    }
}

class Car {
    public string $model;
    public int $wheels;
    public Engine $engine;   // object property: shared by the shallow clone

    public function __construct(string $model, int $power) {
        $this->model = $model;
        $this->wheels = 4;
        $this->engine = new Engine($power);
    }

    // PHP calls __clone() on the freshly copied object, not the original.
    public function __clone() {
        // Deep-copy the engine so the clone owns an independent part.
        $this->engine = clone $this->engine;
        echo "Cloned a {$this->model}\n";
    }
}

$original = new Car("Roadster", 200);

// `clone` makes a shallow copy, then runs __clone() on the new object.
$copy = clone $original;

// Scalar and string properties are independent after the clone.
$copy->model = "Coupe";
$copy->wheels = 2;

echo $original->model, " ", $original->wheels, " ", $original->engine->power, "\n";
echo $copy->model, " ", $copy->wheels, " ", $copy->engine->power, "\n";

// Because __clone() deep-copied the engine, tuning the copy's engine
// leaves the original's engine untouched.
$copy->engine->power = 400;
echo $original->engine->power, "\n";
echo $copy->engine->power, "\n";