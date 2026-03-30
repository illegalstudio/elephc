<?php

class Animal {
    protected $name = "animal";

    public function label() {
        return $this->name;
    }

    public function speak() {
        return "animal";
    }

    public function run() {
        return $this->speak();
    }
}

class Dog extends Animal {
    public function __construct() {
        $this->name = "dog";
    }

    public function speak() {
        return parent::speak() . "-woof";
    }
}

$dog = new Dog();
echo $dog->label() . "\n";
echo $dog->run() . "\n";
