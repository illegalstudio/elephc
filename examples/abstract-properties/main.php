<?php

abstract class Shape {
    abstract public int $sides { get; set; }
    abstract public string $name { get; set; }

    public function describe() {
        return $this->name . " has " . $this->sides . " sides";
    }
}

class Triangle extends Shape {
    public int $sides = 3;
    public string $name = "triangle";
}

class Square extends Shape {
    public int $sides = 4;
    public string $name = "square";
}

$shapes = [new Triangle(), new Square()];
foreach ($shapes as $shape) {
    echo $shape->describe();
    echo "\n";
}
