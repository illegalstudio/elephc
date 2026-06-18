<?php

// A tiny command handler: the method to run is chosen at runtime by name.
class Commands
{
    public function greet(string $who): string
    {
        return "Hello, " . $who;
    }

    public function shout(string $text): string
    {
        return strtoupper($text) . "!";
    }

    public static function help(): string
    {
        return "commands: greet, shout";
    }
}

$handler = new Commands();

// Dispatch instance methods by a name held in a variable.
foreach (["greet", "shout"] as $command) {
    echo $handler->$command($command === "greet" ? "world" : "loud"), "\n";
}

// Dispatch a static method through a dynamic class name.
$class = "Commands";
echo $class::help(), "\n";
