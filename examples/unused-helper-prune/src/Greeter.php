<?php

namespace App;

// The class the program actually uses; it is pulled in normally.
class Greeter
{
    public function hello(string $name): string
    {
        return "Hello, $name!";
    }
}
