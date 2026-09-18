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

// A named virtual call has a runtime-selected receiver but a closed set of
// checked implementations. Its property and literal-array reads are likewise
// visible to the optimizer's EIR effect refinement.
class Renderer
{
    public $prefix = "result: ";

    public function render(array $parts): string
    {
        return $this->prefix . $parts[0];
    }
}

final class LoudRenderer extends Renderer
{
    public function render(array $parts): string
    {
        return strtoupper($this->prefix . $parts[0]);
    }
}

$renderer = $argc > 1 ? new LoudRenderer() : new Renderer();
echo $renderer->render(["effects"]), "\n";

// A string-keyed array spread into a call binds its keys as NAMED arguments, and that holds when
// the target itself is dynamic: a class name in a variable, a method name in a variable, or a
// static call through a variable class. Parameters the array does not name keep their defaults.
class Viewport
{
    public function __construct(public int $width = 80, public int $height = 24) {}

    public function resize(int $width = 80, int $height = 24): string
    {
        return $width . "x" . $height;
    }

    public static function describe(string $label = "viewport", int $height = 24): string
    {
        return $label . "@" . $height;
    }
}

$class = "Viewport";
$method = "resize";
$options = ["height" => 40];

$viewport = new $class(...$options);
echo "dynamic new: " . $viewport->width . "x" . $viewport->height . "\n";
echo "dynamic method: " . $viewport->$method(...$options) . "\n";
echo "dynamic static: " . $class::describe(...$options) . "\n";

// Integer keys in the same spread stay positional, so one array can carry both.
$mixed_keys = [0 => 100, "height" => 50];
echo "mixed keys: " . (new $class(...$mixed_keys))->width . "\n";
