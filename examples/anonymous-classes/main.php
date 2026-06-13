<?php

// An interface with a single behavior.
interface Formatter
{
    public function format(string $value): string;
}

// A factory that returns one-off implementations as anonymous classes.
function make_formatter(string $kind): Formatter
{
    if ($kind === "shout") {
        return new class implements Formatter {
            public function format(string $value): string
            {
                return strtoupper($value) . "!";
            }
        };
    }

    return new class(">> ") implements Formatter {
        public function __construct(private string $prefix) {}

        public function format(string $value): string
        {
            return $this->prefix . $value;
        }
    };
}

echo make_formatter("shout")->format("hello"), "\n";
echo make_formatter("quiet")->format("hello"), "\n";

// Anonymous classes can also extend a base class.
abstract class Counter
{
    protected int $count = 0;

    abstract public function step(): int;

    public function advance(): int
    {
        $this->count = $this->count + $this->step();
        return $this->count;
    }
}

$byTwos = new class extends Counter {
    public function step(): int
    {
        return 2;
    }
};

echo $byTwos->advance(), $byTwos->advance(), $byTwos->advance(), "\n";
