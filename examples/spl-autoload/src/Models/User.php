<?php
namespace App\Models;

class User {
    public function __construct(public string $name) {}

    public function greet(): string {
        return "hello " . $this->name;
    }
}
